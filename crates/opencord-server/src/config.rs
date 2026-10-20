//! `opencord.toml`, with `OPENCORD_*` environment overrides.

use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use opencord_common::limits::IDENTIFY_RATE;
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
# The key that seals certificates, tokens, passwords and every file's key.
# Back it up: without it, uploaded files cannot be read. To protect files
# against a stolen disk, keep it on another disk. Default: secrets.key in
# the data directory. OPENCORD_SECRETS_FILE
# secrets_file = "/run/secrets/opencord-secrets.key"

[tls]
# Leave both unset to use a self-signed certificate generated on first start.
# Clients pin its fingerprint the first time they connect.
# OPENCORD_TLS_CERT, OPENCORD_TLS_KEY
# cert = "/etc/letsencrypt/live/chat.example.com/fullchain.pem"
# key = "/etc/letsencrypt/live/chat.example.com/privkey.pem"

[gateway]
# How often clients send a heartbeat. OPENCORD_HEARTBEAT_INTERVAL_MS
heartbeat_interval_ms = 30000
# Sign-ins allowed from one address per minute. Raise it when many people
# share an address, at a LAN party say. OPENCORD_IDENTIFY_PER_MINUTE
identify_per_minute = 5

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

[music]
# Music in voice channels, played by Lavalink nodes added in the app.
# OPENCORD_MUSIC_ENABLED
enabled = true
# UDP port Lavalink sends music to; Lavalink nodes on other machines must
# reach it. OPENCORD_MUSIC_UDP_PORT
audio_udp_port = 7712
# The domain Lavalink connects to: "primary", or another active domain with
# a trusted certificate.
bridge_domain = "primary"
# Tracks from Lavalink's http and local sources let members make Lavalink
# fetch any address or file; refused unless this is on.
allow_unsafe_sources = false

[uploads]
# The largest file anyone may upload, in MB (1 MB = 1,048,576 bytes): the
# owner's own limit and the ceiling for every role's limit. 1 to 102400.
# OPENCORD_UPLOADS_MAX_FILE_MB
max_file_mb = 4096
# Where files on this server's own storage are kept. Default: "attachments"
# in the data directory. OPENCORD_UPLOADS_LOCAL_DIR
# local_dir = "/mnt/storage/opencord-attachments"

[network]
# Services that answer with the address a request came from, used to find
# the server's public address. Empty turns detection off; the owner can
# also turn it off in the app. OPENCORD_IP_LOOKUP_V4, OPENCORD_IP_LOOKUP_V6
ip_lookup_v4 = "https://api.ipify.org"
ip_lookup_v6 = "https://api6.ipify.org"
"#;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerSection,
    pub tls: TlsSection,
    pub gateway: GatewaySection,
    pub log: LogSection,
    pub voice: VoiceSection,
    pub music: MusicSection,
    pub network: NetworkSection,
    pub uploads: UploadsSection,
    pub acme: AcmeSection,
    pub tunnel: TunnelSection,
}

/// Media uploads (design 2026-10-18).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UploadsSection {
    pub max_file_mb: u64,
    pub local_dir: Option<PathBuf>,
}

impl Default for UploadsSection {
    fn default() -> Self {
        Self {
            max_file_mb: 4096,
            local_dir: None,
        }
    }
}

/// Cloudflare Tunnel (Phase 3 plan §9.2).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TunnelSection {
    /// The cloudflared to run; empty: the one downloaded into the data
    /// directory, else the one on the PATH.
    pub cloudflared: Option<PathBuf>,
    /// Where cloudflared's latest release is described, for downloading it.
    pub releases_api: String,
}

impl Default for TunnelSection {
    fn default() -> Self {
        Self {
            cloudflared: None,
            releases_api: "https://api.github.com/repos/cloudflare/cloudflared/releases/latest"
                .to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerSection {
    pub bind: SocketAddr,
    pub public_host: String,
    pub public_port: Option<u16>,
    pub data_dir: PathBuf,
    pub name: String,
    pub secrets_file: Option<PathBuf>,
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
    pub identify_per_minute: u32,
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
            secrets_file: None,
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
            identify_per_minute: IDENTIFY_RATE.burst,
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

/// Music in voice channels (Phase 3 plan §13.5).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MusicSection {
    pub enabled: bool,
    pub audio_udp_port: u16,
    pub bridge_domain: String,
    /// Tracks from Lavalink's `http` and `local` sources (plan §15.2).
    pub allow_unsafe_sources: bool,
}

impl Default for MusicSection {
    fn default() -> Self {
        Self {
            enabled: true,
            audio_udp_port: 7712,
            bridge_domain: "primary".to_owned(),
            allow_unsafe_sources: false,
        }
    }
}

/// Where certificates come from (Phase 3 plan §7). Let's Encrypt unless
/// changed; not in the default file: private ACME servers (step-ca, for
/// one) and tests change it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AcmeSection {
    pub directory: String,
    /// Used when the owner turns on the test environment.
    pub test_directory: String,
    /// A PEM file of the ACME server's own root, when not a public one.
    pub root: Option<PathBuf>,
    /// The ports the ACME server checks HTTP-01 and TLS-ALPN-01 on.
    pub http_port: u16,
    pub tls_alpn_port: u16,
    /// How long a DNS-01 record may take to appear, at most, in seconds.
    pub dns_propagation_s: u32,
    /// The DNS providers' APIs.
    pub cloudflare_api: String,
    pub duckdns_api: String,
    pub desec_api: String,
}

impl Default for AcmeSection {
    fn default() -> Self {
        Self {
            directory: "https://acme-v02.api.letsencrypt.org/directory".to_owned(),
            test_directory: "https://acme-staging-v02.api.letsencrypt.org/directory".to_owned(),
            root: None,
            http_port: 80,
            tls_alpn_port: 443,
            dns_propagation_s: 120,
            cloudflare_api: "https://api.cloudflare.com/client/v4".to_owned(),
            duckdns_api: "https://www.duckdns.org".to_owned(),
            desec_api: "https://desec.io/api/v1".to_owned(),
        }
    }
}

/// How the server finds its public address (Phase 3 plan §5.1).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NetworkSection {
    pub ip_lookup_v4: String,
    pub ip_lookup_v6: String,
    /// The port domain checks ask name servers on. Not in the default
    /// file: only tests change it.
    pub dns_name_server_port: u16,
}

impl Default for NetworkSection {
    fn default() -> Self {
        Self {
            ip_lookup_v4: "https://api.ipify.org".to_owned(),
            ip_lookup_v6: "https://api6.ipify.org".to_owned(),
            dns_name_server_port: 53,
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
    #[error("invalid configuration for {key}: {message}")]
    Invalid { key: &'static str, message: String },
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
        let config = loaded.config.with_env(&env)?.resolve_paths(base_dir);
        if !opencord_common::upload::MAX_FILE_MB_RANGE.contains(&config.uploads.max_file_mb) {
            return Err(ConfigError::Invalid {
                key: "uploads.max_file_mb",
                message: format!(
                    "must be between {} and {}",
                    opencord_common::upload::MAX_FILE_MB_RANGE.start(),
                    opencord_common::upload::MAX_FILE_MB_RANGE.end(),
                ),
            });
        }
        Ok(Loaded {
            config,
            created: loaded.created,
        })
    }

    /// Port that clients connect to.
    pub fn public_port(&self) -> u16 {
        self.server.public_port.unwrap_or(self.server.bind.port())
    }

    pub fn secrets_file(&self) -> PathBuf {
        self.server
            .secrets_file
            .clone()
            .unwrap_or_else(|| self.server.data_dir.join(crate::secrets::SECRETS_FILE))
    }

    pub fn attachments_dir(&self) -> PathBuf {
        self.uploads
            .local_dir
            .clone()
            .unwrap_or_else(|| self.server.data_dir.join("attachments"))
    }

    pub fn uploads_temp_dir(&self) -> PathBuf {
        self.server.data_dir.join("uploads")
    }

    pub fn max_file_bytes(&self) -> u64 {
        self.uploads.max_file_mb * opencord_common::upload::MIB
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
        if let Some(value) = env("OPENCORD_SECRETS_FILE") {
            config.server.secrets_file = Some(PathBuf::from(value));
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
        if let Some(value) = env("OPENCORD_IDENTIFY_PER_MINUTE") {
            config.gateway.identify_per_minute = parse_env("OPENCORD_IDENTIFY_PER_MINUTE", value)?;
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
        if let Some(value) = env("OPENCORD_MUSIC_ENABLED") {
            config.music.enabled = parse_env("OPENCORD_MUSIC_ENABLED", value)?;
        }
        if let Some(value) = env("OPENCORD_MUSIC_UDP_PORT") {
            config.music.audio_udp_port = parse_env("OPENCORD_MUSIC_UDP_PORT", value)?;
        }
        if let Some(value) = env("OPENCORD_IP_LOOKUP_V4") {
            config.network.ip_lookup_v4 = value;
        }
        if let Some(value) = env("OPENCORD_IP_LOOKUP_V6") {
            config.network.ip_lookup_v6 = value;
        }
        if let Some(value) = env("OPENCORD_VOICE_PUBLIC_ADDRESS") {
            config.voice.public_address = value;
        }
        if let Some(value) = env("OPENCORD_UPLOADS_MAX_FILE_MB") {
            config.uploads.max_file_mb = parse_env("OPENCORD_UPLOADS_MAX_FILE_MB", value)?;
        }
        if let Some(value) = env("OPENCORD_UPLOADS_LOCAL_DIR") {
            config.uploads.local_dir = Some(PathBuf::from(value));
        }
        Ok(config)
    }

    fn resolve_paths(self, base_dir: &Path) -> Self {
        Self {
            server: ServerSection {
                data_dir: base_dir.join(&self.server.data_dir),
                secrets_file: self.server.secrets_file.map(|p| base_dir.join(p)),
                ..self.server
            },
            uploads: UploadsSection {
                local_dir: self.uploads.local_dir.map(|p| base_dir.join(p)),
                ..self.uploads
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
            acme: AcmeSection {
                root: self.acme.root.map(|root| base_dir.join(root)),
                ..self.acme
            },
            tunnel: TunnelSection {
                cloudflared: self.tunnel.cloudflared.map(|path| {
                    if path.components().count() > 1 {
                        base_dir.join(path)
                    } else {
                        path
                    }
                }),
                ..self.tunnel
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
        // Absolute on every system (`/srv/…` has no drive on Windows); a
        // TOML literal string keeps Windows' backslashes.
        let data_dir = dir.path().join("srv").join("opencord");
        fs::write(
            &path,
            format!(
                "[server]\nbind = \"127.0.0.1:9000\"\ndata_dir = '{}'\n",
                data_dir.display()
            ),
        )
        .unwrap();

        let loaded = Config::load_or_create(&path, no_env).unwrap();

        assert!(!loaded.created);
        assert_eq!(loaded.config.server.bind, "127.0.0.1:9000".parse().unwrap());
        assert_eq!(loaded.config.server.data_dir, data_dir);
        assert_eq!(loaded.config.server.name, "Opencord");
    }

    #[test]
    fn environment_overrides_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");
        // Absolute on every system (`/certs/…` has no drive on Windows).
        let cert = dir.path().join("certs").join("full.pem");
        let key = dir.path().join("certs").join("key.pem");
        let (cert_text, key_text) = (cert.display().to_string(), key.display().to_string());
        let env: HashMap<&str, &str> = HashMap::from([
            ("OPENCORD_BIND", "127.0.0.1:7000"),
            ("OPENCORD_PUBLIC_HOST", "chat.example.com"),
            ("OPENCORD_PUBLIC_PORT", "443"),
            ("OPENCORD_DATA_DIR", "state"),
            ("OPENCORD_SERVER_NAME", "Friends"),
            ("OPENCORD_TLS_CERT", cert_text.as_str()),
            ("OPENCORD_TLS_KEY", key_text.as_str()),
            ("OPENCORD_HEARTBEAT_INTERVAL_MS", "5000"),
            ("OPENCORD_IDENTIFY_PER_MINUTE", "200"),
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
        assert_eq!(config.tls.cert, Some(cert));
        assert_eq!(config.tls.key, Some(key));
        assert_eq!(config.gateway.heartbeat_interval_ms, 5000);
        assert_eq!(config.gateway.identify_per_minute, 200);
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

    #[test]
    fn uploads_defaults_and_env() {
        use opencord_common::upload::MIB;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");

        let loaded = Config::load_or_create(&path, no_env).unwrap();
        assert_eq!(loaded.config.max_file_bytes(), 4096 * MIB);
        assert_eq!(
            loaded.config.attachments_dir(),
            dir.path().join("data").join("attachments")
        );

        let env_map = [
            ("OPENCORD_UPLOADS_MAX_FILE_MB", "10".to_string()),
            ("OPENCORD_SECRETS_FILE", "keys/s.key".to_string()),
        ];
        let get_env = |k: &str| {
            env_map
                .iter()
                .find(|(name, _)| *name == k)
                .map(|(_, v)| v.clone())
        };
        let loaded = Config::load_or_create(&path, get_env).unwrap();
        assert_eq!(loaded.config.max_file_bytes(), 10 * MIB);
        assert_eq!(loaded.config.secrets_file(), dir.path().join("keys/s.key"));
    }

    #[test]
    fn max_file_mb_out_of_range_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opencord.toml");

        for bad in ["0", "102401"] {
            let get_env = |k: &str| {
                if k == "OPENCORD_UPLOADS_MAX_FILE_MB" {
                    Some(bad.to_string())
                } else {
                    None
                }
            };
            match Config::load_or_create(&path, get_env) {
                Err(ConfigError::Invalid { key, .. }) => {
                    assert_eq!(key, "uploads.max_file_mb");
                }
                other => panic!("expected ConfigError::Invalid, got {:?}", other),
            }
        }
    }
}
