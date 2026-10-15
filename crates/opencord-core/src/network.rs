//! Domains & network (Phase 3 plan §5, §8): the owner's requests, and the
//! protocol's messages as the app sees them.

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use crate::api::types::{
    CoreError, MappingState, NetworkInfo, NetworkSettings, NetworkSettingsChanges, NoticeSeverity,
    PortProtocol, PortPurpose, PortState, PortStatus, PublicAddress, ServerNotice,
};
use crate::client::{Client, unexpected};

impl Client {
    /// Starts or stops network updates for this session (`NetworkStatus`
    /// events), and returns the network as it is.
    pub async fn watch_network(&self, key: &str, watch: bool) -> Result<NetworkInfo, CoreError> {
        let request = Request::WatchNetwork(proto::WatchNetwork { watch });
        network_info(self.request(key, request).await?)
    }

    /// Sets how the public address is found: detected, or overridden (an
    /// empty override is none).
    pub async fn set_public_address(
        &self,
        key: &str,
        auto_detect: bool,
        override_ipv4: String,
        override_ipv6: String,
    ) -> Result<NetworkInfo, CoreError> {
        let request = Request::SetPublicAddress(proto::SetPublicAddress {
            auto_detect,
            override_ipv4,
            override_ipv6,
        });
        network_info(self.request(key, request).await?)
    }

    pub async fn detect_public_address(&self, key: &str) -> Result<NetworkInfo, CoreError> {
        let request = Request::DetectPublicAddress(proto::DetectPublicAddress {});
        network_info(self.request(key, request).await?)
    }

    pub async fn update_network_settings(
        &self,
        key: &str,
        changes: NetworkSettingsChanges,
    ) -> Result<NetworkInfo, CoreError> {
        let list = |values: Option<Vec<String>>| values.map(|values| proto::StringList { values });
        let request = Request::UpdateNetworkSettings(proto::UpdateNetworkSettings {
            main_port: changes.main_port,
            upnp_enabled: changes.upnp_enabled,
            local_listener_port: changes.local_listener_port,
            trusted_proxies: list(changes.trusted_proxies),
            dns_resolvers: list(changes.dns_resolvers),
            acme_staging: changes.acme_staging,
        });
        network_info(self.request(key, request).await?)
    }
}

fn network_info(response: Response) -> Result<NetworkInfo, CoreError> {
    match response {
        Response::NetworkInfo(info) => Ok(info_from(*info)),
        other => Err(unexpected(&other)),
    }
}

pub fn info_from(info: proto::NetworkInfo) -> NetworkInfo {
    let address = info.public_address.unwrap_or_default();
    let settings = info.settings.unwrap_or_default();
    NetworkInfo {
        public_address: PublicAddress {
            ipv4: address.ipv4,
            ipv6: address.ipv6,
            detected_ipv4: address.detected_ipv4,
            detected_ipv6: address.detected_ipv6,
            override_ipv4: address.override_ipv4,
            override_ipv6: address.override_ipv6,
            auto_detect: address.auto_detect,
            detected_at_ms: address.detected_at_ms,
            detect_error: address.detect_error,
            has_ipv6: address.has_ipv6,
        },
        ports: info.ports.into_iter().filter_map(port).collect(),
        settings: NetworkSettings {
            main_port: settings.main_port,
            upnp_enabled: settings.upnp_enabled,
            local_listener_port: settings.local_listener_port,
            trusted_proxies: settings.trusted_proxies,
            dns_resolvers: settings.dns_resolvers,
            acme_staging: settings.acme_staging,
        },
        listening_port: info.listening_port,
        next_main_port: port_state(info.next_main_port),
        upnp_gateway: info.upnp_gateway,
        upnp_external_ip: info.upnp_external_ip,
        upnp_error: info.upnp_error,
    }
}

/// A port this client knows; newer ones are left out.
fn port(port: proto::PortStatus) -> Option<PortStatus> {
    let purpose = match proto::PortPurpose::try_from(port.purpose).ok()? {
        proto::PortPurpose::Main => PortPurpose::Main,
        proto::PortPurpose::Voice => PortPurpose::Voice,
        proto::PortPurpose::Music => PortPurpose::Music,
        proto::PortPurpose::AcmeHttp => PortPurpose::AcmeHttp,
        proto::PortPurpose::LocalProxy => PortPurpose::LocalProxy,
        proto::PortPurpose::Unspecified => return None,
    };
    let protocol = match proto::PortProtocol::try_from(port.protocol).ok()? {
        proto::PortProtocol::Udp => PortProtocol::Udp,
        _ => PortProtocol::Tcp,
    };
    let mapping = match proto::MappingState::try_from(port.mapping) {
        Ok(proto::MappingState::Mapped) => MappingState::Mapped,
        Ok(proto::MappingState::Failed) => MappingState::Failed,
        _ => MappingState::NotUsed,
    };
    Some(PortStatus {
        purpose,
        protocol,
        port: port.port,
        state: port_state(port.state),
        mapping,
        mapping_error: port.mapping_error,
    })
}

fn port_state(state: i32) -> PortState {
    match proto::PortState::try_from(state) {
        Ok(proto::PortState::Listening) => PortState::Listening,
        Ok(proto::PortState::Off) => PortState::Off,
        Ok(proto::PortState::Free) => PortState::Free,
        Ok(proto::PortState::InUse) => PortState::InUse,
        Ok(proto::PortState::NotPermitted) => PortState::NotPermitted,
        _ => PortState::Unknown,
    }
}

pub fn notice(notice: proto::ServerNotice) -> ServerNotice {
    ServerNotice {
        key: notice.key,
        severity: match proto::NoticeSeverity::try_from(notice.severity) {
            Ok(proto::NoticeSeverity::Warning) => NoticeSeverity::Warning,
            _ => NoticeSeverity::Info,
        },
        title: notice.title,
        body: notice.body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ports_are_left_out_and_states_read() {
        let info = info_from(proto::NetworkInfo {
            ports: vec![
                proto::PortStatus {
                    purpose: proto::PortPurpose::Main as i32,
                    protocol: proto::PortProtocol::Tcp as i32,
                    port: 7710,
                    state: proto::PortState::Listening as i32,
                    mapping: proto::MappingState::Failed as i32,
                    mapping_error: "table full".into(),
                },
                proto::PortStatus {
                    purpose: 99,
                    ..Default::default()
                },
            ],
            next_main_port: proto::PortState::NotPermitted as i32,
            ..Default::default()
        });

        assert_eq!(info.ports.len(), 1);
        assert_eq!(info.ports[0].purpose, PortPurpose::Main);
        assert_eq!(info.ports[0].mapping, MappingState::Failed);
        assert_eq!(info.next_main_port, PortState::NotPermitted);
    }
}
