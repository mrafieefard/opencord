//! Starting and stopping the server.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context as _;
use axum_server::Handle;
use axum_server::tls_rustls::{RustlsAcceptor, RustlsConfig};
use ed25519_dalek::VerifyingKey;
use opencord_common::address::{Fingerprint, InviteLink, ServerAddress};
use opencord_common::limits::SOUND_COOLDOWN_MAX_S;
use opencord_common::snowflake::SnowflakeGenerator;
use tokio::task::JoinHandle;

use crate::certs::store::{CertStore, certified_key};
use crate::certs::{self, Certificates};
use crate::config::{Config, VoiceMode};
use crate::domains::{self, Domains};
use crate::gateway::session::CloseCode;
use crate::guild::Guild;
use crate::handshakes::WatchedAcceptor;
use crate::network::settings::NetworkSettings;
use crate::network::{Listening, Network};
use crate::secrets::Secrets;
use crate::state::{AppState, now_ms};
use crate::voice::nodes::VoiceNodes;
use crate::{bootstrap, db, http, music, tls, voice};
use opencord_voice::bridge::MusicBridge;
use opencord_voice::node::{NodeConfig, NodeEvent, VoiceNode};
use tokio::sync::mpsc::UnboundedReceiver;

const REAP_INTERVAL: Duration = Duration::from_secs(5);
/// How often voice clients send a heartbeat.
pub(crate) const VOICE_HEARTBEAT: Duration = Duration::from_secs(5);
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
    /// Checks voice tokens; voice nodes get it.
    pub voice_key: VerifyingKey,
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
    let voice_key = voice::keys::load_or_generate(&data_dir)?;
    let (embedded, node_events) = embedded_node(&config, &voice_key).await?;
    let (music_bridge, bridge_events) = music::start_bridge(&config, embedded.as_ref())
        .await?
        .unzip();
    let guild = Guild::load(&mut *pool.acquire().await?, bootstrap.meta).await?;
    let invite_code = bootstrap::startup_invite(&pool, now_ms()).await?;
    let secrets =
        Secrets::load_or_generate(&config.secrets_file(), !db::holds_sealed_data(&pool).await?)?;
    let store = Arc::new(CertStore::new(certified_key(
        tls.cert_chain.clone(),
        &tls.key,
    )?));
    let certificates = Certificates::new(Arc::clone(&store), secrets);
    certificates.load(&pool).await?;
    let rustls = RustlsConfig::from_config(Arc::new(tls::server_config_with(store)?));
    let network_settings = NetworkSettings::load(&mut *pool.acquire().await?).await?;
    let domains = load_domains(&pool).await?;
    let listener = bind_main_port(&config, network_settings.main_port)?;
    let state = Arc::new(
        AppState::new(
            config.clone(),
            pool,
            ids,
            tls.fingerprint,
            guild,
            voice_key,
            VoiceNodes::new(embedded, tls.fingerprint),
        )
        .with_music_bridge(music_bridge)
        .with_domains(domains)
        .with_certificates(certificates),
    );
    state
        .attachments
        .load(
            &mut *state.db.acquire().await?,
            state.certificates.secrets(),
        )
        .await?;
    state
        .certificates
        .serve(&state.domains.all(), crate::state::now_ms());
    state.network.restore(network_settings);
    if let Some(bridge) = state.music_bridge.clone() {
        let mut public = state.network.public_ips();
        bridge.set_public_ip(public.borrow().v4.map(IpAddr::V4).or(bridge.public_ip()));
        tokio::spawn(async move {
            while public.changed().await.is_ok() {
                let v4 = public.borrow().v4;
                if let Some(v4) = v4 {
                    bridge.set_public_ip(Some(IpAddr::V4(v4)));
                }
            }
        });
    }
    if let Some(events) = bridge_events {
        music::watch_bridge(&state, events);
    }
    music::start(&state).await?;
    state.commands.load(&mut *state.db.acquire().await?).await?;
    crate::lavalink::start(&state).await?;
    crate::attachments::start(&state).await?;
    if let Some(mut events) = node_events {
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                voice::on_node_event(&state, event).await;
            }
        });
    }

    let handle = Handle::new();
    let app = http::router(Arc::clone(&state)).into_make_service_with_connect_info::<SocketAddr>();
    let bound = listener.local_addr()?;
    let acceptor = WatchedAcceptor::new(RustlsAcceptor::new(rustls), state.lavalink.refusals());
    let serve = tokio::spawn(
        axum_server::from_tcp(listener)?
            .acceptor(acceptor)
            .handle(handle.clone())
            .serve(app),
    );
    let local_addr = match handle.listening().await {
        Some(addr) => addr,
        None => {
            let res = serve.await;
            anyhow::bail!("could not listen on {bound}: serve result: {res:?}");
        }
    };
    let reaper = tokio::spawn(reap_sessions(Arc::clone(&state)));
    state.network.set_listening(
        local_addr.port(),
        Listening {
            voice: state.voice_nodes.embedded().map(VoiceNode::udp_port),
            music: state.music_bridge.as_ref().map(MusicBridge::udp_port),
        },
    );
    Network::start(&state);
    domains::start(&state);
    domains::sync_local_listener(&state).await;
    certs::start(&state);
    crate::tunnel::restore(&state).await;

    let public_port = match (config.server.bind.port(), config.server.public_port) {
        (0, _) => local_addr.port(),
        (_, Some(port)) => port,
        (_, None) => local_addr.port(),
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
        voice_key: state.voice_key.verifying_key(),
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
        if let Some(node) = self.state.voice_nodes.embedded() {
            node.shutdown();
        }
        self.state.music.save_all().await;
        if let Some(bridge) = &self.state.music_bridge {
            bridge.shutdown();
        }
        self.state.shutdown.cancel();
        self.state.network.close_local_listener().await;
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

    /// Runs the AFK check as if it were `later` from now, so a test need
    /// not wait out the timeout. Intended for tests of AFK moves.
    pub async fn check_afk_after(&self, later: Duration) {
        let _writes = self.state.write_lock().await;
        voice::afk::move_idle(&self.state, Instant::now() + later);
    }

    /// When `user_id` last joined, moved or spoke in voice, and since when
    /// their device has had no input. Intended for tests of AFK moves.
    pub fn voice_activity(&self, user_id: i64) -> Option<(Instant, Option<Instant>)> {
        self.state
            .voice()
            .activity(user_id)
            .map(|activity| (activity.active_at, activity.input_idle_since))
    }

    /// The audio bridge, when music is on. Intended for tests that stand in
    /// for the music player.
    /// The Lavalink nodes, for tests of choosing between them.
    pub fn lavalink(&self) -> &crate::lavalink::Lavalink {
        &self.state.lavalink
    }

    pub fn music_bridge(&self) -> Option<&opencord_voice::bridge::MusicBridge> {
        self.state.music_bridge.as_ref()
    }

    /// Renews every certificate now, as if it were due. Intended for tests
    /// of renewal.
    pub async fn renew_certificates_now(&self) {
        crate::certs::renew_all_now(&self.state).await;
    }

    /// Starts or stops the local listener for tunnels and proxies; returns
    /// its port, which the system picks, so tests may run at once.
    pub async fn set_local_listener_on_any_port(&self, on: bool) -> std::io::Result<Option<u16>> {
        let mut settings = self.state.network.settings();
        settings.local_listener_port = 0;
        self.state.network.restore(settings);
        self.set_local_listener(on).await
    }

    /// The local listener's port, while it runs.
    pub fn local_listener_port(&self) -> Option<u16> {
        self.state.network.local_listener_port()
    }

    /// Starts or stops the local listener for tunnels and proxies; returns
    /// its port. Intended for tests until tunnel domains turn it on.
    pub async fn set_local_listener(&self, on: bool) -> std::io::Result<Option<u16>> {
        self.state
            .network
            .set_local_listener(&self.state, on)
            .await?;
        Ok(self.state.network.local_listener_port())
    }

    /// Like [`Self::drop_connections`], for one user only.
    pub fn drop_user_connections(&self, user_id: i64) {
        for session in self.state.sessions.for_user(user_id) {
            session.drop_connection(CloseCode::UNKNOWN, Instant::now());
        }
    }

    /// Intended for tests.
    pub fn hold_processing(&self, hold: bool) {
        self.state.attachments.hold_processing(hold);
    }

    /// Intended for tests.
    pub fn db(&self) -> &sqlx::SqlitePool {
        &self.state.db
    }

    /// Intended for tests.
    pub async fn sweep_attachments(&self, now_ms: i64) {
        crate::attachments::gc::sweep(&self.state, now_ms).await;
    }

    /// Intended for tests.
    pub async fn drain_blob_deletions(&self, now_ms: i64) {
        crate::attachments::gc::drain_blobs(&self.state, now_ms).await;
    }
}

/// The voice node inside this server, unless voice is off or on external
/// nodes only.
async fn embedded_node(
    config: &Config,
    voice_key: &ed25519_dalek::SigningKey,
) -> anyhow::Result<(Option<VoiceNode>, Option<UnboundedReceiver<NodeEvent>>)> {
    if !config.voice.enabled || config.voice.mode != VoiceMode::Embedded {
        return Ok((None, None));
    }
    let public_address = match config.voice.public_address.as_str() {
        "auto" | "" => None,
        address => Some(address.to_owned()),
    };
    let (node, events) = VoiceNode::start(NodeConfig {
        udp_port: config.voice.udp_port,
        public_address,
        verifying_key: Some(voice_key.verifying_key()),
        heartbeat_interval: VOICE_HEARTBEAT,
    })
    .await
    .with_context(|| {
        format!(
            "could not start voice on UDP port {}",
            config.voice.udp_port
        )
    })?;
    Ok((Some(node), Some(events)))
}

/// The saved domains, and the token their HTTP checks look for, made on
/// first start.
async fn load_domains(pool: &sqlx::SqlitePool) -> anyhow::Result<Domains> {
    const VERIFY_TOKEN: &str = "domains.verify_token";
    let mut conn = pool.acquire().await?;
    let rows = db::domains::list(&mut conn).await?;
    let saved = db::meta::load_prefixed(&mut conn, VERIFY_TOKEN).await?;
    let token = match saved.get(VERIFY_TOKEN) {
        Some(token) => token.clone(),
        None => {
            let token = crate::random::code(32);
            db::meta::put(&mut conn, VERIFY_TOKEN, Some(&token)).await?;
            token
        }
    };
    Ok(Domains::load(rows, token))
}

fn bind_listener(addr: SocketAddr) -> std::io::Result<std::net::TcpListener> {
    let domain = if addr.is_ipv6() {
        socket2::Domain::IPV6
    } else {
        socket2::Domain::IPV4
    };
    let socket = socket2::Socket::new(domain, socket2::Type::STREAM, None)?;
    socket.set_reuse_address(true)?;
    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    socket.set_reuse_port(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    Ok(socket.into())
}

/// The main port: the owner's choice from the app when there is one,
/// else the configured one. A port that cannot be had falls back to the
/// configured one, so a bad choice never locks the owner out.
fn bind_main_port(config: &Config, chosen: Option<u16>) -> anyhow::Result<std::net::TcpListener> {
    let configured = config.server.bind;
    let wanted = match chosen {
        // Tests and anything asking for a free port keep doing so.
        Some(port) if configured.port() != 0 => SocketAddr::new(configured.ip(), port),
        _ => configured,
    };
    let listener = match bind_listener(wanted) {
        Ok(listener) => listener,
        Err(error) if wanted != configured => {
            tracing::error!(
                %error,
                port = wanted.port(),
                "could not listen on the main port chosen in the app; using {configured}"
            );
            bind_listener(configured)
                .with_context(|| format!("could not listen on {configured}"))?
        }
        Err(error) => {
            return Err(error).with_context(|| format!("could not listen on {configured}"));
        }
    };
    listener.set_nonblocking(true)?;
    Ok(listener)
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
        {
            let _writes = state.write_lock().await;
            voice::expire_detached(&state, Instant::now());
            voice::afk::move_idle(&state, Instant::now());
        }
        state.rate_limits.prune();
        let longest_cooldown = Duration::from_secs(u64::from(SOUND_COOLDOWN_MAX_S));
        state.voice().sounds.prune(Instant::now(), longest_cooldown);
    }
}
