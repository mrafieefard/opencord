// SPDX-License-Identifier: MPL-2.0

mod common;

use common::{
    TestClient, TestDevice, TestServer, device_keys, fallback_key, invite, join, key, one_time_key,
    owner, owner_again, prekey, register_request, self_id,
};
use opencord_proto::v1 as proto;
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
