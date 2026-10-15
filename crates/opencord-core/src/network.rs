//! Domains & network (Phase 3 plan §5, §8): the owner's requests, and the
//! protocol's messages as the app sees them.

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use crate::api::types::{
    CheckResult, CoreError, Domain, DomainCheck, DomainKind, DomainStatus, DomainUpdate,
    MappingState, NetworkInfo, NetworkSettings, NetworkSettingsChanges, NoticeSeverity,
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

impl Client {
    /// Attaches a domain, pending its check (plan §6.1).
    pub async fn add_domain(
        &self,
        key: &str,
        hostname: String,
        kind: DomainKind,
        public_port: Option<u32>,
    ) -> Result<NetworkInfo, CoreError> {
        let request = Request::AddDomain(proto::AddDomain {
            hostname,
            kind: kind_to_proto(kind) as i32,
            public_port,
        });
        network_info(self.request(key, request).await?)
    }

    pub async fn update_domain(
        &self,
        key: &str,
        domain_id: i64,
        update: DomainUpdate,
    ) -> Result<NetworkInfo, CoreError> {
        let request = Request::UpdateDomain(proto::UpdateDomain {
            domain_id,
            active: update.active,
            primary: update.primary,
            public_port: update.public_port,
            confirm: update.confirm,
        });
        network_info(self.request(key, request).await?)
    }

    pub async fn delete_domain(&self, key: &str, domain_id: i64) -> Result<NetworkInfo, CoreError> {
        let request = Request::DeleteDomain(proto::DeleteDomain { domain_id });
        network_info(self.request(key, request).await?)
    }

    /// Starts a check; its result comes as `NetworkStatus`.
    pub async fn check_domain(&self, key: &str, domain_id: i64) -> Result<NetworkInfo, CoreError> {
        let request = Request::CheckDomain(proto::CheckDomain { domain_id });
        network_info(self.request(key, request).await?)
    }

    /// A domain's last checks, newest first.
    pub async fn domain_checks(
        &self,
        key: &str,
        domain_id: i64,
    ) -> Result<Vec<DomainCheck>, CoreError> {
        let request = Request::FetchDomainChecks(proto::FetchDomainChecks { domain_id });
        match self.request(key, request).await? {
            Response::DomainChecks(list) => Ok(list.checks.into_iter().map(check_from).collect()),
            other => Err(unexpected(&other)),
        }
    }
}

fn kind_to_proto(kind: DomainKind) -> proto::DomainKind {
    match kind {
        DomainKind::Own => proto::DomainKind::Own,
        DomainKind::FreeSubdomain => proto::DomainKind::FreeSubdomain,
        DomainKind::Tunnel => proto::DomainKind::Tunnel,
        DomainKind::ReverseProxy => proto::DomainKind::ReverseProxy,
    }
}

/// A domain this client understands; newer kinds are left out.
fn domain_from(domain: proto::Domain) -> Option<Domain> {
    let kind = match proto::DomainKind::try_from(domain.kind).ok()? {
        proto::DomainKind::Own => DomainKind::Own,
        proto::DomainKind::FreeSubdomain => DomainKind::FreeSubdomain,
        proto::DomainKind::Tunnel => DomainKind::Tunnel,
        proto::DomainKind::ReverseProxy => DomainKind::ReverseProxy,
        proto::DomainKind::Unspecified => return None,
    };
    let status = match proto::DomainStatus::try_from(domain.status) {
        Ok(proto::DomainStatus::Verified) => DomainStatus::Verified,
        Ok(proto::DomainStatus::DnsProblem) => DomainStatus::DnsProblem,
        _ => DomainStatus::Pending,
    };
    Some(Domain {
        id: domain.id,
        hostname: domain.hostname,
        kind,
        public_port: domain.public_port,
        status,
        active: domain.active,
        primary: domain.primary,
        problem_since_ms: domain.problem_since_ms,
        last_check: domain.last_check.map(check_from),
        checking: domain.checking,
        users_24h: domain.users_24h,
        created_at_ms: domain.created_at_ms,
        deactivated_at_ms: domain.deactivated_at_ms,
        trusted_certificate: domain.trusted_certificate,
    })
}

fn check_from(check: proto::DomainCheck) -> DomainCheck {
    DomainCheck {
        result: match proto::CheckResult::try_from(check.result) {
            Ok(proto::CheckResult::Verified) => CheckResult::Verified,
            Ok(proto::CheckResult::PointsElsewhere) => CheckResult::PointsElsewhere,
            Ok(proto::CheckResult::NoRecord) => CheckResult::NoRecord,
            Ok(proto::CheckResult::CloudflareProxied) => CheckResult::CloudflareProxied,
            Ok(proto::CheckResult::PartlyCorrect) => CheckResult::PartlyCorrect,
            Ok(proto::CheckResult::ConfirmedByOwner) => CheckResult::ConfirmedByOwner,
            _ => CheckResult::Failed,
        },
        message: check.message,
        note: check.note,
        found: check.found,
        checked_at_ms: check.checked_at_ms,
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
        domains: info.domains.into_iter().filter_map(domain_from).collect(),
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
