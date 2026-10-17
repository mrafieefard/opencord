//! Opening ports on a home router with UPnP IGD (Phase 3 plan §5.2): off
//! unless the owner turns it on. Mappings last an hour and are renewed
//! every half hour; they are removed when it is turned off and on shutdown.

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use igd_next::aio::Gateway;
use igd_next::aio::tokio::Tokio;
use igd_next::{PortMappingProtocol, SearchOptions};
use opencord_proto::v1::{MappingState, PortProtocol};

/// How long a mapping lasts unless renewed.
const LEASE_SECONDS: u32 = 3600;
pub const RENEW_EVERY: Duration = Duration::from_secs(30 * 60);
/// Finding the router: it answers within a second or two, or never.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(3);
const DESCRIPTION: &str = "Opencord";

/// What UPnP achieved last.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpnpStatus {
    /// The router found, and the public address it reports.
    pub gateway: Option<SocketAddr>,
    pub external_ip: Option<IpAddr>,
    /// Why nothing could be mapped, if so.
    pub error: Option<String>,
    /// Each port tried, and why it failed when it did.
    pub mappings: BTreeMap<(Protocol, u16), Result<(), String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl From<Protocol> for PortMappingProtocol {
    fn from(protocol: Protocol) -> Self {
        match protocol {
            Protocol::Tcp => Self::TCP,
            Protocol::Udp => Self::UDP,
        }
    }
}

impl From<PortProtocol> for Protocol {
    fn from(protocol: PortProtocol) -> Self {
        match protocol {
            PortProtocol::Udp => Self::Udp,
            _ => Self::Tcp,
        }
    }
}

impl UpnpStatus {
    /// How a port's mapping stands, and why it failed.
    pub fn mapping(&self, protocol: PortProtocol, port: u16) -> (MappingState, String) {
        match self.mappings.get(&(protocol.into(), port)) {
            None => (MappingState::NotUsed, String::new()),
            Some(Ok(())) => (MappingState::Mapped, String::new()),
            Some(Err(error)) => (MappingState::Failed, error.clone()),
        }
    }
}

#[derive(Debug, Default)]
pub struct Upnp {
    status: Mutex<UpnpStatus>,
}

impl Upnp {
    pub fn status(&self) -> UpnpStatus {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, status: UpnpStatus) {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status;
    }

    /// Maps `ports` on the router to this machine, or renews them.
    pub async fn map(&self, ports: &[(Protocol, u16)]) {
        let gateway = match find_gateway().await {
            Ok(gateway) => gateway,
            Err(error) => {
                self.set(UpnpStatus {
                    error: Some(error),
                    ..UpnpStatus::default()
                });
                return;
            }
        };
        let Some(local) = local_address_towards(gateway.addr.ip()) else {
            self.set(UpnpStatus {
                gateway: Some(gateway.addr),
                error: Some("no address of this machine reaches the router".to_owned()),
                ..UpnpStatus::default()
            });
            return;
        };
        let mut mappings = BTreeMap::new();
        for (protocol, port) in ports {
            let result = gateway
                .add_port(
                    (*protocol).into(),
                    *port,
                    SocketAddr::new(local, *port),
                    LEASE_SECONDS,
                    DESCRIPTION,
                )
                .await
                .map_err(|error| error.to_string());
            if let Err(error) = &result {
                tracing::warn!(port, ?protocol, %error, "UPnP could not map a port");
            }
            mappings.insert((*protocol, *port), result);
        }
        self.set(UpnpStatus {
            gateway: Some(gateway.addr),
            external_ip: gateway.get_external_ip().await.ok(),
            error: None,
            mappings,
        });
    }

    /// Removes what [`Self::map`] mapped.
    pub async fn unmap(&self) {
        let status = self.status();
        let mapped: Vec<(Protocol, u16)> = status
            .mappings
            .iter()
            .filter(|(_, result)| result.is_ok())
            .map(|(key, _)| *key)
            .collect();
        self.set(UpnpStatus::default());
        if mapped.is_empty() {
            return;
        }
        let Ok(gateway) = find_gateway().await else {
            return;
        };
        for (protocol, port) in mapped {
            let _ = gateway.remove_port(protocol.into(), port).await;
        }
    }
}

async fn find_gateway() -> Result<Gateway<Tokio>, String> {
    let mut options = SearchOptions::default();
    options.timeout = Some(SEARCH_TIMEOUT);
    igd_next::aio::tokio::search_gateway(options)
        .await
        .map_err(|error| format!("no UPnP router answered: {error}"))
}

/// This machine's address on the route to `ip`.
fn local_address_towards(ip: IpAddr) -> Option<IpAddr> {
    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((ip, 9)).ok()?;
    Some(socket.local_addr().ok()?.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Needs a UPnP router on the network: maps a spare port, sees it
    /// mapped, and removes it again.
    #[tokio::test]
    #[ignore = "needs a UPnP router"]
    async fn a_real_router_maps_and_unmaps() {
        let upnp = Upnp::default();
        upnp.map(&[(Protocol::Udp, 47_123)]).await;
        let status = upnp.status();
        println!("{status:?}");
        assert_eq!(status.error, None);
        assert_eq!(
            status.mapping(PortProtocol::Udp, 47_123).0,
            MappingState::Mapped
        );
        upnp.unmap().await;
        assert!(upnp.status().mappings.is_empty());
    }

    #[test]
    fn a_ports_mapping_reads_from_the_status() {
        let mut status = UpnpStatus::default();
        status.mappings.insert((Protocol::Tcp, 7710), Ok(()));
        status
            .mappings
            .insert((Protocol::Udp, 7711), Err("conflict".to_owned()));

        assert_eq!(
            status.mapping(PortProtocol::Tcp, 7710).0,
            MappingState::Mapped
        );
        assert_eq!(
            status.mapping(PortProtocol::Udp, 7711),
            (MappingState::Failed, "conflict".to_owned())
        );
        assert_eq!(
            status.mapping(PortProtocol::Udp, 7712).0,
            MappingState::NotUsed
        );
    }
}
