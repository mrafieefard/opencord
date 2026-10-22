//! Encoding of envelopes into WebSocket frames.

use bytes::Bytes;
use opencord_proto::v1 as proto;
use prost::Message as _;

pub fn encode(payload: proto::envelope::Payload) -> Bytes {
    envelope(0, 0, payload)
}

pub fn envelope(seq: u64, request_id: u64, payload: proto::envelope::Payload) -> Bytes {
    Bytes::from(
        proto::Envelope {
            seq,
            request_id,
            payload: Some(payload),
        }
        .encode_to_vec(),
    )
}

pub fn event(seq: u64, event: &proto::Event) -> Bytes {
    envelope(seq, 0, proto::envelope::Payload::Event(event.clone()))
}

pub fn resumed(replayed_events: u64) -> Bytes {
    encode(proto::envelope::Payload::Resumed(proto::Resumed {
        replayed_events,
    }))
}
