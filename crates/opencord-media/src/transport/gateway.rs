//! The voice gateway WebSocket from the client's side: the URL, TLS pinned
//! to the fingerprint the main server sent, and binary envelopes.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use opencord_common::address::ServerAddress;
use opencord_proto::voice::v1 as voice;
use prost::Message as _;
use rustls::DigitallySignedStruct;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use super::TransportError;

/// Largest voice gateway frame accepted.
const MAX_FRAME: usize = 1 << 20;
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

pub type Socket = WebSocketStream<Box<dyn Io>>;

/// Where a voice gateway URL points.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayUrl {
    pub tls: bool,
    pub address: ServerAddress,
    pub path: String,
}

impl GatewayUrl {
    /// `wss://host:port/path`; a URL without a path means `/voice`. `ws://`
    /// is for tests on one machine.
    pub fn parse(url: &str) -> Result<Self, TransportError> {
        let invalid = || TransportError::Url(url.to_owned());
        let (tls, rest) = if let Some(rest) = url.strip_prefix("wss://") {
            (true, rest)
        } else if let Some(rest) = url.strip_prefix("ws://") {
            (false, rest)
        } else {
            return Err(invalid());
        };
        let (authority, path) = match rest.find('/') {
            Some(slash) => (&rest[..slash], &rest[slash..]),
            None => (rest, "/voice"),
        };
        let address = ServerAddress::parse(authority).map_err(|_| invalid())?;
        Ok(Self {
            tls,
            address,
            path: path.to_owned(),
        })
    }

    fn url(&self) -> String {
        let scheme = if self.tls { "wss" } else { "ws" };
        format!("{scheme}://{}{}", self.address, self.path)
    }
}

/// Opens the WebSocket, pinning `fingerprint` for TLS.
pub async fn open(
    url: &GatewayUrl,
    fingerprint: Option<[u8; 32]>,
) -> Result<Socket, TransportError> {
    let address = (url.address.host.as_str(), url.address.port);
    let tcp = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(address))
        .await
        .map_err(|_| TransportError::Timeout)?
        .map_err(|error| TransportError::Connect(error.to_string()))?;
    let _ = tcp.set_nodelay(true);
    let stream: Box<dyn Io> = if url.tls {
        let fingerprint = fingerprint.ok_or(TransportError::NoFingerprint)?;
        Box::new(tls(tcp, &url.address.host, fingerprint).await?)
    } else {
        Box::new(tcp)
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_FRAME))
        .max_frame_size(Some(MAX_FRAME));
    let (socket, _) = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::client_async_with_config(url.url(), stream, Some(config)),
    )
    .await
    .map_err(|_| TransportError::Timeout)?
    .map_err(|error| TransportError::Connect(error.to_string()))?;
    Ok(socket)
}

async fn tls(
    tcp: TcpStream,
    host: &str,
    fingerprint: [u8; 32],
) -> Result<tokio_rustls::client::TlsStream<TcpStream>, TransportError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(Pinned {
        fingerprint,
        algorithms: provider.signature_verification_algorithms,
    });
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| TransportError::Connect(error.to_string()))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    let name =
        ServerName::try_from(host.to_owned()).map_err(|_| TransportError::Url(host.to_owned()))?;
    TlsConnector::from(Arc::new(config))
        .connect(name, tcp)
        .await
        .map_err(|error| TransportError::Connect(error.to_string()))
}

pub async fn send(
    socket: &mut Socket,
    payload: voice::envelope::Payload,
) -> Result<(), TransportError> {
    let envelope = voice::Envelope {
        seq: 0,
        payload: Some(payload),
    };
    socket
        .send(Message::Binary(envelope.encode_to_vec().into()))
        .await
        .map_err(|error| TransportError::Connect(error.to_string()))
}

/// What came next on the socket.
pub enum Next {
    Envelope(Box<voice::Envelope>),
    /// The connection ended, with the close code if one was sent.
    Closed(Option<u16>),
}

pub async fn next(socket: &mut Socket) -> Next {
    loop {
        match socket.next().await {
            Some(Ok(Message::Binary(bytes))) => match voice::Envelope::decode(bytes) {
                Ok(envelope) => return Next::Envelope(Box::new(envelope)),
                Err(_) => return Next::Closed(None),
            },
            Some(Ok(Message::Close(frame))) => {
                return Next::Closed(frame.map(|frame| u16::from(frame.code)));
            }
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return Next::Closed(None),
        }
    }
}

/// Accepts exactly the certificate the main server vouched for.
#[derive(Debug)]
struct Pinned {
    fingerprint: [u8; 32],
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented: [u8; 32] = Sha256::digest(end_entity.as_ref()).into();
        if presented == self.fingerprint {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "the voice node's certificate is not the one the server named".to_owned(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_default_to_the_voice_path() {
        let url = GatewayUrl::parse("wss://voice1.example.com:7712").unwrap();

        assert!(url.tls);
        assert_eq!(url.address.host, "voice1.example.com");
        assert_eq!(url.address.port, 7712);
        assert_eq!(url.path, "/voice");
        assert_eq!(url.url(), "wss://voice1.example.com:7712/voice");
    }

    #[test]
    fn urls_keep_their_path_and_ipv6_hosts() {
        let url = GatewayUrl::parse("ws://[::1]:9000/custom").unwrap();

        assert!(!url.tls);
        assert_eq!(url.address.host, "::1");
        assert_eq!(url.path, "/custom");
    }

    #[test]
    fn other_schemes_are_refused() {
        assert!(GatewayUrl::parse("https://example.com/voice").is_err());
        assert!(GatewayUrl::parse("example.com:7712").is_err());
    }
}
