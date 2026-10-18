//! Music through the client core against a real server and the fake
//! Lavalink (Phase 3 plan §15): search, and artwork that comes only through
//! the server's proxy and is kept on this device.

use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    ChannelKind, CoreError, CoreEvent, CoreEventPayload, ErrorCode, LavalinkHealth,
    LavalinkNodeSettings, LavalinkPriority, MusicAction, MusicPlacement, MusicPlayState,
    MusicRepeat, ReadySnapshot,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use opencord_server::testing::fake_lavalink::FakeLavalink;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(10);
const PASSWORD: &str = "youshallnotpass";

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.voice.udp_port = 0;
        config.music.audio_udp_port = 0;
        // The picture host below is on this machine.
        config.music.allow_unsafe_sources = true;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }
}

struct Owner {
    client: Client,
    events: UnboundedReceiver<CoreEvent>,
    dir: tempfile::TempDir,
}

impl Owner {
    async fn of(server: &TestServer) -> (Self, ReadySnapshot) {
        let dir = tempfile::tempdir().unwrap();
        let (client, events) = Client::new(dir.path(), tokio::runtime::Handle::current()).unwrap();
        client.set_identity(Identity::generate(), "Owner".to_owned());
        client
            .trust_fingerprint(
                &server.address(),
                &format_fingerprint(&server.handle.fingerprint),
            )
            .unwrap();
        client
            .add_server(&server.address(), server.handle.claim_token.clone())
            .await
            .unwrap();
        let mut owner = Self {
            client,
            events,
            dir,
        };
        let ready = owner.ready().await;
        (owner, ready)
    }

    async fn ready(&mut self) -> ReadySnapshot {
        tokio::time::timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.expect("the event stream ended");
                if let CoreEventPayload::Ready(ready) = event.payload {
                    return ready;
                }
            }
        })
        .await
        .expect("no Ready")
    }
}

/// Adds the fake as a node and waits until it is healthy.
async fn add_node(owner: &Owner, key: &str, lavalink: &FakeLavalink) {
    owner
        .client
        .create_lavalink_node(
            key,
            LavalinkNodeSettings {
                name: "Home".to_owned(),
                host: "127.0.0.1".to_owned(),
                port: u32::from(lavalink.address.port()),
                secure: false,
                priority: LavalinkPriority::Normal,
                max_players: 0,
                strict_address: true,
                enabled: true,
            },
            PASSWORD.to_owned(),
        )
        .await
        .unwrap();
    tokio::time::timeout(WAIT, async {
        loop {
            let nodes = owner.client.lavalink_nodes(key, false).await.unwrap();
            if nodes
                .nodes
                .iter()
                .any(|node| node.health == LavalinkHealth::Healthy)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("the node never became healthy");
}

/// A plain HTTP picture host on this machine, counting its requests.
async fn picture_host() -> (String, Arc<AtomicUsize>) {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        300,
        300,
        image::Rgb([240, 200, 0]),
    ))
    .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
    .unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/cover.png", listener.local_addr().unwrap());
    let counted = Arc::clone(&hits);
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            counted.fetch_add(1, Ordering::SeqCst);
            let png = png.clone();
            tokio::spawn(async move {
                let mut request = [0u8; 2048];
                let _ = socket.read(&mut request).await;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\n\
                     Connection: close\r\n\r\n",
                    png.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&png).await;
            });
        }
    });
    (url, hits)
}

#[tokio::test(flavor = "multi_thread")]
async fn search_results_bring_artwork_that_comes_through_the_server_and_stays() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let (picture, hits) = picture_host().await;
    *lavalink.fake.artwork.lock().unwrap() = Some(picture.clone());
    let server = TestServer::start().await;
    let key = server.address();
    let (owner, ready) = Owner::of(&server).await;
    add_node(&owner, &key, &lavalink).await;
    let voice = ready
        .channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Voice)
        .unwrap()
        .id;
    owner.client.voice_join(&key, voice).await.unwrap();

    let found = owner
        .client
        .music_search(&key, voice, "song".to_owned(), String::new())
        .await
        .unwrap();
    assert_eq!(found.results.len(), 3);
    let track = &found.results[0].track;
    assert_eq!(track.title, "song 1");
    assert_eq!(track.source_display_name, "YouTube");
    assert!(!format!("{found:?}").contains(&picture));

    let path = owner
        .client
        .music_artwork(&key, &track.artwork_id, 512)
        .await
        .unwrap();
    assert!(path.starts_with(&*owner.dir.path().to_string_lossy()));
    let shown = image::open(&path).unwrap();
    assert_eq!((shown.width(), shown.height()), (300, 300));
    let again = owner
        .client
        .music_artwork(&key, &track.artwork_id, 512)
        .await
        .unwrap();
    assert_eq!(again, path);
    assert_eq!(hits.load(Ordering::SeqCst), 1);

    let refused = owner
        .client
        .music_artwork(&key, "../../etc/passwd", 512)
        .await;
    assert!(matches!(refused, Err(CoreError::InvalidInput { .. })));
    let unknown = owner
        .client
        .music_artwork(&key, "0123456789abcdef0123456789abcdef", 128)
        .await;
    assert!(matches!(unknown, Err(CoreError::Server { .. })));
}

impl Owner {
    /// Skips events until `pick` returns something.
    async fn wait_for<T>(&mut self, mut pick: impl FnMut(&CoreEventPayload) -> Option<T>) -> T {
        tokio::time::timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.expect("the event stream ended");
                if let Some(found) = pick(&event.payload) {
                    return found;
                }
            }
        })
        .await
        .expect("timed out waiting for an event")
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[tokio::test(flavor = "multi_thread")]
async fn the_player_comes_with_its_changes_on_this_devices_clock() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    let key = server.address();
    let (mut owner, ready) = Owner::of(&server).await;
    add_node(&owner, &key, &lavalink).await;
    let voice = ready
        .channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Voice)
        .unwrap()
        .id;
    owner.client.voice_join(&key, voice).await.unwrap();

    let empty = owner
        .client
        .music_subscribe(&key, voice, true)
        .await
        .unwrap();
    assert_eq!(empty.state, MusicPlayState::Idle);
    let found = owner
        .client
        .music_search(&key, voice, "song".to_owned(), String::new())
        .await
        .unwrap();
    let ids = found.results.iter().map(|r| r.result_id.clone()).collect();
    let added = owner
        .client
        .music_enqueue(&key, voice, ids, Vec::new(), None, MusicPlacement::End)
        .await
        .unwrap();
    assert_eq!(added.now_playing.as_ref().unwrap().track.title, "song 1");
    assert_eq!(added.queue.len(), 2);

    // The card comes as the track loads, the session once it plays.
    let mut card = None;
    let playing = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::MusicNowPlaying(now) => {
                card = Some(now.clone());
                None
            }
            CoreEventPayload::MusicSession(session) if session.state == MusicPlayState::Playing => {
                Some(session.clone())
            }
            _ => None,
        })
        .await;
    // The server and this device share a clock here: the anchor is now,
    // and later by the playout delay.
    let late = playing.anchor_ms - now_ms();
    assert!((0..=500).contains(&late), "anchor {late} ms from now");
    let card = card.expect("no now-playing card");
    assert_eq!((card.title.as_str(), card.ended), ("song 1", false));

    let item = playing.now_playing.unwrap().item_id;
    let paused = owner
        .client
        .music_playback(&key, voice, MusicAction::Pause, None, Some(item.clone()))
        .await
        .unwrap();
    assert_eq!(paused.state, MusicPlayState::Paused);
    let moved = owner
        .client
        .music_queue_move(&key, voice, paused.queue[0].item_id.clone(), None)
        .await
        .map(|session| session.queue[0].track.title.clone())
        .unwrap();
    assert_eq!(moved, "song 3");
    let shuffled = owner
        .client
        .music_set_mode(&key, voice, Some(MusicRepeat::Queue), None)
        .await
        .unwrap();
    assert_eq!(shuffled.repeat, MusicRepeat::Queue);
    let stale = owner
        .client
        .music_playback(
            &key,
            voice,
            MusicAction::Next,
            None,
            Some("gone".to_owned()),
        )
        .await;
    assert!(
        matches!(
            stale,
            Err(CoreError::Server {
                code: ErrorCode::MusicStale,
                ..
            })
        ),
        "{stale:?}"
    );
    let cleared = owner.client.music_queue_clear(&key, voice).await.unwrap();
    assert!(cleared.queue.is_empty());
    owner.client.music_set_listening(1.5, true, true);
}

/// Plan §14.3: after the first snapshot, changes come as updates the core
/// applies; the app still gets the whole session, the same as the
/// server's.
#[tokio::test(flavor = "multi_thread")]
async fn changes_from_elsewhere_arrive_whole_from_the_servers_updates() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    let key = server.address();
    let (mut owner, ready) = Owner::of(&server).await;
    add_node(&owner, &key, &lavalink).await;
    let voice = ready
        .channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Voice)
        .unwrap()
        .id;
    owner.client.voice_join(&key, voice).await.unwrap();
    owner
        .client
        .music_subscribe(&key, voice, true)
        .await
        .unwrap();
    for query in ["song", "tune"] {
        let found = owner
            .client
            .music_search(&key, voice, query.to_owned(), String::new())
            .await
            .unwrap();
        let ids = found.results.iter().map(|r| r.result_id.clone()).collect();
        owner
            .client
            .music_enqueue(&key, voice, ids, Vec::new(), None, MusicPlacement::End)
            .await
            .unwrap();
    }
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::MusicSession(session) if session.state == MusicPlayState::Playing => {
                Some(())
            }
            _ => None,
        })
        .await;

    // The track ends on the node: the server moves on by itself.
    lavalink.fake.finish(voice);
    let heard = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::MusicSession(session)
                if session.state == MusicPlayState::Playing
                    && session
                        .now_playing
                        .as_ref()
                        .is_some_and(|now| now.track.title == "song 2") =>
            {
                Some(session.clone())
            }
            _ => None,
        })
        .await;

    let asked = owner
        .client
        .music_subscribe(&key, voice, true)
        .await
        .unwrap();
    assert_eq!(heard.version, asked.version);
    assert_eq!(heard.queue, asked.queue);
    assert_eq!(heard.history, asked.history);
    assert_eq!(heard.now_playing, asked.now_playing);
    assert_eq!(heard.queue.len(), 4);
    assert_eq!(heard.history[0].track.title, "song 1");
}
