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
