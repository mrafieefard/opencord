//! Lavalink nodes, Phase 3 L1 (plan §12): added and managed by members who
//! manage the server, connected with their sessions kept, judged by their
//! health, the best one picked for a channel, and the connection test
//! following a player to the audio bridge. Against a fake Lavalink;
//! `docs/music.md` runs the same against a real one.

mod common;

use std::time::{Duration, Instant};

use common::fake_lavalink::{FakeLavalink, Voice};
use common::*;
use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Response;
use proto::{LavalinkHealth as Health, LavalinkTestStepState as Step};

const PASSWORD: &str = "youshallnotpass";

fn settings(name: &str, port: u16) -> proto::LavalinkNodeSettings {
    proto::LavalinkNodeSettings {
        name: name.to_owned(),
        host: "127.0.0.1".to_owned(),
        port: u32::from(port),
        secure: false,
        priority: proto::LavalinkPriority::Normal as i32,
        max_players: 0,
        strict_address: true,
        enabled: true,
    }
}

fn nodes(response: Response) -> proto::LavalinkNodes {
    match response {
        Response::LavalinkNodes(nodes) => *nodes,
        other => panic!("expected the nodes, got {other:?}"),
    }
}

async fn add(owner: &mut TestClient, settings: proto::LavalinkNodeSettings, password: &str) -> i64 {
    let name = settings.name.clone();
    let list = nodes(
        owner
            .ok(Kind::CreateLavalinkNode(proto::CreateLavalinkNode {
                settings: Some(settings),
                password: password.to_owned(),
            }))
            .await,
    );
    list.nodes
        .iter()
        .find(|node| node.settings.as_ref().unwrap().name == name)
        .unwrap()
        .id
}

async fn fetch(owner: &mut TestClient) -> proto::LavalinkNodes {
    nodes(
        owner
            .ok(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
                watch: false,
            }))
            .await,
    )
}

/// Asks until node `id` matches, for at most `within`.
async fn until(
    owner: &mut TestClient,
    id: i64,
    within: Duration,
    matches: impl Fn(&proto::LavalinkNode) -> bool,
) -> proto::LavalinkNode {
    let deadline = Instant::now() + within;
    loop {
        let list = fetch(owner).await;
        let node = list.nodes.into_iter().find(|node| node.id == id).unwrap();
        if matches(&node) {
            return node;
        }
        assert!(Instant::now() < deadline, "node never matched: {node:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn healthy(owner: &mut TestClient, id: i64) -> proto::LavalinkNode {
    until(owner, id, Duration::from_secs(5), |node| {
        node.health == Health::Healthy as i32
    })
    .await
}

async fn update(owner: &mut TestClient, request: proto::UpdateLavalinkNode) {
    owner.ok(Kind::UpdateLavalinkNode(request)).await;
}

/// Runs the connection test and returns its steps once done.
async fn run_test(owner: &mut TestClient, id: i64) -> Vec<proto::LavalinkTestStep> {
    owner
        .ok(Kind::TestLavalinkNode(proto::TestLavalinkNode {
            node_id: id,
        }))
        .await;
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        assert!(Instant::now() < deadline, "the test never finished");
        // Short waits: the bridge step may be quiet for 10 s.
        let event = tokio::time::timeout(Duration::from_secs(1), owner.next_event()).await;
        if let Ok(proto::event::Kind::LavalinkTestProgress(progress)) = event
            && progress.node_id == id
            && progress.done
        {
            return progress.steps;
        }
    }
}

fn step(
    steps: &[proto::LavalinkTestStep],
    kind: proto::LavalinkTestStepKind,
) -> &proto::LavalinkTestStep {
    steps.iter().find(|step| step.kind == kind as i32).unwrap()
}

#[tokio::test]
async fn a_node_connects_keeps_its_password_secret_and_needs_manage_server() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let id = add(&mut owner, settings("Home", lavalink.port()), PASSWORD).await;

    let node = healthy(&mut owner, id).await;
    assert_eq!(node.version, "4.2.2");
    assert_eq!(node.sources, ["youtube", "soundcloud"]);
    assert_eq!(node.plugins, ["youtube-plugin 1.0.0"]);
    assert!(node.connected_since_ms > 0);

    let handshake = lavalink.fake.handshakes.lock().unwrap()[0].clone();
    assert_eq!(handshake["Authorization"], PASSWORD);
    assert!(
        handshake["User-Id"]
            .to_str()
            .unwrap()
            .parse::<i64>()
            .unwrap()
            > 0
    );
    assert!(
        handshake["Client-Name"]
            .to_str()
            .unwrap()
            .starts_with("Opencord/")
    );
    assert!(
        handshake.get("Session-Id").is_none(),
        "nothing to resume yet"
    );
    assert_eq!(
        lavalink.fake.session_updates.lock().unwrap()[0],
        serde_json::json!({ "resuming": true, "timeout": 60 })
    );

    let list = fetch(&mut owner).await;
    assert!(
        !format!("{list:?}").contains(PASSWORD),
        "never sent to clients"
    );
    let bridge = list.bridge.unwrap();
    assert!(bridge.enabled);
    assert!(bridge.url.starts_with("wss://localhost:"), "{}", bridge.url);
    assert!(!bridge.trusted_certificate, "self-signed without a domain");
    assert!(bridge.audio_listening);

    // A member who does not manage the server sees nothing.
    let code = invite(&mut owner, None).await;
    let (mut member, _) = join(&server, 2, &code).await;
    let error = member
        .error(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
            watch: true,
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);

    // At rest the password is sealed.
    let data = server.dir.path().join("data");
    for file in ["opencord.db", "opencord.db-wal"] {
        if let Ok(bytes) = std::fs::read(data.join(file)) {
            assert!(
                !bytes
                    .windows(PASSWORD.len())
                    .any(|window| window == PASSWORD.as_bytes()),
                "{file} holds the password"
            );
        }
    }
}

#[tokio::test]
async fn wrong_passwords_unreachable_hosts_and_tls_mismatches_are_explained() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let closed_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let wrong = add(&mut owner, settings("Wrong", lavalink.port()), "nope").await;
    let away = add(&mut owner, settings("Away", closed_port), PASSWORD).await;
    let mut tls = settings("Tls", lavalink.port());
    tls.secure = true;
    let tls = add(&mut owner, tls, PASSWORD).await;

    let unavailable = |node: &proto::LavalinkNode| node.health == Health::Unavailable as i32;
    let within = Duration::from_secs(5);
    let node = until(&mut owner, wrong, within, unavailable).await;
    assert_eq!(node.reason, "Lavalink refused the password");
    let node = until(&mut owner, away, within, unavailable).await;
    assert_eq!(
        node.reason,
        format!(
            "Nothing is listening on 127.0.0.1:{closed_port}: check the port, and that Lavalink \
             is running"
        )
    );
    let node = until(&mut owner, tls, within, unavailable).await;
    assert_eq!(
        node.reason,
        format!(
            "127.0.0.1:{} does not use TLS: turn TLS off for this node",
            lavalink.port()
        )
    );

    let steps = run_test(&mut owner, wrong).await;
    let rest = step(&steps, proto::LavalinkTestStepKind::Rest);
    assert_eq!(rest.state, Step::Failed as i32);
    assert_eq!(rest.detail, "Lavalink refused the password");
    assert_eq!(
        step(&steps, proto::LavalinkTestStepKind::Websocket).state,
        Step::Skipped as i32
    );
    assert_eq!(
        step(&steps, proto::LavalinkTestStepKind::Bridge).state,
        Step::Skipped as i32
    );

    let events = fetch(&mut owner).await.events;
    assert!(
        events
            .iter()
            .any(|event| event.text == "Wrong is unavailable: Lavalink refused the password"),
        "{events:?}"
    );

    // Fixing the password reconnects.
    update(
        &mut owner,
        proto::UpdateLavalinkNode {
            node_id: wrong,
            password: Some(PASSWORD.to_owned()),
            ..Default::default()
        },
    )
    .await;
    healthy(&mut owner, wrong).await;
}

#[tokio::test]
async fn the_connection_test_follows_a_player_to_the_bridge() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    *lavalink.fake.bridge_fingerprint.lock().unwrap() = Some(server.handle.fingerprint);
    let (mut owner, _) = owner(&server).await;
    let id = add(&mut owner, settings("Home", lavalink.port()), PASSWORD).await;
    healthy(&mut owner, id).await;

    let steps = run_test(&mut owner, id).await;
    for step in &steps {
        assert_eq!(step.state, Step::Passed as i32, "{steps:?}");
    }
    assert_eq!(
        step(&steps, proto::LavalinkTestStepKind::Rest).detail,
        "Lavalink 4.2.2 · sources: youtube, soundcloud"
    );
    let bridge = &step(&steps, proto::LavalinkTestStepKind::Bridge).detail;
    assert!(
        bridge.contains("reached the audio port from 127.0.0.1"),
        "{bridge}"
    );
    let (guild, body) = lavalink.fake.players.lock().unwrap()[0].clone();
    let port = server.handle.local_addr.port();
    assert_eq!(body["voice"]["endpoint"], format!("localhost:{port}"));
    assert_eq!(body["voice"]["channelId"], guild.to_string());
    assert_eq!(
        *lavalink.fake.destroyed.lock().unwrap(),
        [guild],
        "the player is gone"
    );

    *lavalink.fake.voice.lock().unwrap() = Voice::SignInOnly;
    let steps = run_test(&mut owner, id).await;
    let bridge = step(&steps, proto::LavalinkTestStepKind::Bridge);
    assert_eq!(bridge.state, Step::Failed as i32);
    assert!(
        bridge
            .detail
            .starts_with("Lavalink signed in, but didn't send audio to"),
        "{}",
        bridge.detail
    );

    *lavalink.fake.voice.lock().unwrap() = Voice::Silent;
    let steps = run_test(&mut owner, id).await;
    let bridge = step(&steps, proto::LavalinkTestStepKind::Bridge);
    assert_eq!(bridge.state, Step::Failed as i32);
    assert_eq!(
        bridge.detail,
        format!(
            "Lavalink didn't connect to wss://localhost:{port} within 10 s. Check that the node \
             can reach that address and trusts its certificate"
        )
    );
}

#[tokio::test]
async fn new_channels_go_to_the_best_of_three_nodes() {
    let busy = FakeLavalink::start(PASSWORD).await;
    busy.fake.set_load(12, 0.1);
    let hot = FakeLavalink::start(PASSWORD).await;
    hot.fake.set_load(0, 0.7);
    let quiet = FakeLavalink::start(PASSWORD).await;
    quiet.fake.set_load(2, 0.05);
    quiet
        .fake
        .sources
        .lock()
        .unwrap()
        .push("spotify".to_owned());
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let busy = (
        add(&mut owner, settings("Busy", busy.port()), PASSWORD).await,
        busy,
    );
    let hot = (
        add(&mut owner, settings("Hot", hot.port()), PASSWORD).await,
        hot,
    );
    let quiet = (
        add(&mut owner, settings("Quiet", quiet.port()), PASSWORD).await,
        quiet,
    );
    for (id, _) in [&busy, &hot, &quiet] {
        until(&mut owner, *id, Duration::from_secs(5), |node| {
            node.health == Health::Healthy as i32 && node.players > 0 || node.cpu > 0.5
        })
        .await;
    }
    let best = |source: Option<&str>| {
        server
            .handle
            .lavalink()
            .best_node(source, &[])
            .map(|node| node.id)
    };

    assert_eq!(best(None), Some(quiet.0), "the lowest penalty");
    assert_eq!(best(Some("spotify")), Some(quiet.0));
    assert_eq!(best(Some("deezer")), None, "no node has it");

    update(
        &mut owner,
        proto::UpdateLavalinkNode {
            node_id: quiet.0,
            draining: Some(true),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        until(&mut owner, quiet.0, Duration::from_secs(2), |_| true)
            .await
            .health,
        Health::Draining as i32
    );
    assert_eq!(best(None), Some(busy.0), "a draining node gets no channel");
    assert_eq!(best(Some("spotify")), None);

    let mut preferred = settings("Hot", hot.1.port());
    preferred.priority = proto::LavalinkPriority::Preferred as i32;
    update(
        &mut owner,
        proto::UpdateLavalinkNode {
            node_id: hot.0,
            settings: Some(preferred.clone()),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(best(None), Some(hot.0), "priority first");

    preferred.enabled = false;
    update(
        &mut owner,
        proto::UpdateLavalinkNode {
            node_id: hot.0,
            settings: Some(preferred),
            ..Default::default()
        },
    )
    .await;
    until(&mut owner, hot.0, Duration::from_secs(2), |node| {
        node.health == Health::Disabled as i32
    })
    .await;
    assert_eq!(best(None), Some(busy.0));

    owner
        .ok(Kind::DeleteLavalinkNode(proto::DeleteLavalinkNode {
            node_id: busy.0,
        }))
        .await;
    assert_eq!(
        best(None),
        None,
        "only a draining and a disabled node are left"
    );
    assert_eq!(fetch(&mut owner).await.nodes.len(), 2);
}

#[tokio::test]
async fn a_dropped_connection_and_a_restart_resume_the_session() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let id = add(&mut owner, settings("Home", lavalink.port()), PASSWORD).await;
    healthy(&mut owner, id).await;
    let session = lavalink.fake.session().unwrap();

    lavalink.fake.kick();
    let deadline = Instant::now() + Duration::from_secs(5);
    while lavalink.fake.handshakes.lock().unwrap().len() < 2 {
        assert!(Instant::now() < deadline, "no reconnect");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        lavalink.fake.handshakes.lock().unwrap()[1]["Session-Id"],
        session.as_str()
    );
    healthy(&mut owner, id).await;
    let events = fetch(&mut owner).await.events;
    assert!(
        !events
            .iter()
            .any(|event| event.text.contains("unavailable")),
        "a blip within 3 s is not an outage: {events:?}"
    );

    let server = server.restart(|_| {}).await;
    let (mut owner, _) = owner_again(&server).await;
    healthy(&mut owner, id).await;
    let handshakes = lavalink.fake.handshakes.lock().unwrap().clone();
    assert_eq!(handshakes.last().unwrap()["Session-Id"], session.as_str());
    assert_eq!(
        lavalink.fake.session().unwrap(),
        session,
        "resumed, not new"
    );
}

#[tokio::test]
async fn an_unavailable_node_is_announced_and_held_back_when_it_returns() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let id = add(&mut owner, settings("Home", lavalink.port()), PASSWORD).await;
    healthy(&mut owner, id).await;
    owner
        .ok(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
            watch: true,
        }))
        .await;

    lavalink.stop();
    let text = tokio::time::timeout(
        Duration::from_secs(10),
        owner.wait_for(|event| match event {
            proto::event::Kind::LavalinkNodeEvent(event)
                if event.kind == proto::LavalinkEventKind::Disconnected as i32 =>
            {
                Some(event.text.clone())
            }
            _ => None,
        }),
    )
    .await
    .unwrap();
    assert!(text.starts_with("Home is unavailable: "), "{text}");
    let notice = tokio::time::timeout(
        Duration::from_secs(5),
        owner.wait_for(|event| match event {
            proto::event::Kind::ServerNotice(notice) => Some(notice.clone()),
            _ => None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(notice.key, "music.nodes");
    assert_eq!(notice.title, "Music server unavailable");
    assert!(server.handle.lavalink().best_node(None, &[]).is_none());

    let _lavalink = lavalink.restart().await;
    let text = tokio::time::timeout(
        Duration::from_secs(40),
        owner.wait_for(|event| match event {
            proto::event::Kind::LavalinkNodeEvent(event)
                if event.kind == proto::LavalinkEventKind::Recovered as i32 =>
            {
                Some(event.text.clone())
            }
            _ => None,
        }),
    )
    .await
    .unwrap();
    assert_eq!(text, "Home is back");
    let node = until(&mut owner, id, Duration::from_secs(2), |_| true).await;
    assert!(node.recovering, "held back for 30 s");
    assert!(
        server.handle.lavalink().best_node(None, &[]).is_none(),
        "no new channels while it recovers"
    );
}

#[tokio::test]
async fn node_settings_are_checked() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    for (host, port, name) in [
        ("http://lava.example.com", 2333, "Home"),
        ("lava.example.com/v4", 2333, "Home"),
        ("lava.example.com", 0, "Home"),
        ("lava.example.com", 2333, " "),
    ] {
        let mut bad = settings(name, 1);
        bad.host = host.to_owned();
        bad.port = port;
        let error = owner
            .error(Kind::CreateLavalinkNode(proto::CreateLavalinkNode {
                settings: Some(bad),
                password: PASSWORD.to_owned(),
            }))
            .await;
        assert_eq!(
            error.code,
            proto::ErrorCode::InvalidArgument as i32,
            "{host}"
        );
    }
    let error = owner
        .error(Kind::CreateLavalinkNode(proto::CreateLavalinkNode {
            settings: Some(settings("Home", 2333)),
            password: String::new(),
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::InvalidArgument as i32);
    let mut disabled = settings("Spare", 2333);
    disabled.enabled = false;
    for _ in 0..20 {
        add(&mut owner, disabled.clone(), PASSWORD).await;
    }
    let error = owner
        .error(Kind::CreateLavalinkNode(proto::CreateLavalinkNode {
            settings: Some(disabled),
            password: PASSWORD.to_owned(),
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::InvalidArgument as i32);
    let node = &fetch(&mut owner).await.nodes[0];
    assert_eq!(node.health, Health::Disabled as i32);
}

/// The owner again after a restart.
async fn owner_again(server: &TestServer) -> (TestClient, proto::Ready) {
    let mut client = TestClient::connect(server, key(1)).await;
    let ready = client.identify("owner", None, None).await.unwrap();
    (client, ready)
}

/// The same against a real Lavalink 4.2 or newer: it connects, and the
/// connection test passes all three steps, its player reaching the bridge.
/// Ignored unless given one (`docs/music.md`):
///
/// - `OPENCORD_TEST_LAVALINK`: its address, `http://127.0.0.1:2333`
/// - `OPENCORD_TEST_LAVALINK_PASSWORD`
/// - `OPENCORD_TEST_BRIDGE_TLS`: a directory with `cert.pem` and `key.pem`
///   for `localhost`, which that Lavalink trusts
#[tokio::test]
#[ignore = "needs a Lavalink: see docs/music.md"]
async fn a_real_lavalink_passes_the_connection_test() {
    let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let address = env("OPENCORD_TEST_LAVALINK");
    let password = env("OPENCORD_TEST_LAVALINK_PASSWORD");
    let tls = std::path::PathBuf::from(env("OPENCORD_TEST_BRIDGE_TLS"));
    let port: u16 = address.rsplit(':').next().unwrap().parse().unwrap();
    let server = TestServer::start_with(|config| {
        config.tls.cert = Some(tls.join("cert.pem"));
        config.tls.key = Some(tls.join("key.pem"));
    })
    .await;
    let (mut owner, _) = owner(&server).await;
    let id = add(&mut owner, settings("Real", port), &password).await;
    let node = healthy(&mut owner, id).await;
    println!("{node:?}");
    assert!(node.version.starts_with('4'), "{}", node.version);

    let steps = run_test(&mut owner, id).await;
    println!("{steps:#?}");
    for step in &steps {
        assert_eq!(step.state, Step::Passed as i32, "{steps:?}");
    }

    let wrong = add(&mut owner, settings("Wrong", port), "not-the-password").await;
    let node = until(&mut owner, wrong, Duration::from_secs(5), |node| {
        node.health == Health::Unavailable as i32
    })
    .await;
    assert_eq!(node.reason, "Lavalink refused the password");
}
