// SPDX-License-Identifier: MPL-2.0

mod common;

use std::time::Duration;

use common::{
    TestClient, TestDevice, TestServer, device_keys, fallback_key, invite, join, key, now_ms,
    one_time_key, owner, owner_again, prekey, private_message_kind, register_request, self_id,
};
use opencord_proto::v1 as proto;
use opencord_server::private::PrivateCounts;
use prost::Message as _;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Response;

#[tokio::test]
async fn ready_says_whether_private_messages_are_on() {
    let default_server = TestServer::start().await;
    let (_client, ready) = owner(&default_server).await;
    assert!(ready.private_messages);

    let disabled_server =
        TestServer::start_with(|config| config.private_messages.enabled = false).await;
    let (_client, ready) = owner(&disabled_server).await;
    assert!(!ready.private_messages);
}

async fn fetch_devices(client: &mut TestClient, user_ids: Vec<i64>) -> proto::DeviceLists {
    match client
        .ok(Kind::FetchDevices(proto::FetchDevices { user_ids }))
        .await
    {
        Response::DeviceLists(lists) => lists,
        other => panic!("expected DeviceLists, got {other:?}"),
    }
}

async fn claim(client: &mut TestClient, user_id: i64, device: &TestDevice) -> proto::ClaimedKeys {
    match client
        .ok(Kind::ClaimKeys(proto::ClaimKeys {
            devices: vec![proto::DeviceRef {
                user_id,
                device_id: device.id.to_vec(),
            }],
        }))
        .await
    {
        Response::ClaimedKeys(claimed) => claimed,
        other => panic!("expected ClaimedKeys, got {other:?}"),
    }
}

fn device_ids(lists: &proto::DeviceLists, user_id: i64) -> Vec<Vec<u8>> {
    lists
        .users
        .iter()
        .find(|user| user.user_id == user_id)
        .unwrap_or_else(|| panic!("no list for {user_id}"))
        .devices
        .iter()
        .map(|device| device.device_id.clone())
        .collect()
}

fn assert_code(error: &proto::Error, code: proto::ErrorCode) {
    assert_eq!(error.code, code as i32, "{error:?}");
}

#[tokio::test]
async fn a_device_registers_and_reports_its_keys() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);

    let registration = client.register_device(&key(1), &device, 5).await;

    assert_eq!(
        registration,
        proto::DeviceRegistration {
            one_time_keys_left: 5,
            pending_events: 0,
        }
    );
    let lists = fetch_devices(&mut client, vec![me]).await;
    assert_eq!(
        lists.users,
        vec![proto::UserDevices {
            user_id: me,
            devices: vec![device_keys(&key(1), &device)],
        }]
    );
}

#[tokio::test]
async fn a_device_signed_by_someone_else_is_refused() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let code = invite(&mut client, None).await;
    let (mut other, _) = join(&server, 2, &code).await;
    let device = TestDevice::new(10);

    let error = client.error(register_request(&key(9), &device, 1)).await;

    assert_code(&error, proto::ErrorCode::InvalidArgument);
    other
        .register_device(&key(2), &TestDevice::new(20), 1)
        .await;
    let lists = fetch_devices(&mut other, vec![me]).await;
    assert!(device_ids(&lists, me).is_empty());
    assert_eq!(server.handle.private_counts(me).await.devices, 0);
}

#[tokio::test]
async fn a_prekey_signed_by_another_key_is_refused() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    let stranger = TestDevice::new(11);
    let register = |one_time_keys: Vec<proto::Prekey>, fallback: proto::Prekey| {
        Kind::RegisterDevice(proto::RegisterDevice {
            device: Some(device_keys(&key(1), &device)),
            one_time_keys,
            fallback_key: Some(fallback),
        })
    };
    let good_fallback = prekey(&device, fallback_key(&device), true);

    // A one-time key signed by another device.
    let foreign = prekey(&stranger, one_time_key(&device, 0), false);
    let error = client
        .error(register(vec![foreign], good_fallback.clone()))
        .await;
    assert_code(&error, proto::ErrorCode::InvalidArgument);
    // A one-time key signed as a fallback key.
    let as_fallback = prekey(&device, one_time_key(&device, 0), true);
    let error = client
        .error(register(vec![as_fallback], good_fallback.clone()))
        .await;
    assert_code(&error, proto::ErrorCode::InvalidArgument);
    // A fallback key signed as a one-time key.
    let as_one_time = prekey(&device, fallback_key(&device), false);
    let error = client.error(register(Vec::new(), as_one_time)).await;
    assert_code(&error, proto::ErrorCode::InvalidArgument);

    assert_eq!(server.handle.private_counts(me).await.devices, 0);
    // The same device, signed right, registers; a bad upload is refused too.
    client.register_device(&key(1), &device, 0).await;
    let error = client
        .error(Kind::UploadKeys(proto::UploadKeys {
            one_time_keys: vec![prekey(&stranger, one_time_key(&device, 1), false)],
            fallback_key: None,
        }))
        .await;
    assert_code(&error, proto::ErrorCode::InvalidArgument);
    assert_eq!(server.handle.private_counts(me).await.one_time_keys, 0);
}

#[tokio::test]
async fn claims_use_up_one_time_keys_then_the_fallback() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let code = invite(&mut client, None).await;
    let (mut other, _) = join(&server, 2, &code).await;
    let device = TestDevice::new(10);
    client.register_device(&key(1), &device, 2).await;
    other
        .register_device(&key(2), &TestDevice::new(20), 1)
        .await;

    let mut bundles = Vec::new();
    for _ in 0..3 {
        let claimed = claim(&mut other, me, &device).await;
        assert_eq!(claimed.bundles.len(), 1);
        bundles.push(claimed.bundles.into_iter().next().unwrap());
    }

    for bundle in &bundles {
        assert_eq!(bundle.user_id, me);
        assert_eq!(bundle.device, Some(device_keys(&key(1), &device)));
    }
    let one_time: Vec<_> = bundles[..2]
        .iter()
        .map(|bundle| {
            assert!(!bundle.fallback);
            bundle.prekey.clone().unwrap()
        })
        .collect();
    assert_ne!(one_time[0].key, one_time[1].key);
    for prekey_claimed in &one_time {
        let i = prekey_claimed.key[1];
        assert!(i < 2);
        assert_eq!(
            *prekey_claimed,
            prekey(&device, one_time_key(&device, i), false)
        );
    }
    assert!(bundles[2].fallback);
    assert_eq!(
        bundles[2].prekey,
        Some(prekey(&device, fallback_key(&device), true))
    );
    let registration = match client
        .ok(Kind::UploadKeys(proto::UploadKeys::default()))
        .await
    {
        Response::DeviceRegistration(registration) => registration,
        other => panic!("expected a DeviceRegistration, got {other:?}"),
    };
    assert_eq!(registration.one_time_keys_left, 0);
    // An unknown device is left out.
    let claimed = claim(&mut other, me, &TestDevice::new(99)).await;
    assert!(claimed.bundles.is_empty());
}

#[tokio::test]
async fn a_session_registers_one_device() {
    let server = TestServer::start().await;
    let (mut client, _) = owner(&server).await;
    let first = TestDevice::new(10);
    client.register_device(&key(1), &first, 1).await;

    let error = client
        .error(register_request(&key(1), &TestDevice::new(11), 1))
        .await;

    assert_code(&error, proto::ErrorCode::Conflict);
    let again = client.register_device(&key(1), &first, 1).await;
    assert_eq!(again.one_time_keys_left, 1);
}

#[tokio::test]
async fn an_eleventh_device_pushes_out_the_one_seen_longest_ago() {
    let server = TestServer::start_with(|config| config.gateway.identify_per_minute = 100).await;
    let (first, ready) = owner(&server).await;
    let me = self_id(&ready);
    let mut clients = vec![first];
    for _ in 1..11 {
        clients.push(owner_again(&server).await.0);
    }

    for (seed, client) in (10..).zip(clients.iter_mut()) {
        client
            .register_device(&key(1), &TestDevice::new(seed), 1)
            .await;
    }

    let lists = fetch_devices(&mut clients[10], vec![me]).await;
    let ids = device_ids(&lists, me);
    assert_eq!(ids.len(), 10);
    assert!(!ids.contains(&TestDevice::new(10).id.to_vec()));
    assert!(ids.contains(&TestDevice::new(20).id.to_vec()));
    // The first session no longer holds a device.
    let error = clients[0]
        .error(Kind::FetchDevices(proto::FetchDevices {
            user_ids: vec![me],
        }))
        .await;
    assert_code(&error, proto::ErrorCode::PrivateMessagesUnavailable);
}

#[tokio::test]
async fn a_new_registration_with_new_keys_drops_the_old_one_time_keys() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    client.register_device(&key(1), &device, 5).await;

    let same_keys = client.register_device(&key(1), &device, 2).await;
    assert_eq!(same_keys.one_time_keys_left, 5);
    let renewed = TestDevice {
        id: device.id,
        ed: key(77),
        curve: [77; 32],
    };
    let new_keys = client.register_device(&key(1), &renewed, 2).await;

    assert_eq!(new_keys.one_time_keys_left, 2);
    assert_eq!(server.handle.private_counts(me).await.devices, 1);
}

#[tokio::test]
async fn unregistering_removes_the_device_and_its_keys() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    client
        .register_device(&key(1), &TestDevice::new(10), 5)
        .await;
    let before = server.handle.private_counts(me).await;
    assert_eq!((before.devices, before.one_time_keys), (1, 5));

    client
        .ok(Kind::UnregisterDevice(proto::UnregisterDevice {}))
        .await;

    let after = server.handle.private_counts(me).await;
    assert_eq!((after.devices, after.one_time_keys), (0, 0));
    let error = client
        .error(Kind::FetchDevices(proto::FetchDevices {
            user_ids: vec![me],
        }))
        .await;
    assert_code(&error, proto::ErrorCode::PrivateMessagesUnavailable);
    // Without a device it does nothing.
    client
        .ok(Kind::UnregisterDevice(proto::UnregisterDevice {}))
        .await;
}

#[tokio::test]
async fn requests_need_a_registered_device() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);

    for request in [
        Kind::FetchDevices(proto::FetchDevices { user_ids: vec![me] }),
        Kind::ClaimKeys(proto::ClaimKeys {
            devices: vec![proto::DeviceRef {
                user_id: me,
                device_id: vec![1; 16],
            }],
        }),
        Kind::UploadKeys(proto::UploadKeys::default()),
    ] {
        let error = client.error(request).await;
        assert_code(&error, proto::ErrorCode::PrivateMessagesUnavailable);
    }
}

#[tokio::test]
async fn a_disabled_server_refuses_registration() {
    let server = TestServer::start_with(|config| config.private_messages.enabled = false).await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);

    let error = client
        .error(register_request(&key(1), &TestDevice::new(10), 1))
        .await;

    assert_code(&error, proto::ErrorCode::PrivateMessagesUnavailable);
    let error = client
        .error(Kind::UpdatePrivacy(proto::UpdatePrivacy::default()))
        .await;
    assert_code(&error, proto::ErrorCode::PrivateMessagesUnavailable);
    assert_eq!(server.handle.private_counts(me).await.devices, 0);
}

#[tokio::test]
async fn privacy_and_blocks_replace_earlier_ones_and_survive_a_restart() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let code = invite(&mut client, None).await;
    let (_a, a_ready) = join(&server, 2, &code).await;
    let (_b, b_ready) = join(&server, 3, &code).await;
    let (a, b) = (self_id(&a_ready), self_id(&b_ready));
    assert_eq!(server.handle.private_privacy(me), None);

    client
        .ok(Kind::UpdatePrivacy(proto::UpdatePrivacy {
            allow_friend_requests: true,
            allow_message_requests: true,
            blocked_user_ids: vec![a],
        }))
        .await;
    assert_eq!(
        server.handle.private_privacy(me),
        Some((true, true, vec![a]))
    );
    client
        .ok(Kind::UpdatePrivacy(proto::UpdatePrivacy {
            allow_friend_requests: false,
            allow_message_requests: true,
            // Not members, the caller, and a repeat: dropped.
            blocked_user_ids: vec![b, 424_242, me, b],
        }))
        .await;

    assert_eq!(
        server.handle.private_privacy(me),
        Some((false, true, vec![b]))
    );
    drop(client);
    let server = server.restart(|_| {}).await;
    assert_eq!(
        server.handle.private_privacy(me),
        Some((false, true, vec![b]))
    );
}

#[tokio::test]
async fn too_many_blocked_ids_are_refused() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);

    let error = client
        .error(Kind::UpdatePrivacy(proto::UpdatePrivacy {
            allow_friend_requests: true,
            allow_message_requests: true,
            blocked_user_ids: (0..10_001).collect(),
        }))
        .await;

    assert_code(&error, proto::ErrorCode::InvalidArgument);
    assert_eq!(server.handle.private_privacy(me), None);
}

const DAY_MS: i64 = 86_400_000;

async fn fetch_pending(client: &mut TestClient, after_id: i64, limit: u32) -> proto::PendingEvents {
    match client
        .ok(Kind::FetchPendingEvents(proto::FetchPendingEvents {
            after_id,
            limit,
        }))
        .await
    {
        Response::PendingEvents(page) => page,
        other => panic!("expected PendingEvents, got {other:?}"),
    }
}

async fn ack_pending(client: &mut TestClient, ids: Vec<i64>) {
    client
        .ok(Kind::AckPendingEvents(proto::AckPendingEvents { ids }))
        .await;
}

/// `private_message_kind`'s body length, which tells test events apart.
fn body_len(event: &proto::PendingEvent) -> usize {
    match &event.kind {
        Some(proto::pending_event::Kind::PrivateMessage(message)) => message.body.len(),
        other => panic!("expected a private message, got {other:?}"),
    }
}

fn messages(lengths: impl IntoIterator<Item = usize>) -> Vec<proto::pending_event::Kind> {
    lengths.into_iter().map(private_message_kind).collect()
}

fn is_pending(kind: &Event) -> bool {
    matches!(kind, Event::PendingEvent(_))
}

#[tokio::test]
async fn an_offline_device_gets_its_events_after_reconnecting_in_order() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    client.register_device(&key(1), &device, 1).await;
    drop(client);

    server
        .handle
        .queue_private_events(me, device.id, messages([1, 2, 3]))
        .await;

    let (mut client, _) = owner_again(&server).await;
    let registration = client.register_device(&key(1), &device, 0).await;
    assert_eq!(registration.pending_events, 3);
    let page = fetch_pending(&mut client, 0, 0).await;
    assert!(!page.more);
    assert_eq!(
        page.events.iter().map(body_len).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert!(page.events.windows(2).all(|pair| pair[0].id < pair[1].id));
    assert!(page.events.iter().all(|event| event.created_at_ms > 0));
}

#[tokio::test]
async fn events_stay_until_acknowledged() {
    let server = TestServer::start_with(|config| config.gateway.identify_per_minute = 100).await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    client.register_device(&key(1), &device, 0).await;
    server
        .handle
        .queue_private_events(me, device.id, messages([1, 2, 3]))
        .await;

    let first = fetch_pending(&mut client, 0, 0).await;
    assert_eq!(first.events.len(), 3);
    drop(client);
    let (mut client, _) = owner_again(&server).await;
    client.register_device(&key(1), &device, 0).await;
    let again = fetch_pending(&mut client, 0, 0).await;
    assert_eq!(again.events, first.events);

    // Another device's event, named in this device's acknowledgement.
    let (mut other, _) = owner_again(&server).await;
    let other_device = TestDevice::new(11);
    other.register_device(&key(1), &other_device, 0).await;
    server
        .handle
        .queue_private_events(me, other_device.id, messages([9]))
        .await;
    let theirs = server.handle.private_pending_for(me, other_device.id).await;
    assert_eq!(theirs.len(), 1);
    let ids = first.events[..2]
        .iter()
        .map(|event| event.id)
        .chain([theirs[0].id])
        .collect();
    ack_pending(&mut client, ids).await;

    assert_eq!(
        server.handle.private_pending_for(me, other_device.id).await,
        theirs
    );
    drop(client);
    let (mut client, _) = owner_again(&server).await;
    let registration = client.register_device(&key(1), &device, 0).await;
    assert_eq!(registration.pending_events, 1);
    let left = fetch_pending(&mut client, 0, 0).await;
    assert_eq!(left.events, vec![first.events[2].clone()]);
}

#[tokio::test]
async fn catch_up_pages_by_count_and_by_bytes() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    client.register_device(&key(1), &device, 0).await;
    server
        .handle
        .queue_private_events(me, device.id, messages(vec![10; 250]))
        .await;

    // By count.
    let mut after = 0;
    let mut pages = Vec::new();
    let mut ids = Vec::new();
    loop {
        let page = fetch_pending(&mut client, after, 0).await;
        pages.push((page.events.len(), page.more));
        ids.extend(page.events.iter().map(|event| event.id));
        after = page.events.last().expect("a page is never empty").id;
        if !page.more {
            break;
        }
    }
    assert_eq!(pages, vec![(100, true), (100, true), (50, false)]);
    assert_eq!(ids.len(), 250);
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    // A limit is kept, up to 100.
    let small = fetch_pending(&mut client, 0, 7).await;
    assert_eq!((small.events.len(), small.more), (7, true));
    let capped = fetch_pending(&mut client, 0, 500).await;
    assert_eq!((capped.events.len(), capped.more), (100, true));
    for chunk in ids.chunks(125) {
        ack_pending(&mut client, chunk.to_vec()).await;
    }
    assert_eq!(server.handle.private_counts(me).await.pending, 0);

    // By bytes.
    server
        .handle
        .queue_private_events(me, device.id, messages(vec![32_768; 30]))
        .await;
    let mut after = 0;
    let mut received = Vec::new();
    loop {
        let page = fetch_pending(&mut client, after, 0).await;
        let encoded: usize = page.events.iter().map(|event| event.encoded_len()).sum();
        assert!(!page.events.is_empty());
        assert!(encoded <= 786_432, "a page of {encoded} bytes");
        if received.is_empty() {
            assert!(page.events.len() < 30);
            assert!(page.more);
        }
        after = page.events.last().expect("a page is never empty").id;
        received.extend(page.events);
        if !page.more {
            break;
        }
    }
    assert_eq!(received.len(), 30);
    assert!(received.iter().all(|event| body_len(event) == 32_768));
}

#[tokio::test]
async fn too_many_acknowledgements_are_refused() {
    let server = TestServer::start().await;
    let (mut client, _) = owner(&server).await;
    client
        .register_device(&key(1), &TestDevice::new(10), 0)
        .await;

    let error = client
        .error(Kind::AckPendingEvents(proto::AckPendingEvents {
            ids: (1..=501).collect(),
        }))
        .await;

    assert_code(&error, proto::ErrorCode::InvalidArgument);
}

#[tokio::test]
async fn a_live_event_reaches_the_session_holding_the_device() {
    let server = TestServer::start().await;
    let (mut holder, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    holder.register_device(&key(1), &device, 0).await;
    let (mut other, _) = owner_again(&server).await;
    other
        .register_device(&key(1), &TestDevice::new(11), 0)
        .await;

    server
        .handle
        .queue_private_events(me, device.id, messages([5]))
        .await;

    let live = holder
        .wait_for(|kind| match kind {
            Event::PendingEvent(event) => Some(event.clone()),
            _ => None,
        })
        .await;
    assert_eq!(
        vec![live],
        server.handle.private_pending_for(me, device.id).await
    );
    other
        .assert_no_event(Duration::from_millis(300), is_pending)
        .await;
}

#[tokio::test]
async fn a_new_session_takes_the_device_from_the_old_one() {
    let server = TestServer::start().await;
    let (mut first, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    first.register_device(&key(1), &device, 0).await;
    let (mut second, _) = owner_again(&server).await;
    second.register_device(&key(1), &device, 0).await;

    server
        .handle
        .queue_private_events(me, device.id, messages([5]))
        .await;

    second.wait_for(|kind| is_pending(kind).then_some(())).await;
    first
        .assert_no_event(Duration::from_millis(300), is_pending)
        .await;
}

#[tokio::test]
async fn the_cap_drops_the_oldest() {
    let server =
        TestServer::start_with(|config| config.private_messages.max_pending_per_device = 100).await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    client.register_device(&key(1), &device, 0).await;
    let (mut other, _) = owner_again(&server).await;
    let other_device = TestDevice::new(11);
    other.register_device(&key(1), &other_device, 0).await;

    // Over two deliveries, and in one.
    server
        .handle
        .queue_private_events(me, device.id, messages(1..=60))
        .await;
    server
        .handle
        .queue_private_events(me, device.id, messages(61..=120))
        .await;
    server
        .handle
        .queue_private_events(me, other_device.id, messages(1..=120))
        .await;

    for id in [device.id, other_device.id] {
        let left = server.handle.private_pending_for(me, id).await;
        assert_eq!(
            left.iter().map(body_len).collect::<Vec<_>>(),
            (21..=120).collect::<Vec<_>>()
        );
    }
    assert_eq!(server.handle.private_counts(me).await.pending, 200);
}

#[tokio::test]
async fn expiry_removes_old_events_and_unseen_devices() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let me = self_id(&ready);
    let device = TestDevice::new(10);
    client.register_device(&key(1), &device, 2).await;
    server
        .handle
        .queue_private_events(me, device.id, messages([1, 2, 3]))
        .await;
    let now = i64::try_from(now_ms()).unwrap();

    server.handle.sweep_private(now + 29 * DAY_MS).await;
    let counts = server.handle.private_counts(me).await;
    assert_eq!((counts.devices, counts.pending), (1, 3));

    server.handle.sweep_private(now + 31 * DAY_MS).await;
    let counts = server.handle.private_counts(me).await;
    assert_eq!(
        (counts.devices, counts.one_time_keys, counts.pending),
        (1, 2, 0)
    );

    server.handle.sweep_private(now + 61 * DAY_MS).await;
    let counts = server.handle.private_counts(me).await;
    assert_eq!((counts.devices, counts.one_time_keys), (0, 0));
    // The session no longer holds it.
    let error = client
        .error(Kind::FetchPendingEvents(
            proto::FetchPendingEvents::default(),
        ))
        .await;
    assert_code(&error, proto::ErrorCode::PrivateMessagesUnavailable);
}

#[tokio::test]
async fn a_kicked_member_leaves_nothing_behind() {
    let server = TestServer::start().await;
    let (mut owner_client, owner_ready) = owner(&server).await;
    let owner_id = self_id(&owner_ready);
    let code = invite(&mut owner_client, None).await;
    let nothing = PrivateCounts {
        devices: 0,
        one_time_keys: 0,
        pending: 0,
        friend_requests: 0,
    };

    // Kicked, then banned.
    for (seed, kick) in [(2, true), (3, false)] {
        let (mut member, ready) = join(&server, seed, &code).await;
        let member_id = self_id(&ready);
        let device = TestDevice::new(seed * 10);
        member.register_device(&key(seed), &device, 3).await;
        member
            .ok(Kind::UpdatePrivacy(proto::UpdatePrivacy {
                allow_friend_requests: false,
                allow_message_requests: false,
                blocked_user_ids: vec![owner_id],
            }))
            .await;
        server
            .handle
            .queue_private_events(member_id, device.id, messages([1, 2]))
            .await;
        assert_eq!(
            server.handle.private_counts(member_id).await,
            PrivateCounts {
                devices: 1,
                one_time_keys: 3,
                pending: 2,
                friend_requests: 0,
            }
        );

        let request = if kick {
            Kind::KickMember(proto::KickMember {
                user_id: member_id,
                reason: None,
            })
        } else {
            Kind::BanMember(proto::BanMember {
                user_id: member_id,
                reason: None,
            })
        };
        owner_client.ok(request).await;

        assert_eq!(server.handle.private_counts(member_id).await, nothing);
        assert_eq!(server.handle.private_privacy(member_id), None);
    }
}
