//! `opencord-voice-node`: a voice node on a machine of its own (Phase 2 plan
//! §3.4). It serves the voice gateway over TLS and media over UDP like the
//! embedded node, and takes its orders from the main server over the
//! control channel.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use axum::Router;
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use opencord_common::address::Fingerprint;
use opencord_voice::node::{NodeConfig, VoiceNode};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::{http, server, tls, voice};
use uplink::Uplink;

pub mod config;
mod uplink;

pub use config::{DEFAULT_VOICE_NODE_TOML, NodeSection, VoiceNodeConfig};

/// A running voice node.
#[derive(Debug)]
pub struct VoiceNodeHandle {
    endpoint: String,
    /// Where the voice gateway listens.
    pub local_addr: SocketAddr,
    pub fingerprint: Fingerprint,
    node: VoiceNode,
    registered: watch::Receiver<bool>,
    handle: Handle<SocketAddr>,
    serve: JoinHandle<std::io::Result<()>>,
    uplink: JoinHandle<()>,
}

/// Starts the node on the configured address.
pub async fn start(config: VoiceNodeConfig) -> anyhow::Result<VoiceNodeHandle> {
    let bind = config.node.bind;
    let listener =
        std::net::TcpListener::bind(bind).with_context(|| format!("could not listen on {bind}"))?;
    start_on(config, listener).await
}

/// Starts the node with its voice gateway on `listener`.
pub async fn start_on(
    config: VoiceNodeConfig,
    listener: std::net::TcpListener,
) -> anyhow::Result<VoiceNodeHandle> {
    let checked = config.check()?;
    let secret = read_secret(&config.node.secret_file).await?;
    let data_dir = &config.node.data_dir;
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("could not create {}", data_dir.display()))?;
    let tls = tls::load_or_generate(
        &config.tls,
        &data_dir.join("tls"),
        &hostnames(&checked.endpoint_host),
    )?;
    let (node, events) = VoiceNode::start(NodeConfig {
        udp_port: config.node.udp_port,
        public_address: checked.public_address.clone(),
        verifying_key: None,
        heartbeat_interval: server::VOICE_HEARTBEAT,
    })
    .await
    .with_context(|| format!("could not start voice on UDP port {}", config.node.udp_port))?;

    listener
        .set_nonblocking(true)
        .context("could not set up the listener")?;
    let rustls = RustlsConfig::from_config(Arc::new(tls::server_config(&tls)?));
    let app = Router::new()
        .route("/voice", get(voice_gateway))
        .with_state(node.clone());
    let handle = Handle::new();
    let serve = tokio::spawn(
        axum_server::from_tcp_rustls(listener, rustls)
            .context("could not set up the listener")?
            .handle(handle.clone())
            .serve(app.into_make_service()),
    );
    let local_addr = handle
        .listening()
        .await
        .ok_or_else(|| anyhow!("the voice gateway could not listen"))?;

    let (registered_tx, registered) = watch::channel(false);
    let uplink = Uplink {
        main_server: checked.main_server,
        main_fingerprint: checked.main_fingerprint,
        secret,
        endpoint: config.node.endpoint.clone(),
        certificate_fingerprint: tls.fingerprint,
        public_address: checked.public_address.unwrap_or_default(),
        udp_port: node.udp_port(),
    };
    let uplink = tokio::spawn(uplink::run(uplink, node.clone(), events, registered_tx));
    Ok(VoiceNodeHandle {
        endpoint: config.node.endpoint,
        local_addr,
        fingerprint: tls.fingerprint,
        node,
        registered,
        handle,
        serve,
        uplink,
    })
}

impl VoiceNodeHandle {
    /// The `wss://` URL clients reach this node at.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn udp_port(&self) -> u16 {
        self.node.udp_port()
    }

    /// Waits until the node is registered with the main server.
    pub async fn registered(&self) {
        let mut registered = self.registered.clone();
        let _ = registered.wait_for(|registered| *registered).await;
    }

    /// Leaves the main server, which moves this node's calls elsewhere, ends
    /// every voice session and stops listening.
    pub async fn shutdown(self) {
        self.uplink.abort();
        let _ = self.uplink.await;
        self.node.shutdown();
        self.handle.shutdown();
        let _ = self.serve.await;
    }
}

async fn voice_gateway(State(node): State<VoiceNode>, upgrade: WebSocketUpgrade) -> Response {
    http::voice_gateway_upgrade(node, upgrade)
}

async fn read_secret(path: &Path) -> anyhow::Result<Vec<u8>> {
    voice::control::read_secret(path).await.map_err(|problem| {
        anyhow!(
            "the shared secret in {} is unusable: {problem}",
            path.display()
        )
    })
}

/// Writes a new random shared secret to `path`, readable only by its owner.
/// An existing file is left alone.
pub fn generate_secret(path: &Path) -> anyhow::Result<()> {
    let secret = hex::encode(crate::random::bytes::<32>());
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .with_context(|| format!("could not create {}", path.display()))?;
    std::io::Write::write_all(&mut file, format!("{secret}\n").as_bytes())
        .with_context(|| format!("could not write {}", path.display()))
}

fn hostnames(endpoint_host: &str) -> Vec<String> {
    let mut names = vec![endpoint_host.to_owned()];
    for fallback in ["localhost", "127.0.0.1", "::1"] {
        if !names.iter().any(|name| name == fallback) {
            names.push(fallback.to_owned());
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use opencord_common::address::format_fingerprint;

    use super::*;
    use crate::config::{Config, ExternalNode, VoiceMode};

    #[tokio::test]
    async fn a_node_that_stopped_leaves_the_main_server() {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("wss://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let secret_file = dir.path().join("node.secret");
        std::fs::write(&secret_file, "a-shared-secret-for-this-test").unwrap();
        let mut main_config = Config::default();
        main_config.server.bind = "127.0.0.1:0".parse().unwrap();
        main_config.server.data_dir = dir.path().join("main");
        main_config.voice.mode = VoiceMode::External;
        main_config.voice.external_nodes = vec![ExternalNode {
            endpoint: endpoint.clone(),
            secret_file: secret_file.clone(),
        }];
        let main = server::start(main_config).await.unwrap();
        let mut config = VoiceNodeConfig::default();
        config.node.main_server = format!("127.0.0.1:{}", main.local_addr.port());
        config.node.main_fingerprint = Some(format_fingerprint(&main.fingerprint));
        config.node.secret_file = secret_file;
        config.node.endpoint = endpoint;
        config.node.udp_port = 0;
        config.node.data_dir = dir.path().join("node");
        let node = start_on(config, listener).await.unwrap();
        node.registered().await;

        node.node.shutdown();
        let left = tokio::time::timeout(Duration::from_secs(5), async {
            while !node.uplink.is_finished() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;

        assert!(left.is_ok(), "the uplink kept the stopped node registered");
    }

    #[test]
    fn a_generated_secret_is_long_random_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.secret");
        let second = dir.path().join("second.secret");

        generate_secret(&first).unwrap();
        generate_secret(&second).unwrap();
        let written = std::fs::read_to_string(&first).unwrap();
        let again = generate_secret(&first);

        assert_eq!(written.trim().len(), 64);
        assert!(written.trim().chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(written, std::fs::read_to_string(&second).unwrap());
        assert!(again.is_err());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), written);
    }

    #[cfg(unix)]
    #[test]
    fn a_generated_secret_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("voice.secret");

        generate_secret(&path).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
