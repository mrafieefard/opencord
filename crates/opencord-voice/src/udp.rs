//! UDP sockets beside each other on one port.

use std::net::{Ipv6Addr, SocketAddr};

use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

/// A socket for IPv6 alone on `port`, beside the IPv4 one already there.
/// On Linux an IPv6 socket takes IPv4 too unless told otherwise, so it
/// would not bind next to the IPv4 one, and IPv6 would go unheard.
pub fn bind_v6_only(port: u16) -> std::io::Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_only_v6(true)?;
    socket.set_nonblocking(true)?;
    socket.bind(&SocketAddr::from((Ipv6Addr::UNSPECIFIED, port)).into())?;
    UdpSocket::from_std(socket.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ipv4_and_ipv6_share_a_port() {
        let v4 = UdpSocket::bind("0.0.0.0:0").await.unwrap();
        let port = v4.local_addr().unwrap().port();

        let Ok(v6) = bind_v6_only(port) else {
            eprintln!("skipped: no IPv6 here");
            return;
        };

        let client = UdpSocket::bind("[::1]:0").await.unwrap();
        client.send_to(b"hi", ("::1", port)).await.unwrap();
        let mut buffer = [0u8; 8];
        let (size, _) = v6.recv_from(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..size], b"hi");
    }
}
