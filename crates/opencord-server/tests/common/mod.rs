//! A real server on a random port, and a protocol-level test client.

#![allow(dead_code)]

pub mod domains;
pub use opencord_server::testing::fake_lavalink;
pub mod music;
pub mod pebble;

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
use rustls::pki_types::ServerName;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;

/// The longest to wait for a frame the server owes. Generous: on Windows a
/// connection to a closed local port takes about 2 s to be refused, and a
/// request that tries several (a failing storage test) answers after that.
pub const TIMEOUT: Duration = Duration::from_secs(15);

pub struct TestServer {
    pub handle: ServerHandle,
    pub dir: tempfile::TempDir,
}

impl TestServer {
    pub async fn start() -> Self {
        Self::start_with(|_| {}).await
    }

    pub async fn start_with(customize: impl FnOnce(&mut Config)) -> Self {
        Self::start_in(tempfile::tempdir().unwrap(), customize).await
    }

    /// Stops the server and starts it again on the same data.
    pub async fn restart(self, customize: impl FnOnce(&mut Config)) -> Self {
        self.handle.shutdown().await;
        Self::start_in(self.dir, customize).await
    }

    async fn start_in(dir: tempfile::TempDir, customize: impl FnOnce(&mut Config)) -> Self {
        // The server's warnings and errors, shown with a failing test's
        // output (RUST_LOG for more).
        static LOG: std::sync::Once = std::sync::Once::new();
        LOG.call_once(|| {
            let filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_test_writer()
                .try_init();
        });
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        config.voice.udp_port = 0;
        config.music.audio_udp_port = 0;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
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
        self.get_with(path, &[]).await
    }

    /// HTTPS GET with extra headers; returns the status code and body.
    pub async fn get_with(&self, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
        self.get_as(path, "localhost", headers).await
    }

    /// HTTPS GET as if to `host` (its `Host` header).
    pub async fn get_as(&self, path: &str, host: &str, headers: &[(&str, &str)]) -> (u16, String) {
        let mut stream = tls_connect(self).await;
        let extra: String = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect();
        let request =
            format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n{extra}\r\n");
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

    /// An HTTPS request with a binary body; returns the status code and
    /// the body's bytes.
    pub async fn send_http(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> (u16, Vec<u8>) {
        let mut stream = tls_connect(self).await;
        let extra: String = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect();
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
             Content-Length: {}\r\n{extra}\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).await.unwrap();
        // A server that refuses early may stop reading the body.
        let _ = stream.write_all(body).await;
        // TLS may still hold the body's tail until flushed.
        let _ = stream.flush().await;
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response).await;
        let status = std::str::from_utf8(&response[9..12])
            .unwrap()
            .parse()
            .unwrap();
        let start = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map_or(response.len(), |at| at + 4);
        (status, response[start..].to_vec())
    }

    /// An HTTPS request with a binary body; returns the status code,
    /// headers, and the body's bytes.
    pub async fn send_http_full(
        &self,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> (u16, Vec<(String, String)>, Vec<u8>) {
        let mut stream = tls_connect(self).await;
        let extra: String = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect();
        let head = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
             Content-Length: {}\r\n{extra}\r\n",
            body.len()
        );
        stream.write_all(head.as_bytes()).await.unwrap();
        let _ = stream.write_all(body).await;
        let _ = stream.flush().await;
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response).await;
        let status = std::str::from_utf8(&response[9..12])
            .unwrap()
            .parse()
            .unwrap();
        let header_end = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap_or(response.len());
        let header_str = std::str::from_utf8(&response[..header_end]).unwrap_or_default();
        let mut parsed_headers = Vec::new();
        for line in header_str.lines().skip(1) {
            if let Some((k, v)) = line.split_once(':') {
                parsed_headers.push((k.trim().to_lowercase(), v.trim().to_owned()));
            }
        }
        let body_start = if header_end + 4 <= response.len() {
            header_end + 4
        } else {
            response.len()
        };
        (status, parsed_headers, response[body_start..].to_vec())
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

#[allow(clippy::large_enum_variant)]
pub enum Received {
    Envelope(proto::Envelope),
    Closed(Option<u16>),
}

pub struct TestClient {
    ws: WebSocketStream<TlsStream<TcpStream>>,
    pub hello: proto::Hello,
    pub key: SigningKey,
    /// Fingerprint of the certificate this connection verified; the
    /// `Identify` signature covers it.
    pub certificate: Fingerprint,
    next_request_id: u64,
    events: VecDeque<(u64, proto::Event)>,
    pub last_seq: u64,
}

impl TestClient {
    /// Connects and reads `Hello`.
    pub async fn connect(server: &TestServer, key: SigningKey) -> Self {
        Self::connect_as(server, key, "localhost").await
    }

    /// Connects as if through `host` (the URL's, so its `Host` header).
    pub async fn connect_as(server: &TestServer, key: SigningKey, host: &str) -> Self {
        let tls = tls_connect(server).await;
        let url = format!("wss://{host}:{}/gateway", server.handle.local_addr.port());
        let (ws, _) = tokio_tungstenite::client_async(url, tls).await.unwrap();
        let mut client = Self {
            ws,
            hello: proto::Hello::default(),
            key,
            certificate: server.handle.fingerprint,
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
        let challenge = auth::Challenge {
            server_id: self.hello.server_id.as_slice().try_into().unwrap(),
            certificate: self.certificate,
            nonce: self.hello.nonce.as_slice().try_into().unwrap(),
            timestamp_ms: now_ms(),
        };
        proto::Identify {
            public_key: self.key.verifying_key().to_bytes().to_vec(),
            signature: auth::sign(&self.key, &challenge).to_vec(),
            timestamp_ms: challenge.timestamp_ms,
            display_name: display_name.to_owned(),
            invite_code: invite.map(str::to_owned),
            claim_token: claim.map(str::to_owned),
            protocol_version: opencord_common::PROTOCOL_VERSION,
            host: String::new(),
            host_signature: Vec::new(),
        }
    }

    /// An `Identify` as a client sends one that verified the server by an
    /// authority under `host` and saw `certificate` (a proxy's).
    pub fn identify_through(
        &self,
        display_name: &str,
        invite: Option<&str>,
        host: &str,
        certificate: Fingerprint,
    ) -> proto::Identify {
        let challenge = |certificate| auth::Challenge {
            server_id: self.hello.server_id.as_slice().try_into().unwrap(),
            certificate,
            nonce: self.hello.nonce.as_slice().try_into().unwrap(),
            timestamp_ms: now_ms(),
        };
        let signed = challenge(certificate);
        proto::Identify {
            public_key: self.key.verifying_key().to_bytes().to_vec(),
            signature: auth::sign(&self.key, &signed).to_vec(),
            timestamp_ms: signed.timestamp_ms,
            display_name: display_name.to_owned(),
            invite_code: invite.map(str::to_owned),
            claim_token: None,
            protocol_version: opencord_common::PROTOCOL_VERSION,
            host: host.to_owned(),
            host_signature: auth::sign(&self.key, &challenge(auth::host_binding(host))).to_vec(),
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

    /// Every event that arrives within `wait`.
    pub async fn collect_for(&mut self, wait: Duration) -> Vec<proto::event::Kind> {
        let deadline = tokio::time::Instant::now() + wait;
        let mut seen = Vec::new();
        while let Ok(event) = tokio::time::timeout_at(deadline, self.next_event()).await {
            seen.push(event);
        }
        seen
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

/// Connects the owner after the server was already claimed.
pub async fn owner_again(server: &TestServer) -> (TestClient, proto::Ready) {
    let mut client = TestClient::connect(server, key(1)).await;
    let ready = client.identify("Owner", None, None).await.unwrap();
    (client, ready)
}

pub fn close_code(code: u16) -> CloseCode {
    CloseCode::from(code)
}

/// A WebSocket to `path` on the server.
pub async fn websocket(
    server: &TestServer,
    path: &str,
) -> Result<WebSocketStream<TlsStream<TcpStream>>, tokio_tungstenite::tungstenite::Error> {
    let tls = tls_connect(server).await;
    let url = format!("wss://localhost:{}{path}", server.handle.local_addr.port());
    tokio_tungstenite::client_async(url, tls)
        .await
        .map(|(ws, _)| ws)
}

pub async fn tls_connect(server: &TestServer) -> TlsStream<TcpStream> {
    let config = opencord_server::tls::client_config(Some(server.handle.fingerprint)).unwrap();
    let tcp = TcpStream::connect(server.handle.local_addr).await.unwrap();
    TlsConnector::from(Arc::new(config))
        .connect(ServerName::try_from("localhost").unwrap(), tcp)
        .await
        .unwrap()
}

impl TestClient {
    /// The oldest event already read but not yet consumed.
    pub fn queued_event(&mut self) -> Option<(u64, proto::event::Kind)> {
        self.events
            .pop_front()
            .map(|(seq, event)| (seq, event.kind.expect("an event kind")))
    }

    /// Sends a request that must succeed.
    pub async fn ok(&mut self, kind: proto::request::Kind) -> proto::response::Result {
        match self.request(kind).await {
            proto::response::Result::Error(error) => panic!("request failed: {error:?}"),
            result => result,
        }
    }

    /// Like [`Self::ok`], but waits out the rate limit as the app does, for
    /// tests that send many requests at once without testing the limit.
    pub async fn ok_paced(&mut self, kind: proto::request::Kind) -> proto::response::Result {
        loop {
            match self.request(kind.clone()).await {
                proto::response::Result::Error(error)
                    if error.code == proto::ErrorCode::RateLimited as i32 =>
                {
                    let wait = error.retry_after_ms.unwrap_or(100);
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
                proto::response::Result::Error(error) => panic!("request failed: {error:?}"),
                result => return result,
            }
        }
    }

    /// Sends a request that must fail; returns the error.
    pub async fn error(&mut self, kind: proto::request::Kind) -> proto::Error {
        match self.request(kind).await {
            proto::response::Result::Error(error) => error,
            result => panic!("expected an error, got {result:?}"),
        }
    }

    /// Skips events until `pick` returns something.
    pub async fn wait_for<T>(
        &mut self,
        mut pick: impl FnMut(&proto::event::Kind) -> Option<T>,
    ) -> T {
        loop {
            let event = self.next_event().await;
            if let Some(found) = pick(&event) {
                return found;
            }
        }
    }

    /// Reads events for `wait` and fails if any matches.
    pub async fn assert_no_event(
        &mut self,
        wait: Duration,
        matches: impl Fn(&proto::event::Kind) -> bool,
    ) {
        while let Some((_, event)) = self.events.pop_front() {
            let kind = event.kind.expect("an event kind");
            assert!(!matches(&kind), "unexpected event {kind:?}");
        }
        let deadline = tokio::time::Instant::now() + wait;
        while let Ok(message) = tokio::time::timeout_at(deadline, self.ws.next()).await {
            let Some(Ok(Message::Binary(bytes))) = message else {
                continue;
            };
            let envelope = proto::Envelope::decode(bytes).unwrap();
            if let Some(Payload::Event(event)) = envelope.payload {
                self.last_seq = envelope.seq;
                let kind = event.kind.expect("an event kind");
                assert!(!matches(&kind), "unexpected event {kind:?}");
            }
        }
    }
}

/// Creates an invite as `client` and returns its code.
pub async fn invite(client: &mut TestClient, max_uses: Option<u32>) -> String {
    match client
        .ok(proto::request::Kind::CreateInvite(proto::CreateInvite {
            max_uses,
            expires_in_s: None,
            domain_id: None,
        }))
        .await
    {
        proto::response::Result::Invite(invite) => invite.code,
        other => panic!("expected an invite, got {other:?}"),
    }
}

/// Joins with an invite as the key with `seed`.
pub async fn join(server: &TestServer, seed: u8, invite_code: &str) -> (TestClient, proto::Ready) {
    let mut client = TestClient::connect(server, key(seed)).await;
    let ready = client
        .identify(&format!("user{seed}"), Some(invite_code), None)
        .await
        .unwrap();
    (client, ready)
}

pub fn channel_id(ready: &proto::Ready, name: &str) -> i64 {
    ready
        .channels
        .iter()
        .find(|channel| channel.name == name)
        .unwrap_or_else(|| panic!("no channel named {name}"))
        .id
}

pub fn self_id(ready: &proto::Ready) -> i64 {
    ready.self_user.as_ref().unwrap().id
}

pub fn decode_response(bytes: &[u8]) -> proto::response::Result {
    let response = proto::Response::decode(bytes).expect("valid Response proto");
    response.result.expect("a response result")
}

pub fn png(width: u32, height: u32) -> Vec<u8> {
    use image::{ImageBuffer, Rgb};
    let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(width, height, Rgb([128, 128, 128]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}

pub async fn upload(
    client: &mut TestClient,
    server: &TestServer,
    token: &str,
    channel_id: i64,
    name: &str,
    bytes: &[u8],
) -> proto::Attachment {
    let result = client
        .ok(proto::request::Kind::CreateUpload(proto::CreateUpload {
            channel_id,
            filename: name.to_owned(),
            size_bytes: bytes.len() as u64,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let upload = match result {
        proto::response::Result::Upload(upload) => upload,
        other => panic!("expected Upload response, got {other:?}"),
    };
    let piece_bytes = upload.piece_bytes as usize;
    if bytes.is_empty() {
        let (status, _) = server
            .send_http(
                "PUT",
                &format!("/media/uploads/{}", upload.upload_id),
                &[
                    ("Authorization", &format!("Bearer {token}")),
                    ("Upload-Offset", "0"),
                ],
                &[],
            )
            .await;
        assert_eq!(status, 200);
    } else {
        for (chunk_idx, chunk) in bytes.chunks(piece_bytes).enumerate() {
            let offset = (chunk_idx * piece_bytes) as u64;
            let (status, _) = server
                .send_http(
                    "PUT",
                    &format!("/media/uploads/{}", upload.upload_id),
                    &[
                        ("Authorization", &format!("Bearer {token}")),
                        ("Upload-Offset", &offset.to_string()),
                    ],
                    chunk,
                )
                .await;
            assert_eq!(status, 200);
        }
    }
    let (status, _) = server
        .send_http(
            "POST",
            &format!("/media/uploads/{}/finish", upload.upload_id),
            &[("Authorization", &format!("Bearer {token}"))],
            &[],
        )
        .await;
    assert_eq!(status, 202);
    client
        .wait_for(|kind| match kind {
            proto::event::Kind::UploadReady(ready) => ready.attachment.clone(),
            _ => None,
        })
        .await
}

pub fn voice_ogg(seconds_tone: f32, seconds_silence: f32) -> Vec<u8> {
    use opencord_common::sound::ogg;
    let mut encoder =
        opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Voip).unwrap();
    let tone_samples = (seconds_tone * 48_000.0).round() as usize;
    let silence_samples = (seconds_silence * 48_000.0).round() as usize;
    let total_samples = tone_samples + silence_samples;
    let mut pcm = Vec::with_capacity(total_samples);
    for t in 0..tone_samples {
        let sample = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * (t as f32) / 48_000.0).sin();
        pcm.push(sample);
    }
    pcm.resize(total_samples, 0.0);

    let frame_size = 960; // 20 ms at 48 kHz
    let mut packets = Vec::new();
    let mut out = vec![0u8; 1000];
    for chunk in pcm.chunks(frame_size) {
        let mut frame = chunk.to_vec();
        frame.resize(frame_size, 0.0);
        let len = encoder.encode_float(&frame, &mut out).unwrap();
        packets.push(out[..len].to_vec());
    }
    ogg::write(1, 0, &packets, total_samples as u64)
}

pub fn media_token(ready: &proto::Ready) -> String {
    ready.media_token.as_ref().unwrap().token.clone()
}

/// A private messages device: its id, its Ed25519 key and a stand-in
/// Curve25519 key (the server never uses it).
pub struct TestDevice {
    pub id: [u8; 16],
    pub ed: SigningKey,
    pub curve: [u8; 32],
}

impl TestDevice {
    pub fn new(seed: u8) -> Self {
        Self {
            id: [seed; 16],
            ed: SigningKey::from_bytes(&[seed ^ 0xa5; 32]),
            curve: [seed; 32],
        }
    }

    fn seed(&self) -> u8 {
        self.id[0]
    }
}

/// The device's keys, signed by `identity`.
pub fn device_keys(identity: &SigningKey, device: &TestDevice) -> proto::DeviceKeys {
    let ed25519 = device.ed.verifying_key().to_bytes();
    proto::DeviceKeys {
        device_id: device.id.to_vec(),
        curve25519: device.curve.to_vec(),
        ed25519: ed25519.to_vec(),
        signature: opencord_common::private::sign_device(
            identity,
            &device.id,
            &device.curve,
            &ed25519,
        )
        .to_vec(),
    }
}

/// A one-time key (`fallback: false`) or a fallback key, signed by the
/// device.
pub fn prekey(device: &TestDevice, key: [u8; 32], fallback: bool) -> proto::Prekey {
    proto::Prekey {
        key: key.to_vec(),
        signature: opencord_common::private::sign_prekey(&device.ed, &device.id, &key, fallback)
            .to_vec(),
    }
}

/// `granter`'s permission slip for `grantee`.
pub fn slip(granter: &SigningKey, grantee: &[u8; 32]) -> Vec<u8> {
    opencord_common::private::sign_slip(granter, grantee).to_vec()
}

/// The device's `i`th one-time key, as `register_device` uploads it.
pub fn one_time_key(device: &TestDevice, i: u8) -> [u8; 32] {
    let mut key = [0; 32];
    key[0] = device.seed();
    key[1] = i;
    key
}

/// The device's fallback key, as `register_device` uploads it.
pub fn fallback_key(device: &TestDevice) -> [u8; 32] {
    one_time_key(device, 255)
}

/// A valid `RegisterDevice` with `one_time_keys` keys.
pub fn register_request(
    identity: &SigningKey,
    device: &TestDevice,
    one_time_keys: u8,
) -> proto::request::Kind {
    proto::request::Kind::RegisterDevice(proto::RegisterDevice {
        device: Some(device_keys(identity, device)),
        one_time_keys: (0..one_time_keys)
            .map(|i| prekey(device, one_time_key(device, i), false))
            .collect(),
        fallback_key: Some(prekey(device, fallback_key(device), true)),
    })
}

impl TestClient {
    /// Registers `device` for this session, with `one_time_keys` keys.
    pub async fn register_device(
        &mut self,
        identity: &SigningKey,
        device: &TestDevice,
        one_time_keys: u8,
    ) -> proto::DeviceRegistration {
        match self
            .ok(register_request(identity, device, one_time_keys))
            .await
        {
            proto::response::Result::DeviceRegistration(registration) => registration,
            other => panic!("expected a DeviceRegistration, got {other:?}"),
        }
    }
}

/// A private message copy from a made-up sender, with a body of `body_len`
/// bytes.
pub fn private_message_kind(body_len: usize) -> proto::pending_event::Kind {
    proto::pending_event::Kind::PrivateMessage(proto::PrivateMessage {
        sender: Some(proto::User {
            id: 4_242,
            public_key: vec![42; 32],
            display_name: "sender".to_owned(),
            avatar_hash: String::new(),
        }),
        sender_device_id: vec![42; 16],
        olm_type: 1,
        body: vec![7; body_len],
        sent_at_ms: 1,
    })
}
