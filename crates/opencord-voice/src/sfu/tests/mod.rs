//! Real str0m clients against the SFU over an in-memory network, on a
//! virtual clock.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use opencord_common::video::{FrameMarking, H264_PAYLOAD_TYPE, LAYER_RIDS, VideoKind, video_mid};
use opencord_common::voice::{AUDIO_MID, OPUS_PAYLOAD_TYPE, receive_mid, rtc_config};
use str0m::bwe::Bitrate;
use str0m::config::Fingerprint;
use str0m::format::Codec;
use str0m::ice::{IceCreds, StunMessageBuilder, TransId};
use str0m::media::{MediaKind, Pt};
use str0m::net::{Protocol, Receive};
use str0m::rtp::{ExtensionValues, RawPacket, RtpWrite, Ssrc};
use str0m::{Candidate, Event, Input, Output, Rtc, RtcConfig};
use str0m_netem::{Netem, NetemConfig};

use super::*;

mod video;

const NODE_V4: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 7711);
const NODE_V6: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1)),
    7711,
);
const CHANNEL: i64 = 5;
const STEP: Duration = Duration::from_millis(1);
/// Payload bytes per video packet the test clients send.
const VIDEO_PACKET: usize = 1000;

/// A video packet as a client received it.
#[derive(Debug, Clone)]
struct VideoReceived {
    ssrc: u32,
    seq: u64,
    timestamp: u32,
    marking: Option<FrameMarking>,
    bytes: usize,
    at: Instant,
}

/// A track a client publishes, and how it sends frames.
struct Publishing {
    setup: TrackSetup,
    next_seq: Vec<u64>,
    next_frame: Vec<Instant>,
    keyframe_due: Vec<bool>,
    /// Layers it encodes: all, until the node says otherwise.
    active: Vec<bool>,
    /// Layers its uplink cannot carry: never sent, whatever it is asked.
    held: Vec<bool>,
    /// Bits per second each layer sends.
    rate: Vec<u32>,
}

impl Publishing {
    fn new(setup: TrackSetup, now: Instant) -> Self {
        let layers = setup.layers.len();
        Self {
            next_seq: (0..layers)
                .map(|layer| 10_000 * (layer as u64 + 1))
                .collect(),
            next_frame: vec![now; layers],
            keyframe_due: vec![true; layers],
            active: vec![true; layers],
            held: vec![false; layers],
            rate: setup.layers.iter().map(|layer| layer.max_bitrate).collect(),
            setup,
        }
    }
}

struct Client {
    rtc: Rtc,
    address: SocketAddr,
    /// The username its connectivity checks carry: node's, then its own.
    username: String,
    ssrc: u32,
    peer: PeerId,
    next_seq: u64,
    received: Vec<(u32, u64, Vec<u8>)>,
    connected: bool,
    publishing: Vec<Publishing>,
    video: Vec<VideoReceived>,
    /// Every RTP packet that arrived, retransmissions and padding too, by
    /// SSRC.
    raw: HashMap<u32, usize>,
    /// Keyframe requests it received, by rid.
    keyframe_requests: Vec<Option<String>>,
    start: Instant,
}

impl Client {
    fn send_audio(&mut self, now: Instant, payload: &[u8]) {
        let seq = self.next_seq;
        self.next_seq += 1;
        let timestamp = u32::try_from(seq * 960).unwrap();
        let mut api = self.rtc.direct_api();
        let stream = api.stream_tx(&Ssrc::from(self.ssrc)).unwrap();
        stream.write_rtp(
            RtpWrite::new(
                Pt::from(OPUS_PAYLOAD_TYPE),
                seq.into(),
                timestamp,
                now,
                payload.to_vec(),
            )
            .ext_vals(ExtensionValues {
                audio_level: Some(-30),
                voice_activity: Some(true),
                ..Default::default()
            }),
        );
    }

    /// Receiving `ssrc`'s audio, as a ClientConnect would set up.
    fn hear(&mut self, ssrc: u32) {
        let mid = receive_mid(ssrc);
        let mut api = self.rtc.direct_api();
        api.declare_media(mid.as_str().into(), MediaKind::Audio);
        api.expect_stream_rx(Ssrc::from(ssrc), None, mid.as_str().into(), None);
    }

    /// Ready to receive a track, as a TrackUpdate would set up.
    fn watch(&mut self, setup: &TrackSetup) {
        let mid = receive_mid(setup.ssrc);
        let mut api = self.rtc.direct_api();
        api.declare_media(mid.as_str().into(), MediaKind::Video);
        api.expect_stream_rx(
            Ssrc::from(setup.ssrc),
            Some(Ssrc::from(setup.rtx_ssrc)),
            mid.as_str().into(),
            None,
        );
    }

    /// Sends one frame of a layer, cut into packets.
    fn send_frame(&mut self, now: Instant, track: usize, layer: usize) {
        let publishing = &mut self.publishing[track];
        let setup = publishing.setup.layers[layer].clone();
        let keyframe = std::mem::take(&mut publishing.keyframe_due[layer]);
        let bytes = (publishing.rate[layer] / setup.fps / 8) as usize;
        let count = bytes.div_ceil(VIDEO_PACKET).max(1);
        let timestamp = (now.duration_since(self.start).as_micros() * 9 / 100) as u32;
        let mut api = self.rtc.direct_api();
        for index in 0..count {
            let seq = publishing.next_seq[layer];
            publishing.next_seq[layer] += 1;
            let mut ext_vals = ExtensionValues::default();
            ext_vals.user_values.set(FrameMarking {
                keyframe,
                start: index == 0,
                layer: layer as u8,
                size: (keyframe && index == 0).then_some((setup.width as u16, setup.height as u16)),
            });
            let size = VIDEO_PACKET
                .min(bytes.saturating_sub(index * VIDEO_PACKET))
                .max(1);
            let Some(stream) = api.stream_tx(&Ssrc::from(setup.ssrc)) else {
                return;
            };
            stream.write_rtp(
                RtpWrite::new(
                    Pt::from(H264_PAYLOAD_TYPE),
                    seq.into(),
                    timestamp,
                    now,
                    vec![0x41; size],
                )
                .marker(index + 1 == count)
                .nackable(true)
                .ext_vals(ext_vals),
            );
        }
    }

    /// What the client sends now.
    fn drain(&mut self) -> Vec<Vec<u8>> {
        let mut sent = Vec::new();
        loop {
            match self.rtc.poll_output().unwrap() {
                Output::Timeout(_) => return sent,
                Output::Transmit(transmit) => sent.push(transmit.contents.to_vec()),
                Output::Event(Event::Connected) => self.connected = true,
                Output::Event(Event::RtpPacket(packet)) => {
                    if *packet.header.payload_type == H264_PAYLOAD_TYPE {
                        self.video.push(VideoReceived {
                            ssrc: *packet.header.ssrc,
                            seq: *packet.seq_no,
                            timestamp: packet.header.timestamp,
                            marking: packet.header.ext_vals.user_values.get().copied(),
                            bytes: packet.payload.len(),
                            at: packet.timestamp,
                        });
                    } else {
                        self.received.push((
                            *packet.header.ssrc,
                            *packet.seq_no,
                            packet.payload.to_vec(),
                        ));
                    }
                }
                Output::Event(Event::KeyframeRequest(request)) => {
                    let rid = request.rid.map(|rid| rid.to_string());
                    // The encoder makes the next frame of that layer a
                    // keyframe, as the client does.
                    for publishing in &mut self.publishing {
                        let mid = video_mid(publishing.setup.layers[0].ssrc);
                        if request.mid.to_string() != mid {
                            continue;
                        }
                        for (index, layer) in publishing.setup.layers.iter().enumerate() {
                            if rid.as_deref() == Some(layer.rid.as_str()) {
                                publishing.keyframe_due[index] = true;
                            }
                        }
                    }
                    self.keyframe_requests.push(rid);
                }
                Output::Event(Event::RawPacket(raw)) => {
                    if let RawPacket::RtpRx(header, _) = *raw {
                        *self.raw.entry(*header.ssrc).or_default() += 1;
                    }
                }
                Output::Event(_) => {}
            }
        }
    }

    fn payloads_from(&self, ssrc: u32) -> Vec<Vec<u8>> {
        self.received
            .iter()
            .filter(|(from, _, _)| *from == ssrc)
            .map(|(_, _, payload)| payload.clone())
            .collect()
    }

    /// The video it got on `ssrc`, from `since`.
    fn video_on(&self, ssrc: u32, since: Instant) -> Vec<&VideoReceived> {
        self.video
            .iter()
            .filter(|packet| packet.ssrc == ssrc && packet.at >= since)
            .collect()
    }

    /// The layer of the last video packet on `ssrc`.
    fn last_layer(&self, ssrc: u32) -> Option<u8> {
        self.video
            .iter()
            .rev()
            .find(|packet| packet.ssrc == ssrc)
            .and_then(|packet| packet.marking)
            .map(|marking| marking.layer)
    }
}

/// A camera track: 180p15, 360p30 and 720p30 layers on SSRCs from `base`.
fn camera(track_id: &str, base: u32) -> TrackSetup {
    let layer = |index: u32, width, height, fps, max_bitrate| LayerSetup {
        rid: LAYER_RIDS[index as usize].to_owned(),
        ssrc: base + index * 2,
        rtx_ssrc: base + index * 2 + 1,
        width,
        height,
        fps,
        max_bitrate,
    };
    TrackSetup {
        track_id: track_id.to_owned(),
        kind: VideoKind::Camera,
        layers: vec![
            layer(0, 320, 180, 15, 150_000),
            layer(1, 640, 360, 30, 500_000),
            layer(2, 1280, 720, 30, 1_500_000),
        ],
        ssrc: base + 100,
        rtx_ssrc: base + 101,
    }
}

struct Net {
    sfu: Sfu,
    clients: Vec<Client>,
    now: Instant,
    events: Vec<SfuEvent>,
    /// Impairments on the way from the node to a client, and back.
    downlinks: HashMap<SocketAddr, Netem<Vec<u8>>>,
    uplinks: HashMap<SocketAddr, Netem<Vec<u8>>>,
}

impl Net {
    fn new() -> Self {
        Self {
            sfu: Sfu::new(NODE_V4, NODE_V6).unwrap(),
            clients: Vec::new(),
            now: Instant::now(),
            events: Vec::new(),
            downlinks: HashMap::new(),
            uplinks: HashMap::new(),
        }
    }

    /// A client joins the channel and connects; returns its index.
    fn join(&mut self, ssrc: u32, state: PeerState) -> usize {
        let index = self.clients.len();
        let address = SocketAddr::from((Ipv4Addr::new(10, 0, 1, 1), 5000 + index as u16));
        let setup = PeerSetup {
            user_id: 100 + index as i64,
            channel_id: CHANNEL,
            audio_ssrc: ssrc,
            state,
        };
        let (peer, node) = self.sfu.add_peer(self.now, setup).unwrap();
        let credentials = IceCreds::new();
        let username = format!("{}:{}", node.ice_ufrag, credentials.ufrag);
        let mut rtc = rtc_config()
            .set_local_ice_credentials(credentials.clone())
            .enable_bwe(Some(Bitrate::kbps(1000)))
            .enable_raw_packets(true)
            .build(self.now);
        rtc.add_local_candidate(Candidate::host(address, "udp").unwrap());
        rtc.add_remote_candidate(Candidate::host(NODE_V4, "udp").unwrap());
        let fingerprint = {
            let mut api = rtc.direct_api();
            api.set_remote_ice_credentials(IceCreds {
                ufrag: node.ice_ufrag,
                pass: node.ice_pwd,
            });
            api.set_remote_fingerprint(Fingerprint {
                hash_func: "sha-256".to_owned(),
                bytes: node.dtls_fingerprint,
            });
            api.set_ice_controlling(true);
            api.enable_twcc_feedback();
            api.declare_media(AUDIO_MID.into(), MediaKind::Audio);
            api.declare_stream_tx(Ssrc::from(ssrc), None, AUDIO_MID.into(), None);
            api.start_dtls(true).unwrap();
            api.local_dtls_fingerprint().bytes.clone()
        };
        self.sfu
            .start_peer(
                peer,
                Transport {
                    ice_ufrag: credentials.ufrag,
                    ice_pwd: credentials.pass,
                    dtls_fingerprint: fingerprint,
                },
            )
            .unwrap();
        let others: Vec<u32> = self.clients.iter().map(|c| c.ssrc).collect();
        let tracks: Vec<TrackSetup> = self
            .clients
            .iter()
            .flat_map(|c| c.publishing.iter().map(|p| p.setup.clone()))
            .collect();
        let mut client = Client {
            rtc,
            address,
            username,
            ssrc,
            peer,
            next_seq: 1000,
            received: Vec::new(),
            connected: false,
            publishing: Vec::new(),
            video: Vec::new(),
            raw: HashMap::new(),
            keyframe_requests: Vec::new(),
            start: self.now,
        };
        for other in others {
            client.hear(other);
        }
        for track in &tracks {
            client.watch(track);
        }
        for existing in &mut self.clients {
            existing.hear(ssrc);
        }
        self.clients.push(client);
        self.run(Duration::from_millis(500));
        assert!(
            self.clients[index].connected,
            "client {index} did not connect"
        );
        index
    }

    /// `index` publishes a track; everyone else gets ready to receive it.
    fn publish(&mut self, index: usize, setup: TrackSetup) {
        let client = &mut self.clients[index];
        {
            let mid = video_mid(setup.layers[0].ssrc);
            let mut api = client.rtc.direct_api();
            api.declare_media(mid.as_str().into(), MediaKind::Video);
            for layer in &setup.layers {
                api.declare_stream_tx(
                    Ssrc::from(layer.ssrc),
                    Some(Ssrc::from(layer.rtx_ssrc)),
                    mid.as_str().into(),
                    Some(layer.rid.as_str().into()),
                );
            }
        }
        self.sfu
            .publish_track(self.now, client.peer, setup.clone())
            .unwrap();
        client
            .publishing
            .push(Publishing::new(setup.clone(), self.now));
        for (other, client) in self.clients.iter_mut().enumerate() {
            if other != index {
                client.watch(&setup);
            }
        }
    }

    /// What `index` wants to watch, as (track id, tile height).
    fn want(&mut self, index: usize, wants: &[(&str, u32)]) {
        let wants: Vec<Want> = wants
            .iter()
            .map(|(track_id, max_height)| Want {
                track_id: (*track_id).to_owned(),
                max_height: *max_height,
            })
            .collect();
        self.sfu
            .set_wants(self.now, self.clients[index].peer, &wants);
    }

    /// Caps the link from the node to `index`; `None` lifts it.
    fn limit_downlink(&mut self, index: usize, config: Option<NetemConfig>) {
        let address = self.clients[index].address;
        match config {
            Some(config) => match self.downlinks.get_mut(&address) {
                Some(netem) => netem.set_config(config),
                None => {
                    self.downlinks.insert(address, Netem::new(config));
                }
            },
            None => {
                if let Some(netem) = self.downlinks.get_mut(&address) {
                    netem.set_config(NetemConfig::new());
                }
            }
        }
    }

    fn limit_uplink(&mut self, index: usize, config: NetemConfig) {
        let address = self.clients[index].address;
        self.uplinks.insert(address, Netem::new(config));
    }

    /// Moves every packet and timer along for `duration` of virtual time;
    /// publishers send frames as their layers' frame rates say.
    fn run(&mut self, duration: Duration) {
        let end = self.now + duration;
        while self.now < end {
            self.send_frames();
            self.exchange();
            self.now += STEP;
            self.sfu.handle_timeout(self.now);
            for client in &mut self.clients {
                client.rtc.handle_input(Input::Timeout(self.now)).unwrap();
            }
        }
        self.exchange();
    }

    fn send_frames(&mut self) {
        let now = self.now;
        for client in &mut self.clients {
            for track in 0..client.publishing.len() {
                for layer in 0..client.publishing[track].setup.layers.len() {
                    let publishing = &mut client.publishing[track];
                    if !publishing.active[layer]
                        || publishing.held[layer]
                        || publishing.next_frame[layer] > now
                    {
                        continue;
                    }
                    let fps = publishing.setup.layers[layer].fps;
                    publishing.next_frame[layer] += Duration::from_secs(1) / fps;
                    client.send_frame(now, track, layer);
                }
            }
        }
    }

    fn exchange(&mut self) {
        for _ in 0..50 {
            let mut moved = false;
            for index in 0..self.clients.len() {
                let address = self.clients[index].address;
                for datagram in self.clients[index].drain() {
                    moved = true;
                    match self.uplinks.get_mut(&address) {
                        Some(netem) => {
                            netem.handle_input(str0m_netem::Input::Packet(self.now, datagram))
                        }
                        None => self.sfu.handle_receive(self.now, address, &datagram),
                    }
                }
            }
            for (address, netem) in &mut self.uplinks {
                for datagram in due(netem, self.now) {
                    moved = true;
                    self.sfu.handle_receive(self.now, *address, &datagram);
                }
            }
            while let Some(transmit) = self.sfu.poll_transmit() {
                moved = true;
                match self.downlinks.get_mut(&transmit.destination) {
                    Some(netem) => {
                        netem.handle_input(str0m_netem::Input::Packet(self.now, transmit.contents))
                    }
                    None => self.deliver(transmit.destination, &transmit.contents),
                }
            }
            let released: Vec<(SocketAddr, Vec<u8>)> = self
                .downlinks
                .iter_mut()
                .flat_map(|(address, netem)| {
                    due(netem, self.now)
                        .into_iter()
                        .map(|datagram| (*address, datagram))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (address, datagram) in released {
                moved = true;
                self.deliver(address, &datagram);
            }
            while let Some(event) = self.sfu.poll_event() {
                self.follow_layer_wants(&event);
                self.events.push(event);
            }
            if !moved {
                return;
            }
        }
    }

    fn deliver(&mut self, destination: SocketAddr, contents: &[u8]) {
        let Some(client) = self
            .clients
            .iter_mut()
            .find(|client| client.address == destination)
        else {
            return;
        };
        let input = Input::Receive(
            self.now,
            Receive {
                proto: Protocol::Udp,
                source: NODE_V4,
                destination: client.address,
                contents: contents.try_into().unwrap(),
            },
        );
        client.rtc.handle_input(input).unwrap();
    }

    /// Publishers encode the layers the node asks for, as the client does.
    fn follow_layer_wants(&mut self, event: &SfuEvent) {
        let SfuEvent::LayerWants {
            peer,
            track_id,
            rids,
        } = event
        else {
            return;
        };
        let Some(client) = self.clients.iter_mut().find(|c| c.peer == *peer) else {
            return;
        };
        let Some(publishing) = client
            .publishing
            .iter_mut()
            .find(|p| p.setup.track_id == *track_id)
        else {
            return;
        };
        for (index, layer) in publishing.setup.layers.iter().enumerate() {
            let active = rids.contains(&layer.rid);
            if active && !publishing.active[index] {
                publishing.keyframe_due[index] = true;
                publishing.next_frame[index] = self.now;
            }
            publishing.active[index] = active;
        }
    }

    /// `index` stops sending one layer of `track_id` as if its uplink could
    /// not carry it (`held`), or starts again with a keyframe.
    fn hold(&mut self, index: usize, track_id: &str, layer: usize, held: bool) {
        let now = self.now;
        let publishing = self.clients[index]
            .publishing
            .iter_mut()
            .find(|p| p.setup.track_id == track_id)
            .unwrap();
        if publishing.held[layer] && !held {
            publishing.keyframe_due[layer] = true;
            publishing.next_frame[layer] = now;
        }
        publishing.held[layer] = held;
    }

    /// `index` says one 20 ms frame per step of `run`.
    fn talk(&mut self, index: usize, frames: usize) -> Vec<Vec<u8>> {
        let mut said = Vec::new();
        for frame in 0..frames {
            let payload = vec![index as u8, frame as u8, 0xab];
            self.clients[index].send_audio(self.now, &payload);
            said.push(payload);
            self.run(Duration::from_millis(20));
        }
        said
    }

    /// The layers the node last asked `index` to encode for `track_id`.
    fn layer_wants(&self, index: usize, track_id: &str) -> Option<Vec<String>> {
        let peer = self.clients[index].peer;
        self.events.iter().rev().find_map(|event| match event {
            SfuEvent::LayerWants {
                peer: from,
                track_id: id,
                rids,
            } if *from == peer && id == track_id => Some(rids.clone()),
            _ => None,
        })
    }
}

/// Datagrams an impaired link lets through by `now`.
fn due(netem: &mut Netem<Vec<u8>>, now: Instant) -> Vec<Vec<u8>> {
    let mut released = Vec::new();
    netem.handle_input(str0m_netem::Input::Timeout(now));
    while let Some(str0m_netem::Output::Packet(datagram)) = netem.poll_output() {
        released.push(datagram);
    }
    released
}

#[test]
fn opus_has_the_agreed_payload_type() {
    let rtc = RtcConfig::new()
        .clear_codecs()
        .enable_opus(true, false)
        .build(Instant::now());
    let opus = rtc
        .codec_config()
        .find(|params| params.spec().codec == Codec::Opus)
        .unwrap();

    assert_eq!(*opus.pt(), OPUS_PAYLOAD_TYPE);
}

#[test]
fn people_in_a_channel_hear_each_other() {
    let mut net = Net::new();
    let alice = net.join(1001, PeerState::default());
    let bob = net.join(1002, PeerState::default());

    let alice_said = net.talk(alice, 5);
    let bob_said = net.talk(bob, 5);

    assert_eq!(net.clients[bob].payloads_from(1001), alice_said);
    assert_eq!(net.clients[alice].payloads_from(1002), bob_said);
    assert!(net.clients[alice].payloads_from(1001).is_empty(), "no echo");
}

#[test]
fn muted_and_suppressed_people_are_not_forwarded() {
    for state in [
        PeerState {
            server_mute: true,
            ..PeerState::default()
        },
        PeerState {
            self_mute: true,
            ..PeerState::default()
        },
        PeerState {
            suppress: true,
            ..PeerState::default()
        },
    ] {
        let mut net = Net::new();
        let alice = net.join(1001, PeerState::default());
        let bob = net.join(1002, PeerState::default());
        net.sfu.set_state(net.clients[alice].peer, state);

        net.talk(alice, 5);

        assert!(net.clients[bob].payloads_from(1001).is_empty(), "{state:?}");
    }
}

#[test]
fn deafened_people_receive_no_audio() {
    let mut net = Net::new();
    let alice = net.join(1001, PeerState::default());
    let bob = net.join(1002, PeerState::default());
    net.sfu.set_state(
        net.clients[bob].peer,
        PeerState {
            server_deaf: true,
            ..PeerState::default()
        },
    );

    net.talk(alice, 5);

    assert!(net.clients[bob].received.is_empty());
}

#[test]
fn audio_after_a_mute_continues_the_sequence() {
    let mut net = Net::new();
    let alice = net.join(1001, PeerState::default());
    let bob = net.join(1002, PeerState::default());
    let peer = net.clients[alice].peer;
    net.talk(alice, 3);
    net.sfu.set_state(
        peer,
        PeerState {
            server_mute: true,
            ..PeerState::default()
        },
    );
    net.talk(alice, 4);
    net.sfu.set_state(peer, PeerState::default());

    net.talk(alice, 3);

    let seqs: Vec<u64> = net.clients[bob]
        .received
        .iter()
        .map(|(_, seq, _)| *seq)
        .collect();
    assert_eq!(seqs.len(), 6);
    assert!(
        seqs.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "a gap the mute made: {seqs:?}"
    );
}

#[test]
fn people_who_leave_are_no_longer_forwarded() {
    let mut net = Net::new();
    let alice = net.join(1001, PeerState::default());
    let bob = net.join(1002, PeerState::default());
    let carol = net.join(1003, PeerState::default());

    net.sfu.remove_peer(net.clients[bob].peer);
    net.run(Duration::from_millis(50));
    let said = net.talk(alice, 3);

    assert_eq!(net.clients[carol].payloads_from(1001), said);
    assert!(net.clients[bob].received.is_empty());
}

#[test]
fn big_channels_forward_only_the_loudest() {
    use opencord_common::voice::{LOUDEST_HEARD, LOUDEST_ONLY_ABOVE};
    let mut sfu = Sfu::new(NODE_V4, NODE_V6).unwrap();
    let now = Instant::now();
    let people = u32::try_from(LOUDEST_ONLY_ABOVE).unwrap() + 1;
    let ids: Vec<PeerId> = (0..people)
        .map(|index| {
            let setup = PeerSetup {
                user_id: i64::from(index),
                channel_id: CHANNEL,
                audio_ssrc: 2000 + index,
                state: PeerState::default(),
            };
            sfu.add_peer(now, setup).unwrap().0
        })
        .collect();
    // The first is loudest, the last quietest.
    for (rank, id) in ids.iter().enumerate() {
        sfu.peers.get_mut(id).unwrap().level = -(rank as f32);
    }

    let heard: Vec<bool> = ids
        .iter()
        .map(|id| sfu.among_loudest(now, CHANNEL, *id))
        .collect();

    assert_eq!(heard.iter().filter(|heard| **heard).count(), LOUDEST_HEARD);
    assert!(heard[..LOUDEST_HEARD].iter().all(|heard| *heard));

    // Someone muted makes room for the next loudest.
    sfu.set_state(
        ids[0],
        PeerState {
            server_mute: true,
            ..PeerState::default()
        },
    );
    let later = now + audio::LOUDEST_EVERY;
    assert!(!sfu.among_loudest(later, CHANNEL, ids[0]));
    assert!(sfu.among_loudest(later, CHANNEL, ids[LOUDEST_HEARD]));
}

#[test]
fn small_channels_forward_everyone() {
    let mut sfu = Sfu::new(NODE_V4, NODE_V6).unwrap();
    let now = Instant::now();
    let setup = PeerSetup {
        user_id: 1,
        channel_id: CHANNEL,
        audio_ssrc: 3000,
        state: PeerState::default(),
    };
    let (quiet, _) = sfu.add_peer(now, setup).unwrap();

    assert!(sfu.among_loudest(now, CHANNEL, quiet));
}

#[test]
fn a_client_that_changes_network_keeps_hearing_the_others() {
    let mut net = Net::new();
    let alice = net.join(1001, PeerState::default());
    let bob = net.join(1002, PeerState::default());

    // Alice's machine moves to another network, as the transport does.
    let moved = SocketAddr::from((Ipv4Addr::new(10, 0, 2, 7), 6000));
    let old = net.clients[alice].address;
    let client = &mut net.clients[alice];
    client
        .rtc
        .add_local_candidate(Candidate::host(moved, "udp").unwrap());
    client
        .rtc
        .direct_api()
        .invalidate_candidate(&Candidate::host(old, "udp").unwrap());
    client.address = moved;
    net.run(Duration::from_millis(500));
    let said = net.talk(bob, 20);

    assert_eq!(net.clients[alice].payloads_from(1002), said);
}

#[test]
fn a_forged_move_takes_nobodys_audio_elsewhere() {
    let mut net = Net::new();
    let alice = net.join(1001, PeerState::default());
    let bob = net.join(1002, PeerState::default());

    // Someone who knows Alice's username, but not her password, claims a
    // new address for her.
    let elsewhere = SocketAddr::from((Ipv4Addr::new(10, 0, 9, 9), 6666));
    let forged = StunMessageBuilder::new()
        .binding()
        .request()
        .username(&net.clients[alice].username)
        .prio(0x7fff_ffff)
        .ice_controlling(7)
        .use_candidate()
        .build(TransId::new());
    let mut packet = vec![0u8; 512];
    let size = forged
        .to_bytes(Some(b"not the password"), &mut packet, hmac_sha1)
        .unwrap();
    net.sfu.handle_receive(net.now, elsewhere, &packet[..size]);

    // Bob's next frame, before Alice's client has a chance to check in:
    // Bob's clock and the node's run, Alice's does not.
    let bob_address = net.clients[bob].address;
    net.clients[bob].send_audio(net.now, &[2, 0, 0xab]);
    let mut destinations = Vec::new();
    for step in 1..=20 {
        let now = net.now + STEP * step;
        net.clients[bob]
            .rtc
            .handle_input(Input::Timeout(now))
            .unwrap();
        for datagram in net.clients[bob].drain() {
            net.sfu.handle_receive(now, bob_address, &datagram);
        }
        net.sfu.handle_timeout(now);
        while let Some(transmit) = net.sfu.poll_transmit() {
            destinations.push(transmit.destination);
        }
    }

    assert!(
        destinations.contains(&net.clients[alice].address),
        "{destinations:?}"
    );
    assert!(!destinations.contains(&elsewhere), "{destinations:?}");
}

#[test]
fn only_a_nomination_signed_with_the_peers_password_counts() {
    let nomination = |password: &[u8], nominate: bool| {
        let builder = StunMessageBuilder::new()
            .binding()
            .request()
            .username("node:client")
            .prio(1)
            .ice_controlling(7);
        let message = if nominate {
            builder.use_candidate()
        } else {
            builder
        }
        .build(TransId::new());
        let mut packet = vec![0u8; 512];
        let size = message
            .to_bytes(Some(password), &mut packet, hmac_sha1)
            .unwrap();
        packet.truncate(size);
        packet
    };

    assert!(signed_nomination(
        &nomination(b"the password", true),
        "the password"
    ));
    assert!(!signed_nomination(
        &nomination(b"a guess", true),
        "the password"
    ));
    assert!(!signed_nomination(
        &nomination(b"the password", false),
        "the password"
    ));
    assert!(!signed_nomination(&[0x80, 0x6f, 0, 1], "the password"));
}
