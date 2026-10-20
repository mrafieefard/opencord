//! Profile pictures and the server icon: set over HTTPS with a media token,
//! stored as 256-pixel square PNGs by their hash, announced to everyone,
//! and fetched only by members.

mod common;

use std::io::Cursor;

use common::{TestServer, invite, join, owner, self_id};
use image::{ImageBuffer, ImageFormat, Rgba};
use opencord_common::avatar;
use opencord_proto::v1 as proto;
use prost::Message as _;
use proto::event::Kind as Event;
use proto::response::Result as Response;

fn media_token(ready: &proto::Ready) -> String {
    ready.media_token.as_ref().unwrap().token.clone()
}

fn picture(width: u32, height: u32) -> Vec<u8> {
    let image: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_fn(width, height, |x, y| Rgba([x as u8, y as u8, 120, 255]));
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    bytes
}

async fn call(
    server: &TestServer,
    method: &str,
    path: &str,
    token: &str,
    body: &[u8],
) -> (u16, Response) {
    let authorization = format!("Bearer {token}");
    let (status, bytes) = server
        .send_http(method, path, &[("Authorization", &authorization)], body)
        .await;
    let response = proto::Response::decode(bytes.as_slice()).unwrap();
    (status, response.result.unwrap())
}

fn error_code(reply: (u16, Response)) -> (u16, proto::ErrorCode) {
    match reply {
        (status, Response::Error(error)) => {
            (status, proto::ErrorCode::try_from(error.code).unwrap())
        }
        other => panic!("expected an error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_profile_picture_reaches_everyone_and_only_members_fetch_it() {
    let server = TestServer::start().await;
    let (mut owner, owner_ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (_member, member_ready) = join(&server, 2, &code).await;
    let member_id = self_id(&member_ready);
    let member_token = media_token(&member_ready);

    let (status, reply) = call(
        &server,
        "POST",
        "/media/avatar",
        &member_token,
        &picture(600, 400),
    )
    .await;
    let Response::User(user) = reply else {
        panic!("expected the user, got {reply:?}");
    };
    assert_eq!(status, 200);
    assert!(
        avatar::valid_hash(&user.avatar_hash),
        "{}",
        user.avatar_hash
    );
    let announced = owner
        .wait_for(|event| match event {
            Event::MemberUpdate(update) => update
                .member
                .as_ref()
                .and_then(|member| member.user.as_ref())
                .filter(|user| user.id == member_id)
                .map(|user| user.avatar_hash.clone()),
            _ => None,
        })
        .await;
    assert_eq!(announced, user.avatar_hash);

    // Members fetch it, a 256-pixel square; nobody else does.
    let path = format!("/media/avatars/{}", user.avatar_hash);
    let authorization = format!("Bearer {}", media_token(&owner_ready));
    let (status, png) = server
        .send_http("GET", &path, &[("Authorization", &authorization)], b"")
        .await;
    assert_eq!(status, 200);
    assert!(avatar::is_normalized(&png));
    assert_eq!(avatar::hash(&png), user.avatar_hash);
    let (status, _) = server.send_http("GET", &path, &[], b"").await;
    assert_eq!(status, 401);
    let (status, _) = server
        .send_http(
            "GET",
            "/media/avatars/..%2F..%2Fsecrets.key",
            &[("Authorization", &authorization)],
            b"",
        )
        .await;
    assert_eq!(status, 400);

    // Removed, it is gone for everyone.
    let (_, reply) = call(&server, "DELETE", "/media/avatar", &member_token, b"").await;
    let Response::User(user) = reply else {
        panic!("expected the user, got {reply:?}");
    };
    assert!(user.avatar_hash.is_empty());
}

#[tokio::test]
async fn a_picture_already_in_shape_keeps_its_hash() {
    let server = TestServer::start().await;
    let (_owner, ready) = owner(&server).await;
    let png = avatar::normalize(&picture(300, 300)).unwrap();
    let (_, reply) = call(&server, "POST", "/media/avatar", &media_token(&ready), &png).await;
    let Response::User(user) = reply else {
        panic!("expected the user, got {reply:?}");
    };
    assert_eq!(user.avatar_hash, avatar::hash(&png));
}

#[tokio::test]
async fn what_isnt_a_picture_is_refused() {
    let server = TestServer::start().await;
    let (_owner, ready) = owner(&server).await;
    let reply = call(
        &server,
        "POST",
        "/media/avatar",
        &media_token(&ready),
        b"not a picture",
    )
    .await;
    assert_eq!(error_code(reply), (400, proto::ErrorCode::InvalidArgument));
}

#[tokio::test]
async fn the_server_icon_needs_manage_server_and_reaches_everyone() {
    let server = TestServer::start().await;
    let (mut owner, owner_ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &code).await;

    let refused = call(
        &server,
        "POST",
        "/media/server-icon",
        &media_token(&member_ready),
        &picture(64, 64),
    )
    .await;
    assert_eq!(error_code(refused), (403, proto::ErrorCode::Forbidden));

    let (_, reply) = call(
        &server,
        "POST",
        "/media/server-icon",
        &media_token(&owner_ready),
        &picture(64, 64),
    )
    .await;
    let Response::Server(info) = reply else {
        panic!("expected the server, got {reply:?}");
    };
    assert!(avatar::valid_hash(&info.icon_hash));
    let announced = member
        .wait_for(|event| match event {
            Event::ServerUpdate(update) => update.server.as_ref().map(|s| s.icon_hash.clone()),
            _ => None,
        })
        .await;
    assert_eq!(announced, info.icon_hash);

    // Someone joining later sees it in Ready.
    let (_late, late_ready) = join(&server, 3, &code).await;
    assert_eq!(late_ready.server.unwrap().icon_hash, info.icon_hash);
}
