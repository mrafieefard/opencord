//! Domains & network (Phase 3 plan §5, §8): the owner's requests, and the
//! protocol's messages as the app sees them.

use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use crate::api::types::{
    AcmeAccount, CertificateState, CertificateStatus, CertificateStep, CheckResult, CoreError,
    DnsProvider, Domain, DomainCheck, DomainKind, DomainStatus, DomainUpdate, MappingState,
    NetworkInfo, NetworkSettings, NetworkSettingsChanges, NoticeSeverity, PortProtocol,
    PortPurpose, PortState, PortStatus, PublicAddress, ServerNotice, TunnelKind, TunnelState,
    TunnelStatus,
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

impl Client {
    /// Starts getting a Let's Encrypt certificate; progress comes as
    /// `NetworkStatus` (plan §7).
    pub async fn request_certificate(
        &self,
        key: &str,
        domain_id: i64,
        email: Option<String>,
        agree_terms: bool,
    ) -> Result<NetworkInfo, CoreError> {
        let request = Request::RequestCertificate(proto::RequestCertificate {
            domain_id,
            email,
            agree_terms,
        });
        network_info(self.request(key, request).await?)
    }

    /// The owner's own certificate and key, in PEM (plan §7.4).
    pub async fn upload_certificate(
        &self,
        key: &str,
        domain_id: i64,
        chain_pem: String,
        key_pem: String,
    ) -> Result<NetworkInfo, CoreError> {
        let request = Request::UploadCertificate(proto::UploadCertificate {
            domain_id,
            chain_pem,
            key_pem,
        });
        network_info(self.request(key, request).await?)
    }

    /// A free subdomain: DuckDNS or deSEC, pointed here and kept pointed
    /// here by the server (plan §9.1).
    pub async fn add_free_subdomain(
        &self,
        key: &str,
        provider: String,
        name: String,
        token: String,
        email: Option<String>,
        agree_terms: bool,
    ) -> Result<NetworkInfo, CoreError> {
        let request = Request::AddFreeSubdomain(proto::AddFreeSubdomain {
            provider,
            name,
            token,
            email,
            agree_terms,
        });
        network_info(self.request(key, request).await?)
    }

    /// Starts a Cloudflare Tunnel: quick, or named with its token and
    /// hostname (plan §9.2).
    pub async fn start_tunnel(
        &self,
        key: &str,
        kind: TunnelKind,
        token: String,
        hostname: String,
    ) -> Result<NetworkInfo, CoreError> {
        let request = Request::StartTunnel(proto::StartTunnel {
            kind: match kind {
                TunnelKind::Quick => proto::TunnelKind::Quick,
                TunnelKind::Named => proto::TunnelKind::Named,
            } as i32,
            token,
            hostname,
        });
        network_info(self.request(key, request).await?)
    }

    pub async fn stop_tunnel(&self, key: &str) -> Result<NetworkInfo, CoreError> {
        network_info(
            self.request(key, Request::StopTunnel(proto::StopTunnel {}))
                .await?,
        )
    }

    /// cloudflared's last lines of output.
    pub async fn tunnel_log(&self, key: &str) -> Result<Vec<String>, CoreError> {
        match self
            .request(key, Request::FetchTunnelLog(proto::FetchTunnelLog {}))
            .await?
        {
            Response::TunnelLog(log) => Ok(log.lines),
            other => Err(unexpected(&other)),
        }
    }

    /// Has the server download cloudflared; progress comes as
    /// `NetworkStatus`.
    pub async fn install_cloudflared(&self, key: &str) -> Result<NetworkInfo, CoreError> {
        network_info(
            self.request(
                key,
                Request::InstallCloudflared(proto::InstallCloudflared {}),
            )
            .await?,
        )
    }

    /// Connects a DNS provider; an empty token disconnects it.
    pub async fn set_dns_provider(
        &self,
        key: &str,
        kind: String,
        token: String,
    ) -> Result<NetworkInfo, CoreError> {
        let request = Request::SetDnsProvider(proto::SetDnsProvider { kind, token });
        network_info(self.request(key, request).await?)
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
        certificate: certificate_from(domain.certificate.unwrap_or_default()),
    })
}

fn certificate_from(certificate: proto::CertificateStatus) -> CertificateStatus {
    let state = match proto::CertificateState::try_from(certificate.state) {
        Ok(proto::CertificateState::Requesting) => CertificateState::Requesting,
        Ok(proto::CertificateState::Valid) => CertificateState::Valid,
        Ok(proto::CertificateState::RenewalFailed) => CertificateState::RenewalFailed,
        Ok(proto::CertificateState::Failed) => CertificateState::Failed,
        Ok(proto::CertificateState::Provided) => CertificateState::Provided,
        Ok(proto::CertificateState::Expired) => CertificateState::Expired,
        _ => CertificateState::None,
    };
    let step = match proto::CertificateStep::try_from(certificate.step) {
        Ok(proto::CertificateStep::Account) => Some(CertificateStep::Account),
        Ok(proto::CertificateStep::Order) => Some(CertificateStep::Order),
        Ok(proto::CertificateStep::Challenge) => Some(CertificateStep::Challenge),
        Ok(proto::CertificateStep::Validation) => Some(CertificateStep::Validation),
        Ok(proto::CertificateStep::Issuing) => Some(CertificateStep::Issuing),
        _ => None,
    };
    CertificateStatus {
        state,
        issuer: certificate.issuer,
        not_after_ms: certificate.not_after_ms,
        test: certificate.test,
        uploaded: certificate.uploaded,
        step,
        error: certificate.error,
        retry_at_ms: certificate.retry_at_ms,
    }
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
        dns_providers: info
            .dns_providers
            .into_iter()
            .map(|provider| DnsProvider {
                kind: provider.kind,
                masked: provider.masked,
            })
            .collect(),
        acme: info
            .acme
            .map_or_else(AcmeAccount::default, |acme| AcmeAccount {
                exists: acme.exists,
                email: acme.email,
                terms_url: acme.terms_url,
            }),
        tunnel: info.tunnel.map_or_else(TunnelStatus::default, tunnel_from),
    }
}

fn tunnel_from(tunnel: proto::TunnelStatus) -> TunnelStatus {
    TunnelStatus {
        state: match proto::TunnelState::try_from(tunnel.state) {
            Ok(proto::TunnelState::Starting) => TunnelState::Starting,
            Ok(proto::TunnelState::Running) => TunnelState::Running,
            Ok(proto::TunnelState::Restarting) => TunnelState::Restarting,
            Ok(proto::TunnelState::Failed) => TunnelState::Failed,
            _ => TunnelState::Off,
        },
        kind: match proto::TunnelKind::try_from(tunnel.kind) {
            Ok(proto::TunnelKind::Quick) => Some(TunnelKind::Quick),
            Ok(proto::TunnelKind::Named) => Some(TunnelKind::Named),
            _ => None,
        },
        hostname: tunnel.hostname,
        domain_id: tunnel.domain_id,
        error: tunnel.error,
        restarts: tunnel.restarts,
        cloudflared_version: tunnel.cloudflared_version,
        installing: tunnel.installing,
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
    let mapping = mapping_state(port.mapping);
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

pub(crate) fn mapping_state(mapping: i32) -> MappingState {
    match proto::MappingState::try_from(mapping) {
        Ok(proto::MappingState::Mapped) => MappingState::Mapped,
        Ok(proto::MappingState::Failed) => MappingState::Failed,
        _ => MappingState::NotUsed,
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
