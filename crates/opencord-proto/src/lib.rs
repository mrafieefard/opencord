//! Protobuf types for the Opencord protocols, generated from `proto/`.

/// The main gateway.
pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/opencord.v1.rs"));
}

/// The voice gateway.
pub mod voice {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/opencord.voice.v1.rs"));
    }
}

/// Voice tokens and the control channel between servers and voice nodes.
pub mod internal {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/opencord.internal.v1.rs"));
    }
}

#[cfg(test)]
mod tests {
    use prost::Message as _;

    use super::v1::{Envelope, Request, SendMessage, envelope, request};

    #[test]
    fn envelope_round_trips_a_request() {
        let original = Envelope {
            seq: 0,
            request_id: 7,
            payload: Some(envelope::Payload::Request(Request {
                kind: Some(request::Kind::SendMessage(SendMessage {
                    channel_id: 42,
                    content: "hello".into(),
                    nonce: "n-1".into(),
                })),
            })),
        };

        let decoded = Envelope::decode(original.encode_to_vec().as_slice()).unwrap();

        assert_eq!(decoded, original);
    }
}
