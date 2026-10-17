//! A channel's music stream (Phase 3 plan §13.3, §18.4) from the node to
//! real clients: everyone in the channel hears it, as its own stream, not
//! a participant; listeners who muted music or are deafened get nothing.

use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_media::transport::{ReceivedAudio, VoiceEvent};
use opencord_voice::node::NodeCommand;
use opencord_voice::sfu::{MusicPacket, PeerState};
use tokio::sync::mpsc::UnboundedReceiver;

mod common;
use common::*;

/// Plays `frames` frames of music from a Lavalink `connection`, every
/// 20 ms, numbered from `first`.
async fn play(node: &Node, connection: u64, first: u16, frames: u16) {
    let mut every = tokio::time::interval(Duration::from_millis(20));
    for index in 0..frames {
        every.tick().await;
        let sequence = first.wrapping_add(index);
        node.node.music_frame(
            CHANNEL,
            MusicPacket {
                connection,
                sequence,
                timestamp: u32::from(sequence) * 960,
                opus: Arc::from(&[0xfc, 0xff, 0xfe][..]),
                received_at: Instant::now(),
            },
        );
    }
}

/// The music packets that arrive within `within`.
async fn music(events: &mut UnboundedReceiver<VoiceEvent>, within: Duration) -> Vec<ReceivedAudio> {
    let mut heard = Vec::new();
    let end = tokio::time::Instant::now() + within;
    while let Ok(Some(event)) = tokio::time::timeout_at(end, events.recv()).await {
        if let VoiceEvent::MusicAudio(audio) = event {
            heard.push(audio);
        }
    }
    heard
}

fn consecutive(heard: &[ReceivedAudio]) -> bool {
    heard.windows(2).all(|pair| pair[1].seq == pair[0].seq + 1)
}

#[tokio::test]
async fn everyone_in_the_channel_hears_the_music() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (_alice, mut alice_events) = join(&node, 1).await;
    let (_bob, mut bob_events) = join(&node, 2).await;

    let playing = play(&node, 1, 65_500, 50);
    let (_, alice_heard, bob_heard) = tokio::join!(
        playing,
        music(&mut alice_events, Duration::from_millis(1500)),
        music(&mut bob_events, Duration::from_millis(1500)),
    );

    for heard in [&alice_heard, &bob_heard] {
        assert!(heard.len() >= 48, "{} of 50 frames", heard.len());
        assert!(consecutive(heard), "through the wrap, without a gap");
        assert_eq!(heard[0].payload.as_ref(), &[0xfc, 0xff, 0xfe]);
        assert_eq!(heard[0].user_id, opencord_media::transport::MUSIC_SOURCE);
    }
}

/// Plan §21.2, L3: a 10-person channel hears each frame within 150 ms of
/// each other (here on one machine; a LAN adds little).
#[tokio::test]
async fn ten_listeners_hear_each_frame_together() {
    let _machine = shared_machine().await;
    let node = node().await;
    let mut listeners = Vec::new();
    for user_id in 1..=10 {
        listeners.push(join(&node, user_id).await);
    }

    let playing = play(&node, 1, 0, 100);
    let hearing = futures_util::future::join_all(
        listeners
            .iter_mut()
            .map(|(_, events)| music(events, Duration::from_millis(2500))),
    );
    let (_, heard) = tokio::join!(playing, hearing);

    let mut widest = Duration::ZERO;
    for seq in 10..90 {
        let arrivals: Vec<Instant> = heard
            .iter()
            .filter_map(|frames| frames.iter().find(|frame| frame.seq == seq))
            .map(|frame| frame.arrived)
            .collect();
        assert_eq!(arrivals.len(), 10, "frame {seq} reached everyone");
        let first = arrivals.iter().min().unwrap();
        let last = arrivals.iter().max().unwrap();
        widest = widest.max(*last - *first);
    }
    assert!(
        widest < Duration::from_millis(150),
        "the widest spread of a frame was {widest:?}"
    );
}

#[tokio::test]
async fn music_is_announced_not_a_participant() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (_alice, mut events) = join(&node, 1).await;

    play(&node, 1, 0, 1).await;

    wait_for(&mut events, |event| {
        matches!(event, VoiceEvent::Music { active: true })
    })
    .await;
    let others = music(&mut events, Duration::from_millis(300)).await;
    assert!(others.len() <= 1);
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, VoiceEvent::ClientConnected { .. }),
            "music never joins as someone"
        );
    }
}

#[tokio::test]
async fn someone_joining_mid_song_hears_it() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (_alice, _alice_events) = join(&node, 1).await;
    play(&node, 1, 0, 5).await;

    let (_carol, mut carol_events) = join(&node, 3).await;
    let (_, heard) = tokio::join!(
        play(&node, 1, 5, 25),
        music(&mut carol_events, Duration::from_millis(800)),
    );

    assert!(heard.len() >= 20, "{} frames", heard.len());
}

#[tokio::test]
async fn muted_and_deafened_listeners_get_no_music() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (_alice, mut alice_events) = join(&node, 1).await;
    let (bob, mut bob_events) = join(&node, 2).await;
    let (_dave, mut dave_events) = join(&node, 4).await;
    bob.set_music_muted(true);
    node.node.send(NodeCommand::Update {
        user_id: 4,
        channel_id: CHANNEL,
        state: PeerState {
            self_deaf: true,
            ..PeerState::default()
        },
        permissions: CONNECT,
    });
    tokio::time::sleep(Duration::from_millis(300)).await;

    let (_, alice_heard, bob_heard, dave_heard) = tokio::join!(
        play(&node, 1, 0, 25),
        music(&mut alice_events, Duration::from_millis(800)),
        music(&mut bob_events, Duration::from_millis(800)),
        music(&mut dave_events, Duration::from_millis(800)),
    );

    assert!(alice_heard.len() >= 20);
    assert!(bob_heard.is_empty(), "muted: {} frames", bob_heard.len());
    assert!(
        dave_heard.is_empty(),
        "deafened: {} frames",
        dave_heard.len()
    );

    bob.set_music_muted(false);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let (_, bob_heard) = tokio::join!(
        play(&node, 1, 25, 25),
        music(&mut bob_events, Duration::from_millis(800)),
    );
    assert!(bob_heard.len() >= 20, "unmuted: {} frames", bob_heard.len());
    assert!(
        consecutive(&bob_heard),
        "no gap where the muted frames were"
    );
}

#[tokio::test]
async fn a_failover_to_another_lavalink_continues_the_stream() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (_alice, mut events) = join(&node, 1).await;

    let (_, before) = tokio::join!(
        play(&node, 1, 1000, 20),
        music(&mut events, Duration::from_millis(600)),
    );
    let (_, after) = tokio::join!(
        play(&node, 2, 7, 20),
        music(&mut events, Duration::from_millis(600)),
    );

    let last = before.last().unwrap();
    let first = after.first().unwrap();
    assert_eq!(first.seq, last.seq + 1);
    assert!(first.timestamp.wrapping_sub(last.timestamp) >= 960);
    assert!(consecutive(&after));
}

#[tokio::test]
async fn stopping_the_music_ends_the_stream() {
    let _machine = shared_machine().await;
    let node = node().await;
    let (_alice, mut events) = join(&node, 1).await;
    play(&node, 1, 0, 3).await;
    wait_for(&mut events, |event| {
        matches!(event, VoiceEvent::Music { active: true })
    })
    .await;

    node.node.send(NodeCommand::StopMusic {
        channel_id: CHANNEL,
    });

    wait_for(&mut events, |event| {
        matches!(event, VoiceEvent::Music { active: false })
    })
    .await;
}
