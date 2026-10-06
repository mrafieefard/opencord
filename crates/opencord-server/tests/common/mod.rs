//! A real server on a random port, and a protocol-level test client.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use opencord_common::address::Fingerprint;
use opencord_common::auth::{self, SigningKey};
use opencord_proto::v1 as proto;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use prost::Message as _;
use proto::envelope::Payload;
use rustls::DigitallySignedStruct;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

pub const TIMEOUT: Duration = Duration::from_secs(5);

pub struct TestServer {
    pub handle: ServerHandle,
    pub dir: tempfile::TempDir,
}

impl TestServer {
    pub async fn start() -> Self {
        Self::start_with(|_| {}).await
    }

    pub async fn start_with(customize: impl FnOnce(&mut Config)) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        customize(&mut config);
        let handle = server::start(config).await.unwrap();
        Self { handle, dir }
    }

    pub fn claim_token(&self) -> String {
        self.handle
            .claim_token
            .clone()
            .expect("an unclaimed server")
    }

    /// Plain HTTPS GET; returns the status code and body.
    pub async fn get(&self, path: &str) -> (u16, String) {
        let mut stream = tls_connect(self).await;
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        let status = response[9..12].parse().unwrap();
        let body = response
            .split_once("\r\n\r\n")
            .map(|(_, body)| body.to_owned())
            .unwrap_or_default();
        (status, body)
    }
}

pub fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

pub fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}

pub enum Received {
    Envelope(proto::Envelope),
    Closed(Option<u16>),
}

pub struct TestClient {
    ws: WebSocketStream<TlsStream<TcpStream>>,
    pub hello: proto::Hello,
    pub key: SigningKey,
    next_request_id: u64,
    events: VecDeque<(u64, proto::Event)>,
    pub last_seq: u64,
}

impl TestClient {
    /// Connects and reads `Hello`.
    pub async fn connect(server: &TestServer, key: SigningKey) -> Self {
        let tls = tls_connect(server).await;
        let url = format!(
            "wss://localhost:{}/gateway",
            server.handle.local_addr.port()
        );
        let (ws, _) = tokio_tungstenite::client_async(url, tls).await.unwrap();
        let mut client = Self {
            ws,
            hello: proto::Hello::default(),
            key,
            next_request_id: 1,
            events: VecDeque::new(),
            last_seq: 0,
        };
        match client.recv_payload().await {
            Payload::Hello(hello) => client.hello = hello,
            other => panic!("expected Hello, got {other:?}"),
        }
        client
    }

    pub fn identify_message(
        &self,
        display_name: &str,
        invite: Option<&str>,
        claim: Option<&str>,
    ) -> proto::Identify {
        let server_id: auth::ServerId = self.hello.server_id.as_slice().try_into().unwrap();
        let nonce: auth::Nonce = self.hello.nonce.as_slice().try_into().unwrap();
        let timestamp_ms = now_ms();
        proto::Identify {
            public_key: self.key.verifying_key().to_bytes().to_vec(),
            signature: auth::sign(&self.key, &server_id, &nonce, timestamp_ms).to_vec(),
            timestamp_ms,
            display_name: display_name.to_owned(),
            invite_code: invite.map(str::to_owned),
            claim_token: claim.map(str::to_owned),
            protocol_version: opencord_common::PROTOCOL_VERSION,
        }
    }

    pub async fn identify(
        &mut self,
        display_name: &str,
        invite: Option<&str>,
        claim: Option<&str>,
    ) -> Result<proto::Ready, proto::Error> {
        let identify = self.identify_message(display_name, invite, claim);
        self.send(Payload::Identify(identify)).await;
        match self.recv_payload().await {
            Payload::Ready(ready) => Ok(*ready),
            Payload::Error(error) => Err(error),
            other => panic!("expected Ready or Error, got {other:?}"),
        }
    }

    pub async fn send(&mut self, payload: Payload) {
        self.send_envelope(proto::Envelope {
            seq: 0,
            request_id: 0,
            payload: Some(payload),
        })
        .await;
    }

    pub async fn send_envelope(&mut self, envelope: proto::Envelope) {
        self.ws
            .send(Message::Binary(envelope.encode_to_vec().into()))
            .await
            .unwrap();
    }

    pub async fn recv(&mut self) -> Received {
        loop {
            let message = tokio::time::timeout(TIMEOUT, self.ws.next())
                .await
                .expect("timed out waiting for a frame");
            match message {
                Some(Ok(Message::Binary(bytes))) => {
                    return Received::Envelope(proto::Envelope::decode(bytes).unwrap());
                }
                Some(Ok(Message::Close(frame))) => {
                    return Received::Closed(frame.map(|frame| u16::from(frame.code)));
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return Received::Closed(None),
            }
        }
    }

    /// Next non-event payload; events are queued for `next_event`.
    pub async fn recv_payload(&mut self) -> Payload {
        loop {
            match self.recv().await {
                Received::Envelope(envelope) => match envelope.payload.expect("a payload") {
                    Payload::Event(event) => {
                        self.last_seq = envelope.seq;
                        self.events.push_back((envelope.seq, event));
                    }
                    payload => return payload,
                },
                Received::Closed(code) => panic!("connection closed ({code:?})"),
            }
        }
    }

    pub async fn request(&mut self, kind: proto::request::Kind) -> proto::response::Result {
        let request_id = self.next_request_id;
        self.next_request_id += 1;
        self.send_envelope(proto::Envelope {
            seq: 0,
            request_id,
            payload: Some(Payload::Request(proto::Request { kind: Some(kind) })),
        })
        .await;
        loop {
            match self.recv().await {
                Received::Envelope(envelope) => match envelope.payload.expect("a payload") {
                    Payload::Event(event) => {
                        self.last_seq = envelope.seq;
                        self.events.push_back((envelope.seq, event));
                    }
                    Payload::Response(response) if envelope.request_id == request_id => {
                        return response.result.expect("a result");
                    }
                    other => panic!("unexpected {other:?}"),
                },
                Received::Closed(code) => panic!("connection closed ({code:?})"),
            }
        }
    }

    pub async fn next_event(&mut self) -> proto::event::Kind {
        if let Some((_, event)) = self.events.pop_front() {
            return event.kind.expect("an event kind");
        }
        match self.recv().await {
            Received::Envelope(envelope) => match envelope.payload.expect("a payload") {
                Payload::Event(event) => {
                    self.last_seq = envelope.seq;
                    event.kind.expect("an event kind")
                }
                other => panic!("expected an event, got {other:?}"),
            },
            Received::Closed(code) => panic!("connection closed ({code:?})"),
        }
    }

    /// Asserts nothing arrives within `wait`.
    pub async fn expect_silence(&mut self, wait: Duration) {
        if let Some((_, event)) = self.events.pop_front() {
            panic!("unexpected queued event {event:?}");
        }
        if let Ok(message) = tokio::time::timeout(wait, self.ws.next()).await {
            panic!("expected silence, got {message:?}");
        }
    }

    /// Waits for the server to close the connection; returns the code.
    pub async fn expect_close(&mut self) -> Option<u16> {
        loop {
            match self.recv().await {
                Received::Envelope(_) => {}
                Received::Closed(code) => return code,
            }
        }
    }

    pub async fn close(mut self) {
        let _ = self.ws.close(None).await;
    }
}

/// Connects the owner with the server's claim token.
pub async fn owner(server: &TestServer) -> (TestClient, proto::Ready) {
    let mut client = TestClient::connect(server, key(1)).await;
    let ready = client
        .identify("Owner", None, Some(&server.claim_token()))
        .await
        .unwrap();
    (client, ready)
}

pub fn close_code(code: u16) -> CloseCode {
    CloseCode::from(code)
}

async fn tls_connect(server: &TestServer) -> TlsStream<TcpStream> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let verifier = Arc::new(PinnedFingerprint {
        fingerprint: server.handle.fingerprint,
        algorithms: provider.signature_verification_algorithms,
    });
    let config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();
    let tcp = TcpStream::connect(server.handle.local_addr).await.unwrap();
    TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("localhost").unwrap(), tcp)
        .await
        .unwrap()
}

#[derive(Debug)]
struct PinnedFingerprint {
    fingerprint: Fingerprint,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedFingerprint {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let presented: Fingerprint = Sha256::digest(end_entity.as_ref()).into();
        if presented == self.fingerprint {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("fingerprint mismatch".to_owned()))
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
