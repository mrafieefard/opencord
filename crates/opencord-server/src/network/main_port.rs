//! The main port (D69): TLS for apps, browsers and Lavalink, and plain HTTP
//! from the tunnels and reverse proxies that took TLS off in front of it.
//! The first byte tells them apart, since a TLS connection opens with a
//! handshake record. Plain HTTP comes only from this machine, trusted
//! proxies and the ranges `plain_http_from` names.

use std::future::Future;
use std::io;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::Extension;
use axum::middleware::AddExtension;
use axum_server::accept::Accept;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tower_layer::Layer;

use super::settings::IpRange;
use crate::handshakes::WatchedAcceptor;
use crate::state::AppState;

/// The first byte of a TLS connection: a handshake record.
const TLS_HANDSHAKE: u8 = 0x16;
/// A connection that sends nothing for this long is dropped.
const FIRST_BYTE_WITHIN: Duration = Duration::from_secs(10);
/// What plain HTTP from anywhere else gets, before the connection closes.
const REFUSED: &[u8] = b"HTTP/1.1 400 Bad Request\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
Content-Length: 43\r\n\
Connection: close\r\n\
\r\n\
This port speaks HTTPS: use https:// here.\n";

/// How a request reached the main port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    Tls,
    /// Plain HTTP from a tunnel or a reverse proxy.
    Proxy,
}

/// The main port's acceptor: TLS (noting certificate refusals), or plain
/// HTTP from the addresses allowed it.
#[derive(Clone)]
pub struct MainAcceptor {
    tls: WatchedAcceptor,
    state: Arc<AppState>,
    plain_from: Arc<[IpRange]>,
}

impl MainAcceptor {
    pub fn new(tls: WatchedAcceptor, state: Arc<AppState>) -> Self {
        let plain_from = state
            .config
            .network
            .plain_http_from
            .iter()
            .filter_map(|item| IpRange::parse(item))
            .collect();
        Self {
            tls,
            state,
            plain_from,
        }
    }

    fn takes_plain(&self, ip: IpAddr) -> bool {
        takes_plain(
            ip,
            |ip| self.state.network.settings().trusts(ip),
            &self.plain_from,
        )
    }
}

/// Whether plain HTTP from `ip` is taken: from this machine, a trusted
/// proxy, or a range the config names.
pub fn takes_plain(ip: IpAddr, trusts: impl Fn(IpAddr) -> bool, ranges: &[IpRange]) -> bool {
    let ip = ip.to_canonical();
    ip.is_loopback() || trusts(ip) || ranges.iter().any(|range| range.contains(ip))
}

type Accepted<S> = io::Result<(MainStream, AddExtension<S, Via>)>;

impl<S: Send + 'static> Accept<TcpStream, S> for MainAcceptor {
    type Stream = MainStream;
    type Service = AddExtension<S, Via>;
    type Future = Pin<Box<dyn Future<Output = Accepted<S>> + Send>>;

    fn accept(&self, stream: TcpStream, service: S) -> Self::Future {
        let acceptor = self.clone();
        Box::pin(async move {
            let mut first = [0u8; 1];
            let read = tokio::time::timeout(FIRST_BYTE_WITHIN, stream.peek(&mut first))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "nothing was sent"))??;
            if read == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            if first[0] == TLS_HANDSHAKE {
                let (tls, service) = acceptor.tls.accept(stream, service).await?;
                return Ok((
                    MainStream::Tls(Box::new(tls)),
                    Extension(Via::Tls).layer(service),
                ));
            }
            let peer = stream.peer_addr()?.ip();
            if !acceptor.takes_plain(peer) {
                refuse(stream).await;
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("plain HTTP from {peer}"),
                ));
            }
            Ok((
                MainStream::Plain(stream),
                Extension(Via::Proxy).layer(service),
            ))
        })
    }
}

/// Says the port wants HTTPS, then closes.
async fn refuse(mut stream: TcpStream) {
    let _ = tokio::time::timeout(Duration::from_secs(2), async {
        stream.write_all(REFUSED).await?;
        stream.shutdown().await
    })
    .await;
}

/// A connection to the main port, with or without TLS.
pub enum MainStream {
    Tls(Box<TlsStream<TcpStream>>),
    Plain(TcpStream),
}

impl AsyncRead for MainStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
            Self::Plain(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MainStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            Self::Plain(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_write_vectored(cx, bufs),
            Self::Plain(stream) => Pin::new(stream).poll_write_vectored(cx, bufs),
        }
    }

    fn is_write_vectored(&self) -> bool {
        match self {
            Self::Tls(stream) => stream.is_write_vectored(),
            Self::Plain(stream) => stream.is_write_vectored(),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            Self::Plain(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Tls(stream) => Pin::new(stream.as_mut()).poll_shutdown(cx),
            Self::Plain(stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_http_comes_from_this_machine_trusted_proxies_and_named_ranges() {
        let ranges = [IpRange::parse("10.0.0.0/8").unwrap()];
        let trusts = |ip: IpAddr| ip == "192.0.2.9".parse::<IpAddr>().unwrap();
        let takes = |ip: &str| takes_plain(ip.parse().unwrap(), trusts, &ranges);

        assert!(takes("127.0.0.1"));
        assert!(takes("::1"));
        assert!(takes("::ffff:127.0.0.1"));
        assert!(takes("192.0.2.9"));
        assert!(takes("10.20.30.40"));
        assert!(!takes("203.0.113.5"));
        assert!(!takes("192.168.1.10"));
    }

    #[test]
    fn the_refusal_is_a_whole_http_reply() {
        let text = std::str::from_utf8(REFUSED).unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap();
        assert!(head.contains(&format!("Content-Length: {}", body.len())));
    }
}
