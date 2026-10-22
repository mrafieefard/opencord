//! Simulcast forwarding (plan §6) with real str0m clients.

use str0m::media::KeyframeRequestKind;
use str0m_netem::{DataSize, Probability, RandomLoss};

use super::*;

fn three(net: &mut Net) -> (usize, usize, usize) {
    let alice = net.join(1001, PeerState::default());
    let bob = net.join(1002, PeerState::default());
    let carol = net.join(1003, PeerState::default());
    (alice, bob, carol)
}

/// Bits per second of video `client` got on `ssrc` over the last
/// `window`.
fn received_rate(net: &Net, client: usize, ssrc: u32, window: Duration) -> u64 {
    let since = net.now - window;
    let bytes: usize = net.clients[client]
        .video_on(ssrc, since)
        .iter()
        .map(|packet| packet.bytes)
        .sum();
    bytes as u64 * 8 * 1000 / window.as_millis() as u64
}

#[test]
fn a_wanted_track_comes_as_the_tallest_layer_that_fits_the_tile() {
    let mut net = Net::new();
    let (alice, bob, carol) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 720)]);
    net.want(carol, &[("cam-a", 360)]);

    net.run(Duration::from_secs(8));

    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(2));
    let since = net.now - Duration::from_secs(2);
    let carol_layers: Vec<u8> = net.clients[carol]
        .video_on(track.ssrc, since)
        .iter()
        .filter_map(|packet| packet.marking.map(|marking| marking.layer))
        .collect();
    assert!(!carol_layers.is_empty());
    assert!(
        carol_layers.iter().all(|layer| *layer == 1),
        "{carol_layers:?}"
    );
    assert!(net.clients[alice].video.is_empty(), "no echo of her own");
}

#[test]
fn nobody_gets_a_track_they_did_not_ask_for() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());

    net.run(Duration::from_secs(2));

    assert!(net.clients[bob].video.is_empty());
}

#[test]
fn a_hidden_tile_gets_no_video_bytes() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 360)]);
    net.run(Duration::from_secs(2));
    assert!(!net.clients[bob].video.is_empty());

    net.want(bob, &[]);
    net.run(Duration::from_millis(300));
    let packets = net.clients[bob].raw.get(&track.ssrc).copied().unwrap_or(0);
    let resends = net.clients[bob]
        .raw
        .get(&track.rtx_ssrc)
        .copied()
        .unwrap_or(0);
    net.run(Duration::from_secs(3));

    assert_eq!(
        net.clients[bob].raw.get(&track.ssrc).copied().unwrap_or(0),
        packets
    );
    assert_eq!(
        net.clients[bob]
            .raw
            .get(&track.rtx_ssrc)
            .copied()
            .unwrap_or(0),
        resends
    );
}

#[test]
fn switching_layers_keeps_one_continuous_stream() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 360)]);
    net.run(Duration::from_secs(3));
    let switched_at = net.now;

    net.want(bob, &[("cam-a", 180)]);
    net.run(Duration::from_secs(2));

    let packets = net.clients[bob].video_on(track.ssrc, net.clients[bob].start);
    let seqs: Vec<u64> = packets.iter().map(|packet| packet.seq).collect();
    assert!(
        seqs.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "gaps or repeats: {seqs:?}"
    );
    let timestamps: Vec<u32> = packets.iter().map(|packet| packet.timestamp).collect();
    assert!(
        timestamps
            .windows(2)
            .all(|pair| pair[1].wrapping_sub(pair[0]) < 90_000)
    );
    let first_low = packets
        .iter()
        .find(|packet| packet.at >= switched_at && packet.marking.is_some_and(|m| m.layer == 0))
        .expect("the low layer arrived");
    let marking = first_low.marking.unwrap();
    assert!(marking.keyframe && marking.start, "{marking:?}");
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(0));
}

#[test]
fn a_switch_asks_the_sender_for_a_keyframe_at_most_once_a_second_per_layer() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.run(Duration::from_millis(100));
    net.clients[alice].keyframe_requests.clear();

    // A tile that keeps changing size.
    for round in 0..30 {
        let height = if round % 2 == 0 { 360 } else { 180 };
        net.want(bob, &[("cam-a", height)]);
        net.run(Duration::from_millis(100));
    }

    let asked = &net.clients[alice].keyframe_requests;
    for rid in ["l", "m"] {
        let count = asked.iter().filter(|r| r.as_deref() == Some(rid)).count();
        assert!((1..=4).contains(&count), "{count} requests for {rid}");
    }
}

#[test]
fn a_receivers_keyframe_request_reaches_the_sender() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 360)]);
    net.run(Duration::from_secs(3));
    net.clients[alice].keyframe_requests.clear();

    net.clients[bob]
        .rtc
        .direct_api()
        .stream_rx(&Ssrc::from(track.ssrc))
        .unwrap()
        .request_keyframe(KeyframeRequestKind::Pli);
    net.run(Duration::from_millis(300));

    assert_eq!(
        net.clients[alice].keyframe_requests,
        vec![Some("m".to_owned())]
    );
}

#[test]
fn the_sender_is_asked_only_for_layers_someone_needs() {
    let mut net = Net::new();
    let (alice, bob, carol) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());

    net.run(Duration::from_millis(1500));
    assert_eq!(net.layer_wants(alice, "cam-a"), Some(vec![]));

    net.want(bob, &[("cam-a", 180)]);
    net.run(Duration::from_secs(2));
    assert_eq!(net.layer_wants(alice, "cam-a"), Some(vec!["l".to_owned()]));
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(0));

    net.want(carol, &[("cam-a", 720)]);
    net.run(Duration::from_secs(8));
    let asked = net.layer_wants(alice, "cam-a").unwrap();
    assert!(
        asked.contains(&"l".to_owned()) && asked.contains(&"h".to_owned()),
        "{asked:?}"
    );
    assert_eq!(net.clients[carol].last_layer(track.ssrc), Some(2));
}

#[test]
fn a_track_far_over_its_ceiling_is_stopped() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let mut track = camera("cam-a", 5000);
    track.layers.truncate(1);
    net.publish(alice, track.clone());
    net.clients[alice].publishing[0].rate[0] = 400_000;
    net.want(bob, &[("cam-a", 180)]);

    net.run(Duration::from_secs(4));
    assert!(
        !net.events
            .iter()
            .any(|e| matches!(e, SfuEvent::TrackStopped { .. }))
    );
    net.run(Duration::from_secs(3));

    let stopped = net.events.iter().any(|event| {
        matches!(event, SfuEvent::TrackStopped { peer, track_id }
            if *peer == net.clients[alice].peer && track_id == "cam-a")
    });
    assert!(stopped);
    net.run(Duration::from_millis(300));
    let received = net.clients[bob].video.len();
    net.run(Duration::from_secs(1));
    assert_eq!(net.clients[bob].video.len(), received);
}

#[test]
fn a_track_a_little_over_its_ceiling_carries_on() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let mut track = camera("cam-a", 5000);
    track.layers.truncate(1);
    net.publish(alice, track.clone());
    net.clients[alice].publishing[0].rate[0] = 170_000;
    net.want(bob, &[("cam-a", 180)]);

    net.run(Duration::from_secs(8));

    assert!(
        !net.events
            .iter()
            .any(|e| matches!(e, SfuEvent::TrackStopped { .. }))
    );
    assert!(received_rate(&net, bob, track.ssrc, Duration::from_secs(2)) > 100_000);
}

#[test]
fn inbound_beyond_the_cap_is_dropped() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let mut track = camera("cam-a", 5000);
    track.layers.truncate(1);
    net.publish(alice, track.clone());
    // Far over what a 150 kbps track and voice may send.
    net.clients[alice].publishing[0].rate[0] = 3_000_000;
    net.want(bob, &[("cam-a", 180)]);

    net.run(Duration::from_secs(3));

    let rate = received_rate(&net, bob, track.ssrc, Duration::from_secs(1));
    assert!(rate < 700_000, "{rate} bit/s forwarded");
}

#[test]
fn a_capped_downlink_gets_a_lower_layer_and_recovers() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 720)]);
    net.run(Duration::from_secs(10));
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(2));

    let capped_at = net.now;
    net.limit_downlink(
        bob,
        Some(NetemConfig::new().link(Bitrate::kbps(500), DataSize::bytes(12_500))),
    );
    net.run(Duration::from_secs(10));
    let lower = net.clients[bob]
        .video_on(track.ssrc, capped_at)
        .into_iter()
        .find(|packet| packet.marking.is_some_and(|m| m.layer < 2))
        .map(|packet| packet.at.duration_since(capped_at))
        .expect("switched to a lower layer");
    assert!(lower <= Duration::from_secs(2), "after {lower:?}");
    // Once the estimate settles, 500 kbps holds the low layer, not the
    // middle one.
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(0));

    net.limit_downlink(bob, None);
    net.run(Duration::from_secs(25));
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(2));
}

#[test]
fn lost_video_is_resent_on_both_legs() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 360)]);
    let lossy = || {
        NetemConfig::new()
            .loss(RandomLoss::new(Probability::new(0.05)))
            .seed(9)
    };
    net.limit_uplink(alice, lossy());
    net.limit_downlink(bob, Some(lossy()));

    net.run(Duration::from_secs(4));
    net.limit_downlink(bob, None);
    net.uplinks.clear();
    net.run(Duration::from_secs(1));

    let mut seqs: Vec<u64> = net.clients[bob]
        .video_on(track.ssrc, net.clients[bob].start)
        .iter()
        .map(|packet| packet.seq)
        .collect();
    seqs.sort_unstable();
    seqs.dedup();
    assert!(seqs.len() > 300, "{} packets", seqs.len());
    let gaps: Vec<(u64, u64)> = seqs
        .windows(2)
        .filter(|pair| pair[1] != pair[0] + 1)
        .map(|pair| (pair[0], pair[1]))
        .collect();
    assert!(gaps.is_empty(), "never filled: {gaps:?}");
    let resent = net.clients[bob]
        .raw
        .get(&track.rtx_ssrc)
        .copied()
        .unwrap_or(0);
    assert!(resent > 0);
}

#[test]
fn someone_who_joins_later_can_watch() {
    let mut net = Net::new();
    let alice = net.join(1001, PeerState::default());
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.run(Duration::from_secs(1));

    let bob = net.join(1002, PeerState::default());
    net.want(bob, &[("cam-a", 180)]);
    net.run(Duration::from_secs(2));

    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(0));
}

#[test]
fn an_unpublished_track_or_a_departed_publisher_stops_the_video() {
    for leave in [false, true] {
        let mut net = Net::new();
        let (alice, bob, _) = three(&mut net);
        let track = camera("cam-a", 5000);
        net.publish(alice, track.clone());
        net.want(bob, &[("cam-a", 180)]);
        net.run(Duration::from_secs(2));
        assert!(!net.clients[bob].video.is_empty());

        if leave {
            net.sfu.remove_peer(net.clients[alice].peer);
        } else {
            assert!(net.sfu.unpublish_track(net.clients[alice].peer, "cam-a"));
        }
        net.run(Duration::from_millis(300));
        let received = net.clients[bob].video.len();
        net.run(Duration::from_secs(1));

        assert_eq!(net.clients[bob].video.len(), received, "leave: {leave}");
    }
}

#[test]
fn audio_flows_alongside_video() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    net.publish(alice, camera("cam-a", 5000));
    net.want(bob, &[("cam-a", 720)]);
    net.run(Duration::from_secs(2));

    let said = net.talk(alice, 10);

    assert_eq!(net.clients[bob].payloads_from(1001), said);
}

#[test]
fn heavy_loss_on_a_downlink_moves_it_down_without_waiting_for_the_estimate() {
    // A 500 kbps link that only drops: no queue to show the overload as
    // delay. The node reads the loss its receiver reports.
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 720)]);
    net.run(Duration::from_secs(10));
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(2));

    let capped_at = net.now;
    net.limit_downlink(
        bob,
        Some(NetemConfig::new().link(Bitrate::kbps(500), DataSize::bytes(1_500))),
    );
    net.run(Duration::from_secs(3));

    let lower = net.clients[bob]
        .video_on(track.ssrc, capped_at)
        .into_iter()
        .find(|packet| packet.marking.is_some_and(|m| m.layer < 2))
        .map(|packet| packet.at.duration_since(capped_at))
        .expect("switched to a lower layer");
    assert!(lower <= Duration::from_millis(1500), "after {lower:?}");
}

#[test]
fn a_downlink_with_a_shallow_queue_gets_a_lower_layer_within_2_s() {
    // Most links queue tens of milliseconds, so congestion shows as loss
    // more than as delay.
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 720)]);
    net.run(Duration::from_secs(10));
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(2));

    let capped_at = net.now;
    net.limit_downlink(
        bob,
        Some(NetemConfig::new().link(Bitrate::kbps(500), DataSize::bytes(3_000))),
    );
    net.run(Duration::from_secs(3));

    let lower = net.clients[bob]
        .video_on(track.ssrc, capped_at)
        .into_iter()
        .find(|packet| packet.marking.is_some_and(|m| m.layer < 2))
        .map(|packet| packet.at.duration_since(capped_at))
        .expect("switched to a lower layer");
    assert!(lower <= Duration::from_secs(2), "after {lower:?}");
}

#[test]
fn a_layer_the_sender_cannot_send_is_backed_by_the_one_below() {
    // A sender whose uplink cannot carry its top layer drops it (plan
    // §7.10); the node keeps asking for it, asks for the layer below as
    // well, and moves its receivers there until the top layer is back.
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let track = camera("cam-a", 5000);
    net.publish(alice, track.clone());
    net.want(bob, &[("cam-a", 720)]);
    net.run(Duration::from_secs(5));
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(2));
    assert_eq!(net.layer_wants(alice, "cam-a"), Some(vec!["h".to_owned()]));

    net.hold(alice, "cam-a", 2, true);
    net.run(Duration::from_secs(4));

    let wants = net.layer_wants(alice, "cam-a").unwrap();
    assert!(wants.contains(&"h".to_owned()), "{wants:?}");
    assert!(wants.contains(&"m".to_owned()), "{wants:?}");
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(1));

    net.hold(alice, "cam-a", 2, false);
    net.run(Duration::from_secs(4));
    assert_eq!(net.clients[bob].last_layer(track.ssrc), Some(2));
    assert_eq!(net.layer_wants(alice, "cam-a"), Some(vec!["h".to_owned()]));
}

#[test]
fn a_screen_goes_only_to_those_watching_it() {
    let mut net = Net::new();
    let (alice, bob, carol) = three(&mut net);
    let user = |index: usize| 100 + index as i64;
    let screen = TrackSetup {
        kind: VideoKind::Screen,
        ..camera("screen-a", 5000)
    };
    net.publish(alice, screen.clone());
    net.want(bob, &[("screen-a", 720)]);
    net.want(carol, &[("screen-a", 720)]);

    // Wanting a screen is not enough: nobody watches yet.
    net.run(Duration::from_secs(2));
    assert!(net.clients[bob].video.is_empty());
    assert!(net.clients[carol].video.is_empty());

    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, user(alice), vec![user(bob)]);
    net.run(Duration::from_secs(3));
    assert!(net.clients[bob].last_layer(screen.ssrc).is_some());
    assert!(net.clients[carol].video.is_empty());

    // The stream ended (or bob stopped watching).
    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, user(alice), Vec::new());
    net.run(Duration::from_millis(500));
    let stopped = net.now;
    net.run(Duration::from_secs(1));
    assert!(net.clients[bob].video_on(screen.ssrc, stopped).is_empty());
}

#[test]
fn a_screens_sound_goes_only_to_those_watching_it_on_its_own_sequence() {
    let mut net = Net::new();
    let (alice, bob, carol) = three(&mut net);
    let user = |index: usize| 100 + index as i64;
    let sound_of_alice = screen_audio(net.clients[alice].ssrc);
    let share = |net: &mut Net, from: u64, packets: u64| -> Vec<Vec<u8>> {
        let mut sent = Vec::new();
        for seq in from..from + packets {
            let payload = vec![0x5a, seq as u8];
            let now = net.now;
            net.clients[alice].send_screen_audio(now, seq, &payload);
            sent.push(payload);
            net.run(Duration::from_millis(20));
        }
        sent
    };

    // Watchers set, but no screen live: the sound goes nowhere.
    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, user(alice), vec![user(bob)]);
    share(&mut net, 1, 5);
    assert!(net.clients[bob].payloads_from(sound_of_alice).is_empty());

    net.publish(alice, screen("screen-a", 7000));
    let voice = net.talk(alice, 3);
    let watched = share(&mut net, 6, 10);

    assert_eq!(net.clients[bob].payloads_from(sound_of_alice), watched);
    assert!(net.clients[carol].payloads_from(sound_of_alice).is_empty());
    // Voice is not stream sound, and goes to everyone as before.
    assert_eq!(net.clients[carol].payloads_from(1001), voice);
    assert!(
        net.clients[bob]
            .received
            .iter()
            .filter(|(ssrc, _, _)| *ssrc == sound_of_alice)
            .map(|(_, seq, _)| *seq)
            .collect::<Vec<_>>()
            .windows(2)
            .all(|pair| pair[1] == pair[0] + 1),
        "no gaps where the sound went nowhere"
    );

    // Bob stops watching; carol starts.
    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, user(alice), vec![user(carol)]);
    let later = share(&mut net, 16, 5);
    assert_eq!(net.clients[bob].payloads_from(sound_of_alice), watched);
    assert_eq!(net.clients[carol].payloads_from(sound_of_alice), later);
}

/// A screen share: a 640×360 low layer at 15 fps and the main one at
/// 1280×720 and 30 fps (plan §9.2).
fn screen(track_id: &str, base: u32) -> TrackSetup {
    let layer = |index: u32, rid: &str, width, height, fps, max_bitrate| LayerSetup {
        rid: rid.to_owned(),
        ssrc: base + index * 2,
        rtx_ssrc: base + index * 2 + 1,
        width,
        height,
        fps,
        max_bitrate,
    };
    TrackSetup {
        track_id: track_id.to_owned(),
        kind: VideoKind::Screen,
        layers: vec![
            layer(0, "l", 640, 360, 15, 300_000),
            layer(1, "h", 1280, 720, 30, 2_000_000),
        ],
        ssrc: base + 100,
        rtx_ssrc: base + 101,
    }
}

#[test]
fn a_viewer_with_room_gets_a_screens_main_layer() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let share = screen("screen-a", 7000);
    net.publish(alice, share.clone());
    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, 100 + alice as i64, vec![100 + bob as i64]);
    net.want(bob, &[("screen-a", 720)]);

    net.run(Duration::from_secs(15));

    assert_eq!(net.clients[bob].last_layer(share.ssrc), Some(1));
}

/// A share of a screen that does not change, its pictures sent again once
/// a second (`video/sender.rs`, `KEEPALIVE`), with the sender's 100 ms
/// polling on top.
const STILL_SCREEN: Duration = Duration::from_millis(1100);

#[test]
fn a_still_screen_keeps_its_main_layer_and_its_sender_is_left_alone() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let share = screen("screen-a", 7000);
    net.publish(alice, share.clone());
    net.still(alice, "screen-a", STILL_SCREEN);
    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, 100 + alice as i64, vec![100 + bob as i64]);
    net.want(bob, &[("screen-a", 1080)]);
    net.run(Duration::from_secs(20));
    let settled = net.now;
    let asked_before = net.events.len();

    net.run(Duration::from_secs(10));

    let frames: Vec<u8> = net.clients[bob]
        .video_on(share.ssrc, settled)
        .iter()
        .filter(|packet| packet.marking.is_some_and(|marking| marking.start))
        .filter_map(|packet| packet.marking.map(|marking| marking.layer))
        .collect();
    // Every picture of the last 10 s, all on the main layer.
    assert!(frames.len() >= 8, "{frames:?}");
    assert!(frames.iter().all(|layer| *layer == 1), "{frames:?}");
    // And the sender was not told to change what it encodes.
    let changes: Vec<&SfuEvent> = net.events[asked_before..]
        .iter()
        .filter(|event| {
            matches!(event, SfuEvent::LayerWants { track_id, .. } | SfuEvent::LayersAvailable { track_id, .. } if track_id == "screen-a")
        })
        .collect();
    assert!(changes.is_empty(), "{changes:?}");
    assert_eq!(
        net.layer_wants(alice, "screen-a"),
        Some(vec!["h".to_owned()])
    );
}

#[test]
fn a_screen_in_a_tile_a_little_smaller_than_its_main_layer_gets_the_main_layer() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let share = screen("screen-a", 7000);
    net.publish(alice, share.clone());
    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, 100 + alice as i64, vec![100 + bob as i64]);
    // 640 px of a 720 px share: text stays readable only scaled down.
    net.want(bob, &[("screen-a", 640)]);

    net.run(Duration::from_secs(15));

    assert_eq!(net.clients[bob].last_layer(share.ssrc), Some(1));
}

/// A 1080p30 screen share: the low layer and a 1920×1080 main layer.
fn screen_1080p(track_id: &str, base: u32) -> TrackSetup {
    let mut setup = screen(track_id, base);
    let main = &mut setup.layers[1];
    main.width = 1920;
    main.height = 1080;
    main.max_bitrate = 4_000_000;
    setup
}

/// How long after asking for a still screen `viewer` first got a picture
/// of its main layer, if within `limit`.
fn main_layer_after(
    net: &mut Net,
    viewer: usize,
    share: &TrackSetup,
    limit: Duration,
) -> Option<Duration> {
    let asked = net.now;
    net.want(viewer, &[(share.track_id.as_str(), 1080)]);
    while net.now - asked < limit {
        net.run(Duration::from_millis(100));
        let first = net.clients[viewer]
            .video_on(share.ssrc, asked)
            .into_iter()
            .find(|packet| packet.marking.is_some_and(|marking| marking.layer == 1))
            .map(|packet| packet.at - asked);
        if first.is_some() {
            return first;
        }
    }
    None
}

/// A still screen shared for a while before `viewer` starts watching it.
fn still_share_watched_late(share: &TrackSetup) -> Option<Duration> {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    net.publish(alice, share.clone());
    net.still(alice, &share.track_id, STILL_SCREEN);
    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, 100 + alice as i64, vec![100 + bob as i64]);
    net.run(Duration::from_secs(10));
    main_layer_after(&mut net, bob, share, Duration::from_secs(10))
}

#[test]
fn a_still_720p_screen_watched_late_reaches_its_main_layer_within_2_s() {
    let after = still_share_watched_late(&screen("screen-a", 7000));

    assert!(
        after.is_some_and(|after| after <= Duration::from_secs(2)),
        "{after:?}"
    );
}

#[test]
fn a_still_1080p_screen_watched_late_reaches_its_main_layer_within_2_s() {
    let after = still_share_watched_late(&screen_1080p("screen-a", 7000));

    assert!(
        after.is_some_and(|after| after <= Duration::from_secs(2)),
        "{after:?}"
    );
}

#[test]
fn a_downlink_too_small_for_a_screens_main_layer_is_not_tried_again_and_again() {
    let mut net = Net::new();
    let (alice, bob, _) = three(&mut net);
    let share = screen_1080p("screen-a", 7000);
    net.publish(alice, share.clone());
    let now = net.now;
    net.sfu
        .set_stream_viewers(now, CHANNEL, 100 + alice as i64, vec![100 + bob as i64]);
    net.limit_downlink(
        bob,
        Some(NetemConfig::new().link(Bitrate::kbps(1000), DataSize::bytes(25_000))),
    );
    net.run(Duration::from_secs(5));
    net.want(bob, &[("screen-a", 1080)]);
    let asked = net.now;

    net.run(Duration::from_secs(40));

    // Each picture's layer, in order: the main layer (4 Mbit/s while the
    // screen moves) may be tried once, but not every few seconds.
    let layers: Vec<u8> = net.clients[bob]
        .video_on(share.ssrc, asked)
        .iter()
        .filter_map(|packet| packet.marking.filter(|marking| marking.start))
        .map(|marking| marking.layer)
        .collect();
    let tries = layers.windows(2).filter(|pair| pair[0] < pair[1]).count();
    assert!(tries <= 2, "{tries} switches up");
    assert_eq!(net.clients[bob].last_layer(share.ssrc), Some(0));
    let lately = net.now - Duration::from_secs(5);
    assert!(!net.clients[bob].video_on(share.ssrc, lately).is_empty());
}
