//! Video through a voice node on localhost (plan §6, §13, §15): real
//! clients publishing the synthetic test pattern and watching it.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opencord_common::permissions::Permissions;
use opencord_common::voice::close;
use opencord_media::transport::gateway::{self, GatewayUrl, Next};
use opencord_media::transport::impairment::Impairment;
use opencord_media::transport::transform::XorTransform;
use opencord_media::transport::{
    AudioFrame, ConnectOptions, Layer, ReceivedVideo, SinkWant, TrackError, TrackKind,
    VoiceConnection, VoiceEvent,
};
use opencord_media::video::pattern::{TestPattern, check};
use opencord_proto::v1::{ErrorCode, ScreenShareResolution};
use opencord_proto::voice::v1 as voice;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

mod common;
use common::*;

const VIDEO: u64 = CONNECT | Permissions::VIDEO.bits();

fn camera_layers() -> Vec<Layer> {
    let layer = |rid: &str, width, height, fps, max_bitrate| Layer {
        rid: rid.to_owned(),
        width,
        height,
        fps,
        max_bitrate,
    };
    vec![
        layer("l", 320, 180, 15, 150_000),
        layer("m", 640, 360, 30, 500_000),
        layer("h", 1280, 720, 30, 1_500_000),
    ]
}

/// A client sending the test pattern as the node asks: the layers it
/// wants, keyframes on request.
struct Publisher {
    connection: Arc<VoiceConnection>,
    pattern: Arc<Mutex<TestPattern>>,
    /// Every other event it got.
    seen: Arc<Mutex<Vec<VoiceEvent>>>,
    task: JoinHandle<()>,
}

impl Publisher {
    async fn start(
        connection: VoiceConnection,
        mut events: UnboundedReceiver<VoiceEvent>,
        pattern: TestPattern,
    ) -> Result<Self, TrackError> {
        connection
            .publish_track(pattern.request(TrackKind::Camera))
            .await?;
        let connection = Arc::new(connection);
        let pattern = Arc::new(Mutex::new(pattern));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let task = {
            let (connection, pattern, seen) = (
                Arc::clone(&connection),
                Arc::clone(&pattern),
                Arc::clone(&seen),
            );
            tokio::spawn(async move {
                loop {
                    let due = pattern.lock().unwrap().next_due();
                    let wake = due.unwrap_or_else(|| Instant::now() + Duration::from_millis(50));
                    tokio::select! {
                        event = events.recv() => match event {
                            Some(VoiceEvent::Encode { layers, fps_scale, size_scale, .. }) => {
                                pattern.lock().unwrap().encode(&layers, fps_scale, size_scale);
                            }
                            Some(VoiceEvent::KeyframeRequested { layer, .. }) => {
                                pattern.lock().unwrap().keyframe(layer);
                            }
                            Some(other) => seen.lock().unwrap().push(other),
                            None => return,
                        },
                        () = tokio::time::sleep_until(wake.into()) => {
                            let frames = pattern.lock().unwrap().frames(Instant::now());
                            for frame in frames {
                                connection.send_video(frame);
                            }
                        }
                    }
                }
            })
        };
        Ok(Self {
            connection,
            pattern,
            seen,
            task,
        })
    }

    fn saw(&self, pick: impl Fn(&VoiceEvent) -> bool) -> bool {
        self.seen.lock().unwrap().iter().any(pick)
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The pictures that arrive within `within`.
async fn frames_within(
    events: &mut UnboundedReceiver<VoiceEvent>,
    within: Duration,
) -> Vec<ReceivedVideo> {
    let mut frames = Vec::new();
    let end = tokio::time::Instant::now() + within;
    while let Ok(Some(event)) = tokio::time::timeout_at(end, events.recv()).await {
        if let VoiceEvent::Video(frame) = event {
            frames.push(frame);
        }
    }
    frames
}

/// Waits until a picture of a layer `pick` likes arrives; returns when.
async fn layer_arrives(
    events: &mut UnboundedReceiver<VoiceEvent>,
    within: Duration,
    pick: impl Fn(u8) -> bool,
) -> Option<Instant> {
    let end = tokio::time::Instant::now() + within;
    while let Ok(Some(event)) = tokio::time::timeout_at(end, events.recv()).await {
        if let VoiceEvent::Video(frame) = event
            && pick(frame.layer)
        {
            return Some(frame.arrived);
        }
    }
    None
}

fn want(track_id: &str, max_height: u32) -> Vec<SinkWant> {
    vec![SinkWant {
        track_id: track_id.to_owned(),
        max_height,
    }]
}

async fn camera_publisher(node: &Node, user_id: i64, track_id: &str) -> Publisher {
    let (connection, events) = join_with(
        node,
        user_id,
        token_with(node, user_id, VIDEO, None),
        ConnectOptions::default(),
    )
    .await;
    Publisher::start(
        connection,
        events,
        TestPattern::new(track_id, camera_layers(), Instant::now()),
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_published_camera_reaches_whoever_wants_it_intact() {
    // Which layer it settles on depends on the bandwidth estimate: timing.
    let _machine = whole_machine().await;
    let node = node().await;
    let (bob, mut bob_events) = join(&node, 2).await;
    let _alice = camera_publisher(&node, 1, "cam-1").await;
    let track = wait_for(&mut bob_events, |event| {
        matches!(event, VoiceEvent::Track { user_id: 1, .. })
    })
    .await;
    let VoiceEvent::Track { track, .. } = track else {
        unreachable!()
    };
    assert_eq!(track.kind, TrackKind::Camera);
    assert_eq!(track.layers.len(), 3);

    bob.set_sink_wants(want("cam-1", 360));
    let frames = frames_within(&mut bob_events, Duration::from_secs(3)).await;

    assert!(frames.len() > 30, "{} frames", frames.len());
    assert!(frames[0].keyframe);
    for frame in &frames {
        let checked = check(&frame.nal_units).expect("a picture arrived damaged");
        assert_eq!(checked.layer, frame.layer);
        assert!(frame.layer <= 1, "never taller than the tile");
        assert_eq!(frame.user_id, 1);
    }
    // A debug build's bandwidth estimate can dip at first; it settles on
    // the tile's layer.
    let layers: Vec<u8> = frames.iter().map(|frame| frame.layer).collect();
    assert_eq!(layers.last(), Some(&1), "{layers:?}");
    for run in frames.chunk_by(|a, b| a.layer == b.layer) {
        let numbers: Vec<u64> = run
            .iter()
            .filter_map(|frame| check(&frame.nal_units))
            .map(|checked| checked.number)
            .collect();
        assert!(
            numbers.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "a picture went missing: {numbers:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hidden_tile_gets_no_video() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (bob, mut bob_events) = join(&node, 2).await;
    let _alice = camera_publisher(&node, 1, "cam-1").await;
    bob.set_sink_wants(want("cam-1", 180));
    assert!(
        !frames_within(&mut bob_events, Duration::from_secs(2))
            .await
            .is_empty()
    );

    bob.set_sink_wants(Vec::new());
    frames_within(&mut bob_events, Duration::from_millis(300)).await;

    assert!(
        frames_within(&mut bob_events, Duration::from_secs(2))
            .await
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unused_layers_stop_being_encoded() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (bob, mut bob_events) = join(&node, 2).await;
    let alice = camera_publisher(&node, 1, "cam-1").await;
    bob.set_sink_wants(want("cam-1", 180));

    frames_within(&mut bob_events, Duration::from_secs(3)).await;

    assert_eq!(alice.pattern.lock().unwrap().active(), vec!["l".to_owned()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_camera_needs_the_permission_and_room_in_the_channel() {
    let _machine = shared_machine().await;
    let node = node().await;
    let limits = voice::Limits {
        screen_share_max_resolution: ScreenShareResolution::ScreenShareResolution720p as i32,
        screen_share_max_fps: 30,
        voice_bitrate: 64_000,
        camera_allowed: true,
        max_camera_participants: 1,
        max_stream_viewers: 50,
    };
    let pattern = |id| TestPattern::new(id, camera_layers(), Instant::now());
    let (carol, _carol_events) = join(&node, 3).await;
    let refused = carol
        .publish_track(pattern("cam-3").request(TrackKind::Camera))
        .await;
    assert!(
        matches!(&refused, Err(TrackError::Refused { reason, .. }) if *reason == ErrorCode::Forbidden as i32),
        "{refused:?}"
    );

    let (alice, _alice_events) = join_with(
        &node,
        1,
        token_with(&node, 1, VIDEO, Some(limits)),
        ConnectOptions::default(),
    )
    .await;
    alice
        .publish_track(pattern("cam-1").request(TrackKind::Camera))
        .await
        .unwrap();
    let (bob, _bob_events) = join_with(
        &node,
        2,
        token_with(&node, 2, VIDEO, Some(limits)),
        ConnectOptions::default(),
    )
    .await;
    let full = bob
        .publish_track(pattern("cam-2").request(TrackKind::Camera))
        .await;

    assert!(
        matches!(&full, Err(TrackError::Refused { reason, .. }) if *reason == ErrorCode::CameraLimit as i32),
        "{full:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_track_far_over_its_ceiling_is_stopped() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (bob, mut bob_events) = join(&node, 2).await;
    let (connection, events) = join_with(
        &node,
        1,
        token_with(&node, 1, VIDEO, None),
        ConnectOptions::default(),
    )
    .await;
    let mut pattern = TestPattern::new("cam-1", camera_layers()[..1].to_vec(), Instant::now());
    pattern.set_bitrate(0, 450_000);
    let alice = Publisher::start(connection, events, pattern).await.unwrap();
    bob.set_sink_wants(want("cam-1", 180));

    wait_for(&mut bob_events, |event| {
        matches!(event, VoiceEvent::TrackRemoved { user_id: 1, .. })
    })
    .await;

    let stopped = |event: &VoiceEvent| {
        matches!(
            event,
            VoiceEvent::TrackStopped { reason, .. } if *reason == ErrorCode::QualityLimit as i32
        )
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while !alice.saw(stopped) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(alice.saw(stopped), "the publisher was not told");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn someone_who_joins_later_sees_the_tracks_already_there() {
    let _machine = shared_machine().await;
    let node = node().await;
    let _alice = camera_publisher(&node, 1, "cam-1").await;

    let (bob, mut bob_events) = join(&node, 2).await;
    wait_for(&mut bob_events, |event| {
        matches!(event, VoiceEvent::Track { user_id: 1, track } if track.track_id == "cam-1")
    })
    .await;
    bob.set_sink_wants(want("cam-1", 180));

    let frames = frames_within(&mut bob_events, Duration::from_secs(2)).await;
    assert!(!frames.is_empty());
    assert!(frames.iter().all(|frame| frame.layer == 0));
}

/// A raw voice gateway client that sends `wants` MediaSinkWants in a
/// burst; returns how the node closed, if it did within a second.
async fn send_wants_burst(node: &Node, user_id: i64, wants: usize) -> Option<Option<u16>> {
    let url = GatewayUrl::parse(&node.gateway).unwrap();
    let mut socket = gateway::open(&url, None).await.unwrap();
    let Next::Envelope(_) = gateway::next(&mut socket).await else {
        panic!("no Hello");
    };
    let identify = voice::envelope::Payload::Identify(voice::Identify {
        token: token_for(node, user_id),
        user_id,
        session_id: format!("session-{user_id}"),
        channel_id: CHANNEL,
        client_caps: None,
        max_e2ee_version: 0,
    });
    gateway::send(&mut socket, identify).await.unwrap();
    let Next::Envelope(_) = gateway::next(&mut socket).await else {
        panic!("no Ready");
    };
    for _ in 0..wants {
        let payload =
            voice::envelope::Payload::MediaSinkWants(voice::MediaSinkWants { wants: Vec::new() });
        gateway::send(&mut socket, payload).await.unwrap();
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, gateway::next(&mut socket)).await {
            Ok(Next::Closed(code)) => return Some(code),
            Ok(Next::Envelope(_)) => {}
            Err(_) => return None,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn more_than_twenty_wants_a_second_close_the_gateway() {
    let _machine = shared_machine().await;
    let node = node().await;

    assert_eq!(send_wants_burst(&node, 1, 15).await, None);
    assert_eq!(
        send_wants_burst(&node, 2, 25).await,
        Some(Some(close::RATE_LIMITED))
    );
}

fn audio(index: u8) -> AudioFrame {
    AudioFrame {
        payload: vec![0xf8, index, 0x5a, 0x00, 0x77],
        position: u64::from(index) * 960,
        marker: index == 0,
        audio_level: -30,
        voice_activity: true,
    }
}

/// Plan §13's proof: with every payload XORed end to end, audio and video
/// (including layer switches) still flow through the node, which reads
/// only RTP headers and extensions.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_xor_transform_end_to_end_does_not_stop_the_node() {
    let _machine = shared_machine().await;
    let node = node().await;
    let xor = || ConnectOptions {
        transform: Some(Box::new(XorTransform { key: 0xa5 })),
    };
    let (bob, mut bob_events) = join_with(&node, 2, token_for(&node, 2), xor()).await;
    let (carol, mut carol_events) = join(&node, 3).await;
    let (connection, events) = join_with(&node, 1, token_with(&node, 1, VIDEO, None), xor()).await;
    let alice = Publisher::start(
        connection,
        events,
        TestPattern::new("cam-1", camera_layers(), Instant::now()),
    )
    .await
    .unwrap();
    bob.set_sink_wants(want("cam-1", 360));
    carol.set_sink_wants(want("cam-1", 360));
    let mut layers_seen = Vec::new();

    for height in [360, 180, 360] {
        bob.set_sink_wants(want("cam-1", height));
        let frames = frames_within(&mut bob_events, Duration::from_secs(4)).await;
        assert!(!frames.is_empty(), "nothing at {height}p");
        for frame in &frames {
            let checked = check(&frame.nal_units).expect("XOR undone, the picture is intact");
            assert_eq!(checked.layer, frame.layer);
        }
        layers_seen.extend(frames.iter().map(|frame| frame.layer));
    }
    let carols = frames_within(&mut carol_events, Duration::from_millis(500)).await;
    assert!(!carols.is_empty());
    assert!(
        carols.iter().all(|frame| check(&frame.nal_units).is_none()),
        "without the key, every picture is scrambled"
    );
    assert!(layers_seen.contains(&0) && layers_seen.contains(&1));

    for index in 0..5 {
        alice.connection.send_audio(audio(index));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut heard = Vec::new();
    let end = tokio::time::Instant::now() + Duration::from_millis(500);
    while let Ok(Some(event)) = tokio::time::timeout_at(end, bob_events.recv()).await {
        if let VoiceEvent::Audio(audio) = event {
            heard.push(audio.payload.to_vec());
        }
    }
    let said: Vec<Vec<u8>> = (0..5).map(|index| audio(index).payload).collect();
    assert_eq!(heard, said);
}

/// Plan V4's acceptance: with a 500 kbps cap on a receiver's downlink, the
/// node moves it to a lower layer within 2 s, and back up once the cap is
/// lifted.
///
/// Real time on one machine: a debug build's processing delays distort the
/// bandwidth estimate, so this runs in release builds (`cargo test
/// --release`); the SFU's own tests check the same on a virtual clock in
/// every build.
#[tokio::test(flavor = "multi_thread", worker_threads = 12)]
#[cfg_attr(debug_assertions, ignore = "needs a release build's timing")]
async fn a_500_kbps_receiver_gets_a_lower_layer_within_2_s_and_recovers() {
    let _machine = whole_machine().await;
    let node = node().await;
    let (bob, mut bob_events) = join(&node, 2).await;
    let _alice = camera_publisher(&node, 1, "cam-1").await;
    bob.set_sink_wants(want("cam-1", 720));
    // Both ends ramp up: the node probes the receiver's downlink before it
    // asks for the top layer, then the sender probes its uplink for it.
    let top = layer_arrives(&mut bob_events, Duration::from_secs(25), |layer| layer == 2).await;
    assert!(top.is_some(), "never reached the top layer");
    frames_within(&mut bob_events, Duration::from_secs(2)).await;

    let capped = Impairment {
        rate: 500_000,
        ..Impairment::default()
    };
    bob.set_impairment(capped, Impairment::default());
    let capped_at = Instant::now();
    let lower = layer_arrives(&mut bob_events, Duration::from_secs(5), |layer| layer < 2).await;
    let after = lower.map(|at| at.saturating_duration_since(capped_at));
    assert!(
        after.is_some_and(|after| after <= Duration::from_secs(2)),
        "a lower layer after {after:?}"
    );

    bob.set_impairment(Impairment::default(), Impairment::default());
    let back = layer_arrives(&mut bob_events, Duration::from_secs(25), |layer| layer == 2).await;
    assert!(back.is_some(), "never came back up");
}

/// Polls `done` until it holds or `within` passes; returns whether it held.
async fn eventually(within: Duration, done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    done()
}

/// Plan §7.10 on a real connection: a sender whose uplink carries 1 Mbit/s
/// stops encoding its camera's top layer; the node moves its receiver to a
/// layer the sender still encodes; the top layer is encoded again once the
/// cap is lifted and the estimator has probed for room.
#[tokio::test(flavor = "multi_thread", worker_threads = 12)]
async fn a_capped_uplink_stops_encoding_the_cameras_top_layer_until_lifted() {
    let _machine = whole_machine().await;
    let node = node().await;
    let (bob, mut bob_events) = join(&node, 2).await;
    let alice = camera_publisher(&node, 1, "cam-1").await;
    bob.set_sink_wants(want("cam-1", 720));
    let top = layer_arrives(&mut bob_events, Duration::from_secs(25), |layer| layer == 2).await;
    assert!(top.is_some(), "never reached the top layer");
    let encoding = |rid: &str| {
        alice
            .pattern
            .lock()
            .unwrap()
            .active()
            .contains(&rid.to_owned())
    };

    let capped = Impairment {
        rate: 1_000_000,
        ..Impairment::default()
    };
    alice
        .connection
        .set_impairment(Impairment::default(), capped);
    let capped_at = Instant::now();
    assert!(
        eventually(Duration::from_secs(5), || !encoding("h")).await,
        "still encoding the top layer"
    );
    let lower = layer_arrives(&mut bob_events, Duration::from_secs(5), |layer| layer < 2).await;
    assert!(
        lower.is_some_and(|at| at.saturating_duration_since(capped_at) <= Duration::from_secs(5)),
        "the receiver got no lower layer"
    );

    alice
        .connection
        .set_impairment(Impairment::default(), Impairment::default());
    assert!(
        eventually(Duration::from_secs(25), || encoding("h")).await,
        "the top layer never came back"
    );
}
