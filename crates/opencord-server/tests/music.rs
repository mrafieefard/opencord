//! Music sessions, Phase 3 L2 (plan §12.7, §14, §16): the queue and its
//! playback rules driven by Lavalink's events, permissions and limits,
//! everyone leaving, failover between nodes, moving players, and restarts
//! keeping the music on its node. Against the fake Lavalink; `docs/music.md`
//! runs the failover against two real ones.

mod common;

use std::time::{Duration, Instant};

use common::fake_lavalink::FakeLavalink;
use common::music::*;
use common::*;
use opencord_common::permissions::Permissions;
use opencord_proto::music_sync;
use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Response;
use proto::{MusicAction as Action, MusicPlayState as Play};

#[tokio::test]
async fn music_needs_a_node_the_channel_and_the_permissions() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let voice = channel_id(&ready, "General");
    owner.ok(join_voice(voice)).await;
    let error = owner
        .error(Kind::MusicSearch(proto::MusicSearch {
            channel_id: voice,
            query: "song".to_owned(),
            source: String::new(),
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::MusicNotConfigured);
    drop((owner, server));

    let Room {
        server: _server,
        mut owner,
        mut member,
        member_id,
        voice,
        everyone,
        lavalink,
        ..
    } = room().await;
    let error = member
        .error(Kind::MusicSearch(proto::MusicSearch {
            channel_id: voice,
            query: "song".to_owned(),
            source: String::new(),
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::MusicNotInChannel);
    member.ok(join_voice(voice)).await;
    let mine = search(&mut member, voice, "mine").await;
    assert_eq!(mine.len(), 3);
    assert_eq!(
        *lavalink.fake.loads.lock().unwrap(),
        ["ytmsearch:mine"],
        "the default source is YouTube Music"
    );

    // Results are the searcher's own, and nothing else can be sent.
    let error = owner.error(enqueue(voice, vec![mine[0].clone()])).await;
    assert_eq!(code(&error), proto::ErrorCode::NotFound);
    let error = member
        .error(enqueue(voice, vec!["enc:youtube:anything".to_owned()]))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::NotFound);

    // Adding plays at once when nothing does; the encoded track stays here.
    let now = session(member.ok(enqueue(voice, mine.clone())).await);
    assert_eq!(title(now.now_playing.as_ref()), "mine 1");
    assert_eq!(now.queue.len(), 2);
    assert!(!format!("{now:?}").contains("enc:"));
    fake_plays(&lavalink, voice, "mine 1").await;
    until(&mut member, voice, "playing", |now| playing(now, "mine 1")).await;

    // Without MUSIC_CONTROL: no pausing, but one's own track can be skipped.
    owner
        .ok(Kind::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some((Permissions::DEFAULT_EVERYONE - Permissions::MUSIC_CONTROL).bits()),
            ..Default::default()
        }))
        .await;
    let error = member.error(playback(voice, Action::Pause)).await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);
    member.ok(playback(voice, Action::Next)).await;
    fake_plays(&lavalink, voice, "mine 2").await;

    // Play next and positions need MUSIC_MANAGE_QUEUE.
    let more = search(&mut member, voice, "more").await;
    let error = member
        .error(Kind::MusicEnqueue(proto::MusicEnqueue {
            history_item_ids: Vec::new(),
            channel_id: voice,
            result_ids: more.clone(),
            before_item_id: None,
            placement: proto::MusicPlacement::Next as i32,
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);

    // Other members' tracks need MUSIC_MANAGE_QUEUE to remove; one's own don't.
    let owners = search(&mut owner, voice, "owners").await;
    let now = session(owner.ok(enqueue(voice, owners)).await);
    let owners_item = now.queue.last().unwrap().item_id.clone();
    let members_item = now.queue[0].item_id.clone();
    let error = member
        .error(Kind::MusicQueueRemove(proto::MusicQueueRemove {
            channel_id: voice,
            item_ids: vec![owners_item],
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);
    member
        .ok(Kind::MusicQueueRemove(proto::MusicQueueRemove {
            channel_id: voice,
            item_ids: vec![members_item],
        }))
        .await;

    // Stopping needs MUSIC_MANAGE.
    let error = member.error(playback(voice, Action::Stop)).await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);

    // Blocked members can still listen, but not play or control.
    owner
        .ok(Kind::MusicBlockMember(proto::MusicBlockMember {
            user_id: member_id,
            blocked: true,
            reason: "spam".to_owned(),
        }))
        .await;
    let error = member
        .error(Kind::MusicSearch(proto::MusicSearch {
            channel_id: voice,
            query: "x".to_owned(),
            source: String::new(),
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::MusicBlocked);
    member.ok(snapshot_request(voice)).await;
    let blocks = match owner
        .ok(Kind::FetchMusicBlocks(proto::FetchMusicBlocks {}))
        .await
    {
        Response::MusicBlocks(blocks) => blocks.blocks,
        other => panic!("expected blocks, got {other:?}"),
    };
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].reason, "spam");
    let error = member
        .error(Kind::FetchMusicBlocks(proto::FetchMusicBlocks {}))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);

    owner.ok(playback(voice, Action::Stop)).await;
    until(&mut owner, voice, "stopped", |now| {
        now.state == Play::Idle as i32 && now.queue.is_empty()
    })
    .await;
    let deadline = Instant::now() + Duration::from_secs(5);
    while !lavalink.fake.destroyed.lock().unwrap().contains(&voice) {
        assert!(Instant::now() < deadline, "the player was never destroyed");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn the_queue_follows_lavalinks_events_and_the_playback_rules() {
    let Room {
        server: _server,
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    until(&mut owner, voice, "playing", |now| playing(now, "song 1")).await;

    // A finished track goes to history and the next one plays.
    lavalink.fake.finish(voice);
    let now = until(&mut owner, voice, "on to song 2", |now| {
        playing(now, "song 2")
    })
    .await;
    assert_eq!(title(now.history.first()), "song 1");
    fake_plays(&lavalink, voice, "song 2").await;

    // Pause and resume reach the node; positions follow its reports.
    owner.ok(playback(voice, Action::Pause)).await;
    assert!(lavalink.fake.player(voice).unwrap().paused);
    let now = session(owner.ok(snapshot_request(voice)).await);
    assert_eq!(now.state, Play::Paused as i32);
    owner.ok(playback(voice, Action::Resume)).await;
    assert!(!lavalink.fake.player(voice).unwrap().paused);
    report_position(&lavalink, voice, 42_000).await;
    let now = until(&mut owner, voice, "at 42 s", |now| {
        (42_000..44_000).contains(&now.position_ms)
    })
    .await;
    assert!(now.server_time_ms > 0);

    // Seek, and a stale skip is refused.
    let current = now.now_playing.unwrap().item_id;
    owner
        .ok(Kind::MusicPlayback(proto::MusicPlayback {
            channel_id: voice,
            action: Action::Seek as i32,
            position_ms: Some(90_000),
            expected_item_id: Some(current.clone()),
        }))
        .await;
    assert_eq!(lavalink.fake.player(voice).unwrap().position, 90_000);
    let error = owner
        .error(Kind::MusicPlayback(proto::MusicPlayback {
            channel_id: voice,
            action: Action::Next as i32,
            position_ms: None,
            expected_item_id: Some("not-this-one".to_owned()),
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::MusicStale);

    // Previous, past 5 s: the track starts again.
    owner.ok(playback(voice, Action::Previous)).await;
    assert_eq!(lavalink.fake.player(voice).unwrap().position, 0);
    let now = until(&mut owner, voice, "at the start", |now| {
        now.position_ms < 2_000
    })
    .await;
    assert_eq!(title(now.now_playing.as_ref()), "song 2");
    // Within 5 s: back to song 1, and song 2 is next.
    owner.ok(playback(voice, Action::Previous)).await;
    let now = until(&mut owner, voice, "back to song 1", |now| {
        playing(now, "song 1")
    })
    .await;
    assert_eq!(title(now.queue.first()), "song 2");
    fake_plays(&lavalink, voice, "song 1").await;

    // Relative moves: song 3 before song 2.
    let song_2 = now.queue[0].item_id.clone();
    let song_3 = now.queue[1].item_id.clone();
    let now = session(
        owner
            .ok(Kind::MusicQueueMove(proto::MusicQueueMove {
                channel_id: voice,
                item_id: song_3,
                before_item_id: Some(song_2),
            }))
            .await,
    );
    assert_eq!(title(now.queue.first()), "song 3");

    // Repeat the track: it plays again when it ends.
    owner
        .ok(Kind::MusicSetMode(proto::MusicSetMode {
            channel_id: voice,
            repeat: Some(proto::MusicRepeat::Track as i32),
            shuffle: None,
        }))
        .await;
    lavalink.fake.finish(voice);
    fake_plays(&lavalink, voice, "song 1").await;
    let now = until(&mut owner, voice, "song 1 again", |now| {
        playing(now, "song 1")
    })
    .await;
    assert_eq!(now.repeat, proto::MusicRepeat::Track as i32);
    owner
        .ok(Kind::MusicSetMode(proto::MusicSetMode {
            channel_id: voice,
            repeat: Some(proto::MusicRepeat::Off as i32),
            shuffle: None,
        }))
        .await;

    // Tracks that can't be played are skipped with a notice; three in a
    // row stop the music.
    let broken = search(&mut owner, voice, "broken").await;
    owner
        .ok(Kind::MusicEnqueue(proto::MusicEnqueue {
            history_item_ids: Vec::new(),
            channel_id: voice,
            result_ids: broken,
            before_item_id: None,
            placement: proto::MusicPlacement::Now as i32,
        }))
        .await;
    let notice = owner
        .wait_for(|event| match event {
            proto::event::Kind::MusicNotice(notice) if notice.text.starts_with("Couldn't") => {
                Some(notice.text.clone())
            }
            _ => None,
        })
        .await;
    assert_eq!(notice, "Couldn't play 'broken 1', skipped");
    owner
        .wait_for(|event| match event {
            proto::event::Kind::MusicNotice(notice) if notice.text.starts_with("Music stopped") => {
                Some(())
            }
            _ => None,
        })
        .await;
    let now = until(&mut owner, voice, "stopped after three", |now| {
        now.state == Play::Paused as i32
    })
    .await;
    assert_eq!(title(now.now_playing.as_ref()), "song 3");
    let failed: Vec<&str> = now.history[..3]
        .iter()
        .map(|item| title(Some(item)))
        .collect();
    assert_eq!(failed, ["broken 3", "broken 2", "broken 1"]);
    owner
        .ok(Kind::MusicQueueClear(proto::MusicQueueClear {
            channel_id: voice,
        }))
        .await;
    let now = session(owner.ok(snapshot_request(voice)).await);
    assert!(now.queue.is_empty());
    assert_eq!(
        title(now.now_playing.as_ref()),
        "song 3",
        "what plays stays"
    );

    // Everyone who can see the channel gets the now-playing card.
    owner.ok(playback(voice, Action::Stop)).await;
    let card = owner
        .wait_for(|event| match event {
            proto::event::Kind::MusicNowPlaying(card) if card.ended => Some(card.clone()),
            _ => None,
        })
        .await;
    assert_eq!(card.channel_id, voice);
}

#[tokio::test]
async fn limits_and_settings_are_enforced_by_the_server() {
    let Room {
        server: _server,
        mut owner,
        mut member,
        voice,
        lavalink: _lavalink,
        ..
    } = room().await;
    member.ok(join_voice(voice)).await;
    let error = member
        .error(Kind::UpdateMusicSettings(proto::UpdateMusicSettings {
            queue_limit: Some(4),
            ..Default::default()
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);
    let settings = match owner
        .ok(Kind::UpdateMusicSettings(proto::UpdateMusicSettings {
            queue_limit: Some(4),
            max_tracks_per_member: Some(2),
            allow_streams: Some(false),
            max_track_ms: Some(60_000),
            ..Default::default()
        }))
        .await
    {
        Response::MusicSettings(settings) => *settings,
        other => panic!("expected settings, got {other:?}"),
    };
    assert_eq!(settings.queue_limit, 4);
    assert!(settings.supported);
    assert_eq!(
        settings.available_sources,
        ["youtube_music", "youtube", "soundcloud"]
    );
    let update = member
        .wait_for(|event| match event {
            proto::event::Kind::MusicSettingsUpdate(update) => update.settings.clone(),
            _ => None,
        })
        .await;
    assert_eq!(update.max_tracks_per_member, 2);

    let found = search(&mut member, voice, "song").await;
    let error = member.error(enqueue(voice, found.clone())).await;
    assert_eq!(
        code(&error),
        proto::ErrorCode::MusicTrackTooLong,
        "3-minute tracks, 1 minute allowed"
    );
    let live = search(&mut owner, voice, "live").await;
    let error = owner.error(enqueue(voice, live)).await;
    assert_eq!(code(&error), proto::ErrorCode::MusicStreamsNotAllowed);
    owner
        .ok(Kind::UpdateMusicSettings(proto::UpdateMusicSettings {
            max_track_ms: Some(0),
            ..Default::default()
        }))
        .await;
    let error = member.error(enqueue(voice, found.clone())).await;
    assert_eq!(code(&error), proto::ErrorCode::MusicMemberLimit);
    member.ok(enqueue(voice, found[..2].to_vec())).await;
    let more = search(&mut owner, voice, "more").await;
    let error = owner.error(enqueue(voice, more.clone())).await;
    assert_eq!(code(&error), proto::ErrorCode::MusicQueueFull);
    owner.ok(enqueue(voice, more[..2].to_vec())).await;

    let error = member
        .error(Kind::MusicSearch(proto::MusicSearch {
            channel_id: voice,
            query: "https://127.0.0.1/a.mp3".to_owned(),
            source: String::new(),
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::MusicLinkNotAllowed);

    // Searches are limited to 10 in 10 s.
    let mut limited = false;
    for _ in 0..12 {
        if let Response::Error(error) = member
            .request(Kind::MusicSearch(proto::MusicSearch {
                channel_id: voice,
                query: "again".to_owned(),
                source: String::new(),
            }))
            .await
        {
            assert_eq!(code(&error), proto::ErrorCode::RateLimited);
            limited = true;
        }
    }
    assert!(limited);

    // Turning music off ends what plays.
    owner
        .ok(Kind::UpdateMusicSettings(proto::UpdateMusicSettings {
            enabled: Some(false),
            ..Default::default()
        }))
        .await;
    until(&mut owner, voice, "ended", |now| {
        now.state == Play::Idle as i32
    })
    .await;
    let error = owner.error(enqueue(voice, more)).await;
    assert_eq!(code(&error), proto::ErrorCode::MusicNotConfigured);
}

#[tokio::test]
async fn everyone_leaving_pauses_and_coming_back_resumes() {
    let Room {
        server: _server,
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    owner
        .ok(Kind::UpdateMusicSettings(proto::UpdateMusicSettings {
            stop_after_empty_s: Some(1),
            ..Default::default()
        }))
        .await;
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    until(&mut owner, voice, "playing", |now| playing(now, "song 1")).await;
    report_position(&lavalink, voice, 30_000).await;
    until(&mut owner, voice, "at 30 s", |now| {
        now.position_ms >= 30_000
    })
    .await;

    owner.ok(leave_voice()).await;
    let now = until(&mut owner, voice, "paused for nobody", |now| {
        now.paused_empty
    })
    .await;
    assert_eq!(now.state, Play::Paused as i32);
    assert!(lavalink.fake.player(voice).unwrap().paused);
    // After the setting's time the player goes; the queue stays.
    let deadline = Instant::now() + Duration::from_secs(5);
    while lavalink.fake.player(voice).is_some() {
        assert!(Instant::now() < deadline, "the player stayed");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    owner.ok(join_voice(voice)).await;
    fake_plays(&lavalink, voice, "song 1").await;
    let player = lavalink.fake.player(voice).unwrap();
    assert!(!player.paused);
    assert!(
        (30_000..32_000).contains(&player.position),
        "played on from where it was: {player:?}"
    );
    let now = until(&mut owner, voice, "playing again", |now| {
        playing(now, "song 1")
    })
    .await;
    assert!(!now.paused_empty);
}

#[tokio::test]
async fn a_failed_node_hands_its_channels_to_another_and_its_old_players_go() {
    let Room {
        server: _server,
        mut owner,
        voice,
        lavalink: first,
        node: first_id,
        ..
    } = room().await;
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    fake_plays(&first, voice, "song 1").await;
    let second = FakeLavalink::start(PASSWORD).await;
    add_node(&mut owner, "Spare", &second).await;
    report_position(&first, voice, 60_000).await;
    until(&mut owner, voice, "at 60 s", |now| {
        now.position_ms >= 60_000
    })
    .await;

    first.stop();
    let stopped = Instant::now();
    fake_plays(&second, voice, "song 1").await;
    assert!(
        stopped.elapsed() < Duration::from_secs(6),
        "music continues within 5 s of a crash (+1 s for the test)"
    );
    let position = second.fake.player(voice).unwrap().position;
    assert!(
        (60_000..69_000).contains(&position),
        "near where it was: {position}"
    );
    let voice_state = second.fake.player(voice).unwrap().voice.unwrap();
    assert_eq!(voice_state["channelId"], voice.to_string());
    owner
        .wait_for(|event| match event {
            proto::event::Kind::MusicNotice(notice)
                if notice.text == "Switched to another music server" =>
            {
                Some(())
            }
            _ => None,
        })
        .await;
    let list = nodes(
        owner
            .ok(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
                watch: false,
            }))
            .await,
    );
    assert!(
        list.events
            .iter()
            .any(|event| event.text == "Moved 1 channel from Home to Spare"),
        "{:?}",
        list.events
    );

    // Back again: its old player is destroyed, so nothing plays twice.
    let first = first.restart().await;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !first.fake.destroyed.lock().unwrap().contains(&voice) {
        assert!(Instant::now() < deadline, "the old player stayed");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(first.fake.player(voice).is_none());
    assert!(second.fake.player(voice).is_some());
    let _ = first_id;
}

#[tokio::test]
async fn players_move_now_and_before_their_node_is_deleted() {
    let Room {
        server: _server,
        mut owner,
        voice,
        lavalink: first,
        node: first_id,
        ..
    } = room().await;
    let second = FakeLavalink::start(PASSWORD).await;
    let second_id = add_node(&mut owner, "Spare", &second).await;
    owner
        .ok(Kind::UpdateLavalinkNode(proto::UpdateLavalinkNode {
            node_id: second_id,
            settings: Some(proto::LavalinkNodeSettings {
                priority: proto::LavalinkPriority::Backup as i32,
                ..node_settings("Spare", second.port())
            }),
            password: None,
            draining: None,
        }))
        .await;
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    fake_plays(&first, voice, "song 1").await;

    owner
        .ok(Kind::MoveLavalinkPlayers(proto::MoveLavalinkPlayers {
            from_node_id: first_id,
            to_node_id: Some(second_id),
        }))
        .await;
    fake_plays(&second, voice, "song 1").await;
    assert!(first.fake.destroyed.lock().unwrap().contains(&voice));

    owner
        .ok(Kind::DeleteLavalinkNode(proto::DeleteLavalinkNode {
            node_id: second_id,
        }))
        .await;
    fake_plays(&first, voice, "song 1").await;
    until(&mut owner, voice, "playing", |now| playing(now, "song 1")).await;
}

#[tokio::test]
async fn a_restart_keeps_the_music_playing_on_its_node() {
    let Room {
        server,
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    fake_plays(&lavalink, voice, "song 1").await;
    report_position(&lavalink, voice, 50_000).await;
    until(&mut owner, voice, "at 50 s", |now| {
        now.position_ms >= 50_000
    })
    .await;
    let updates = lavalink.fake.players.lock().unwrap().len();
    drop(owner);

    let server = server.restart(|_| {}).await;
    let mut owner = TestClient::connect(&server, key(1)).await;
    owner.identify("owner", None, None).await.unwrap();

    // The node kept the player: it only gets a new voice state.
    let deadline = Instant::now() + Duration::from_secs(10);
    let update = loop {
        let players = lavalink.fake.players.lock().unwrap().clone();
        if let Some((_, body)) = players[updates..].iter().find(|(guild, _)| *guild == voice) {
            break body.clone();
        }
        assert!(Instant::now() < deadline, "no new voice state");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert!(update.get("voice").is_some(), "{update}");
    assert!(update.get("track").is_none(), "the track goes on: {update}");
    let now = until(&mut owner, voice, "still playing", |now| {
        playing(now, "song 1")
    })
    .await;
    assert_eq!(now.queue.len(), 2, "the queue survived");
    assert!(now.position_ms >= 50_000);
}

/// Failover between two real Lavalink 4.2 nodes: a track plays on the
/// first; disabling it moves the channel to the second within 5 s; a
/// restart keeps the music on the second. Ignored unless given them
/// (`docs/music.md`):
///
/// - `OPENCORD_TEST_LAVALINK` and `OPENCORD_TEST_LAVALINK_2`: their
///   addresses, `http://127.0.0.1:2333` and `http://127.0.0.1:2334`
/// - `OPENCORD_TEST_LAVALINK_PASSWORD`, the same for both
/// - `OPENCORD_TEST_BRIDGE_TLS`: a directory with `cert.pem` and `key.pem`
///   for `localhost`, which both trust
/// - `OPENCORD_TEST_SEEKABLE_TRACK`: a file both can play with their
///   `local` source, and seek in: an MP3 (Lavalink can't seek in Ogg)
#[tokio::test]
#[ignore = "needs two Lavalinks: see docs/music.md"]
async fn two_real_lavalinks_fail_over() {
    let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let port = |address: String| -> u16 { address.rsplit(':').next().unwrap().parse().unwrap() };
    let first_port = port(env("OPENCORD_TEST_LAVALINK"));
    let second_port = port(env("OPENCORD_TEST_LAVALINK_2"));
    let password = env("OPENCORD_TEST_LAVALINK_PASSWORD");
    let tls = std::path::PathBuf::from(env("OPENCORD_TEST_BRIDGE_TLS"));
    let track = env("OPENCORD_TEST_SEEKABLE_TRACK");
    let configure = |config: &mut opencord_server::config::Config| {
        config.tls.cert = Some(tls.join("cert.pem"));
        config.tls.key = Some(tls.join("key.pem"));
        config.music.allow_unsafe_sources = true;
    };
    let server = TestServer::start_with(configure).await;
    let (mut owner, ready) = owner(&server).await;
    let voice = channel_id(&ready, "General");
    owner.ok(join_voice(voice)).await;
    let add = |name: &str, port: u16| {
        Kind::CreateLavalinkNode(proto::CreateLavalinkNode {
            settings: Some(node_settings(name, port)),
            password: password.clone(),
        })
    };
    let first = nodes(owner.ok(add("First", first_port)).await).nodes[0].id;
    let node_with = |list: &proto::LavalinkNodes, channels: u32| {
        list.nodes
            .iter()
            .find(|node| {
                node.channels == channels && node.health == proto::LavalinkHealth::Healthy as i32
            })
            .map(|node| node.id)
    };
    let wait_nodes = async |owner: &mut TestClient,
                            pick: &dyn Fn(&proto::LavalinkNodes) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let list = nodes(
                owner
                    .ok(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
                        watch: false,
                    }))
                    .await,
            );
            if pick(&list) {
                return list;
            }
            assert!(
                Instant::now() < deadline,
                "the nodes never matched: {list:#?}"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    };
    wait_nodes(&mut owner, &|list| node_with(list, 0) == Some(first)).await;

    let found = search(&mut owner, voice, &track).await;
    assert_eq!(found.len(), 1);
    owner.ok(enqueue(voice, found)).await;
    let now = until(&mut owner, voice, "playing", |now| {
        now.state == Play::Playing as i32
    })
    .await;
    println!("playing on the first node: {now:#?}");
    let seekable = now.now_playing.unwrap().track.unwrap().is_seekable;
    assert!(seekable, "the test track must be seekable");

    let second = nodes(owner.ok(add("Second", second_port)).await)
        .nodes
        .iter()
        .find(|node| node.id != first)
        .unwrap()
        .id;
    wait_nodes(&mut owner, &|list| node_with(list, 0) == Some(second)).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    owner
        .ok(Kind::UpdateLavalinkNode(proto::UpdateLavalinkNode {
            node_id: first,
            settings: Some(proto::LavalinkNodeSettings {
                enabled: false,
                ..node_settings("First", first_port)
            }),
            password: None,
            draining: None,
        }))
        .await;
    let moved = Instant::now();
    wait_nodes(&mut owner, &|list| node_with(list, 1) == Some(second)).await;
    let now = until(&mut owner, voice, "playing on the second", |now| {
        now.state == Play::Playing as i32
    })
    .await;
    println!("moved in {:?}: {now:#?}", moved.elapsed());
    assert!(moved.elapsed() < Duration::from_secs(5));
    assert!(now.position_ms >= 3_000, "went on from where it was");
    drop(owner);

    let server = server.restart(configure).await;
    let mut owner = TestClient::connect(&server, key(1)).await;
    owner.identify("owner", None, None).await.unwrap();
    wait_nodes(&mut owner, &|list| node_with(list, 1) == Some(second)).await;
    let now = until(&mut owner, voice, "playing after the restart", |now| {
        now.state == Play::Playing as i32
    })
    .await;
    println!("after the restart: {now:#?}");

    // Real audio keeps reaching the bridge, so the watchdog stays quiet.
    let bridge = server.handle.music_bridge().unwrap();
    let before = bridge.stats().packets;
    tokio::time::sleep(Duration::from_secs(6)).await;
    let stats = bridge.stats();
    println!("bridge after 6 s: {stats:?}");
    assert!(stats.packets - before > 250, "{stats:?}");
    assert_eq!(stats.connections, 1);
}

#[tokio::test]
async fn recently_played_tracks_can_be_added_again() {
    let Room {
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found[..1].to_vec())).await;
    fake_plays(&lavalink, voice, "song 1").await;
    lavalink.fake.finish(voice);
    let played = until(&mut owner, voice, "song 1 played", |session| {
        !session.history.is_empty()
    })
    .await;
    let item = played.history[0].item_id.clone();

    let again = session(
        owner
            .ok(Kind::MusicEnqueue(proto::MusicEnqueue {
                channel_id: voice,
                result_ids: Vec::new(),
                history_item_ids: vec![item],
                before_item_id: None,
                placement: proto::MusicPlacement::End as i32,
            }))
            .await,
    );
    assert_eq!(title(again.now_playing.as_ref()), "song 1");
    let error = owner
        .error(Kind::MusicEnqueue(proto::MusicEnqueue {
            channel_id: voice,
            result_ids: Vec::new(),
            history_item_ids: vec!["not-in-history".to_owned()],
            before_item_id: None,
            placement: proto::MusicPlacement::End as i32,
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::NotFound);
}

#[tokio::test]
async fn vote_skip_mechanism_advances_when_votes_reach_needed() {
    let Room {
        mut owner,
        mut member,
        everyone,
        voice,
        lavalink,
        ..
    } = room().await;
    member.ok(join_voice(voice)).await;

    // Remove MUSIC_CONTROL from everyone so member cannot direct-skip.
    owner
        .ok(Kind::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some((Permissions::DEFAULT_EVERYONE - Permissions::MUSIC_CONTROL).bits()),
            ..Default::default()
        }))
        .await;

    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found.clone())).await;
    fake_plays(&lavalink, voice, "song 1").await;
    let playing_session = until(&mut owner, voice, "playing song 1", |s| {
        playing(s, "song 1")
    })
    .await;
    let track_id = playing_session
        .now_playing
        .as_ref()
        .unwrap()
        .item_id
        .clone();

    // 1. Vote skip is off by default -> returns InvalidArgument.
    let error = member
        .error(Kind::MusicVoteSkip(proto::MusicVoteSkip {
            channel_id: voice,
            expected_item_id: track_id.clone(),
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::InvalidArgument);

    // 2. Enable vote skip in settings (threshold 50%).
    owner
        .ok(Kind::UpdateMusicSettings(proto::UpdateMusicSettings {
            vote_skip: Some(true),
            vote_skip_percent: Some(50),
            ..Default::default()
        }))
        .await;

    // 3. Stale track id is rejected with MusicStale.
    let error = member
        .error(Kind::MusicVoteSkip(proto::MusicVoteSkip {
            channel_id: voice,
            expected_item_id: "wrong-id".to_owned(),
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::MusicStale);

    // 4. Two listeners (owner + member). 50% of 2 is 1. Member votes to skip.
    member
        .ok(Kind::MusicVoteSkip(proto::MusicVoteSkip {
            channel_id: voice,
            expected_item_id: track_id.clone(),
        }))
        .await;

    // Track advances!
    fake_plays(&lavalink, voice, "song 2").await;
    let now = until(&mut owner, voice, "advanced to song 2", |s| {
        playing(s, "song 2")
    })
    .await;
    assert_eq!(title(now.now_playing.as_ref()), "song 2");
    assert!(now.skip_votes.is_empty());
}

#[tokio::test]
async fn twenty_channels_play_together_under_load() {
    let Room {
        server,
        mut owner,
        mut member,
        everyone,
        voice,
        lavalink,
        node,
        ..
    } = room().await;

    // Grant everyone MUSIC_MANAGE so all members can add and control music in any voice channel.
    owner
        .ok(Kind::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some(
                (Permissions::DEFAULT_EVERYONE
                    | Permissions::MUSIC_MANAGE
                    | Permissions::MUSIC_MANAGE_QUEUE
                    | Permissions::MUSIC_ADD
                    | Permissions::MUSIC_CONTROL)
                    .bits(),
            ),
            ..Default::default()
        }))
        .await;

    // About 40 requests in a burst: `ok_paced` waits out the rate limit,
    // which a slow runner can reach.
    // Create 19 more voice channels (total 20 voice channels).
    let mut channels = vec![voice];
    for i in 1..20 {
        let channel = match owner
            .ok_paced(Kind::CreateChannel(proto::CreateChannel {
                kind: proto::ChannelKind::Voice as i32,
                name: format!("Voice {i}"),
                topic: None,
                parent_id: None,
            }))
            .await
        {
            Response::Channel(c) => c.id,
            other => panic!("expected channel, got {other:?}"),
        };
        channels.push(channel);
    }
    assert_eq!(channels.len(), 20);

    // Join two more users so we have 4 clients, each handling 5 channels (5 searches & 5 enqueues each).
    let code = invite(&mut owner, None).await;
    let (mut user3, _) = join(&server, 3, &code).await;
    let (mut user4, _) = join(&server, 4, &code).await;

    // Distribute channels across 4 clients to respect per-user rate limits (10 / 10 s).
    for (i, channel) in channels.iter().enumerate() {
        let client = match i / 5 {
            0 => &mut owner,
            1 => &mut member,
            2 => &mut user3,
            _ => &mut user4,
        };
        let found = search(client, *channel, &format!("track-{i}")).await;
        client
            .ok_paced(enqueue(*channel, found[..1].to_vec()))
            .await;
    }

    // Verify all 20 channels have active players in FakeLavalink.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let active = lavalink.fake.players.lock().unwrap().len();
        if active >= 20 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "expected 20 players, got {active}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Simulate playback for all 20 channels.
    for (i, channel) in channels.iter().enumerate() {
        fake_plays(&lavalink, *channel, &format!("track-{i} 1")).await;
    }

    // Verify node channels count in FetchLavalinkNodes.
    let list = nodes(
        owner
            .ok_paced(Kind::FetchLavalinkNodes(proto::FetchLavalinkNodes {
                watch: false,
            }))
            .await,
    );
    let n = list.nodes.iter().find(|n| n.id == node).unwrap();
    assert_eq!(n.channels, 20, "node reports 20 playing channels");

    // Check each channel's session:
    // Channel 0 has the owner listening -> Playing.
    // Channels 1..19 have no listeners -> Paused (paused_empty), ready with track loaded.
    for (i, channel) in channels.iter().enumerate() {
        let s = session(owner.ok_paced(snapshot_request(*channel)).await);
        assert_eq!(title(s.now_playing.as_ref()), format!("track-{i} 1"));
        if i == 0 {
            assert_eq!(s.state, Play::Playing as i32);
            assert!(!s.paused_empty);
        } else {
            assert_eq!(s.state, Play::Paused as i32);
            assert!(s.paused_empty);
        }
    }

    // When a listener joins an empty channel, it immediately unpauses.
    user3.ok(join_voice(channels[1])).await;
    let s = until(
        &mut user3,
        channels[1],
        "unpaused when listener joins",
        |s| s.state == Play::Playing as i32 && !s.paused_empty,
    )
    .await;
    assert_eq!(title(s.now_playing.as_ref()), "track-1 1");
}

/// Plan §21.2 L7: two people moving rows of Up next at the same time
/// always end with the same order, every track once (moves are relative,
/// §14.2).
#[tokio::test]
async fn two_people_moving_tracks_at_once_end_with_the_same_order() {
    let Room {
        mut owner,
        mut member,
        everyone,
        voice,
        lavalink,
        ..
    } = room().await;
    member.ok(join_voice(voice)).await;
    owner
        .ok(Kind::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some(
                (Permissions::DEFAULT_EVERYONE | Permissions::MUSIC_MANAGE_QUEUE).bits(),
            ),
            ..Default::default()
        }))
        .await;
    for query in ["song", "tune"] {
        let found = search(&mut owner, voice, query).await;
        owner.ok(enqueue(voice, found)).await;
    }
    fake_plays(&lavalink, voice, "song 1").await;
    let start = session(owner.ok(snapshot_request(voice)).await);
    let ids: Vec<String> = start
        .queue
        .iter()
        .map(|item| item.item_id.clone())
        .collect();
    assert!(ids.len() >= 4, "enough to move: {}", ids.len());
    let mut expected: Vec<String> = ids.clone();
    expected.sort();

    let move_before = |item: &str, before: Option<&str>| {
        Kind::MusicQueueMove(proto::MusicQueueMove {
            channel_id: voice,
            item_id: item.to_owned(),
            before_item_id: before.map(str::to_owned),
        })
    };
    let last = ids.len() - 1;
    for (mine, theirs) in [
        (
            move_before(&ids[last], Some(&ids[0])),
            move_before(&ids[0], None),
        ),
        (
            move_before(&ids[1], Some(&ids[2])),
            move_before(&ids[2], Some(&ids[1])),
        ),
        (
            move_before(&ids[0], Some(&ids[last])),
            move_before(&ids[last], Some(&ids[0])),
        ),
    ] {
        let (a, b) = tokio::join!(owner.request(mine), member.request(theirs));
        assert!(
            !matches!(a, Response::Error(_)) && !matches!(b, Response::Error(_)),
            "{a:?} {b:?}"
        );
    }

    let owners = session(owner.ok(snapshot_request(voice)).await);
    let members = session(member.ok(snapshot_request(voice)).await);
    let order = |session: &proto::MusicSession| -> Vec<String> {
        session
            .queue
            .iter()
            .map(|item| item.item_id.clone())
            .collect()
    };
    assert_eq!(order(&owners), order(&members));
    let mut seen = order(&owners);
    seen.sort();
    assert_eq!(seen, expected, "every track once");
}

/// Plan §14.3: after a snapshot, changes come as updates by item id. A
/// move sends one placement without the track; applied in order, the
/// updates give exactly the session the server holds.
#[tokio::test]
async fn changes_reach_listeners_as_small_updates() {
    let Room {
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    for query in ["song", "tune"] {
        let found = search(&mut owner, voice, query).await;
        owner.ok(enqueue(voice, found)).await;
    }
    fake_plays(&lavalink, voice, "song 1").await;
    let mut held = session(owner.ok(snapshot_request(voice)).await);
    owner.collect_for(Duration::from_millis(300)).await;

    let last = held.queue.last().unwrap().item_id.clone();
    let first = held.queue[0].item_id.clone();
    owner
        .ok(Kind::MusicQueueMove(proto::MusicQueueMove {
            channel_id: voice,
            item_id: last.clone(),
            before_item_id: Some(first.clone()),
        }))
        .await;
    owner.ok(playback(voice, Action::Pause)).await;
    owner.ok(playback(voice, Action::Next)).await;

    let updates: Vec<proto::MusicUpdate> = owner
        .collect_for(Duration::from_millis(500))
        .await
        .into_iter()
        .filter_map(|event| match event {
            proto::event::Kind::MusicUpdate(update) if update.channel_id == voice => Some(*update),
            proto::event::Kind::MusicSnapshot(_) => panic!("a whole snapshot for a change"),
            _ => None,
        })
        .collect();
    assert!(updates.len() >= 3, "{updates:#?}");
    let moved = updates[0].queue.as_ref().unwrap();
    assert_eq!(moved.placed.len(), 1);
    assert_eq!(moved.placed[0].item_id, last);
    assert_eq!(moved.placed[0].item, None, "the track is not sent again");
    assert_eq!(moved.placed[0].before_item_id.as_ref(), Some(&first));
    for update in &updates {
        assert_eq!(music_sync::apply(&mut held, update), Ok(true));
    }
    let server = session(owner.ok(snapshot_request(voice)).await);
    assert_eq!(held.queue, server.queue);
    assert_eq!(held.history, server.history);
    assert_eq!(held.now_playing, server.now_playing);
    assert_eq!(held.version, server.version);
}

/// A client that missed an update asks for the session; asking does not
/// subscribe it.
#[tokio::test]
async fn the_session_can_be_fetched_without_subscribing() {
    let Room {
        mut owner,
        mut member,
        voice,
        lavalink,
        ..
    } = room().await;
    let found = search(&mut owner, voice, "song").await;
    owner.ok(enqueue(voice, found)).await;
    fake_plays(&lavalink, voice, "song 1").await;

    let fetched = session(
        member
            .ok(Kind::FetchMusicSession(proto::FetchMusicSession {
                channel_id: voice,
            }))
            .await,
    );
    assert_eq!(title(fetched.now_playing.as_ref()), "song 1");

    owner.ok(playback(voice, Action::Pause)).await;
    let heard = member.collect_for(Duration::from_millis(400)).await;
    assert!(
        !heard.iter().any(|event| matches!(
            event,
            proto::event::Kind::MusicUpdate(_) | proto::event::Kind::MusicSnapshot(_)
        )),
        "not subscribed: {heard:?}"
    );
}
