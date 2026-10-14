//! A voice node on localhost with real clients (opencord-media's
//! transport): the voice gateway over WebSocket, media over UDP.

use std::time::{Duration, Instant};

use opencord_common::voice::close;
use opencord_media::transport::{AudioFrame, TransportError, VoiceConnection, VoiceEvent};
use opencord_voice::node::{NodeCommand, NodeEvent};
use opencord_voice::sfu::PeerState;
use tokio::sync::mpsc::UnboundedReceiver;

mod common;
use common::*;

fn frame(index: u8) -> AudioFrame {
    AudioFrame {
        payload: vec![0xf8, index, 0xff, 0xfe],
        position: u64::from(index) * 960,
        marker: index == 0,
        audio_level: -30,
        voice_activity: true,
    }
}

/// Sends a frame every 20 ms for `frames` frames.
async fn talk(connection: &VoiceConnection, frames: u8) {
    let mut every = tokio::time::interval(Duration::from_millis(20));
    for index in 0..frames {
        every.tick().await;
        connection.send_audio(frame(index));
    }
}

/// Every audio event that arrives within `within`: who, and when.
async fn heard(
    events: &mut UnboundedReceiver<VoiceEvent>,
    within: Duration,
) -> Vec<(i64, Instant)> {
    let mut heard = Vec::new();
    let end = tokio::time::Instant::now() + within;
    while let Ok(Some(event)) = tokio::time::timeout_at(end, events.recv()).await {
        if let VoiceEvent::Audio(audio) = event {
            heard.push((audio.user_id, audio.arrived));
        }
    }
    heard
}

/// What the node reported until it went quiet for `quiet`.
async fn node_events(node: &mut Node, quiet: Duration) -> Vec<NodeEvent> {
    let mut events = Vec::new();
    while let Ok(Some(event)) = tokio::time::timeout(quiet, node.events.recv()).await {
        events.push(event);
    }
    events
}

fn reports_someone_gone(events: &[NodeEvent]) -> bool {
    events
        .iter()
        .any(|event| matches!(event, NodeEvent::Disconnected { .. }))
}

fn update(user_id: i64, state: PeerState) -> NodeCommand {
    NodeCommand::Update {
        user_id,
        channel_id: CHANNEL,
        state,
        permissions: CONNECT,
    }
}

#[tokio::test]
async fn two_people_hear_each_other() {
    let node = node().await;
    let (alice, mut alice_events) = join(&node, 1).await;
    let (bob, mut bob_events) = join(&node, 2).await;
    wait_for(&mut alice_events, |event| {
        matches!(event, VoiceEvent::ClientConnected { user_id: 2, .. })
    })
    .await;

    talk(&alice, 10).await;
    let bob_heard = heard(&mut bob_events, Duration::from_millis(300)).await;
    talk(&bob, 10).await;
    let alice_heard = heard(&mut alice_events, Duration::from_millis(300)).await;

    assert!(bob_heard.len() >= 9, "Bob heard {} frames", bob_heard.len());
    assert!(bob_heard.iter().all(|(from, _)| *from == 1));
    assert!(
        alice_heard.len() >= 9,
        "Alice heard {} frames",
        alice_heard.len()
    );
    assert!(alice_heard.iter().all(|(from, _)| *from == 2));
}

#[tokio::test]
async fn a_server_mute_stops_forwarding_within_100_ms() {
    let node = node().await;
    let (alice, _alice_events) = join(&node, 1).await;
    let (_bob, mut bob_events) = join(&node, 2).await;
    let speaker = tokio::spawn(async move {
        talk(&alice, 50).await;
        alice
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let muted_at = Instant::now();
    node.node.send(update(
        1,
        PeerState {
            server_mute: true,
            ..PeerState::default()
        },
    ));
    let heard = heard(&mut bob_events, Duration::from_millis(500)).await;
    let _alice = speaker.await.unwrap();

    let last = heard.iter().map(|(_, at)| *at).max().unwrap();
    assert!(
        last <= muted_at + Duration::from_millis(100),
        "audio kept coming {:?} after the mute",
        last.saturating_duration_since(muted_at)
    );
}

#[tokio::test]
async fn deafened_people_receive_no_audio() {
    let node = node().await;
    let (alice, _alice_events) = join(&node, 1).await;
    let (_bob, mut bob_events) = join(&node, 2).await;
    node.node.send(update(
        2,
        PeerState {
            self_deaf: true,
            ..PeerState::default()
        },
    ));
    tokio::time::sleep(Duration::from_millis(50)).await;

    talk(&alice, 10).await;

    assert!(
        heard(&mut bob_events, Duration::from_millis(300))
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn a_dropped_gateway_resumes_without_interrupting_media() {
    let node = node().await;
    let (alice, mut alice_events) = join(&node, 1).await;
    let (_bob, mut bob_events) = join(&node, 2).await;

    let start = Instant::now();
    alice.drop_gateway();
    let talking = tokio::spawn(async move {
        talk(&alice, 40).await;
        alice
    });
    let heard = heard(&mut bob_events, Duration::from_millis(1_100)).await;
    let _alice = talking.await.unwrap();
    wait_for(&mut alice_events, |event| {
        matches!(event, VoiceEvent::Resumed)
    })
    .await;

    assert!(heard.len() >= 36, "Bob heard {} of 40 frames", heard.len());
    let mut arrivals: Vec<Instant> = heard.iter().map(|(_, at)| *at).collect();
    arrivals.sort();
    arrivals.insert(0, start);
    let longest_gap = arrivals
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .max()
        .unwrap();
    assert!(
        longest_gap < Duration::from_millis(200),
        "a gap of {longest_gap:?}"
    );
}

#[tokio::test]
async fn a_forged_token_is_refused() {
    let node = node().await;
    let mut forged = target(&node, 1, token_for(&node, 1));
    forged.token[0] ^= 1;

    let refused = VoiceConnection::connect(forged).await.err();

    assert!(matches!(
        refused,
        Some(TransportError::Refused(Some(close::AUTHENTICATION_FAILED)))
    ));
}

#[tokio::test]
async fn a_token_works_once() {
    let node = node().await;
    let token = token_for(&node, 1);
    let first = VoiceConnection::connect(target(&node, 1, token.clone())).await;

    let second = VoiceConnection::connect(target(&node, 1, token))
        .await
        .err();

    assert!(first.is_ok());
    assert!(matches!(
        second,
        Some(TransportError::Refused(Some(close::AUTHENTICATION_FAILED)))
    ));
}

#[tokio::test]
async fn the_main_server_can_disconnect_someone() {
    let mut node = node().await;
    let (_alice, mut alice_events) = join(&node, 1).await;
    let (_bob, mut bob_events) = join(&node, 2).await;
    let connected = tokio::time::timeout(WAIT, node.events.recv())
        .await
        .unwrap();
    assert!(matches!(connected, Some(NodeEvent::Connected { .. })));

    node.node.send(NodeCommand::Disconnect {
        user_id: 1,
        channel_id: CHANNEL,
    });

    let closed = wait_for(&mut alice_events, |event| {
        matches!(event, VoiceEvent::Closed { .. })
    })
    .await;
    assert!(matches!(
        closed,
        VoiceEvent::Closed {
            code: Some(close::DISCONNECTED)
        }
    ));
    wait_for(&mut bob_events, |event| {
        matches!(event, VoiceEvent::ClientDisconnected { user_id: 1 })
    })
    .await;
    let events = node_events(&mut node, Duration::from_millis(200)).await;
    assert!(
        !reports_someone_gone(&events),
        "the main server asked for it: {events:?}"
    );
}

#[tokio::test]
async fn a_replaced_connection_is_not_reported_as_gone() {
    let mut node = node().await;
    let (_first, mut first_events) = join(&node, 1).await;
    let (_second, _second_events) = join(&node, 1).await;
    wait_for(&mut first_events, |event| {
        matches!(
            event,
            VoiceEvent::Closed {
                code: Some(close::SESSION_REPLACED)
            }
        )
    })
    .await;

    let events = node_events(&mut node, Duration::from_millis(200)).await;

    assert!(!reports_someone_gone(&events), "{events:?}");
}

#[tokio::test]
async fn a_shutdown_reports_nobody_as_gone() {
    let mut node = node().await;
    let (_alice, mut alice_events) = join(&node, 1).await;

    node.node.shutdown();
    wait_for(&mut alice_events, |event| {
        matches!(
            event,
            VoiceEvent::Closed {
                code: Some(close::NODE_SHUTDOWN)
            }
        )
    })
    .await;
    let events = node_events(&mut node, Duration::from_millis(200)).await;

    assert!(!reports_someone_gone(&events), "{events:?}");
}

#[tokio::test]
async fn a_node_lets_nobody_in_until_it_knows_the_main_servers_key() {
    let node = start_node(false).await;

    let refused = VoiceConnection::connect(target(&node, 1, token_for(&node, 1)))
        .await
        .err();
    node.node.set_verifying_key(node.key.verifying_key());
    let (_alice, _alice_events) = join(&node, 1).await;

    assert!(matches!(
        refused,
        Some(TransportError::Refused(Some(close::AUTHENTICATION_FAILED)))
    ));
}

#[tokio::test]
async fn a_suspended_node_ends_every_session_and_takes_none_until_it_has_the_key_again() {
    let node = node().await;
    let (_alice, mut alice_events) = join(&node, 1).await;

    node.node.suspend();
    let closed = wait_for(&mut alice_events, |event| {
        matches!(event, VoiceEvent::Closed { .. })
    })
    .await;
    let refused = VoiceConnection::connect(target(&node, 2, token_for(&node, 2)))
        .await
        .err();
    node.node.set_verifying_key(node.key.verifying_key());
    let (_bob, _bob_events) = join(&node, 2).await;

    assert!(matches!(
        closed,
        VoiceEvent::Closed {
            code: Some(close::NODE_SHUTDOWN)
        }
    ));
    assert!(matches!(
        refused,
        Some(TransportError::Refused(Some(close::AUTHENTICATION_FAILED)))
    ));
}

#[tokio::test]
async fn the_node_counts_its_participants_and_traffic() {
    let node = node().await;
    let (alice, _alice_events) = join(&node, 1).await;
    let (_bob, mut bob_events) = join(&node, 2).await;
    let both = node.node.load();

    talk(&alice, 10).await;
    heard(&mut bob_events, Duration::from_millis(200)).await;
    node.node.send(NodeCommand::Disconnect {
        user_id: 2,
        channel_id: CHANNEL,
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let after = node.node.load();

    assert_eq!((both.channels, both.participants), (1, 2));
    assert_eq!((after.channels, after.participants), (1, 1));
    assert!(after.packets_in >= 10, "{after:?}");
    assert!(after.packets_out >= 10, "{after:?}");
    assert!(after.bytes_in > both.bytes_in, "{after:?}");
    assert!(after.bytes_out > both.bytes_out, "{after:?}");
}

#[tokio::test]
async fn pauses_and_talk_spurt_marks_reach_the_listener() {
    let node = node().await;
    let (alice, _alice_events) = join(&node, 1).await;
    let (_bob, mut bob_events) = join(&node, 2).await;
    let spurt = |position: u64, marker: bool| AudioFrame {
        payload: vec![0xf8, 1, 0xff, 0xfe],
        position,
        marker,
        audio_level: -30,
        voice_activity: true,
    };

    alice.send_audio(spurt(0, true));
    alice.send_audio(spurt(960, false));
    tokio::time::sleep(Duration::from_millis(40)).await;
    // A second of silence later.
    alice.send_audio(spurt(48_960, true));
    let mut received = Vec::new();
    while received.len() < 3 {
        if let VoiceEvent::Audio(audio) = wait_for(&mut bob_events, |event| {
            matches!(event, VoiceEvent::Audio(_))
        })
        .await
        {
            received.push((audio.timestamp, audio.marker));
        }
    }

    let first = received[0].0;
    let relative: Vec<(u32, bool)> = received
        .iter()
        .map(|(timestamp, marker)| (timestamp.wrapping_sub(first), *marker))
        .collect();
    assert_eq!(relative, [(0, true), (960, false), (48_960, true)]);
}

#[tokio::test]
async fn a_new_network_keeps_the_call_going() {
    let node = node().await;
    let (alice, mut alice_events) = join(&node, 1).await;
    let (bob, mut bob_events) = join(&node, 2).await;
    wait_for(&mut alice_events, |event| {
        matches!(event, VoiceEvent::ClientConnected { user_id: 2, .. })
    })
    .await;

    let changed_at = Instant::now();
    alice.simulate_network_change();
    let talking = tokio::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_millis(20));
        for index in 0..150u8 {
            every.tick().await;
            alice.send_audio(frame(index));
            bob.send_audio(frame(index));
        }
        (alice, bob)
    });
    let bob_heard = heard(&mut bob_events, Duration::from_millis(3_200)).await;
    let _ = talking.await.unwrap();
    let alice_heard = heard(&mut alice_events, Duration::from_millis(200)).await;

    // Within 3 s of the change, both hear the other again.
    let back = |heard: &[(i64, Instant)]| {
        heard
            .iter()
            .map(|(_, at)| *at)
            .find(|at| *at > changed_at + Duration::from_millis(100))
            .map(|at| at - changed_at)
    };
    let bob_back = back(&bob_heard).expect("Bob never heard Alice again");
    let alice_back = back(&alice_heard).expect("Alice never heard Bob again");
    assert!(bob_back < Duration::from_secs(3), "{bob_back:?}");
    assert!(alice_back < Duration::from_secs(3), "{alice_back:?}");
    assert!(
        bob_heard.len() >= 100,
        "Bob heard {} of 150",
        bob_heard.len()
    );
}

#[tokio::test]
async fn a_silent_participant_sends_almost_nothing() {
    let node = node().await;
    let (_alice, _alice_events) = join(&node, 1).await;
    let (_bob, _bob_events) = join(&node, 2).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let before = node.node.load();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let after = node.node.load();

    // Keep-alives only: connectivity checks and RTCP, from both people.
    let bits_per_second = (after.bytes_in - before.bytes_in) * 8 / 3 / 2;
    assert!(bits_per_second < 3_000, "{bits_per_second} bit/s each");
}

#[tokio::test]
async fn speaking_reaches_the_main_server_at_most_every_ten_seconds() {
    let mut node = node().await;
    let (alice, _alice_events) = join(&node, 1).await;
    let _ = node_events(&mut node, Duration::from_millis(300)).await;

    alice.set_speaking(1);
    alice.set_speaking(0);
    alice.set_speaking(1);
    let events = node_events(&mut node, Duration::from_millis(500)).await;

    let spoke: Vec<_> = events
        .iter()
        .filter(|event| matches!(event, NodeEvent::Spoke { .. }))
        .collect();
    assert_eq!(spoke.len(), 1, "{events:?}");
    assert!(matches!(
        spoke[0],
        NodeEvent::Spoke {
            user_id: 1,
            channel_id: CHANNEL
        }
    ));
}

#[tokio::test]
async fn the_connection_stats_match_what_arrives() {
    use opencord_media::transport::impairment::Impairment;

    let _machine = whole_machine().await;
    let node = node().await;
    let (alice, _alice_events) = join(&node, 1).await;
    let (bob, mut bob_events) = join(&node, 2).await;
    let delay = Duration::from_millis(50);
    bob.set_impairment(
        Impairment {
            loss: 0.2,
            latency: delay,
            jitter: Duration::from_millis(15),
            ..Impairment::default()
        },
        Impairment {
            latency: delay,
            ..Impairment::default()
        },
    );

    // Five seconds of Alice talking, 250 frames.
    let talking = tokio::spawn(async move {
        let mut every = tokio::time::interval(Duration::from_millis(20));
        for index in 0..250u32 {
            every.tick().await;
            alice.send_audio(AudioFrame {
                payload: vec![0xf8, 1, 0xff, 0xfe],
                position: u64::from(index) * 960,
                marker: index == 0,
                audio_level: -30,
                voice_activity: true,
            });
        }
        alice
    });
    let mut arrived = 0u32;
    let mut stats = Vec::new();
    let end = tokio::time::Instant::now() + Duration::from_secs(6);
    while let Ok(Some(event)) = tokio::time::timeout_at(end, bob_events.recv()).await {
        match event {
            VoiceEvent::Audio(_) => arrived += 1,
            VoiceEvent::Stats(report) => stats.push(report),
            _ => {}
        }
    }
    let _alice = talking.await.unwrap();

    let measured = 1.0 - f64::from(arrived) / 250.0;
    let last = stats.last().copied().expect("no statistics");
    println!(
        "measured loss {measured:.3}, {} reports, last {last:?}",
        stats.len()
    );
    assert!(stats.len() >= 4, "{} reports in 6 s", stats.len());
    let reported: Vec<f32> = stats.iter().filter_map(|report| report.loss_in).collect();
    let mean = reported.iter().sum::<f32>() / reported.len().max(1) as f32;
    assert!(
        (f64::from(mean) - measured).abs() < 0.08,
        "reported {mean:.3} lost, measured {measured:.3}"
    );
    let rtt = stats
        .iter()
        .filter_map(|report| report.rtt)
        .max()
        .expect("no round trip");
    assert!(
        rtt >= 2 * delay && rtt < 2 * delay + Duration::from_millis(100),
        "round trip {rtt:?}"
    );
    let jitter = stats
        .iter()
        .filter_map(|report| report.jitter)
        .max()
        .expect("no jitter");
    assert!(
        jitter > Duration::from_millis(2) && jitter < Duration::from_millis(40),
        "jitter {jitter:?}"
    );
    assert!(last.bitrate_in > 0 && last.bitrate_out > 0, "{last:?}");
}
