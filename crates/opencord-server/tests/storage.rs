mod common;

use common::*;
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

fn code(error: &proto::Error) -> proto::ErrorCode {
    proto::ErrorCode::try_from(error.code).unwrap()
}

fn failing_s3_config() -> proto::S3Config {
    proto::S3Config {
        endpoint: "http://127.0.0.1:1".into(),
        region: "us-east-1".into(),
        bucket: "test-bucket".into(),
        key_prefix: "files/".into(),
        path_style: true,
        allow_http: false,
        access_key_id: "AKIAEXAMPLE".into(),
        secret_access_key: "secret".into(),
    }
}

fn get_storage(res: Response) -> proto::Storage {
    match res {
        Response::Storage(storage) => storage,
        other => panic!("expected Storage, got {other:?}"),
    }
}

#[tokio::test]
async fn storage_requests_are_owner_only() {
    let server = TestServer::start().await;
    let (mut owner_client, _owner_ready) = owner(&server).await;
    let invite_code = invite(&mut owner_client, None).await;
    let (mut member, member_ready) = join(&server, 2, &invite_code).await;
    let member_id = self_id(&member_ready);

    let admin_role = match owner_client
        .ok(Request::CreateRole(proto::CreateRole {
            name: "admin".into(),
            color: 0,
            permissions: Permissions::ADMINISTRATOR.bits(),
            hoist: false,
            mentionable: false,
            max_upload_bytes: None,
        }))
        .await
    {
        Response::Role(role) => role,
        other => panic!("expected Role, got {other:?}"),
    };

    owner_client
        .ok(Request::AddMemberRole(proto::AddMemberRole {
            user_id: member_id,
            role_id: admin_role.id,
        }))
        .await;

    let requests = vec![
        Request::GetStorage(proto::GetStorage {}),
        Request::AddStorage(proto::AddStorage {
            name: "s3".into(),
            s3: Some(failing_s3_config()),
        }),
        Request::UpdateStorage(proto::UpdateStorage {
            storage_id: 1,
            name: Some("new".into()),
            s3: None,
        }),
        Request::TestStorage(proto::TestStorage { target: None }),
        Request::ActivateStorage(proto::ActivateStorage { storage_id: 1 }),
        Request::RemoveStorage(proto::RemoveStorage { storage_id: 1 }),
        Request::StartStorageMove(proto::StartStorageMove {}),
        Request::StopStorageMove(proto::StopStorageMove {}),
    ];

    for req in requests {
        let err = member.error(req).await;
        assert_eq!(code(&err), proto::ErrorCode::Forbidden);
    }
}

#[tokio::test]
async fn the_local_storage_is_listed_with_its_usage() {
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let token = media_token(&ready);
    let channel_id = channel_id(&ready, "general");

    let initial = get_storage(client.ok(Request::GetStorage(proto::GetStorage {})).await);
    assert_eq!(initial.backends.len(), 1);
    let local = &initial.backends[0];
    assert_eq!(local.id, 1);
    assert_eq!(local.kind, proto::StorageKind::Local as i32);
    assert!(local.active);
    assert!(!local.has_secret);
    assert!(local.s3.is_none());
    assert_eq!(local.file_count, 0);
    assert_eq!(local.bytes, 0);
    assert!(initial.temp_free_bytes > 0);

    let content = b"hello storage world";
    let attachment = upload(
        &mut client,
        &server,
        &token,
        channel_id,
        "test.txt",
        content,
    )
    .await;
    client
        .ok(Request::SendMessage(proto::SendMessage {
            channel_id,
            content: "a message".into(),
            attachment_ids: vec![attachment.id],
            ..Default::default()
        }))
        .await;

    let updated = get_storage(client.ok(Request::GetStorage(proto::GetStorage {})).await);
    assert_eq!(updated.backends.len(), 1);
    let local_updated = &updated.backends[0];
    assert_eq!(local_updated.file_count, 1);
    assert_eq!(local_updated.bytes, content.len() as u64);
    assert!(updated.temp_free_bytes > 0);
}

#[tokio::test]
async fn the_local_storage_cannot_be_removed() {
    let server = TestServer::start().await;
    let (mut client, _ready) = owner(&server).await;

    let err = client
        .error(Request::RemoveStorage(proto::RemoveStorage {
            storage_id: 1,
        }))
        .await;
    assert_eq!(code(&err), proto::ErrorCode::InvalidArgument);
}

#[tokio::test]
async fn plain_http_needs_allow_http() {
    let server = TestServer::start().await;
    let (mut client, _ready) = owner(&server).await;

    let mut config = failing_s3_config();
    config.allow_http = false;
    let err = client
        .error(Request::AddStorage(proto::AddStorage {
            name: "test".into(),
            s3: Some(config),
        }))
        .await;
    assert_eq!(code(&err), proto::ErrorCode::InvalidArgument);

    let storage = get_storage(client.ok(Request::GetStorage(proto::GetStorage {})).await);
    assert_eq!(storage.backends.len(), 1);
}

#[tokio::test]
async fn a_failing_bucket_cannot_be_added() {
    let server = TestServer::start().await;
    let (mut client, _ready) = owner(&server).await;

    let mut config = failing_s3_config();
    config.allow_http = true;
    let err = client
        .error(Request::AddStorage(proto::AddStorage {
            name: "test".into(),
            s3: Some(config),
        }))
        .await;
    assert_eq!(code(&err), proto::ErrorCode::StorageUnavailable);
    assert!(err.message.contains("Write a small file"));

    let storage = get_storage(client.ok(Request::GetStorage(proto::GetStorage {})).await);
    assert_eq!(storage.backends.len(), 1);
}

#[tokio::test]
async fn testing_settings_saves_nothing() {
    let server = TestServer::start().await;
    let (mut client, _ready) = owner(&server).await;

    let mut config = failing_s3_config();
    config.allow_http = true;
    let res = client
        .ok(Request::TestStorage(proto::TestStorage {
            target: Some(proto::test_storage::Target::S3(config)),
        }))
        .await;
    match res {
        Response::StorageTest(test) => {
            assert!(!test.ok);
            assert_eq!(test.steps.len(), 1);
            assert_eq!(test.steps[0].name, "Write a small file");
        }
        other => panic!("expected StorageTest, got {other:?}"),
    }

    let storage = get_storage(client.ok(Request::GetStorage(proto::GetStorage {})).await);
    assert_eq!(storage.backends.len(), 1);
}

#[tokio::test]
async fn unknown_storage_is_not_found() {
    let server = TestServer::start().await;
    let (mut client, _ready) = owner(&server).await;

    let err_act = client
        .error(Request::ActivateStorage(proto::ActivateStorage {
            storage_id: 12345,
        }))
        .await;
    assert_eq!(code(&err_act), proto::ErrorCode::NotFound);

    let err_rem = client
        .error(Request::RemoveStorage(proto::RemoveStorage {
            storage_id: 12345,
        }))
        .await;
    assert_eq!(code(&err_rem), proto::ErrorCode::NotFound);
}
