//! Domains & network, Phase 3 D1: the public address, the ports and the
//! owner's network settings (plan §5, §8).

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::routing::get;
use common::*;
use opencord_proto::v1 as proto;
use opencord_server::config::Config;
use opencord_server::server;
use proto::request::Kind;
use proto::response::Result as Response;

/// A stand-in for ipify, answering with whatever address it is set to.
struct Lookup {
    url: String,
    answer: Arc<Mutex<String>>,
}

async fn lookup(answer: &str) -> Lookup {
    let answer = Arc::new(Mutex::new(answer.to_owned()));
    let app = Router::new()
        .route(
            "/",
            get(|State(answer): State<Arc<Mutex<String>>>| async move {
                answer.lock().unwrap().clone()
            }),
        )
        .with_state(Arc::clone(&answer));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    Lookup { url, answer }
}

fn info(response: Response) -> proto::NetworkInfo {
    match response {
        Response::NetworkInfo(info) => *info,
        other => panic!("expected network info, got {other:?}"),
    }
}

fn address(info: &proto::NetworkInfo) -> &proto::PublicAddress {
    info.public_address.as_ref().unwrap()
}

fn port(info: &proto::NetworkInfo, purpose: proto::PortPurpose) -> &proto::PortStatus {
    info.ports
        .iter()
        .find(|port| port.purpose == purpose as i32)
        .unwrap()
}

#[tokio::test]
async fn only_the_owner_sees_the_network() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _) = join(&server, 2, &code).await;

    let error = member
        .error(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);
    let error = member
        .error(Kind::UpdateNetworkSettings(proto::UpdateNetworkSettings {
            upnp_enabled: Some(true),
            ..Default::default()
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);

    let info = info(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    assert_eq!(
        info.listening_port,
        u32::from(server.handle.local_addr.port())
    );
    let main = port(&info, proto::PortPurpose::Main);
    assert_eq!(main.state, proto::PortState::Listening as i32);
    let voice = port(&info, proto::PortPurpose::Voice);
    assert_eq!(voice.state, proto::PortState::Listening as i32);
    assert_ne!(voice.port, 0);
    let music = port(&info, proto::PortPurpose::Music);
    assert_eq!(music.state, proto::PortState::Listening as i32);
    let local = port(&info, proto::PortPurpose::LocalProxy);
    assert_eq!(local.state, proto::PortState::Off as i32);
}

#[tokio::test]
async fn the_public_address_is_detected_and_a_change_is_announced() {
    let lookup = lookup("203.0.113.10").await;
    let url = lookup.url.clone();
    let server = TestServer::start_with(|config| config.network.ip_lookup_v4 = url).await;
    let (mut owner, _) = owner(&server).await;

    let first = info(
        owner
            .ok(Kind::DetectPublicAddress(proto::DetectPublicAddress {}))
            .await,
    );
    assert_eq!(address(&first).ipv4, "203.0.113.10");
    assert_eq!(address(&first).detected_ipv4, "203.0.113.10");
    assert!(address(&first).detected_at_ms > 0);
    assert!(address(&first).detect_error.is_empty());

    owner
        .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;
    *lookup.answer.lock().unwrap() = "198.51.100.20".to_owned();
    owner
        .ok(Kind::DetectPublicAddress(proto::DetectPublicAddress {}))
        .await;

    let notice = owner
        .wait_for(|event| match event {
            proto::event::Kind::ServerNotice(notice) => Some(notice.clone()),
            _ => None,
        })
        .await;
    assert_eq!(notice.key, "network.public_address");
    assert!(notice.body.contains("198.51.100.20"), "{}", notice.body);
    assert!(notice.body.contains("203.0.113.10"), "{}", notice.body);
    let status = owner
        .wait_for(|event| match event {
            proto::event::Kind::NetworkStatus(status) => status.info.clone(),
            _ => None,
        })
        .await;
    assert_eq!(address(&status).ipv4, "198.51.100.20");
    // The music bridge tells Lavalink nodes outside the new address.
    assert_eq!(
        server.handle.music_bridge().unwrap().public_ip(),
        Some("198.51.100.20".parse().unwrap())
    );
}

#[tokio::test]
async fn a_failed_lookup_says_why_and_keeps_the_last_address() {
    let lookup = lookup("203.0.113.10").await;
    let url = lookup.url.clone();
    let server = TestServer::start_with(|config| config.network.ip_lookup_v4 = url).await;
    let (mut owner, _) = owner(&server).await;
    owner
        .ok(Kind::DetectPublicAddress(proto::DetectPublicAddress {}))
        .await;

    *lookup.answer.lock().unwrap() = "<html>busy</html>".to_owned();
    let info = info(
        owner
            .ok(Kind::DetectPublicAddress(proto::DetectPublicAddress {}))
            .await,
    );

    assert_eq!(address(&info).ipv4, "203.0.113.10");
    assert!(!address(&info).detect_error.is_empty());
}

#[tokio::test]
async fn an_override_wins_over_detection() {
    let lookup = lookup("203.0.113.10").await;
    let url = lookup.url.clone();
    let server = TestServer::start_with(|config| config.network.ip_lookup_v4 = url).await;
    let (mut owner, _) = owner(&server).await;
    owner
        .ok(Kind::DetectPublicAddress(proto::DetectPublicAddress {}))
        .await;

    let set = |v4: &str, v6: &str| {
        Kind::SetPublicAddress(proto::SetPublicAddress {
            auto_detect: false,
            override_ipv4: v4.to_owned(),
            override_ipv6: v6.to_owned(),
        })
    };
    assert_eq!(
        owner.error(set("not an address", "")).await.code,
        proto::ErrorCode::InvalidArgument as i32
    );
    assert_eq!(
        owner.error(set("127.0.0.1", "")).await.code,
        proto::ErrorCode::InvalidArgument as i32
    );
    assert_eq!(
        owner.error(set("", "203.0.113.1")).await.code,
        proto::ErrorCode::InvalidArgument as i32,
        "an IPv4 address in the IPv6 field"
    );
    let info = info(owner.ok(set("192.0.2.50", "2001:db8::50")).await);

    assert_eq!(address(&info).ipv4, "192.0.2.50");
    assert_eq!(address(&info).ipv6, "2001:db8::50");
    assert_eq!(address(&info).detected_ipv4, "203.0.113.10");
    assert!(!address(&info).auto_detect);
    let cleared = info_of(owner.ok(set("", "")).await);
    assert_eq!(address(&cleared).ipv4, "203.0.113.10");
}

fn info_of(response: Response) -> proto::NetworkInfo {
    info(response)
}

#[tokio::test]
async fn settings_are_checked_and_kept() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let bad = owner
        .error(Kind::UpdateNetworkSettings(proto::UpdateNetworkSettings {
            trusted_proxies: Some(proto::StringList {
                values: vec!["10.0.0.0/40".to_owned()],
            }),
            ..Default::default()
        }))
        .await;
    assert_eq!(bad.code, proto::ErrorCode::InvalidArgument as i32);

    let info = info(
        owner
            .ok(Kind::UpdateNetworkSettings(proto::UpdateNetworkSettings {
                trusted_proxies: Some(proto::StringList {
                    values: vec!["10.0.0.0/8".to_owned(), "::1".to_owned()],
                }),
                dns_resolvers: Some(proto::StringList {
                    values: vec!["9.9.9.9".to_owned(), "[2620:fe::fe]:5353".to_owned()],
                }),
                acme_staging: Some(true),
                ..Default::default()
            }))
            .await,
    );

    let settings = info.settings.unwrap();
    assert_eq!(settings.trusted_proxies, ["10.0.0.0/8", "::1"]);
    assert_eq!(settings.dns_resolvers, ["9.9.9.9", "[2620:fe::fe]:5353"]);
    assert!(settings.acme_staging);
}

/// A server on a data directory that outlives it.
async fn start_in(dir: &std::path::Path, port: u16) -> server::ServerHandle {
    let mut config = Config::default();
    config.server.bind = format!("127.0.0.1:{port}").parse().unwrap();
    config.server.data_dir = dir.join("data");
    config.voice.udp_port = 0;
    config.music.audio_udp_port = 0;
    config.network.ip_lookup_v4 = String::new();
    config.network.ip_lookup_v6 = String::new();
    server::start(config).await.unwrap()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test]
async fn a_new_main_port_is_used_from_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let configured = free_port();
    let handle = start_in(dir.path(), configured).await;
    let server = TestServer {
        handle,
        dir: tempfile::tempdir().unwrap(),
    };
    let (mut owner, _) = owner(&server).await;
    let chosen = free_port();
    let info = info(
        owner
            .ok(Kind::UpdateNetworkSettings(proto::UpdateNetworkSettings {
                main_port: Some(u32::from(chosen)),
                ..Default::default()
            }))
            .await,
    );
    assert_eq!(
        info.listening_port,
        u32::from(configured),
        "not before a restart"
    );
    assert_eq!(info.next_main_port, proto::PortState::Free as i32);
    owner.close().await;
    server.handle.shutdown().await;

    let restarted = start_in(dir.path(), configured).await;
    assert_eq!(restarted.local_addr.port(), chosen);
    restarted.shutdown().await;

    // Taken by something else: the configured port, so the owner can still
    // get in.
    let squatter = std::net::TcpListener::bind(("127.0.0.1", chosen)).unwrap();
    let fallback = start_in(dir.path(), configured).await;
    assert_eq!(fallback.local_addr.port(), configured);
    drop(squatter);
    fallback.shutdown().await;
}

#[tokio::test]
async fn the_local_listener_serves_plain_http_on_this_machine_only() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    owner
        .ok(Kind::UpdateNetworkSettings(proto::UpdateNetworkSettings {
            local_listener_port: Some(u32::from(free_port())),
            ..Default::default()
        }))
        .await;

    let port = server
        .handle
        .set_local_listener(true)
        .await
        .unwrap()
        .unwrap();

    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    // This machine's address on its network does not reach it.
    if let Some(lan) = lan_address() {
        assert!(
            tokio::net::TcpStream::connect((lan, port)).await.is_err(),
            "{lan}:{port} reaches the local listener"
        );
    }
    let info = info(
        owner
            .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
            .await,
    );
    let local = port_of(&info, proto::PortPurpose::LocalProxy);
    assert_eq!(local.state, proto::PortState::Listening as i32);

    assert_eq!(server.handle.set_local_listener(false).await.unwrap(), None);
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    while tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .is_ok()
    {
        assert!(tokio::time::Instant::now() < deadline, "still listening");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn port_of(info: &proto::NetworkInfo, purpose: proto::PortPurpose) -> proto::PortStatus {
    port(info, purpose).clone()
}

/// This machine's address on the route out, when it has one besides
/// loopback (nothing is sent).
fn lan_address() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:53").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback()).then_some(ip)
}
