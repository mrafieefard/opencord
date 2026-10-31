//! Cloudflare Tunnel (Phase 3 plan §9.2): `cloudflared` run and watched by
//! the server, forwarding to the local listener. A quick tunnel gets a
//! random `*.trycloudflare.com` name, which becomes a tunnel domain; a
//! named tunnel runs with the owner's token, for the hostname they set up
//! in Cloudflare. cloudflared is started again when it stops, after a pause
//! that grows; its output is kept for the owner, without the token.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use opencord_proto::v1 as proto;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::db::meta;
use crate::domains::DomainKind;
use crate::error::ApiError;
use crate::state::AppState;

pub mod install;

/// Lines of cloudflared's output kept for the owner.
const LOG_LINES: usize = 200;
const FIRST_PAUSE: Duration = Duration::from_secs(1);
const LONGEST_PAUSE: Duration = Duration::from_secs(60);
/// A run this long resets the pause before the next start.
const STEADY: Duration = Duration::from_secs(60);
const QUICK_SUFFIX: &str = ".trycloudflare.com";

const META_KIND: &str = "tunnel.kind";
const META_TOKEN: &str = "tunnel.token";
const META_HOSTNAME: &str = "tunnel.hostname";
const TOKEN_PURPOSE: &str = "tunnel.token";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Quick,
    Named { token: String },
}

impl Kind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Named { .. } => "named",
        }
    }
}

#[derive(Debug, Default)]
struct Inner {
    kind: Option<Kind>,
    state: Option<proto::TunnelState>,
    hostname: String,
    domain_id: i64,
    error: Option<String>,
    restarts: u32,
    run: Option<CancellationToken>,
    version: Option<String>,
    installing: bool,
}

#[derive(Debug, Default)]
pub struct Tunnel {
    inner: Mutex<Inner>,
    log: Mutex<VecDeque<String>>,
}

impl Tunnel {
    pub fn status(&self) -> proto::TunnelStatus {
        let inner = lock(&self.inner);
        proto::TunnelStatus {
            state: inner.state.unwrap_or(proto::TunnelState::Off) as i32,
            kind: match inner.kind {
                Some(Kind::Quick) => proto::TunnelKind::Quick,
                Some(Kind::Named { .. }) => proto::TunnelKind::Named,
                None => proto::TunnelKind::Unspecified,
            } as i32,
            hostname: inner.hostname.clone(),
            domain_id: inner.domain_id,
            error: inner.error.clone().unwrap_or_default(),
            restarts: inner.restarts,
            cloudflared_version: inner.version.clone().unwrap_or_default(),
            installing: inner.installing,
        }
    }

    /// Whether a tunnel is meant to run: the local listener stays on.
    pub fn wanted(&self) -> bool {
        lock(&self.inner).kind.is_some()
    }

    pub fn log(&self) -> Vec<String> {
        lock(&self.log).iter().cloned().collect()
    }

    fn push_log(&self, line: String) {
        let mut log = lock(&self.log);
        if log.len() == LOG_LINES {
            log.pop_front();
        }
        log.push_back(line);
    }

    fn set_state(&self, state: proto::TunnelState, error: Option<String>) {
        let mut inner = lock(&self.inner);
        inner.state = Some(state);
        if error.is_some() || state == proto::TunnelState::Running {
            inner.error = error;
        }
    }
}

/// The cloudflared to run: the configured one, the one downloaded into the
/// data directory, or the one on the PATH.
pub fn cloudflared(state: &AppState) -> Option<PathBuf> {
    if let Some(path) = &state.config.tunnel.cloudflared {
        return path.is_file().then(|| path.clone());
    }
    let downloaded = install::target(&state.config.server.data_dir);
    if downloaded.is_file() {
        return Some(downloaded);
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("cloudflared"))
            .find(|path| path.is_file())
    })
}

/// `2026.10.0`, from `cloudflared --version`.
async fn version_of(path: &Path) -> Option<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(path).arg("--version").output(),
    )
    .await
    .ok()?
    .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace()
        .skip_while(|word| *word != "version")
        .nth(1)
        .map(str::to_owned)
}

/// Looks for cloudflared again, after it may have been installed.
pub async fn detect(state: &AppState) {
    let version = match cloudflared(state) {
        Some(path) => version_of(&path).await,
        None => None,
    };
    lock(&state.tunnel.inner).version = version;
}

/// Starts the tunnel saved before the server stopped, if one was running.
pub async fn restore(state: &Arc<AppState>) {
    detect(state).await;
    let Ok(mut conn) = state.db.acquire().await else {
        return;
    };
    let Ok(saved) = meta::load_prefixed(&mut conn, "tunnel.").await else {
        return;
    };
    drop(conn);
    let kind = match saved.get(META_KIND).map(String::as_str) {
        Some("quick") => Kind::Quick,
        Some("named") => {
            let token = saved
                .get(META_TOKEN)
                .and_then(|sealed| hex::decode(sealed).ok())
                .and_then(|sealed| {
                    state
                        .certificates
                        .secrets()
                        .open_string(TOKEN_PURPOSE, &sealed)
                        .ok()
                });
            let Some(token) = token else {
                tracing::warn!("the saved tunnel token could not be opened");
                return;
            };
            Kind::Named { token }
        }
        _ => return,
    };
    let hostname = saved.get(META_HOSTNAME).cloned().unwrap_or_default();
    if let Err(error) = run(state, kind, hostname).await {
        tracing::warn!(error = %error.message, "the tunnel could not start again");
    }
}

/// Starts a tunnel, replacing the one running (plan §9.2).
pub async fn start(
    state: &Arc<AppState>,
    kind: proto::TunnelKind,
    token: &str,
    hostname: &str,
) -> Result<(), ApiError> {
    let kind = match kind {
        proto::TunnelKind::Quick => Kind::Quick,
        proto::TunnelKind::Named => {
            let token = token.trim();
            let token = if token.is_empty() {
                match &lock(&state.tunnel.inner).kind {
                    Some(Kind::Named { token }) => token.clone(),
                    _ => return Err(ApiError::invalid_argument("paste the tunnel token")),
                }
            } else {
                token.to_owned()
            };
            if token.len() < 32 || token.contains(char::is_whitespace) {
                return Err(ApiError::invalid_argument(
                    "that is not a tunnel token: copy the long one from the tunnel's install \
                     command in Cloudflare's dashboard",
                ));
            }
            Kind::Named { token }
        }
        proto::TunnelKind::Unspecified => {
            return Err(ApiError::invalid_argument(
                "choose a quick or a named tunnel",
            ));
        }
    };
    let hostname = match kind {
        Kind::Quick => String::new(),
        Kind::Named { .. } => crate::domains::hostname::normalize(hostname)
            .map_err(|error| ApiError::new(proto::ErrorCode::DomainInvalid, error.to_string()))?,
    };
    if cloudflared(state).is_none() {
        return Err(ApiError::invalid_argument(
            "cloudflared is not installed: install it, or let Opencord download it",
        ));
    }
    stop_running(state);
    save(state, Some(&kind), &hostname).await?;
    run(state, kind, hostname).await
}

/// Stops the tunnel. A quick tunnel's domain goes with it; a named one's
/// stays, for the owner to delete or to run cloudflared elsewhere.
pub async fn stop(state: &Arc<AppState>) -> Result<(), ApiError> {
    let quick_domain = {
        let inner = lock(&state.tunnel.inner);
        (inner.kind == Some(Kind::Quick) && inner.domain_id != 0).then_some(inner.domain_id)
    };
    stop_running(state);
    {
        let mut inner = lock(&state.tunnel.inner);
        let version = inner.version.take();
        *inner = Inner {
            version,
            ..Inner::default()
        };
    }
    save(state, None, "").await?;
    if let Some(id) = quick_domain {
        state.domains.remove_own(state, id).await?;
    }
    state.network.changed(state);
    tracing::info!("the tunnel was stopped");
    Ok(())
}

fn stop_running(state: &AppState) {
    if let Some(run) = lock(&state.tunnel.inner).run.take() {
        run.cancel();
    }
}

async fn save(state: &AppState, kind: Option<&Kind>, hostname: &str) -> Result<(), ApiError> {
    let token = match kind {
        Some(Kind::Named { token }) => Some(hex::encode(
            state
                .certificates
                .secrets()
                .seal(TOKEN_PURPOSE, token.as_bytes()),
        )),
        _ => None,
    };
    let mut conn = state.db.acquire().await?;
    meta::put(&mut conn, META_KIND, kind.map(Kind::as_str)).await?;
    meta::put(&mut conn, META_TOKEN, token.as_deref()).await?;
    meta::put(
        &mut conn,
        META_HOSTNAME,
        Some(hostname).filter(|host| !host.is_empty()),
    )
    .await?;
    Ok(())
}

/// Sets the tunnel up and starts watching cloudflared.
async fn run(state: &Arc<AppState>, kind: Kind, hostname: String) -> Result<(), ApiError> {
    let path = cloudflared(state).ok_or_else(|| {
        ApiError::invalid_argument(
            "cloudflared is not installed: install it, or let Opencord download it",
        )
    })?;
    let domain_id = match kind {
        Kind::Quick => 0,
        Kind::Named { .. } => named_domain(state, &hostname).await?,
    };
    let cancel = state.shutdown.child_token();
    {
        let mut inner = lock(&state.tunnel.inner);
        inner.kind = Some(kind.clone());
        inner.state = Some(proto::TunnelState::Starting);
        inner.hostname = hostname;
        inner.domain_id = domain_id;
        inner.error = None;
        inner.restarts = 0;
        inner.run = Some(cancel.clone());
    }
    lock(&state.tunnel.log).clear();
    // cloudflared sends plain HTTP to the main port, from this machine.
    let port = state.network.main_port();
    state.network.changed(state);
    let state = Arc::clone(state);
    tokio::spawn(async move {
        supervise(&state, &path, &kind, port, &cancel).await;
    });
    Ok(())
}

/// The named tunnel's domain, added when it is not yet.
async fn named_domain(state: &Arc<AppState>, hostname: &str) -> Result<i64, ApiError> {
    if let Some(domain) = state
        .domains
        .all()
        .into_iter()
        .find(|domain| domain.hostname == hostname)
    {
        if domain.kind != DomainKind::Tunnel {
            return Err(ApiError::new(
                proto::ErrorCode::DomainExists,
                format!("{hostname} is already added as another kind of domain"),
            ));
        }
        return Ok(domain.id);
    }
    let domain = state
        .domains
        .add(state, hostname, DomainKind::Tunnel, None)
        .await?;
    Ok(domain.id)
}

/// Runs cloudflared until the tunnel stops, starting it again when it
/// ends by itself.
async fn supervise(
    state: &Arc<AppState>,
    path: &Path,
    kind: &Kind,
    port: u16,
    cancel: &CancellationToken,
) {
    let tunnel = &state.tunnel;
    let mut pause = FIRST_PAUSE;
    loop {
        let started = tokio::time::Instant::now();
        let ended = run_once(state, path, kind, port, cancel).await;
        if cancel.is_cancelled() {
            return;
        }
        if started.elapsed() >= STEADY {
            pause = FIRST_PAUSE;
        }
        let error = match ended {
            Ok(why) => why,
            Err(error) => {
                tunnel.set_state(
                    proto::TunnelState::Failed,
                    Some(format!("cloudflared could not start: {error}")),
                );
                state.network.changed(state);
                return;
            }
        };
        {
            let mut inner = lock(&tunnel.inner);
            inner.restarts += 1;
            inner.state = Some(proto::TunnelState::Restarting);
            inner.error = Some(error);
        }
        tracing::info!(
            pause_s = pause.as_secs(),
            "cloudflared stopped; starting it again"
        );
        state.network.changed(state);
        tokio::select! {
            () = tokio::time::sleep(pause) => {}
            () = cancel.cancelled() => return,
        }
        pause = (pause * 2).min(LONGEST_PAUSE);
    }
}

/// One run of cloudflared: until it exits (`Ok` with why) or the tunnel
/// stops. `Err` when it cannot be started at all.
async fn run_once(
    state: &Arc<AppState>,
    path: &Path,
    kind: &Kind,
    port: u16,
    cancel: &CancellationToken,
) -> std::io::Result<String> {
    let home = state.config.server.data_dir.join("cloudflared");
    std::fs::create_dir_all(&home)?;
    let mut command = Command::new(path);
    command
        .arg("tunnel")
        .arg("--no-autoupdate")
        // Not the owner's own ~/.cloudflared configuration.
        .env("HOME", &home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let token = match kind {
        Kind::Quick => {
            command.args(["--url", &format!("http://127.0.0.1:{port}")]);
            None
        }
        Kind::Named { token } => {
            // From the environment, so it does not show in process lists.
            command.arg("run").env("TUNNEL_TOKEN", token);
            Some(token.clone())
        }
    };
    let mut child = command.spawn()?;
    let (lines_tx, mut lines) = mpsc::unbounded_channel();
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(read_lines(stdout, lines_tx.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(read_lines(stderr, lines_tx));
    }
    let mut last_error = None;
    let take = |line: String, last_error: &mut Option<String>| {
        let line = match &token {
            Some(token) => line.replace(token.as_str(), "••••"),
            None => line,
        };
        if is_error(&line) {
            *last_error = Some(plain_error(&line));
        }
        line
    };
    loop {
        tokio::select! {
            line = lines.recv() => {
                let Some(line) = line else { continue };
                let line = take(line, &mut last_error);
                on_line(state, kind, &line).await;
                state.tunnel.push_log(line);
            }
            status = child.wait() => {
                // What it said last, until its output ends, then why it
                // ended.
                let rest = async {
                    while let Some(line) = lines.recv().await {
                        let line = take(line, &mut last_error);
                        state.tunnel.push_log(line);
                    }
                };
                let _ = tokio::time::timeout(Duration::from_secs(1), rest).await;
                let status = status?;
                return Ok(last_error.unwrap_or_else(|| format!("cloudflared stopped ({status})")));
            }
            () = cancel.cancelled() => {
                let _ = child.kill().await;
                return Ok(String::new());
            }
        }
    }
}

async fn read_lines(stream: impl AsyncRead + Unpin, lines: mpsc::UnboundedSender<String>) {
    let mut reader = BufReader::new(stream).lines();
    while let Ok(Some(line)) = reader.next_line().await {
        if lines.send(line).is_err() {
            return;
        }
    }
}

fn is_error(line: &str) -> bool {
    line.contains(" ERR ") || line.starts_with("ERR ")
}

/// `2026-... ERR Failed to ... error="..."` without the time.
fn plain_error(line: &str) -> String {
    line.split_once("ERR ")
        .map_or(line, |(_, rest)| rest)
        .trim()
        .to_owned()
}

/// The quick tunnel's address, from cloudflared's banner.
fn quick_address(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start + "https://".len()..];
    let end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '.'))
        .unwrap_or(rest.len());
    let host = &rest[..end];
    (host.ends_with(QUICK_SUFFIX) && host != &QUICK_SUFFIX[1..]).then(|| host.to_owned())
}

async fn on_line(state: &Arc<AppState>, kind: &Kind, line: &str) {
    if *kind == Kind::Quick
        && let Some(host) = quick_address(line)
    {
        quick_domain(state, &host).await;
    }
    if line.contains("Registered tunnel connection") {
        let first = lock(&state.tunnel.inner).state != Some(proto::TunnelState::Running);
        state.tunnel.set_state(proto::TunnelState::Running, None);
        state.network.changed(state);
        if first {
            tracing::info!("the tunnel is connected");
            verify(state);
        }
    }
}

/// The quick tunnel's new address becomes its domain, in place of the
/// last one: each start gets a new address.
async fn quick_domain(state: &Arc<AppState>, host: &str) {
    let old: Vec<i64> = state
        .domains
        .all()
        .into_iter()
        .filter(|domain| {
            domain.kind == DomainKind::Tunnel
                && domain.hostname.ends_with(QUICK_SUFFIX)
                && domain.hostname != host
        })
        .map(|domain| domain.id)
        .collect();
    for id in old {
        if let Err(error) = state.domains.remove_own(state, id).await {
            tracing::warn!(error = %error.message, "the last quick tunnel's domain could not be removed");
        }
    }
    let id = match state.domains.all().into_iter().find(|d| d.hostname == host) {
        Some(domain) => domain.id,
        None => match state
            .domains
            .add(state, host, DomainKind::Tunnel, None)
            .await
        {
            Ok(domain) => domain.id,
            Err(error) => {
                tracing::warn!(error = %error.message, "the quick tunnel's domain could not be added");
                return;
            }
        },
    };
    {
        let mut inner = lock(&state.tunnel.inner);
        inner.hostname = host.to_owned();
        inner.domain_id = id;
    }
    tracing::info!(hostname = host, "the quick tunnel has its address");
    state.network.changed(state);
    // Connected before the address showed: check it now.
    if lock(&state.tunnel.inner).state == Some(proto::TunnelState::Running) {
        verify(state);
    }
}

/// Checks the tunnel's domain until it answers through Cloudflare, which
/// takes a while for a new name.
fn verify(state: &Arc<AppState>) {
    let id = lock(&state.tunnel.inner).domain_id;
    if id != 0 {
        crate::domains::check_until_verified(state, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_quick_address_is_read_from_the_banner() {
        assert_eq!(
            quick_address(
                "2026-10-15T10:00:00Z INF |  https://shy-words-here.trycloudflare.com  |"
            ),
            Some("shy-words-here.trycloudflare.com".to_owned())
        );
        assert_eq!(
            quick_address("INF Requesting new quick Tunnel on trycloudflare.com..."),
            None
        );
        assert_eq!(
            quick_address("see https://developers.cloudflare.com/x"),
            None
        );
    }

    #[test]
    fn errors_lose_their_time() {
        assert_eq!(
            plain_error("2026-10-15T10:00:00Z ERR Failed to dial a quic connection"),
            "Failed to dial a quic connection"
        );
    }
}
