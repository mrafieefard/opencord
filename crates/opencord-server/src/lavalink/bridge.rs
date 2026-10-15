//! Where Lavalink reaches the audio bridge (Phase 3 plan §13.1) and what
//! the node list's checklist says about it (§12.8).

use opencord_proto::v1 as proto;
use proto::PortProtocol;

use crate::state::AppState;

/// The bridge's address, as a Lavalink voice state's `endpoint` carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeAddress {
    pub host: String,
    pub port: u16,
    /// Java checks the certificate: an authority's, a tunnel's or a proxy's.
    pub trusted: bool,
}

impl BridgeAddress {
    /// `host:port`, Lavalink's `endpoint`.
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn url(&self) -> String {
        format!("wss://{}", self.endpoint())
    }
}

/// The bridge domain (`[music] bridge_domain`): the primary domain, or the
/// one named; without domains, the configured host on the main port, whose
/// self-signed certificate only a Lavalink that imported it trusts.
pub fn address(state: &AppState) -> BridgeAddress {
    let wanted = state.config.music.bridge_domain.trim().to_ascii_lowercase();
    let domain = if wanted.is_empty() || wanted == "primary" {
        state.domains.primary()
    } else {
        state
            .domains
            .all()
            .into_iter()
            .find(|domain| domain.active && domain.hostname == wanted)
    };
    match domain {
        Some(domain) => BridgeAddress {
            trusted: domain.trusted_certificate(state),
            host: domain.hostname,
            port: domain.public_port,
        },
        None => BridgeAddress {
            host: if wanted.is_empty() || wanted == "primary" {
                state.config.server.public_host.to_ascii_lowercase()
            } else {
                wanted
            },
            port: state
                .config
                .server
                .public_port
                .unwrap_or_else(|| state.network.main_port()),
            trusted: false,
        },
    }
}

/// Where Lavalink sends audio: the public address and the music port.
pub fn audio_address(state: &AppState) -> Option<(String, u16)> {
    let port = state.music_bridge.as_ref()?.udp_port();
    let ip = state.network.current_public_ips().v4?;
    Some((ip.to_string(), port))
}

pub fn checklist(state: &AppState) -> proto::LavalinkBridge {
    let Some(bridge) = &state.music_bridge else {
        return proto::LavalinkBridge::default();
    };
    let address = address(state);
    let port = bridge.udp_port();
    proto::LavalinkBridge {
        enabled: true,
        url: address.url(),
        trusted_certificate: address.trusted,
        audio_address: audio_address(state)
            .map(|(ip, port)| format!("{ip}:{port}"))
            .unwrap_or_default(),
        audio_port: u32::from(port),
        audio_listening: true,
        mapping: state.network.mapping(PortProtocol::Udp, port) as i32,
    }
}
