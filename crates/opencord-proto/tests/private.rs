// SPDX-License-Identifier: MPL-2.0

use opencord_proto::v1::{
    DeviceRegistration, ErrorCode, Event, FriendRemoved, FriendRequestAnswered,
    FriendRequestCancelled, FriendRequestReceived, FriendRequestSent, PendingEvent, PrivateDelete,
    PrivateEdit, PrivateMessage, PrivatePayload, PrivateText, Ready, RegisterDevice, RelayMoved,
    Request, RequestAccepted, Response, SessionReset, User, event, pending_event, private_payload,
    request, response,
};
use prost::Message as _;

#[test]
fn field_numbers_are_the_planned_ones() {
    let req = Request {
        kind: Some(request::Kind::RegisterDevice(RegisterDevice::default())),
    };
    assert_eq!(req.encode_to_vec(), [0xA2, 0x0B, 0x00]);

    let ev = Event {
        kind: Some(event::Kind::PendingEvent(PendingEvent::default())),
    };
    assert_eq!(ev.encode_to_vec(), [0xC2, 0x07, 0x00]);

    let resp = Response {
        result: Some(response::Result::DeviceRegistration(
            DeviceRegistration::default(),
        )),
    };
    assert_eq!(resp.encode_to_vec(), [0x92, 0x02, 0x00]);

    let ready = Ready {
        private_messages: true,
        ..Default::default()
    };
    assert_eq!(ready.encode_to_vec(), [0xA8, 0x01, 0x01]);

    assert_eq!(ErrorCode::FriendRequestsClosed as i32, 48);
    assert_eq!(ErrorCode::MessageRequestsClosed as i32, 49);
    assert_eq!(ErrorCode::Undeliverable as i32, 50);
    assert_eq!(ErrorCode::DeviceListChanged as i32, 51);
    assert_eq!(ErrorCode::PrivateMessagesUnavailable as i32, 52);
}

#[test]
fn pending_events_and_payloads_round_trip() {
    let user1 = User {
        id: 101,
        public_key: vec![1; 32],
        display_name: "Alice".into(),
        avatar_hash: "avatar1".into(),
    };
    let user2 = User {
        id: 102,
        public_key: vec![2; 32],
        display_name: "Bob".into(),
        avatar_hash: "avatar2".into(),
    };

    let pending_kinds = vec![
        pending_event::Kind::FriendRequestReceived(FriendRequestReceived {
            request_id: 1,
            from: Some(user1.clone()),
            slip: vec![3; 64],
        }),
        pending_event::Kind::FriendRequestSent(FriendRequestSent {
            request_id: 2,
            to: Some(user2.clone()),
        }),
        pending_event::Kind::FriendRequestAnswered(FriendRequestAnswered {
            request_id: 3,
            requester: Some(user1.clone()),
            answerer: Some(user2.clone()),
            accepted: true,
            slip: vec![4; 64],
        }),
        pending_event::Kind::FriendRequestCancelled(FriendRequestCancelled {
            request_id: 4,
            requester: Some(user1.clone()),
            target: Some(user2.clone()),
        }),
        pending_event::Kind::FriendRemoved(FriendRemoved {
            remover: Some(user1.clone()),
            removed: Some(user2.clone()),
        }),
        pending_event::Kind::PrivateMessage(PrivateMessage {
            sender: Some(user1),
            sender_device_id: vec![5; 16],
            olm_type: 1,
            body: vec![6; 128],
            sent_at_ms: 1_700_000_000_000,
        }),
    ];

    for (idx, kind) in pending_kinds.into_iter().enumerate() {
        let original = PendingEvent {
            id: (idx as i64) + 1,
            created_at_ms: 1_700_000_000_000 + (idx as i64),
            kind: Some(kind),
        };
        let encoded = original.encode_to_vec();
        let decoded = PendingEvent::decode(encoded.as_slice()).expect("decode PendingEvent");
        assert_eq!(decoded, original);
    }

    let payload_bodies = vec![
        private_payload::Body::Text(PrivateText {
            content: "hello private world".into(),
        }),
        private_payload::Body::Edit(PrivateEdit {
            message_id: vec![7; 16],
            content: "edited content".into(),
        }),
        private_payload::Body::Delete(PrivateDelete {
            message_id: vec![8; 16],
        }),
        private_payload::Body::RelayMoved(RelayMoved {
            server_id: vec![9; 16],
            server_name: "Community Server".into(),
        }),
        private_payload::Body::RequestAccepted(RequestAccepted {}),
        private_payload::Body::SessionReset(SessionReset {}),
    ];

    for (idx, body) in payload_bodies.into_iter().enumerate() {
        let original = PrivatePayload {
            message_id: vec![(idx as u8) + 10; 16],
            sent_at_ms: 1_700_000_000_100 + (idx as i64),
            recipient_public_key: vec![11; 32],
            slip: vec![12; 64],
            body: Some(body),
        };
        let encoded = original.encode_to_vec();
        let decoded = PrivatePayload::decode(encoded.as_slice()).expect("decode PrivatePayload");
        assert_eq!(decoded, original);
    }
}
