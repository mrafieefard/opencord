//! `opencord.toml`, with `OPENCORD_*` environment overrides.

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Written to disk when no config file exists. Must parse to [`Config::default`].
pub const DEFAULT_CONFIG_TOML: &str = r#"# Opencord server configuration.
# Every setting can also be set with the environment variable shown next to it.

[server]
# Address and port to listen on. OPENCORD_BIND
bind = "0.0.0.0:7710"
# Host name or IP that clients use to reach this server, used in invite links.
# OPENCORD_PUBLIC_HOST
public_host = "localhost"
# Port that clients connect to, if different from the bind port (for example
# behind port forwarding). OPENCORD_PUBLIC_PORT
# public_port = 7710
# Where the database and TLS certificate are kept, relative to this file.
# OPENCORD_DATA_DIR
data_dir = "data"
# Name given to the server on first start. Change it later from the app.
# OPENCORD_SERVER_NAME
name = "Opencord"

[tls]
# Leave both unset to use a self-signed certificate generated on first start.
# Clients pin its fingerprint the first time they connect.
# OPENCORD_TLS_CERT, OPENCORD_TLS_KEY
# cert = "/etc/letsencrypt/live/chat.example.com/fullchain.pem"
# key = "/etc/letsencrypt/live/chat.example.com/privkey.pem"

[gateway]
# How often clients send a heartbeat. OPENCORD_HEARTBEAT_INTERVAL_MS
heartbeat_interval_ms = 30000

[log]
# tracing filter, for example "info" or "opencord_server=debug". OPENCORD_LOG
filter = "info"

[voice]
# Voice, video and screen share. OPENCORD_VOICE_ENABLED
enabled = true
# "embedded" runs the voice node inside this server. "external" uses only
# the voice nodes listed below.
mode = "embedded"
# UDP port for media; forward it on your router too. OPENCORD_VOICE_UDP_PORT
udp_port = 7711
# Host or IP that clients send media to. "auto" uses the host they reached
# this server with. OPENCORD_VOICE_PUBLIC_ADDRESS
public_address = "auto"
max_participants_per_channel = 99
# Total upload cap in megabits per second, for home connections; 0 is no
# cap. When it is reached, video quality drops first.
max_egress_mbps = 0

# Voice nodes on other machines (opencord-voice-node), each with its shared
# secret in a file next to this one: make one with
# `opencord-voice-node generate-secret voice1.secret` and give the node the
# same file. The endpoint must match the node's own exactly.
# [[voice.external_nodes]]
# endpoint = "wss://voice1.example.com:7712"
# secret_file = "voice1.secret"
"#;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerSection,
    pub tls: TlsSection,
    pub gateway: GatewaySection,
    pub log: LogSection,
    pub voice: VoiceSection,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerSection {
    pub bind: SocketAddr,
    pub public_host: String,
    pub public_port: Option<u16>,
    pub data_dir: PathBuf,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TlsSection {
    pub cert: Option<PathBuf>,
    pub key: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GatewaySection {
    pub heartbeat_interval_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogSection {
    pub filter: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct VoiceSection {
    pub enabled: bool,
    pub mode: VoiceMode,
    pub udp_port: u16,
    /// A host or IP, or "auto": the host clients reached this server with.
    pub public_address: String,
    pub max_participants_per_channel: u32,
    /// 0 means no cap.
    pub max_egress_mbps: u32,
    pub external_nodes: Vec<ExternalNode>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceMode {
    #[default]
    Embedded,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalNode {
    pub endpoint: String,
    pub secret_file: PathBuf,
}

impl Default for ServerSection {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], opencord_common::address::DEFAULT_PORT)),
            public_host: "localhost".to_owned(),
            public_port: None,
            data_dir: PathBuf::from("data"),
            name: "Opencord".to_owned(),
        }
    }
}

impl TlsSection {
    pub fn resolve_paths(self, base_dir: &Path) -> Self {
        Self {
            cert: self.cert.map(|cert| base_dir.join(cert)),
            key: self.key.map(|key| base_dir.join(key)),
        }
    }
}

impl Default for GatewaySection {
    fn default() -> Self {
        Self {
            heartbeat_interval_ms: 30_000,
        }
    }
}

impl Default for VoiceSection {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: VoiceMode::Embedded,
            udp_port: 7711,
            public_address: "auto".to_owned(),
            max_participants_per_channel: 99,
            max_egress_mbps: 0,
            external_nodes: Vec::new(),
        }
    }
}

impl Default for LogSection {
    fn default() -> Self {
        Self {
            filter: "info".to_owned(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read or write {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not valid: {source}")]
    Parse {
        path: PathBuf,
        source: Box<toml::de::Error>,
    },
    #[error("environment variable {name} has an invalid value: {value:?}")]
    InvalidEnv { name: &'static str, value: String },
}

/// A loaded config, and whether the file had to be created.
#[derive(Debug)]
pub struct Loaded<T = Config> {
    pub config: T,
    pub created: bool,
}

/// Reads a TOML config file, first writing `default_toml` there if it does
/// not exist.
pub fn read_or_create<T: DeserializeOwned>(
    path: &Path,
    default_toml: &str,
) -> Result<Loaded<T>, ConfigError> {
    let io_error = |source| ConfigError::Io {
        path: path.to_owned(),
        source,
    };
    let created = !path.exists();
    if created {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        fs::write(path, default_toml).map_err(io_error)?;
    }
    let text = fs::read_to_string(path).map_err(io_error)?;
    let config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.to_owned(),
        source: Box::new(source),
    })?;
    Ok(Loaded { config, created })
}

impl Config {
    /// Reads `path`, writing [`DEFAULT_CONFIG_TOML`] there first if it does
    /// not exist. Applies environment overrides from `env`, then resolves
    /// relative paths against the config file's directory.
    pub fn load_or_create(
        path: &Path,
        env: impl Fn(&str) -> Option<String>,
    ) -> Result<Loaded, ConfigError> {
        let loaded: Loaded<Config> = read_or_create(path, DEFAULT_CONFIG_TOML)?;
        let base_dir = path.parent().unwrap_or(Path::new(""));
        Ok(Loaded {
            config: loaded.config.with_env(&env)?.resolve_paths(base_dir),
            created: loaded.created,
        })
    }

    /// Port that clients connect to.
    pub fn public_port(&self) -> u16 {
        self.server.public_port.unwrap_or(self.server.bind.port())
    }

    fn with_env(self, env: &impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let mut config = self;
        if let Some(value) = env("OPENCORD_BIND") {
            config.server.bind = parse_env("OPENCORD_BIND", value)?;
        }
        if let Some(value) = env("OPENCORD_PUBLIC_HOST") {
            config.server.public_host = value;
        }
        if let Some(value) = env("OPENCORD_PUBLIC_PORT") {
            config.server.public_port = Some(parse_env("OPENCORD_PUBLIC_PORT", value)?);
        }
        if let Some(value) = env("OPENCORD_DATA_DIR") {
            config.server.data_dir = PathBuf::from(value);
        }
        if let Some(value) = env("OPENCORD_SERVER_NAME") {
            config.server.name = value;
        }
        if let Some(value) = env("OPENCORD_TLS_CERT") {
            config.tls.cert = Some(PathBuf::from(value));
        }
        if let Some(value) = env("OPENCORD_TLS_KEY") {
            config.tls.key = Some(PathBuf::from(value));
        }
        if let Some(value) = env("OPENCORD_HEARTBEAT_INTERVAL_MS") {
            config.gateway.heartbeat_interval_ms =
                parse_env("OPENCORD_HEARTBEAT_INTERVAL_MS", value)?;
        }
        if let Some(value) = env("OPENCORD_LOG") {
            config.log.filter = value;
        }
        if let Some(value) = env("OPENCORD_VOICE_ENABLED") {
            config.voice.enabled = parse_env("OPENCORD_VOICE_ENABLED", value)?;
        }
        if let Some(value) = env("OPENCORD_VOICE_UDP_PORT") {
            config.voice.udp_port = parse_env("OPENCORD_VOICE_UDP_PORT", value)?;
        }
        if let Some(value) = env("OPENCORD_VOICE_PUBLIC_ADDRESS") {
            config.voice.public_address = value;
        }
        Ok(config)
    }

    fn resolve_paths(self, base_dir: &Path) -> Self {
        Self {
            server: ServerSection {
                data_dir: base_dir.join(&self.server.data_dir),
                ..self.server
            },
            tls: self.tls.resolve_paths(base_dir),
            voice: VoiceSection {
                external_nodes: self
                    .voice
                    .external_nodes
                    .into_iter()
                    .map(|node| ExternalNode {
                        secret_file: base_dir.join(node.secret_file),
                        ..node
                    })
                    .collect(),
                ..self.voice
            },
            ..self
        }
    }
}

fn parse_env<T: FromStr>(name: &'static str, value: String) -> Result<T, ConfigError> {
    value
        .parse()
        .map_err(|_| ConfigError::InvalidEnv { name, value })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;

    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn default_file_matches_default_config() {
        let parsed: Config = toml::from_str(DEFAULT_CONFIG_TOML).unwrap();

        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn creates_the_file_when_missing_and_resolves_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");

        let loaded = Config::load_or_create(&path, no_env).unwrap();

        assert!(loaded.created);
        assert_eq!(fs::read_to_string(&path).unwrap(), DEFAULT_CONFIG_TOML);
        assert_eq!(loaded.config.server.data_dir, dir.path().join("data"));
    }

    #[test]
    fn reads_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");
        fs::write(
            &path,
            "[server]\nbind = \"127.0.0.1:9000\"\ndata_dir = \"/srv/opencord\"\n",
        )
        .unwrap();

        let loaded = Config::load_or_create(&path, no_env).unwrap();

        assert!(!loaded.created);
        assert_eq!(loaded.config.server.bind, "127.0.0.1:9000".parse().unwrap());
        assert_eq!(
            loaded.config.server.data_dir,
            PathBuf::from("/srv/opencord")
        );
        assert_eq!(loaded.config.server.name, "Opencord");
    }

    #[test]
    fn environment_overrides_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");
        let env: HashMap<&str, &str> = HashMap::from([
            ("OPENCORD_BIND", "127.0.0.1:7000"),
            ("OPENCORD_PUBLIC_HOST", "chat.example.com"),
            ("OPENCORD_PUBLIC_PORT", "443"),
            ("OPENCORD_DATA_DIR", "state"),
            ("OPENCORD_SERVER_NAME", "Friends"),
            ("OPENCORD_TLS_CERT", "/certs/full.pem"),
            ("OPENCORD_TLS_KEY", "/certs/key.pem"),
            ("OPENCORD_HEARTBEAT_INTERVAL_MS", "5000"),
            ("OPENCORD_LOG", "debug"),
            ("OPENCORD_VOICE_ENABLED", "false"),
            ("OPENCORD_VOICE_UDP_PORT", "17711"),
            ("OPENCORD_VOICE_PUBLIC_ADDRESS", "203.0.113.7"),
        ]);

        let config = Config::load_or_create(&path, |name| env.get(name).map(|v| (*v).to_owned()))
            .unwrap()
            .config;

        assert_eq!(config.server.bind, "127.0.0.1:7000".parse().unwrap());
        assert_eq!(config.server.public_host, "chat.example.com");
        assert_eq!(config.public_port(), 443);
        assert_eq!(config.server.data_dir, dir.path().join("state"));
        assert_eq!(config.server.name, "Friends");
        assert_eq!(config.tls.cert, Some(PathBuf::from("/certs/full.pem")));
        assert_eq!(config.tls.key, Some(PathBuf::from("/certs/key.pem")));
        assert_eq!(config.gateway.heartbeat_interval_ms, 5000);
        assert_eq!(config.log.filter, "debug");
        assert!(!config.voice.enabled);
        assert_eq!(config.voice.udp_port, 17711);
        assert_eq!(config.voice.public_address, "203.0.113.7");
    }

    #[test]
    fn voice_is_embedded_on_udp_7711_by_default() {
        let voice = Config::default().voice;

        assert!(voice.enabled);
        assert_eq!(voice.mode, VoiceMode::Embedded);
        assert_eq!(voice.udp_port, 7711);
        assert_eq!(voice.public_address, "auto");
        assert_eq!(voice.max_participants_per_channel, 99);
        assert_eq!(voice.max_egress_mbps, 0);
        assert!(voice.external_nodes.is_empty());
    }

    #[test]
    fn reads_external_voice_nodes_with_secrets_beside_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");
        fs::write(
            &path,
            "[voice]\nmode = \"external\"\n\n[[voice.external_nodes]]\n\
             endpoint = \"wss://voice1.example.com:7712\"\nsecret_file = \"voice1.secret\"\n",
        )
        .unwrap();

        let voice = Config::load_or_create(&path, no_env).unwrap().config.voice;

        assert_eq!(voice.mode, VoiceMode::External);
        assert_eq!(
            voice.external_nodes,
            [ExternalNode {
                endpoint: "wss://voice1.example.com:7712".to_owned(),
                secret_file: dir.path().join("voice1.secret"),
            }]
        );
    }

    #[test]
    fn public_port_defaults_to_the_bind_port() {
        let config = Config::default();

        assert_eq!(config.public_port(), 7710);
    }

    #[test]
    fn rejects_invalid_environment_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");

        let result = Config::load_or_create(&path, |name| {
            (name == "OPENCORD_PUBLIC_PORT").then(|| "lots".to_owned())
        });

        assert!(matches!(
            result,
            Err(ConfigError::InvalidEnv {
                name: "OPENCORD_PUBLIC_PORT",
                ..
            })
        ));
    }

    #[test]
    fn rejects_unknown_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");
        fs::write(&path, "[server]\nbnid = \"0.0.0.0:1\"\n").unwrap();

        assert!(matches!(
            Config::load_or_create(&path, no_env),
            Err(ConfigError::Parse { .. })
        ));
    }
}
