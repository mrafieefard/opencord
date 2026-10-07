//! Real str0m clients against the SFU over an in-memory network, on a
//! virtual clock.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

use opencord_common::voice::{AUDIO_MID, OPUS_PAYLOAD_TYPE, receive_mid};
use str0m::config::Fingerprint;
use str0m::format::Codec;
use str0m::ice::IceCreds;
use str0m::media::{MediaKind, Pt};
use str0m::net::{Protocol, Receive};
use str0m::rtp::{ExtensionValues, RtpWrite, Ssrc};
use str0m::{Candidate, Event, Input, Output, Rtc, RtcConfig};

use super::*;

const NODE_V4: SocketAddr = SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 7711);
const NODE_V6: SocketAddr = SocketAddr::new(
    std::net::IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1)),
    7711,
);
const CHANNEL: i64 = 5;
const STEP: Duration = Duration::from_millis(1);

struct Client {
    rtc: Rtc,
    address: SocketAddr,
    ssrc: u32,
    peer: PeerId,
    next_seq: u64,
    received: Vec<(u32, u64, Vec<u8>)>,
    connected: bool,
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

    /// What the client sends now.
    fn drain(&mut self) -> Vec<Vec<u8>> {
        let mut sent = Vec::new();
        loop {
            match self.rtc.poll_output().unwrap() {
                Output::Timeout(_) => return sent,
                Output::Transmit(transmit) => sent.push(transmit.contents.to_vec()),
                Output::Event(Event::Connected) => self.connected = true,
                Output::Event(Event::RtpPacket(packet)) => self.received.push((
                    *packet.header.ssrc,
                    *packet.seq_no,
                    packet.payload.to_vec(),
                )),
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
}

struct Net {
    sfu: Sfu,
    clients: Vec<Client>,
    now: Instant,
}

impl Net {
    fn new() -> Self {
        Self {
            sfu: Sfu::new(NODE_V4, NODE_V6).unwrap(),
            clients: Vec::new(),
            now: Instant::now(),
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
        let mut rtc = RtcConfig::new()
            .set_rtp_mode(true)
            .clear_codecs()
            .enable_opus(true, false)
            .set_local_ice_credentials(credentials.clone())
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
        let mut client = Client {
            rtc,
            address,
            ssrc,
            peer,
            next_seq: 1000,
            received: Vec::new(),
            connected: false,
        };
        for other in others {
            client.hear(other);
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

    /// Moves every packet and timer along for `duration` of virtual time.
    fn run(&mut self, duration: Duration) {
        let end = self.now + duration;
        while self.now < end {
            self.exchange();
            self.now += STEP;
            self.sfu.handle_timeout(self.now);
            for client in &mut self.clients {
                client.rtc.handle_input(Input::Timeout(self.now)).unwrap();
            }
        }
        self.exchange();
    }

    fn exchange(&mut self) {
        for _ in 0..50 {
            let mut moved = false;
            for index in 0..self.clients.len() {
                let address = self.clients[index].address;
                for datagram in self.clients[index].drain() {
                    moved = true;
                    self.sfu.handle_receive(self.now, address, &datagram);
                }
            }
            while let Some(transmit) = self.sfu.poll_transmit() {
                moved = true;
                let Some(client) = self
                    .clients
                    .iter_mut()
                    .find(|client| client.address == transmit.destination)
                else {
                    continue;
                };
                let input = Input::Receive(
                    self.now,
                    Receive {
                        proto: Protocol::Udp,
                        source: NODE_V4,
                        destination: client.address,
                        contents: transmit.contents.as_slice().try_into().unwrap(),
                    },
                );
                client.rtc.handle_input(input).unwrap();
            }
            if !moved {
                return;
            }
        }
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
    let later = now + LOUDEST_EVERY;
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
