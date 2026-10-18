//! Whether a port is free for Opencord (Phase 3 plan §5.2): tried by
//! binding it for a moment.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener, UdpSocket};

use opencord_proto::v1::{PortProtocol, PortState};

/// Whether Opencord could listen on `port`: free, taken by another program,
/// or not allowed (ports below 1024 need a capability on Linux).
pub fn probe(protocol: PortProtocol, port: u16) -> PortState {
    let result = match protocol {
        PortProtocol::Udp => UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).map(drop),
        _ => TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).map(drop),
    };
    match result {
        Ok(()) => PortState::Free,
        Err(error) if error.kind() == ErrorKind::AddrInUse => PortState::InUse,
        Err(error) if error.kind() == ErrorKind::PermissionDenied => PortState::NotPermitted,
        Err(_) => PortState::InUse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_in_use_shows_as_in_use() {
        let taken = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = taken.local_addr().unwrap().port();

        assert_eq!(probe(PortProtocol::Tcp, port), PortState::InUse);
        drop(taken);
        assert_eq!(probe(PortProtocol::Tcp, port), PortState::Free);
    }

    #[test]
    fn udp_is_probed_apart_from_tcp() {
        let taken = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
        let port = taken.local_addr().unwrap().port();

        assert_eq!(probe(PortProtocol::Udp, port), PortState::InUse);
    }
}
