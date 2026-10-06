//! Starting and stopping the server.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use opencord_common::address::{Fingerprint, InviteLink, ServerAddress};
use opencord_common::snowflake::SnowflakeGenerator;
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::gateway::session::CloseCode;
use crate::guild::Guild;
use crate::state::{AppState, now_ms};
use crate::{bootstrap, db, http, tls};

const REAP_INTERVAL: Duration = Duration::from_secs(5);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// A running server.
#[derive(Debug)]
pub struct ServerHandle {
    pub local_addr: SocketAddr,
    pub fingerprint: Fingerprint,
    /// Set while the server has no owner.
    pub claim_token: Option<String>,
    /// Permanent invite to share, once the server has an owner.
    pub invite_link: Option<InviteLink>,
    state: Arc<AppState>,
    handle: Handle<SocketAddr>,
    serve: JoinHandle<std::io::Result<()>>,
    reaper: JoinHandle<()>,
}

/// Opens the data directory (creating everything on first start) and starts
/// listening.
pub async fn start(config: Config) -> anyhow::Result<ServerHandle> {
    let data_dir = config.server.data_dir.clone();
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("could not create {}", data_dir.display()))?;
    let pool = db::connect(&data_dir.join(db::DATABASE_FILE))
        .await
        .context("could not open the database")?;
    let ids = SnowflakeGenerator::new(0)?;
    let bootstrap = bootstrap::initialize(&pool, &config.server.name, &ids, now_ms()).await?;
    let tls = tls::load_or_generate(&config.tls, &data_dir.join("tls"), &hostnames(&config))?;
    let guild = Guild::load(&mut *pool.acquire().await?, bootstrap.meta).await?;
    let invite_code = bootstrap::startup_invite(&pool, now_ms()).await?;
    let rustls = RustlsConfig::from_config(Arc::new(tls::server_config(&tls)?));
    let state = Arc::new(AppState::new(
        config.clone(),
        pool,
        ids,
        tls.fingerprint,
        guild,
    ));

    let handle = Handle::new();
    let app = http::router(Arc::clone(&state)).into_make_service_with_connect_info::<SocketAddr>();
    let serve = tokio::spawn(
        axum_server::bind_rustls(config.server.bind, rustls)
            .handle(handle.clone())
            .serve(app),
    );
    let local_addr = handle
        .listening()
        .await
        .with_context(|| format!("could not listen on {}", config.server.bind))?;
    let reaper = tokio::spawn(reap_sessions(Arc::clone(&state)));

    let public_port = match config.server.bind.port() {
        0 => local_addr.port(),
        _ => config.public_port(),
    };
    let invite_link = invite_code.map(|code| InviteLink {
        address: ServerAddress {
            host: config.server.public_host.to_ascii_lowercase(),
            port: public_port,
        },
        code,
        fingerprint: Some(tls.fingerprint),
    });
    Ok(ServerHandle {
        local_addr,
        fingerprint: tls.fingerprint,
        claim_token: bootstrap.claim_token,
        invite_link,
        state,
        handle,
        serve,
        reaper,
    })
}

impl ServerHandle {
    /// Closes every connection (1001), stops listening and closes the
    /// database.
    pub async fn shutdown(self) {
        self.state.shutdown.cancel();
        self.handle.graceful_shutdown(Some(SHUTDOWN_GRACE));
        let _ = self.serve.await;
        self.reaper.abort();
        self.state.db.close().await;
    }

    /// Drops every connection without ending the sessions, as a network
    /// failure would. Intended for tests of resuming.
    pub fn drop_connections(&self) {
        for session in self.state.sessions.all() {
            session.drop_connection(CloseCode::UNKNOWN, Instant::now());
        }
    }
}

fn hostnames(config: &Config) -> Vec<String> {
    let mut names = vec![config.server.public_host.clone()];
    for fallback in ["localhost", "127.0.0.1", "::1"] {
        if !names.iter().any(|name| name == fallback) {
            names.push(fallback.to_owned());
        }
    }
    names
}

/// Forgets sessions that can no longer resume, and announces users going
/// offline.
async fn reap_sessions(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(REAP_INTERVAL);
    loop {
        interval.tick().await;
        for session in state.sessions.remove_expired(Instant::now()) {
            if !state.sessions.has_user(session.user_id) {
                state.presence.clear(session.user_id);
                state.broadcast_presence(session.user_id);
            }
        }
        state.rate_limits.prune();
    }
}
