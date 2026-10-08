//! `opencord-voice-node.toml`: where the main server is, the shared secret,
//! and how clients reach this node.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, anyhow, bail};
use opencord_common::address::{Fingerprint, ServerAddress, parse_fingerprint};
use serde::Deserialize;

use crate::config::{ConfigError, Loaded, LogSection, TlsSection, read_or_create};

/// Written to disk when no config file exists. Must parse to
/// [`VoiceNodeConfig::default`].
pub const DEFAULT_VOICE_NODE_TOML: &str = r#"# Opencord voice node configuration.
# The main server sends people here for voice once this node has registered
# with it. List this node on the main server too, under
# [[voice.external_nodes]], with the same endpoint and shared secret.

[node]
# The main server, as host:port.
main_server = ""
# The main server's certificate fingerprint, the 64 hex characters that
# `opencord-server fingerprint` prints. Leave it unset if the main server has
# a certificate from a public certificate authority.
# main_fingerprint = ""
# File holding the shared secret, relative to this file. Make one with
# `opencord-voice-node generate-secret voice.secret` and copy it to the main
# server.
secret_file = "voice.secret"
# The address clients use to reach this node, wss://host:port. It must match
# the endpoint listed on the main server exactly.
endpoint = ""
# Address and port the voice gateway listens on.
bind = "0.0.0.0:7712"
# UDP port for media; forward it on your router too.
udp_port = 7711
# Host or IP that clients send media to. "auto" uses the endpoint's host.
public_address = "auto"
# Where the TLS certificate is kept, relative to this file.
data_dir = "data"

[tls]
# Leave both unset to use a self-signed certificate generated on first start.
# The main server tells clients its fingerprint.
# cert = "/etc/letsencrypt/live/voice1.example.com/fullchain.pem"
# key = "/etc/letsencrypt/live/voice1.example.com/privkey.pem"

[log]
# tracing filter, for example "info" or "opencord_server=debug".
filter = "info"
"#;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceNodeConfig {
    pub node: NodeSection,
    pub tls: TlsSection,
    pub log: LogSection,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NodeSection {
    /// `host:port` of the main server.
    pub main_server: String,
    /// The main server's certificate fingerprint; unset trusts the public
    /// certificate authorities.
    pub main_fingerprint: Option<String>,
    pub secret_file: PathBuf,
    /// `wss://host:port`, exactly as listed on the main server.
    pub endpoint: String,
    pub bind: SocketAddr,
    pub udp_port: u16,
    /// A host or IP, or "auto": the endpoint's host.
    pub public_address: String,
    pub data_dir: PathBuf,
}

impl Default for NodeSection {
    fn default() -> Self {
        Self {
            main_server: String::new(),
            main_fingerprint: None,
            secret_file: PathBuf::from("voice.secret"),
            endpoint: String::new(),
            bind: SocketAddr::from(([0, 0, 0, 0], 7712)),
            udp_port: 7711,
            public_address: "auto".to_owned(),
            data_dir: PathBuf::from("data"),
        }
    }
}

/// What a config says once checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub main_server: ServerAddress,
    pub main_fingerprint: Option<Fingerprint>,
    /// The endpoint's host, which the certificate is made for.
    pub endpoint_host: String,
    /// `None` for "auto".
    pub public_address: Option<String>,
}

impl VoiceNodeConfig {
    /// Reads `path`, writing [`DEFAULT_VOICE_NODE_TOML`] there first if it
    /// does not exist, and resolves relative paths against its directory.
    pub fn load_or_create(path: &Path) -> Result<Loaded<Self>, ConfigError> {
        let loaded: Loaded<Self> = read_or_create(path, DEFAULT_VOICE_NODE_TOML)?;
        let base_dir = path.parent().unwrap_or(Path::new(""));
        Ok(Loaded {
            config: loaded.config.resolve_paths(base_dir),
            created: loaded.created,
        })
    }

    /// Checks the settings that have no usable default.
    pub fn check(&self) -> anyhow::Result<Checked> {
        let node = &self.node;
        if node.main_server.is_empty() {
            bail!("node.main_server is not set: give the main server's host:port");
        }
        let main_server = ServerAddress::parse(&node.main_server).map_err(|_| {
            anyhow!(
                "node.main_server is not a host:port: {:?}",
                node.main_server
            )
        })?;
        let main_fingerprint = node
            .main_fingerprint
            .as_deref()
            .map(parse_fingerprint)
            .transpose()
            .context("node.main_fingerprint is not a certificate fingerprint")?;
        let endpoint_host = endpoint_host(&node.endpoint).ok_or_else(|| {
            anyhow!(
                "node.endpoint must be wss://host:port, as listed on the main server: {:?}",
                node.endpoint
            )
        })?;
        let public_address = match node.public_address.as_str() {
            "auto" | "" => None,
            address => Some(address.to_owned()),
        };
        Ok(Checked {
            main_server,
            main_fingerprint,
            endpoint_host,
            public_address,
        })
    }

    fn resolve_paths(self, base_dir: &Path) -> Self {
        Self {
            node: NodeSection {
                secret_file: base_dir.join(&self.node.secret_file),
                data_dir: base_dir.join(&self.node.data_dir),
                ..self.node
            },
            tls: self.tls.resolve_paths(base_dir),
            ..self
        }
    }
}

/// The host of a `wss://host:port` endpoint, which may have a path. The
/// port must be there: clients would otherwise assume the main server's.
fn endpoint_host(endpoint: &str) -> Option<String> {
    let rest = endpoint.strip_prefix("wss://")?;
    let authority = rest.split('/').next()?;
    let has_port = match authority.rsplit_once(']') {
        Some((_, after)) => after.starts_with(':'),
        None => authority.contains(':'),
    };
    if !has_port {
        return None;
    }
    ServerAddress::parse(authority)
        .ok()
        .map(|address| address.host)
}

#[cfg(test)]
mod tests {
    use opencord_common::address::format_fingerprint;

    use super::*;

    fn configured() -> VoiceNodeConfig {
        let mut config = VoiceNodeConfig::default();
        config.node.main_server = "chat.example.com:7710".to_owned();
        config.node.endpoint = "wss://voice1.example.com:7712".to_owned();
        config
    }

    #[test]
    fn default_file_matches_default_config() {
        let parsed: VoiceNodeConfig = toml::from_str(DEFAULT_VOICE_NODE_TOML).unwrap();

        assert_eq!(parsed, VoiceNodeConfig::default());
    }

    #[test]
    fn relative_paths_are_beside_the_config_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord-voice-node.toml");
        std::fs::write(
            &path,
            "[node]\nsecret_file = \"voice1.secret\"\n\n[tls]\ncert = \"c.pem\"\nkey = \"k.pem\"\n",
        )
        .unwrap();

        let config = VoiceNodeConfig::load_or_create(&path).unwrap().config;

        assert_eq!(config.node.secret_file, dir.path().join("voice1.secret"));
        assert_eq!(config.node.data_dir, dir.path().join("data"));
        assert_eq!(config.tls.cert, Some(dir.path().join("c.pem")));
    }

    #[test]
    fn a_missing_file_is_written_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("node.toml");

        let loaded = VoiceNodeConfig::load_or_create(&path).unwrap();

        assert!(loaded.created);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            DEFAULT_VOICE_NODE_TOML
        );
    }

    #[test]
    fn a_complete_config_checks_out() {
        let mut config = configured();
        config.node.main_fingerprint = Some(format_fingerprint(&[7; 32]));
        config.node.public_address = "203.0.113.5".to_owned();

        let checked = config.check().unwrap();

        assert_eq!(checked.main_server.host, "chat.example.com");
        assert_eq!(checked.main_server.port, 7710);
        assert_eq!(checked.main_fingerprint, Some([7; 32]));
        assert_eq!(checked.endpoint_host, "voice1.example.com");
        assert_eq!(checked.public_address.as_deref(), Some("203.0.113.5"));
        assert_eq!(configured().check().unwrap().public_address, None);
    }

    #[test]
    fn the_main_server_and_endpoint_are_required() {
        let mut no_main = configured();
        no_main.node.main_server = String::new();
        let mut no_endpoint = configured();
        no_endpoint.node.endpoint = String::new();
        let mut plain = configured();
        plain.node.endpoint = "ws://voice1.example.com:7712".to_owned();
        let mut no_port = configured();
        no_port.node.endpoint = "wss://voice1.example.com".to_owned();
        let mut bad_fingerprint = configured();
        bad_fingerprint.node.main_fingerprint = Some("AB:CD".to_owned());

        let problem = |config: VoiceNodeConfig| config.check().unwrap_err().to_string();

        assert!(problem(no_main).contains("main_server"));
        assert!(problem(no_endpoint).contains("endpoint"));
        assert!(problem(plain).contains("endpoint"));
        assert!(problem(no_port).contains("endpoint"));
        assert!(problem(bad_fingerprint).contains("main_fingerprint"));
    }

    #[test]
    fn endpoints_may_have_a_path_and_ipv6_hosts() {
        assert_eq!(
            endpoint_host("wss://[2001:db8::7]:7712/voice").as_deref(),
            Some("2001:db8::7")
        );
        assert_eq!(
            endpoint_host("wss://127.0.0.1:9000").as_deref(),
            Some("127.0.0.1")
        );
        assert_eq!(endpoint_host("wss://[2001:db8::7]"), None);
    }
}
