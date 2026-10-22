//! Domains (Phase 3 plan §6–§7): the owner attaches domains to the server,
//! Opencord checks they lead here, and clients connect and are invited
//! through them. Checks run in the background; the owner's page hears of
//! them through `NetworkStatus`, and everyone hears where the server is
//! reached through `ServerAddressesUpdate`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use proto::event::Kind as Event;

use crate::db::domains::{self as table, DomainRow};
use crate::error::ApiError;
use crate::state::{AppState, Audience, now_ms};

pub mod check;
pub mod dns;
pub mod free;
pub mod hostname;
pub mod http_check;
mod schedule;
pub mod web;

use check::{CheckOutcome, CheckResult};

/// Domains a server may have (plan §6.1).
pub const MAX_DOMAINS: usize = 20;
/// A domain may be checked by hand once in this long.
pub const CHECK_COOLDOWN: Duration = Duration::from_secs(10);
/// A deactivated domain keeps working this long (plan §6.3).
pub const DEACTIVATED_GRACE_MS: i64 = 7 * 24 * 3600 * 1000;
const USE_WINDOW_MS: i64 = 24 * 3600 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainKind {
    Own,
    FreeSubdomain,
    Tunnel,
    ReverseProxy,
    /// Proxied by Cloudflare: Cloudflare's certificate for apps, which it
    /// brings to the main port.
    CloudflareProxy,
}

impl DomainKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Own => "own",
            Self::FreeSubdomain => "free_subdomain",
            Self::Tunnel => "tunnel",
            Self::ReverseProxy => "reverse_proxy",
            Self::CloudflareProxy => "cloudflare_proxy",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        [
            Self::Own,
            Self::FreeSubdomain,
            Self::Tunnel,
            Self::ReverseProxy,
            Self::CloudflareProxy,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == text)
    }

    pub fn from_proto(kind: i32) -> Option<Self> {
        match proto::DomainKind::try_from(kind).ok()? {
            proto::DomainKind::Own => Some(Self::Own),
            proto::DomainKind::FreeSubdomain => Some(Self::FreeSubdomain),
            proto::DomainKind::Tunnel => Some(Self::Tunnel),
            proto::DomainKind::ReverseProxy => Some(Self::ReverseProxy),
            proto::DomainKind::CloudflareProxy => Some(Self::CloudflareProxy),
            proto::DomainKind::Unspecified => None,
        }
    }

    fn to_proto(self) -> proto::DomainKind {
        match self {
            Self::Own => proto::DomainKind::Own,
            Self::FreeSubdomain => proto::DomainKind::FreeSubdomain,
            Self::Tunnel => proto::DomainKind::Tunnel,
            Self::ReverseProxy => proto::DomainKind::ReverseProxy,
            Self::CloudflareProxy => proto::DomainKind::CloudflareProxy,
        }
    }

    /// Checked with DNS; the others with HTTP through the tunnel or proxy.
    pub fn checked_by_dns(self) -> bool {
        matches!(self, Self::Own | Self::FreeSubdomain)
    }

    /// Behind a tunnel or proxy, whose certificate an authority signed.
    pub fn behind_proxy(self) -> bool {
        matches!(
            self,
            Self::Tunnel | Self::ReverseProxy | Self::CloudflareProxy
        )
    }

    /// Reached through the plain listener on this machine; Cloudflare's
    /// proxy comes to the main port over TLS instead.
    pub fn through_local_listener(self) -> bool {
        matches!(self, Self::Tunnel | Self::ReverseProxy)
    }

    /// The main port answers for it with a certificate of its own: one from
    /// an authority, or, behind Cloudflare's proxy, the origin certificate
    /// the owner uploads.
    pub fn takes_own_certificate(self) -> bool {
        !self.behind_proxy() || self == Self::CloudflareProxy
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Pending,
    Verified,
    DnsProblem,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Verified => "verified",
            Self::DnsProblem => "dns_problem",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "verified" => Self::Verified,
            "dns_problem" => Self::DnsProblem,
            _ => Self::Pending,
        }
    }

    /// Verified at some point: it may be active.
    pub fn was_verified(self) -> bool {
        matches!(self, Self::Verified | Self::DnsProblem)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    pub id: i64,
    pub hostname: String,
    pub kind: DomainKind,
    pub public_port: u16,
    pub status: Status,
    pub active: bool,
    pub primary: bool,
    pub problem_since: Option<i64>,
    pub deactivated_at: Option<i64>,
    pub last_check: Option<CheckOutcome>,
    pub created_at: i64,
}

impl Domain {
    fn from_row(row: DomainRow) -> Option<Self> {
        Some(Self {
            id: row.id,
            kind: DomainKind::parse(&row.kind)?,
            hostname: row.hostname,
            public_port: u16::try_from(row.public_port).ok()?,
            status: Status::parse(&row.status),
            active: row.active,
            primary: row.is_primary,
            problem_since: row.problem_since,
            deactivated_at: row.deactivated_at,
            last_check: row
                .last_result
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok()),
            created_at: row.created_at,
        })
    }

    fn to_row(&self) -> DomainRow {
        DomainRow {
            id: self.id,
            hostname: self.hostname.clone(),
            kind: self.kind.as_str().to_owned(),
            public_port: i64::from(self.public_port),
            status: self.status.as_str().to_owned(),
            active: self.active,
            is_primary: self.primary,
            problem_since: self.problem_since,
            deactivated_at: self.deactivated_at,
            last_checked_at: self.last_check.as_ref().map(|check| check.checked_at_ms),
            last_result: self
                .last_check
                .as_ref()
                .and_then(|check| serde_json::to_string(check).ok()),
            created_at: self.created_at,
        }
    }

    /// Clients may check it like a website: the certificate is an
    /// authority's, a tunnel's or proxy's, or one this server holds.
    pub fn trusted_certificate(&self, state: &AppState) -> bool {
        self.kind.behind_proxy() || state.certificates.trusted(self.id, now_ms())
    }

    pub fn address(&self, state: &AppState) -> proto::ConnectAddress {
        proto::ConnectAddress {
            host: self.hostname.clone(),
            port: u32::from(self.public_port),
            trusted_certificate: self.trusted_certificate(state),
            domain_id: self.id,
        }
    }
}

#[derive(Debug, Default)]
pub struct Domains {
    list: Mutex<Vec<Domain>>,
    checking: Mutex<HashSet<i64>>,
    checked_by_hand: Mutex<HashMap<i64, Instant>>,
    /// Per domain, who connected through it and when.
    uses: Mutex<HashMap<i64, HashMap<i64, i64>>>,
    /// What `/.well-known/opencord/verify` answers.
    verify_token: Mutex<String>,
    /// The addresses last announced.
    announced: Mutex<Option<proto::ServerAddresses>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Domains {
    /// The domains saved, and the token HTTP checks look for.
    pub fn load(rows: Vec<DomainRow>, verify_token: String) -> Self {
        Self {
            list: Mutex::new(rows.into_iter().filter_map(Domain::from_row).collect()),
            verify_token: Mutex::new(verify_token),
            ..Self::default()
        }
    }

    pub fn verify_token(&self) -> String {
        lock(&self.verify_token).clone()
    }

    pub fn all(&self) -> Vec<Domain> {
        lock(&self.list).clone()
    }

    pub fn get(&self, id: i64) -> Option<Domain> {
        lock(&self.list)
            .iter()
            .find(|domain| domain.id == id)
            .cloned()
    }

    /// The active domain called `host`, if any.
    pub fn active_named(&self, host: &str) -> Option<Domain> {
        let host = host.to_ascii_lowercase();
        lock(&self.list)
            .iter()
            .find(|domain| domain.active && domain.hostname == host)
            .cloned()
    }

    /// The names an `Identify` may be bound to: the configured public host,
    /// active domains, and domains turned off less than a week ago.
    pub fn bindable_names(&self, public_host: &str, now: i64) -> Vec<String> {
        let mut names: Vec<String> = lock(&self.list)
            .iter()
            .filter(|domain| {
                domain.active
                    || domain
                        .deactivated_at
                        .is_some_and(|at| now - at < DEACTIVATED_GRACE_MS)
            })
            .map(|domain| domain.hostname.clone())
            .collect();
        let configured = public_host.to_ascii_lowercase();
        if configured.parse::<std::net::IpAddr>().is_err() && !names.contains(&configured) {
            names.push(configured);
        }
        names
    }

    pub fn primary(&self) -> Option<Domain> {
        lock(&self.list)
            .iter()
            .find(|domain| domain.primary && domain.active)
            .cloned()
    }

    /// Whether an active domain is proxied by Cloudflare: then Cloudflare's
    /// addresses are believed about who they forward.
    pub fn cloudflare_proxied(&self) -> bool {
        lock(&self.list)
            .iter()
            .any(|domain| domain.active && domain.kind == DomainKind::CloudflareProxy)
    }

    /// Whether `host` is an active tunnel or reverse-proxy domain: what
    /// comes through it carries no UDP.
    pub fn behind_proxy(&self, host: &str) -> bool {
        let host = host_without_port(host).to_ascii_lowercase();
        lock(&self.list)
            .iter()
            .any(|domain| domain.active && domain.kind.behind_proxy() && domain.hostname == host)
    }

    /// Someone signed in through `host`: counted for that domain.
    pub fn used(&self, host: Option<&str>, user_id: i64) {
        let Some(host) = host.map(|host| host_without_port(host).to_ascii_lowercase()) else {
            return;
        };
        let id = lock(&self.list)
            .iter()
            .find(|domain| domain.hostname == host)
            .map(|domain| domain.id);
        if let Some(id) = id {
            lock(&self.uses)
                .entry(id)
                .or_default()
                .insert(user_id, now_ms());
        }
    }

    fn users_24h(&self, id: i64, now: i64) -> u32 {
        lock(&self.uses).get(&id).map_or(0, |users| {
            let recent = users
                .values()
                .filter(|at| now - **at < USE_WINDOW_MS)
                .count();
            u32::try_from(recent).unwrap_or(u32::MAX)
        })
    }

    pub fn to_proto(&self, state: &AppState) -> Vec<proto::Domain> {
        let now = now_ms();
        let checking = lock(&self.checking).clone();
        self.all()
            .into_iter()
            .map(|domain| proto::Domain {
                id: domain.id,
                hostname: domain.hostname.clone(),
                kind: domain.kind.to_proto() as i32,
                public_port: u32::from(domain.public_port),
                status: match domain.status {
                    Status::Pending => proto::DomainStatus::Pending,
                    Status::Verified => proto::DomainStatus::Verified,
                    Status::DnsProblem => proto::DomainStatus::DnsProblem,
                } as i32,
                active: domain.active,
                primary: domain.primary,
                problem_since_ms: domain.problem_since.unwrap_or(0),
                last_check: domain.last_check.as_ref().map(check_to_proto),
                checking: checking.contains(&domain.id),
                users_24h: self.users_24h(domain.id, now),
                created_at_ms: domain.created_at,
                deactivated_at_ms: domain.deactivated_at.unwrap_or(0),
                trusted_certificate: domain.trusted_certificate(state),
                certificate: Some(state.certificates.status(&domain, now)),
            })
            .collect()
    }

    /// Where clients reach the server: the active domains, the primary
    /// first, and its public IP address.
    pub fn addresses(&self, state: &AppState) -> proto::ServerAddresses {
        let mut active: Vec<Domain> = self
            .all()
            .into_iter()
            .filter(|domain| domain.active)
            .collect();
        active.sort_by_key(|domain| (!domain.primary, domain.created_at));
        let primary = active
            .first()
            .filter(|domain| domain.primary)
            .map(|domain| domain.address(state));
        let ip = state
            .network
            .public_ips()
            .borrow()
            .v4
            .map(|ip| proto::ConnectAddress {
                host: ip.to_string(),
                port: u32::from(state.network.main_port()),
                trusted_certificate: false,
                domain_id: 0,
            });
        proto::ServerAddresses {
            primary,
            domains: active.iter().map(|domain| domain.address(state)).collect(),
            ip,
        }
    }

    /// Tells everyone where the server is reached, when that changed.
    pub fn announce(&self, state: &AppState) {
        let addresses = self.addresses(state);
        {
            let mut announced = lock(&self.announced);
            if announced.as_ref() == Some(&addresses) {
                return;
            }
            *announced = Some(addresses.clone());
        }
        state.broadcast(
            Event::ServerAddressesUpdate(proto::ServerAddressesUpdate {
                addresses: Some(addresses),
            }),
            Audience::Everyone,
        );
    }

    /// Remembers what Ready gave out, so the next change is announced.
    pub fn mark_announced(&self, addresses: &proto::ServerAddresses) {
        lock(&self.announced).get_or_insert_with(|| addresses.clone());
    }

    /// Adds a domain, pending verification.
    pub async fn add(
        &self,
        state: &Arc<AppState>,
        hostname: &str,
        kind: DomainKind,
        port: Option<u16>,
    ) -> Result<Domain, ApiError> {
        let hostname = hostname::normalize(hostname)
            .map_err(|error| ApiError::new(proto::ErrorCode::DomainInvalid, error.to_string()))?;
        let public_port = match port {
            Some(port) => port,
            None if kind.behind_proxy() => 443,
            None => state.network.main_port(),
        };
        if public_port == 0 {
            return Err(ApiError::invalid_argument("the port is 1 to 65535"));
        }
        let domain = {
            let list = lock(&self.list);
            if list.len() >= MAX_DOMAINS {
                return Err(ApiError::new(
                    proto::ErrorCode::DomainLimit,
                    format!("a server has at most {MAX_DOMAINS} domains"),
                ));
            }
            if list.iter().any(|domain| domain.hostname == hostname) {
                return Err(ApiError::new(
                    proto::ErrorCode::DomainExists,
                    format!("{hostname} is already added"),
                ));
            }
            Domain {
                id: state.ids.next_id(),
                hostname,
                kind,
                public_port,
                status: Status::Pending,
                active: false,
                primary: false,
                problem_since: None,
                deactivated_at: None,
                last_check: None,
                created_at: now_ms(),
            }
        };
        table::insert(&mut *state.db.acquire().await?, &domain.to_row()).await?;
        lock(&self.list).push(domain.clone());
        tracing::info!(hostname = %domain.hostname, kind = domain.kind.as_str(), "a domain was added");
        Ok(domain)
    }

    /// Turns a domain on or off, makes it the primary, changes its port, or
    /// confirms it by hand.
    pub async fn update(
        &self,
        state: &Arc<AppState>,
        request: &proto::UpdateDomain,
    ) -> Result<(), ApiError> {
        let now = now_ms();
        let changed = {
            let mut list = lock(&self.list);
            let index = list
                .iter()
                .position(|domain| domain.id == request.domain_id)
                .ok_or_else(|| ApiError::not_found("domain"))?;
            let mut next = list.clone();
            let domain = &mut next[index];
            if let Some(port) = request.public_port {
                domain.public_port = u16::try_from(port)
                    .ok()
                    .filter(|port| *port != 0)
                    .ok_or_else(|| ApiError::invalid_argument("the port is 1 to 65535"))?;
            }
            if request.confirm {
                if !domain.kind.behind_proxy() {
                    return Err(ApiError::invalid_argument(
                        "only tunnel and reverse-proxy domains are confirmed by hand",
                    ));
                }
                domain.status = Status::Verified;
                domain.problem_since = None;
                domain.last_check = Some(CheckOutcome::new(
                    CheckResult::ConfirmedByOwner,
                    "Confirmed by the owner.",
                    now,
                ));
            }
            let make_primary = request.primary == Some(true);
            if request.active == Some(true) || make_primary {
                if !next[index].status.was_verified() {
                    return Err(ApiError::new(
                        proto::ErrorCode::DomainNotVerified,
                        "check the domain first: only verified domains can be used",
                    ));
                }
                next[index].active = true;
                next[index].deactivated_at = None;
            }
            if request.active == Some(false) {
                if next[index].primary && next.iter().filter(|d| d.active).count() > 1 {
                    return Err(ApiError::conflict(
                        "make another domain the primary before turning this one off",
                    ));
                }
                next[index].active = false;
                next[index].primary = false;
                next[index].deactivated_at = Some(now);
            }
            if make_primary {
                for domain in &mut next {
                    domain.primary = false;
                }
                next[index].primary = true;
            }
            // Exactly one active domain is the primary, while any is active.
            if !next.iter().any(|domain| domain.active && domain.primary)
                && let Some(first) = next.iter_mut().find(|domain| domain.active)
            {
                first.primary = true;
            }
            let changed: Vec<Domain> = next
                .iter()
                .filter(|domain| !list.contains(domain))
                .cloned()
                .collect();
            *list = next;
            changed
        };
        self.save(state, &changed).await?;
        state.certificates.serve(&self.all(), now);
        self.announce(state);
        Ok(())
    }

    /// Removes a domain, its checks and its invites.
    pub async fn delete(&self, state: &Arc<AppState>, id: i64) -> Result<(), ApiError> {
        self.remove(state, id, false).await
    }

    /// Removes a domain the server made itself (a quick tunnel's), moving
    /// the primary to another active domain when it was the primary.
    pub async fn remove_own(&self, state: &Arc<AppState>, id: i64) -> Result<(), ApiError> {
        self.remove(state, id, true).await
    }

    async fn remove(
        &self,
        state: &Arc<AppState>,
        id: i64,
        move_primary: bool,
    ) -> Result<(), ApiError> {
        let changed = {
            let mut list = lock(&self.list);
            let domain = list
                .iter()
                .find(|domain| domain.id == id)
                .cloned()
                .ok_or_else(|| ApiError::not_found("domain"))?;
            if domain.primary && !move_primary && list.iter().filter(|d| d.active).count() > 1 {
                return Err(ApiError::conflict(
                    "make another domain the primary before deleting this one",
                ));
            }
            list.retain(|domain| domain.id != id);
            let mut changed = Vec::new();
            if !list.iter().any(|d| d.active && d.primary)
                && let Some(first) = list.iter_mut().find(|domain| domain.active)
            {
                first.primary = true;
                changed.push(first.clone());
            }
            tracing::info!(hostname = %domain.hostname, "a domain was deleted");
            (changed, domain)
        };
        let (changed, deleted) = changed;
        table::delete(&mut *state.db.acquire().await?, id).await?;
        state.certificates.forget(&deleted);
        lock(&self.uses).remove(&id);
        self.save(state, &changed).await?;
        self.announce(state);
        Ok(())
    }

    async fn save(&self, state: &AppState, changed: &[Domain]) -> Result<(), ApiError> {
        let mut conn = state.db.acquire().await?;
        for domain in changed {
            table::save(&mut conn, &domain.to_row()).await?;
        }
        Ok(())
    }

    /// Checks a domain by hand: at most once per [`CHECK_COOLDOWN`].
    pub fn check_by_hand(&self, state: &Arc<AppState>, id: i64) -> Result<(), ApiError> {
        if self.get(id).is_none() {
            return Err(ApiError::not_found("domain"));
        }
        {
            let mut checked = lock(&self.checked_by_hand);
            let now = Instant::now();
            if let Some(at) = checked.get(&id)
                && now.duration_since(*at) < CHECK_COOLDOWN
            {
                return Err(ApiError::rate_limited(
                    CHECK_COOLDOWN - now.duration_since(*at),
                ));
            }
            checked.insert(id, now);
        }
        self.start_check(state, id);
        Ok(())
    }

    /// Checks a domain in the background, unless one is running.
    pub fn start_check(&self, state: &Arc<AppState>, id: i64) {
        if !lock(&self.checking).insert(id) {
            return;
        }
        state.network.changed(state);
        let state = Arc::clone(state);
        tokio::spawn(async move {
            run_check(&state, id).await;
        });
    }

    /// A domain's last checks, newest first.
    pub async fn history(
        &self,
        state: &AppState,
        id: i64,
    ) -> Result<Vec<proto::DomainCheck>, ApiError> {
        if self.get(id).is_none() {
            return Err(ApiError::not_found("domain"));
        }
        let stored = table::checks(&mut *state.db.acquire().await?, id).await?;
        Ok(stored
            .iter()
            .filter_map(|json| serde_json::from_str::<CheckOutcome>(json).ok())
            .map(|check| check_to_proto(&check))
            .collect())
    }
}

/// Checks a new domain until it passes, while DNS catches up with it (a new
/// free subdomain, a new tunnel): every 5 seconds for a minute, then every
/// 30 seconds, for 10 minutes at most.
pub fn check_until_verified(state: &Arc<AppState>, id: i64) {
    const QUICKLY: Duration = Duration::from_secs(5);
    const SLOWLY: Duration = Duration::from_secs(30);
    const AT_FIRST: Duration = Duration::from_secs(60);
    const AT_MOST: Duration = Duration::from_secs(600);
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let started = tokio::time::Instant::now();
        while started.elapsed() < AT_MOST {
            state.domains.start_check(&state, id);
            let pause = if started.elapsed() < AT_FIRST {
                QUICKLY
            } else {
                SLOWLY
            };
            tokio::select! {
                () = tokio::time::sleep(pause) => {}
                () = state.shutdown.cancelled() => return,
            }
            match state.domains.get(id) {
                Some(domain) if domain.status == Status::Verified => return,
                Some(_) => {}
                None => return,
            }
        }
    });
}

/// Runs the local listener while a tunnel or reverse-proxy domain exists
/// (plan §5.2), and stops it when none does.
pub async fn sync_local_listener(state: &Arc<AppState>) {
    let wanted = state.tunnel.wanted()
        || state
            .domains
            .all()
            .iter()
            .any(|domain| domain.kind.through_local_listener());
    if let Err(error) = state.network.set_local_listener(state, wanted).await {
        tracing::warn!(%error, "the local listener for tunnels and proxies could not start");
    }
}

/// Runs one check and applies what it found.
async fn run_check(state: &Arc<AppState>, id: i64) {
    let Some(domain) = state.domains.get(id) else {
        lock(&state.domains.checking).remove(&id);
        return;
    };
    let now = now_ms();
    let outcome = if domain.kind.checked_by_dns() {
        let checker = state.network.dns_checker();
        let authoritative = checker.authoritative(&domain.hostname).await;
        let resolver = match &authoritative {
            Ok(_) => checker.recursive(&domain.hostname).await.ok(),
            Err(_) => None,
        };
        check::dns_outcome(
            &domain.hostname,
            authoritative,
            resolver.as_ref(),
            *state.network.public_ips().borrow(),
            now,
        )
    } else {
        http_check::check(
            state.network.http(),
            &domain.hostname,
            domain.public_port,
            &state.domains.verify_token(),
            now,
        )
        .await
    };
    apply_check(state, id, outcome).await;
}

/// Updates a domain with a check's outcome, records it, and tells whom it
/// concerns.
async fn apply_check(state: &Arc<AppState>, id: i64, outcome: CheckOutcome) {
    let domains = &state.domains;
    let (updated, broke) = {
        let mut list = lock(&domains.list);
        let Some(domain) = list.iter_mut().find(|domain| domain.id == id) else {
            drop(list);
            lock(&domains.checking).remove(&id);
            return;
        };
        let was = domain.status;
        let passed = outcome.result.passed();
        domain.status = match (passed, was) {
            (true, _) => Status::Verified,
            (false, Status::Pending) => Status::Pending,
            (false, _) => Status::DnsProblem,
        };
        domain.problem_since = match domain.status {
            Status::DnsProblem => domain.problem_since.or(Some(outcome.checked_at_ms)),
            _ => None,
        };
        // A confirmation by hand stands until a check passes or another
        // reason to doubt it comes.
        domain.last_check = Some(outcome.clone());
        let mut first = false;
        if passed && !list.iter().any(|d| d.active) {
            // The first verified domain becomes the primary by itself.
            if let Some(domain) = list.iter_mut().find(|domain| domain.id == id) {
                domain.active = true;
                domain.primary = true;
                domain.deactivated_at = None;
                first = true;
            }
        }
        let updated = list.iter().find(|domain| domain.id == id).cloned();
        let broke = was == Status::Verified
            && updated.as_ref().map(|d| d.status) == Some(Status::DnsProblem);
        if first {
            tracing::info!(id, "the first verified domain became the primary");
        }
        (updated, broke)
    };
    lock(&domains.checking).remove(&id);
    let Some(updated) = updated else {
        return;
    };
    let details = serde_json::to_string(&outcome).unwrap_or_default();
    let result = serde_json::to_value(outcome.result)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    if let Ok(mut conn) = state.db.acquire().await {
        let saved = table::save(&mut conn, &updated.to_row()).await;
        let recorded =
            table::add_check(&mut conn, id, outcome.checked_at_ms, &result, &details).await;
        if let Err(error) = saved.and(recorded) {
            tracing::warn!(%error, "could not save a domain check");
        }
    }
    if broke && let Some(owner) = state.guild().meta.owner_id {
        state.broadcast(
            Event::ServerNotice(proto::ServerNotice {
                key: format!("domain.{id}.dns"),
                severity: proto::NoticeSeverity::Warning as i32,
                title: format!("{} has a DNS problem", updated.hostname),
                body: format!(
                    "{} It stays active meanwhile; certificates wait until it is fixed.",
                    outcome.message
                ),
            }),
            Audience::User(owner),
        );
    }
    state.network.changed(state);
    domains.announce(state);
    if updated.kind == DomainKind::FreeSubdomain && updated.status == Status::Verified {
        state.certificates.request_by_itself(state, &updated).await;
    }
}

fn check_to_proto(check: &CheckOutcome) -> proto::DomainCheck {
    proto::DomainCheck {
        result: match check.result {
            CheckResult::Verified => proto::CheckResult::Verified,
            CheckResult::PointsElsewhere => proto::CheckResult::PointsElsewhere,
            CheckResult::NoRecord => proto::CheckResult::NoRecord,
            CheckResult::CloudflareProxied => proto::CheckResult::CloudflareProxied,
            CheckResult::PartlyCorrect => proto::CheckResult::PartlyCorrect,
            CheckResult::Failed => proto::CheckResult::Failed,
            CheckResult::ConfirmedByOwner => proto::CheckResult::ConfirmedByOwner,
        } as i32,
        message: check.message.clone(),
        note: check.note.clone().unwrap_or_default(),
        found: check.found.iter().map(ToString::to_string).collect(),
        checked_at_ms: check.checked_at_ms,
    }
}

/// `host` from a `Host` header: without its port, or brackets.
pub fn host_without_port(host: &str) -> &str {
    if let Some(bracketed) = host.strip_prefix('[') {
        return bracketed.split(']').next().unwrap_or(bracketed);
    }
    match host.rsplit_once(':') {
        Some((name, port)) if port.bytes().all(|b| b.is_ascii_digit()) => name,
        _ => host,
    }
}

pub use schedule::start;
