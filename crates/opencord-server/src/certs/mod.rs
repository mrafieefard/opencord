//! Certificates for domains (Phase 3 plan §7): from Let's Encrypt, or
//! uploaded; kept in the database with their keys sealed, served by the
//! name clients ask for, renewed in the background.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use instant_acme::AccountCredentials;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use rustls::sign::CertifiedKey;

pub mod acme;
pub mod pem;
pub mod providers;
pub mod store;

use acme::{Http01, Step};
use providers::{Endpoints, Provider, ProviderKind};
use store::{CertStore, certified_key};

use crate::db::certificates::{self as table, AccountRow, CertificateRow, ProviderRow};
use crate::domains::{DEACTIVATED_GRACE_MS, Domain, Status};
use crate::error::ApiError;
use crate::secrets::Secrets;
use crate::state::{AppState, Audience, now_ms};

/// Failed requests allowed per domain per hour before it waits (plan
/// §7.2).
const MAX_FAILURES: usize = 5;
const HOUR_MS: i64 = 3600 * 1000;
const DAY_MS: i64 = 24 * HOUR_MS;
/// After a failed renewal, the next try.
const RENEW_RETRY_MS: i64 = 6 * HOUR_MS;
/// How often renewals are looked at.
const RENEW_EVERY: Duration = Duration::from_secs(3600);
/// The owner hears when a certificate comes this close to expiring.
const WARN_ACME_MS: i64 = 7 * DAY_MS;
const WARN_UPLOADED_MS: i64 = 14 * DAY_MS;
const ACCOUNT: &str = "acme-account";
pub const TERMS_URL: &str = "https://letsencrypt.org/repository/";

/// A certificate this server holds.
#[derive(Clone)]
struct Held {
    issuer: String,
    not_before: i64,
    not_after: i64,
    is_test: bool,
    uploaded: bool,
    chain_pem: String,
    key: Arc<CertifiedKey>,
}

#[derive(Debug, Clone, Default)]
struct Progress {
    step: Option<Step>,
    error: Option<String>,
    retry_at: Option<i64>,
    /// The last renewal failed; the next try waits until this.
    renew_after: Option<i64>,
    warned_at: Option<i64>,
}

pub struct Certificates {
    pub store: Arc<CertStore>,
    pub http01: Http01,
    secrets: Secrets,
    held: Mutex<HashMap<i64, Held>>,
    progress: Mutex<HashMap<i64, Progress>>,
    failures: Mutex<HashMap<i64, Vec<i64>>>,
    providers: Mutex<Vec<Provider>>,
    /// ACME accounts by directory, with their email.
    accounts: Mutex<HashMap<String, Option<String>>>,
    /// The owner agreed to the CA's terms for a certificate requested by
    /// itself later (a free subdomain's), with this email.
    agreed: Mutex<Option<Option<String>>>,
}

impl std::fmt::Debug for Certificates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Certificates").finish_non_exhaustive()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn key_purpose(domain_id: i64) -> String {
    format!("certificate-key:{domain_id}")
}

fn token_purpose(kind: ProviderKind) -> String {
    format!("dns-provider:{}", kind.as_str())
}

impl Certificates {
    pub fn new(store: Arc<CertStore>, secrets: Secrets) -> Self {
        Self {
            store,
            http01: Http01::default(),
            secrets,
            held: Mutex::default(),
            progress: Mutex::default(),
            failures: Mutex::default(),
            providers: Mutex::default(),
            accounts: Mutex::default(),
            agreed: Mutex::default(),
        }
    }

    /// The connected provider of a kind.
    pub fn provider(&self, kind: ProviderKind) -> Option<Provider> {
        lock(&self.providers)
            .iter()
            .find(|provider| provider.kind == kind)
            .cloned()
    }

    /// Whether a certificate can be requested without asking the owner
    /// to agree first: an account exists, or they agreed already.
    pub fn may_request(&self, state: &AppState) -> bool {
        lock(&self.accounts).contains_key(&directory(state)) || lock(&self.agreed).is_some()
    }

    /// Remembers the owner's agreement for a certificate the server
    /// requests by itself.
    pub fn agree(&self, email: Option<String>) {
        *lock(&self.agreed) = Some(email);
    }

    /// Requests a free subdomain's certificate by itself, once it is
    /// verified and has none (plan §7.1: "Free subdomains get one
    /// automatically").
    pub async fn request_by_itself(&self, state: &Arc<AppState>, domain: &Domain) {
        let now = now_ms();
        let held = lock(&self.held).contains_key(&domain.id);
        let busy = lock(&self.progress)
            .get(&domain.id)
            .is_some_and(|progress| progress.step.is_some());
        if held || busy || self.retry_at(domain.id, now).is_some() || !self.may_request(state) {
            return;
        }
        let email = lock(&self.agreed).clone().flatten();
        if let Err(error) = self.request(state, domain.id, email, true).await {
            tracing::info!(hostname = %domain.hostname, error = %error.message, "a free subdomain's certificate could not be requested");
        }
    }

    /// For an `AppState` that never serves TLS itself (tests of handlers):
    /// a throwaway certificate and key.
    pub fn throwaway() -> Self {
        let key = rcgen::KeyPair::generate().expect("a key can be made");
        let params = rcgen::CertificateParams::new(vec!["localhost".to_owned()])
            .expect("localhost is a name");
        let certificate = params.self_signed(&key).expect("it signs itself");
        let certified = certified_key(
            vec![certificate.der().clone()],
            &rustls::pki_types::PrivateKeyDer::Pkcs8(key.serialize_der().into()),
        )
        .expect("a fresh key works");
        Self::new(
            Arc::new(CertStore::new(certified)),
            Secrets::from_key(&crate::random::bytes::<32>()),
        )
    }

    pub fn secrets(&self) -> &Secrets {
        &self.secrets
    }

    /// What the database holds: certificates and provider tokens. One that
    /// cannot be opened is left out and logged.
    pub async fn load(&self, db: &sqlx::SqlitePool) -> anyhow::Result<()> {
        let mut conn = db.acquire().await?;
        for row in table::list(&mut conn).await? {
            match self.held_from(&row) {
                Ok(held) => {
                    lock(&self.held).insert(row.domain_id, held);
                }
                Err(error) => {
                    tracing::warn!(domain_id = row.domain_id, %error, "a certificate could not be loaded");
                }
            }
        }
        let mut providers = Vec::new();
        for row in table::providers(&mut conn).await? {
            let Some(kind) = ProviderKind::parse(&row.kind) else {
                continue;
            };
            match self
                .secrets
                .open_string(&token_purpose(kind), &row.token_enc)
            {
                Ok(token) => providers.push(Provider { kind, token }),
                Err(error) => {
                    tracing::warn!(kind = row.kind, %error, "a DNS provider token could not be opened")
                }
            }
        }
        *lock(&self.providers) = providers;
        *lock(&self.accounts) = table::accounts(&mut conn)
            .await?
            .into_iter()
            .map(|row| (row.directory, row.email))
            .collect();
        Ok(())
    }

    fn held_from(&self, row: &CertificateRow) -> anyhow::Result<Held> {
        let key_pem = self
            .secrets
            .open_string(&key_purpose(row.domain_id), &row.key_enc)?;
        let chain = pem::chain(&row.chain_pem)?;
        let key = certified_key(chain, &pem::key(&key_pem)?)?;
        Ok(Held {
            issuer: row.issuer.clone(),
            not_before: row.not_before,
            not_after: row.not_after,
            is_test: row.is_test,
            uploaded: row.uploaded,
            chain_pem: row.chain_pem.clone(),
            key,
        })
    }

    /// Serves each domain's certificate while the domain is active, or
    /// turned off less than a week ago; never a test one, never an
    /// expired one.
    pub fn serve(&self, domains: &[Domain], now: i64) {
        let held = lock(&self.held);
        for domain in domains {
            // Behind Cloudflare's proxy under Full (strict), the origin
            // certificate must answer before the check can pass.
            let usable = domain.active
                || domain.kind == crate::domains::DomainKind::CloudflareProxy
                || domain
                    .deactivated_at
                    .is_some_and(|at| now - at < DEACTIVATED_GRACE_MS);
            let key = held
                .get(&domain.id)
                .filter(|held| usable && !held.is_test && held.not_after > now)
                .map(|held| Arc::clone(&held.key));
            self.store.set(&domain.hostname, key);
        }
    }

    /// Stops serving a deleted domain's certificate.
    pub fn forget(&self, domain: &Domain) {
        lock(&self.held).remove(&domain.id);
        lock(&self.progress).remove(&domain.id);
        self.store.set(&domain.hostname, None);
    }

    /// Clients may check the domain by its certificate authority.
    pub fn trusted(&self, domain_id: i64, now: i64) -> bool {
        lock(&self.held)
            .get(&domain_id)
            .is_some_and(|held| !held.is_test && held.not_after > now)
    }

    pub fn status(&self, domain: &Domain, now: i64) -> proto::CertificateStatus {
        // Behind Cloudflare's proxy, an uploaded origin certificate shows
        // like any other; without one, Cloudflare's own is what apps see.
        let origin = domain.kind == crate::domains::DomainKind::CloudflareProxy
            && lock(&self.held).contains_key(&domain.id);
        if domain.kind.behind_proxy() && !origin {
            return proto::CertificateStatus {
                state: proto::CertificateState::Provided as i32,
                issuer: match domain.kind {
                    crate::domains::DomainKind::Tunnel
                    | crate::domains::DomainKind::CloudflareProxy => "Cloudflare".to_owned(),
                    _ => "the proxy".to_owned(),
                },
                ..Default::default()
            };
        }
        let held = lock(&self.held).get(&domain.id).cloned();
        let progress = lock(&self.progress)
            .get(&domain.id)
            .cloned()
            .unwrap_or_default();
        let step = progress
            .step
            .map_or(proto::CertificateStep::Unspecified, |step| match step {
                Step::Account => proto::CertificateStep::Account,
                Step::Order => proto::CertificateStep::Order,
                Step::Challenge => proto::CertificateStep::Challenge,
                Step::Validation => proto::CertificateStep::Validation,
                Step::Issuing => proto::CertificateStep::Issuing,
            });
        let state = match (&held, progress.step.is_some(), &progress.error) {
            (_, true, _) => proto::CertificateState::Requesting,
            (Some(held), _, _) if held.not_after <= now => proto::CertificateState::Expired,
            (Some(_), _, Some(_)) => proto::CertificateState::RenewalFailed,
            (Some(_), _, None) => proto::CertificateState::Valid,
            (None, _, Some(_)) => proto::CertificateState::Failed,
            (None, _, None) => proto::CertificateState::None,
        };
        proto::CertificateStatus {
            state: state as i32,
            issuer: held
                .as_ref()
                .map(|held| held.issuer.clone())
                .unwrap_or_default(),
            not_after_ms: held.as_ref().map_or(0, |held| held.not_after),
            test: held.as_ref().is_some_and(|held| held.is_test),
            uploaded: held.as_ref().is_some_and(|held| held.uploaded),
            step: step as i32,
            error: progress.error.unwrap_or_default(),
            retry_at_ms: self.retry_at(domain.id, now).unwrap_or(0),
        }
    }

    /// When a failing domain may be tried again, while it must wait.
    fn retry_at(&self, domain_id: i64, now: i64) -> Option<i64> {
        let failures = lock(&self.failures);
        let recent: Vec<i64> = failures
            .get(&domain_id)
            .map(|times| {
                times
                    .iter()
                    .copied()
                    .filter(|at| now - at < HOUR_MS)
                    .collect()
            })
            .unwrap_or_default();
        let limited = lock(&self.progress)
            .get(&domain_id)
            .and_then(|progress| progress.retry_at)
            .filter(|at| *at > now);
        let too_many = (recent.len() >= MAX_FAILURES)
            .then(|| recent.iter().min().map(|first| first + HOUR_MS))
            .flatten();
        limited.max(too_many)
    }

    pub fn providers(&self) -> Vec<proto::DnsProviderInfo> {
        lock(&self.providers)
            .iter()
            .map(|provider| proto::DnsProviderInfo {
                kind: provider.kind.as_str().to_owned(),
                masked: mask(&provider.token),
            })
            .collect()
    }

    /// Connects a DNS provider, after the token checks out, or disconnects
    /// it (an empty token).
    pub async fn set_provider(
        &self,
        state: &AppState,
        kind: ProviderKind,
        token: &str,
    ) -> Result<(), ApiError> {
        let token = token.trim();
        let mut conn = state.db.acquire().await?;
        if token.is_empty() {
            table::delete_provider(&mut conn, kind.as_str()).await?;
            lock(&self.providers).retain(|provider| provider.kind != kind);
            return Ok(());
        }
        let provider = Provider {
            kind,
            token: token.to_owned(),
        };
        provider
            .verify(state.network.http(), &endpoints(state))
            .await
            .map_err(ApiError::invalid_argument)?;
        table::put_provider(
            &mut conn,
            &ProviderRow {
                id: state.ids.next_id(),
                kind: kind.as_str().to_owned(),
                token_enc: self.secrets.seal(&token_purpose(kind), token.as_bytes()),
                created_at: now_ms(),
            },
        )
        .await?;
        let mut providers = lock(&self.providers);
        providers.retain(|known| known.kind != kind);
        providers.push(provider);
        Ok(())
    }

    /// The ACME account for the CA in use, as the page shows it.
    pub fn account_info(&self, state: &AppState) -> proto::AcmeAccount {
        let accounts = lock(&self.accounts);
        let account = accounts.get(&directory(state));
        proto::AcmeAccount {
            exists: account.is_some(),
            email: account.cloned().flatten().unwrap_or_default(),
            terms_url: TERMS_URL.to_owned(),
        }
    }

    /// Starts getting a certificate for an own domain or free subdomain
    /// that DNS shows pointing here.
    pub async fn request(
        &self,
        state: &Arc<AppState>,
        domain_id: i64,
        email: Option<String>,
        agreed: bool,
    ) -> Result<(), ApiError> {
        let domain = state
            .domains
            .get(domain_id)
            .ok_or_else(|| ApiError::not_found("domain"))?;
        if domain.kind == crate::domains::DomainKind::CloudflareProxy {
            return Err(ApiError::invalid_argument(
                "behind Cloudflare's proxy, upload Cloudflare's origin certificate instead \
                 (SSL/TLS, then Origin Server, in Cloudflare's dashboard)",
            ));
        }
        if domain.kind.behind_proxy() {
            return Err(ApiError::invalid_argument(
                "the tunnel or proxy provides this domain's certificate",
            ));
        }
        if domain.status != Status::Verified {
            return Err(ApiError::new(
                proto::ErrorCode::DomainNotVerified,
                "the domain must point to this server first: check it",
            ));
        }
        let now = now_ms();
        if let Some(at) = self.retry_at(domain_id, now) {
            return Err(ApiError::new(
                proto::ErrorCode::AcmeRateLimited,
                "too many failed tries: wait before trying again",
            )
            .retry_after(Duration::from_millis(u64::try_from(at - now).unwrap_or(0))));
        }
        let email = email
            .map(|email| email.trim().to_owned())
            .filter(|email| !email.is_empty());
        if let Some(email) = &email
            && (!email.contains('@') || email.len() > 254 || email.contains(char::is_whitespace))
        {
            return Err(ApiError::invalid_argument(format!(
                "{email} is not an email address"
            )));
        }
        let has_account = lock(&self.accounts).contains_key(&directory(state));
        if !has_account && !agreed {
            return Err(ApiError::new(
                proto::ErrorCode::AcmeFailed,
                "agree to Let's Encrypt's subscriber agreement first",
            ));
        }
        {
            let mut progress = lock(&self.progress);
            let entry = progress.entry(domain_id).or_default();
            if entry.step.is_some() {
                return Err(ApiError::conflict(
                    "a certificate is already being requested",
                ));
            }
            entry.step = Some(Step::Account);
            entry.error = None;
        }
        state.network.changed(state);
        let state = Arc::clone(state);
        tokio::spawn(async move {
            obtain(&state, &domain, email, agreed).await;
        });
        Ok(())
    }

    /// The owner's own certificate for a domain (plan §7.4).
    pub async fn upload(
        &self,
        state: &AppState,
        domain_id: i64,
        chain_pem: &str,
        key_pem: &str,
    ) -> Result<(), ApiError> {
        let domain = state
            .domains
            .get(domain_id)
            .ok_or_else(|| ApiError::not_found("domain"))?;
        if !domain.kind.takes_own_certificate() {
            return Err(ApiError::invalid_argument(
                "the tunnel or proxy provides this domain's certificate",
            ));
        }
        let now = now_ms();
        let (_, _, leaf) = pem::check_upload(&domain.hostname, chain_pem, key_pem, now)
            .map_err(|error| ApiError::invalid_argument(error.to_string()))?;
        self.keep(state, &domain, chain_pem, key_pem, &leaf, false, true)
            .await?;
        lock(&self.progress).remove(&domain_id);
        tracing::info!(hostname = %domain.hostname, "a certificate was uploaded");
        Ok(())
    }

    /// Saves and serves a certificate.
    #[allow(clippy::too_many_arguments)]
    async fn keep(
        &self,
        state: &AppState,
        domain: &Domain,
        chain_pem: &str,
        key_pem: &str,
        leaf: &pem::Leaf,
        is_test: bool,
        uploaded: bool,
    ) -> Result<(), ApiError> {
        let row = CertificateRow {
            domain_id: domain.id,
            chain_pem: chain_pem.to_owned(),
            key_enc: self
                .secrets
                .seal(&key_purpose(domain.id), key_pem.as_bytes()),
            issuer: leaf.issuer.clone(),
            not_before: leaf.not_before_ms,
            not_after: leaf.not_after_ms,
            is_test,
            uploaded,
            created_at: now_ms(),
        };
        let held = self.held_from(&row).map_err(|error| {
            tracing::warn!(%error, "a new certificate could not be used");
            ApiError::internal()
        })?;
        table::put(&mut *state.db.acquire().await?, &row).await?;
        lock(&self.held).insert(domain.id, held);
        self.serve(&state.domains.all(), now_ms());
        state.domains.announce(state);
        state.network.changed(state);
        Ok(())
    }

    fn failed(&self, domain_id: i64, failure: &acme::Failure, now: i64) {
        lock(&self.failures).entry(domain_id).or_default().push(now);
        let mut progress = lock(&self.progress);
        let entry = progress.entry(domain_id).or_default();
        entry.step = None;
        entry.error = Some(failure.message.clone());
        if failure.rate_limited {
            entry.retry_at = Some(now + HOUR_MS);
        }
    }
}

/// Runs one request, start to finish, and keeps what came of it.
async fn obtain(state: &Arc<AppState>, domain: &Domain, email: Option<String>, agreed: bool) {
    let certificates = &state.certificates;
    let result = issue(state, domain, email.clone(), agreed).await;
    let now = now_ms();
    match result {
        Ok((issued, is_test)) => {
            {
                let mut progress = lock(&certificates.progress);
                let entry = progress.entry(domain.id).or_default();
                entry.step = None;
                entry.error = None;
                entry.renew_after = None;
            }
            if let Some(credentials) = issued.new_account {
                save_account(state, &credentials, email).await;
            } else if let Some(email) = email
                && let Ok(mut conn) = state.db.acquire().await
                && let Ok(Some(row)) = table::account(&mut conn, &directory(state)).await
            {
                lock(&certificates.accounts).insert(row.directory.clone(), Some(email.clone()));
                let _ = table::put_account(
                    &mut conn,
                    &AccountRow {
                        email: Some(email),
                        ..row
                    },
                )
                .await;
            }
            let kept = certificates
                .keep(
                    state,
                    domain,
                    &issued.chain_pem,
                    &issued.key_pem,
                    &issued.leaf,
                    is_test,
                    false,
                )
                .await;
            match kept {
                Ok(()) => tracing::info!(
                    hostname = %domain.hostname,
                    method = ?issued.method,
                    test = is_test,
                    "a certificate was issued"
                ),
                Err(_) => certificates.failed(
                    domain.id,
                    &acme::Failure {
                        message: "The certificate could not be saved.".to_owned(),
                        rate_limited: false,
                        needs_agreement: false,
                    },
                    now,
                ),
            }
        }
        Err(failure) => {
            tracing::info!(hostname = %domain.hostname, reason = %failure.message, "a certificate request failed");
            certificates.failed(domain.id, &failure, now);
        }
    }
    state.network.changed(state);
}

/// The ACME exchange, with what the server knows.
async fn issue(
    state: &Arc<AppState>,
    domain: &Domain,
    email: Option<String>,
    agreed: bool,
) -> Result<(acme::Issued, bool), acme::Failure> {
    let certificates = &state.certificates;
    let config = &state.config.acme;
    let is_test = state.network.settings().acme_staging;
    let directory = directory(state);
    let account = match state.db.acquire().await {
        Ok(mut conn) => table::account(&mut conn, &directory)
            .await
            .ok()
            .flatten()
            .and_then(|row| {
                let json = certificates
                    .secrets
                    .open(ACCOUNT, &row.credentials_enc)
                    .ok()?;
                serde_json::from_slice::<AccountCredentials>(&json).ok()
            }),
        Err(_) => None,
    };
    let providers = lock(&certificates.providers).clone();
    let endpoints = endpoints(state);
    let http = state.network.http().clone();
    let id = domain.id;
    let step = |step: Step| {
        lock(&certificates.progress).entry(id).or_default().step = Some(step);
        state.network.changed(state);
    };
    let issued = acme::issue(
        acme::Request {
            host: &domain.hostname,
            directory: &directory,
            root: config.root.as_ref(),
            account,
            email: email.as_deref(),
            agreed,
            providers: &providers,
            endpoints: &endpoints,
            http: &http,
            http_port: config.http_port,
            tls_alpn_port: config.tls_alpn_port,
            main_port: state.network.main_port(),
            store: &certificates.store,
            http01: &certificates.http01,
            dns: Some(state.network.dns_checker()),
            dns_wait: Duration::from_secs(u64::from(config.dns_propagation_s)),
        },
        step,
    )
    .await?;
    Ok((issued, is_test))
}

async fn save_account(state: &AppState, credentials: &AccountCredentials, email: Option<String>) {
    let Ok(json) = serde_json::to_vec(credentials) else {
        return;
    };
    let row = AccountRow {
        directory: directory(state),
        credentials_enc: state.certificates.secrets.seal(ACCOUNT, &json),
        email: email.clone(),
        created_at: now_ms(),
    };
    lock(&state.certificates.accounts).insert(row.directory.clone(), email);
    if let Ok(mut conn) = state.db.acquire().await
        && let Err(error) = table::put_account(&mut conn, &row).await
    {
        tracing::warn!(%error, "the ACME account could not be saved");
    }
}

/// The CA in use: Let's Encrypt, or its test environment.
fn directory(state: &AppState) -> String {
    if state.network.settings().acme_staging {
        state.config.acme.test_directory.clone()
    } else {
        state.config.acme.directory.clone()
    }
}

pub(crate) fn endpoints(state: &AppState) -> Endpoints {
    let config = &state.config.acme;
    Endpoints {
        cloudflare: config.cloudflare_api.clone(),
        duckdns: config.duckdns_api.clone(),
        desec: config.desec_api.clone(),
    }
}

/// `••••abcd`.
fn mask(token: &str) -> String {
    let tail: String = token
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("••••{tail}")
}

/// Renews what is due, warns about what is about to expire, and stops
/// serving what a domain no longer may, every hour until the server stops.
pub fn start(state: &Arc<AppState>) {
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let mut every = tokio::time::interval(RENEW_EVERY);
        loop {
            tokio::select! {
                _ = every.tick() => look_at_renewals(&state).await,
                () = state.shutdown.cancelled() => return,
            }
        }
    });
}

/// One look at every certificate (plan §7.3).
pub async fn look_at_renewals(state: &Arc<AppState>) {
    let certificates = &state.certificates;
    let now = now_ms();
    let domains = state.domains.all();
    certificates.serve(&domains, now);
    for domain in domains {
        let Some(held) = lock(&certificates.held).get(&domain.id).cloned() else {
            continue;
        };
        let progress = lock(&certificates.progress)
            .get(&domain.id)
            .cloned()
            .unwrap_or_default();
        let left = held.not_after - now;
        if held.uploaded {
            if left < WARN_UPLOADED_MS && progress.warned_at.is_none_or(|at| now - at > DAY_MS) {
                warn(
                    state,
                    &domain,
                    &held,
                    "Upload a new one: it is not renewed here.",
                );
                lock(&certificates.progress)
                    .entry(domain.id)
                    .or_default()
                    .warned_at = Some(now);
            }
            continue;
        }
        let lifetime = held.not_after - held.not_before;
        let due = now >= held.not_before + lifetime * 2 / 3 || ari_due(state, &held).await;
        let waiting = progress.renew_after.is_some_and(|at| now < at)
            || progress.step.is_some()
            || domain.status != Status::Verified;
        if due && !waiting {
            renew(state, &domain).await;
        }
        let failing = lock(&certificates.progress)
            .get(&domain.id)
            .is_some_and(|progress| progress.error.is_some());
        if failing && left < WARN_ACME_MS && progress.warned_at.is_none_or(|at| now - at > DAY_MS) {
            warn(
                state,
                &domain,
                &held,
                "Renewing it keeps failing: see the domain's row.",
            );
            lock(&certificates.progress)
                .entry(domain.id)
                .or_default()
                .warned_at = Some(now);
        }
    }
}

/// Whether Let's Encrypt's renewal information says to renew now.
async fn ari_due(state: &AppState, held: &Held) -> bool {
    let Ok(chain) = pem::chain(&held.chain_pem) else {
        return false;
    };
    let Ok(identifier) = instant_acme::CertificateIdentifier::try_from(&chain[0]) else {
        return false;
    };
    let credentials = match state.db.acquire().await {
        Ok(mut conn) => table::account(&mut conn, &directory(state))
            .await
            .ok()
            .flatten()
            .and_then(|row| {
                state
                    .certificates
                    .secrets
                    .open(ACCOUNT, &row.credentials_enc)
                    .ok()
            })
            .and_then(|json| serde_json::from_slice::<AccountCredentials>(&json).ok()),
        Err(_) => None,
    };
    let Some(credentials) = credentials else {
        return false;
    };
    let builder = match &state.config.acme.root {
        Some(root) => instant_acme::Account::builder_with_root(root),
        None => instant_acme::Account::builder(),
    };
    let Ok(builder) = builder else {
        return false;
    };
    let Ok(account) = builder.from_credentials(credentials).await else {
        return false;
    };
    match account.renewal_info(&identifier).await {
        Ok((info, _)) => {
            let start = info.suggested_window.start.unix_timestamp() * 1000;
            now_ms() >= start
        }
        Err(_) => false,
    }
}

/// Renews every Let's Encrypt certificate now, due or not.
pub async fn renew_all_now(state: &Arc<AppState>) {
    let ids: Vec<i64> = lock(&state.certificates.held)
        .iter()
        .filter(|(_, held)| !held.uploaded)
        .map(|(id, _)| *id)
        .collect();
    for id in ids {
        if let Some(domain) = state.domains.get(id) {
            renew(state, &domain).await;
        }
    }
}

/// Renews in place: the old certificate is served until the new one is in.
async fn renew(state: &Arc<AppState>, domain: &Domain) {
    let certificates = &state.certificates;
    lock(&certificates.progress)
        .entry(domain.id)
        .or_default()
        .step = Some(Step::Account);
    state.network.changed(state);
    obtain(state, domain, None, false).await;
    let failed = lock(&certificates.progress)
        .get(&domain.id)
        .is_some_and(|progress| progress.error.is_some());
    if failed {
        lock(&certificates.progress)
            .entry(domain.id)
            .or_default()
            .renew_after = Some(now_ms() + RENEW_RETRY_MS);
    }
}

fn warn(state: &AppState, domain: &Domain, held: &Held, what: &str) {
    let Some(owner) = state.guild().meta.owner_id else {
        return;
    };
    let days = (held.not_after - now_ms()).max(0) / DAY_MS;
    state.broadcast(
        Event::ServerNotice(proto::ServerNotice {
            key: format!("certificate.{}", domain.id),
            severity: proto::NoticeSeverity::Warning as i32,
            title: format!("{}'s certificate expires soon", domain.hostname),
            body: format!("It expires in {days} days. {what}"),
        }),
        Audience::User(owner),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_show_only_their_end() {
        assert_eq!(mask("abcdefgh1234"), "••••1234");
        assert_eq!(mask("ab"), "••••ab");
    }
}
