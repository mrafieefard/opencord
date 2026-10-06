mod common;

use std::time::Duration;

use common::{TestClient, TestServer, channel_id, invite, join, key, owner, self_id};
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::envelope::Payload;
use proto::event::Kind as Event;
use proto::request::Kind as Request;
use proto::response::Result as Response;

const QUIET: Duration = Duration::from_millis(300);

struct Pair {
    server: TestServer,
    owner: TestClient,
    owner_ready: proto::Ready,
    member: TestClient,
    member_ready: proto::Ready,
}

async fn owner_and_member() -> Pair {
    let server = TestServer::start().await;
    let (mut owner, owner_ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (member, member_ready) = join(&server, 2, &code).await;
    Pair {
        server,
        owner,
        owner_ready,
        member,
        member_ready,
    }
}

fn send(channel_id: i64, content: &str) -> Request {
    Request::SendMessage(proto::SendMessage {
        channel_id,
        content: content.to_owned(),
        nonce: format!("nonce-{content}"),
    })
}

fn fetch(channel_id: i64, before: Option<i64>, limit: u32) -> Request {
    Request::FetchMessages(proto::FetchMessages {
        channel_id,
        before,
        limit,
    })
}

fn message(result: Response) -> proto::Message {
    match result {
        Response::Message(message) => message,
        other => panic!("expected a message, got {other:?}"),
    }
}

fn messages(result: Response) -> Vec<proto::Message> {
    match result {
        Response::Messages(list) => list.messages,
        other => panic!("expected messages, got {other:?}"),
    }
}

fn channel(result: Response) -> proto::Channel {
    match result {
        Response::Channel(channel) => channel,
        other => panic!("expected a channel, got {other:?}"),
    }
}

fn role(result: Response) -> proto::Role {
    match result {
        Response::Role(role) => role,
        other => panic!("expected a role, got {other:?}"),
    }
}

fn create_text_channel(name: &str) -> Request {
    Request::CreateChannel(proto::CreateChannel {
        kind: proto::ChannelKind::Text as i32,
        name: name.to_owned(),
        topic: None,
        parent_id: None,
    })
}

fn create_role(name: &str, permissions: Permissions) -> Request {
    Request::CreateRole(proto::CreateRole {
        name: name.to_owned(),
        color: 0,
        permissions: permissions.bits(),
        hoist: false,
        mentionable: false,
    })
}

fn overwrite(
    channel_id: i64,
    target: proto::OverwriteTarget,
    target_id: i64,
    allow: Permissions,
    deny: Permissions,
) -> Request {
    Request::SetChannelOverwrite(proto::SetChannelOverwrite {
        channel_id,
        overwrite: Some(proto::PermissionOverwrite {
            target_kind: target as i32,
            target_id,
            allow: allow.bits(),
            deny: deny.bits(),
        }),
    })
}

fn code(error: &proto::Error) -> proto::ErrorCode {
    proto::ErrorCode::try_from(error.code).unwrap()
}

#[tokio::test]
async fn members_chat_in_real_time() {
    let Pair {
        mut owner,
        owner_ready,
        mut member,
        member_ready,
        server: _server,
    } = owner_and_member().await;
    let general = channel_id(&owner_ready, "general");

    let sent = message(owner.ok(send(general, "  hello there ")).await);
    let received = member
        .wait_for(|event| match event {
            Event::MessageCreate(create) => create.message.clone(),
            _ => None,
        })
        .await;
    let reply = message(member.ok(send(general, "hi")).await);
    let reply_seen = owner
        .wait_for(|event| match event {
            Event::MessageCreate(create) if create.message.as_ref()?.id == reply.id => {
                create.message.clone()
            }
            _ => None,
        })
        .await;
    let history = messages(member.ok(fetch(general, None, 0)).await);

    assert_eq!(sent.content, "hello there");
    assert_eq!(sent.nonce.as_deref(), Some("nonce-  hello there "));
    assert!(sent.created_at_ms > 0);
    assert_eq!(received, sent);
    assert_eq!(reply_seen.author_id, self_id(&member_ready));
    assert_eq!(
        history.iter().map(|m| m.id).collect::<Vec<_>>(),
        [reply.id, sent.id]
    );
    assert!(history.iter().all(|m| m.nonce.is_none()));
}

#[tokio::test]
async fn edits_and_deletes_reach_the_channel() {
    let Pair {
        mut owner,
        owner_ready,
        mut member,
        server: _server,
        ..
    } = owner_and_member().await;
    let general = channel_id(&owner_ready, "general");
    let sent = message(owner.ok(send(general, "draft")).await);

    let edited = message(
        owner
            .ok(Request::EditMessage(proto::EditMessage {
                message_id: sent.id,
                content: "final".to_owned(),
            }))
            .await,
    );
    let update = member
        .wait_for(|event| match event {
            Event::MessageUpdate(update) => update.message.clone(),
            _ => None,
        })
        .await;
    let foreign_edit = member
        .error(Request::EditMessage(proto::EditMessage {
            message_id: sent.id,
            content: "hacked".to_owned(),
        }))
        .await;
    let foreign_delete = member
        .error(Request::DeleteMessage(proto::DeleteMessage {
            message_id: sent.id,
        }))
        .await;
    owner
        .ok(Request::DeleteMessage(proto::DeleteMessage {
            message_id: sent.id,
        }))
        .await;
    let deleted = member
        .wait_for(|event| match event {
            Event::MessageDelete(delete) => Some(delete.message_id),
            _ => None,
        })
        .await;
    let history = messages(member.ok(fetch(general, None, 0)).await);

    assert_eq!(edited.content, "final");
    assert!(edited.edited_at_ms.is_some());
    assert_eq!(update.content, "final");
    assert_eq!(code(&foreign_edit), proto::ErrorCode::Forbidden);
    assert_eq!(code(&foreign_delete), proto::ErrorCode::Forbidden);
    assert_eq!(deleted, sent.id);
    assert!(history.is_empty());
}

#[tokio::test]
async fn history_pages_backwards() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let general = channel_id(&ready, "general");
    let mut ids = Vec::new();
    for n in 0..4 {
        ids.push(message(owner.ok(send(general, &format!("m{n}"))).await).id);
    }

    let newest = messages(owner.ok(fetch(general, None, 2)).await);
    let older = messages(owner.ok(fetch(general, Some(newest[1].id), 10)).await);

    assert_eq!(
        newest.iter().map(|m| m.id).collect::<Vec<_>>(),
        [ids[3], ids[2]]
    );
    assert_eq!(
        older.iter().map(|m| m.id).collect::<Vec<_>>(),
        [ids[1], ids[0]]
    );
}

#[tokio::test]
async fn members_receive_nothing_from_channels_they_cannot_view() {
    let Pair {
        mut owner,
        owner_ready,
        mut member,
        server: _server,
        ..
    } = owner_and_member().await;
    let everyone = owner_ready.server.as_ref().unwrap().everyone_role_id;
    let secret = channel(
        owner
            .ok(Request::CreateChannel(proto::CreateChannel {
                kind: proto::ChannelKind::Text as i32,
                name: "Secret Plans".to_owned(),
                topic: None,
                parent_id: None,
            }))
            .await,
    );
    owner
        .ok(overwrite(
            secret.id,
            proto::OverwriteTarget::Role,
            everyone,
            Permissions::empty(),
            Permissions::VIEW_CHANNEL,
        ))
        .await;
    member
        .wait_for(|event| match event {
            Event::ChannelDelete(delete) if delete.channel_id == secret.id => Some(()),
            _ => None,
        })
        .await;

    owner.ok(send(secret.id, "classified")).await;
    owner
        .ok(Request::StartTyping(proto::StartTyping {
            channel_id: secret.id,
        }))
        .await;
    let send_error = member.error(send(secret.id, "let me in")).await;
    let fetch_error = member.error(fetch(secret.id, None, 0)).await;

    assert_eq!(secret.name, "secret-plans");
    assert_eq!(code(&send_error), proto::ErrorCode::NotFound);
    assert_eq!(code(&fetch_error), proto::ErrorCode::NotFound);
    member
        .assert_no_event(QUIET, |event| match event {
            Event::MessageCreate(create) => {
                create.message.as_ref().unwrap().channel_id == secret.id
            }
            Event::TypingStart(typing) => typing.channel_id == secret.id,
            Event::ChannelUpdate(update) => update.channel.as_ref().unwrap().id == secret.id,
            _ => false,
        })
        .await;
}

#[tokio::test]
async fn role_changes_reveal_and_hide_channels() {
    let Pair {
        mut owner,
        owner_ready,
        mut member,
        member_ready,
        server: _server,
    } = owner_and_member().await;
    let everyone = owner_ready.server.as_ref().unwrap().everyone_role_id;
    let member_id = self_id(&member_ready);
    let staff_channel = channel(owner.ok(create_text_channel("staff")).await);
    owner
        .ok(overwrite(
            staff_channel.id,
            proto::OverwriteTarget::Role,
            everyone,
            Permissions::empty(),
            Permissions::VIEW_CHANNEL,
        ))
        .await;
    let staff_role = role(owner.ok(create_role("staff", Permissions::empty())).await);
    owner
        .ok(overwrite(
            staff_channel.id,
            proto::OverwriteTarget::Role,
            staff_role.id,
            Permissions::VIEW_CHANNEL,
            Permissions::empty(),
        ))
        .await;
    member
        .wait_for(|event| match event {
            Event::ChannelDelete(delete) if delete.channel_id == staff_channel.id => Some(()),
            _ => None,
        })
        .await;

    owner
        .ok(Request::AddMemberRole(proto::AddMemberRole {
            user_id: member_id,
            role_id: staff_role.id,
        }))
        .await;
    let revealed = member
        .wait_for(|event| match event {
            Event::ChannelCreate(create) => create.channel.clone(),
            _ => None,
        })
        .await;
    owner
        .ok(Request::RemoveMemberRole(proto::RemoveMemberRole {
            user_id: member_id,
            role_id: staff_role.id,
        }))
        .await;
    let hidden = member
        .wait_for(|event| match event {
            Event::ChannelDelete(delete) => Some(delete.channel_id),
            _ => None,
        })
        .await;

    assert_eq!(revealed.id, staff_channel.id);
    assert_eq!(hidden, staff_channel.id);
}

#[tokio::test]
async fn role_permission_changes_update_visibility() {
    let Pair {
        mut owner,
        owner_ready,
        mut member,
        server: _server,
        ..
    } = owner_and_member().await;
    let everyone = owner_ready.server.as_ref().unwrap().everyone_role_id;

    owner
        .ok(Request::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some((Permissions::DEFAULT_EVERYONE - Permissions::VIEW_CHANNEL).bits()),
            ..Default::default()
        }))
        .await;
    let role_update = member
        .wait_for(|event| match event {
            Event::RoleUpdate(update) => update.role.clone(),
            _ => None,
        })
        .await;
    let mut hidden = std::collections::BTreeSet::new();
    while hidden.len() < owner_ready.channels.len() {
        hidden.insert(
            member
                .wait_for(|event| match event {
                    Event::ChannelDelete(delete) => Some(delete.channel_id),
                    _ => None,
                })
                .await,
        );
    }

    assert_eq!(role_update.id, everyone);
    assert_eq!(hidden, owner_ready.channels.iter().map(|c| c.id).collect());
}

#[tokio::test]
async fn banned_users_cannot_identify() {
    let Pair {
        server,
        mut owner,
        mut member,
        member_ready,
        ..
    } = owner_and_member().await;
    let member_id = self_id(&member_ready);
    let code_for_later = invite(&mut owner, None).await;

    owner
        .ok(Request::BanMember(proto::BanMember {
            user_id: member_id,
            reason: Some("spam".to_owned()),
        }))
        .await;
    let close = member.expect_close().await;
    let left = owner
        .wait_for(|event| match event {
            Event::MemberLeave(leave) => Some(leave.user_id),
            _ => None,
        })
        .await;
    let mut retry = TestClient::connect(&server, key(2)).await;
    let refused = retry
        .identify("again", Some(&code_for_later), None)
        .await
        .unwrap_err();
    let bans = match owner.ok(Request::FetchBans(proto::FetchBans {})).await {
        Response::Bans(list) => list.bans,
        other => panic!("expected bans, got {other:?}"),
    };
    owner
        .ok(Request::UnbanMember(proto::UnbanMember {
            user_id: member_id,
        }))
        .await;
    let (_back, back_ready) = join(&server, 2, &code_for_later).await;

    assert_eq!(close, Some(4011));
    assert_eq!(left, member_id);
    assert_eq!(code(&refused), proto::ErrorCode::Unauthorized);
    assert_eq!(bans.len(), 1);
    assert_eq!(bans[0].user.as_ref().unwrap().id, member_id);
    assert_eq!(bans[0].reason.as_deref(), Some("spam"));
    assert_eq!(self_id(&back_ready), member_id);
}

#[tokio::test]
async fn kicked_members_are_disconnected_and_must_be_invited_again() {
    let Pair {
        server,
        mut owner,
        mut member,
        member_ready,
        ..
    } = owner_and_member().await;
    let member_id = self_id(&member_ready);

    owner
        .ok(Request::KickMember(proto::KickMember {
            user_id: member_id,
            reason: None,
        }))
        .await;
    let close = member.expect_close().await;
    let mut retry = TestClient::connect(&server, key(2)).await;
    let refused = retry.identify("again", None, None).await.unwrap_err();
    let code_again = invite(&mut owner, None).await;
    let (_back, _) = join(&server, 2, &code_again).await;

    assert_eq!(close, Some(4010));
    assert_eq!(code(&refused), proto::ErrorCode::Unauthorized);
}

#[tokio::test]
async fn invite_max_uses_is_enforced() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let single_use = invite(&mut owner, Some(1)).await;

    join(&server, 2, &single_use).await;
    let mut late = TestClient::connect(&server, key(3)).await;
    let refused = late
        .identify("late", Some(&single_use), None)
        .await
        .unwrap_err();

    assert_eq!(code(&refused), proto::ErrorCode::Unauthorized);
}

#[tokio::test]
async fn resume_replays_missed_messages() {
    let Pair {
        server,
        member_ready,
        mut member,
        owner_ready,
        ..
    } = owner_and_member().await;
    let general = channel_id(&owner_ready, "general");
    member.assert_no_event(QUIET, |_| false).await;
    let last_seq = member.last_seq;

    server.handle.drop_connections();
    assert_eq!(member.expect_close().await, Some(4000));
    let mut owner_again = TestClient::connect(&server, key(1)).await;
    owner_again.identify("Owner", None, None).await.unwrap();
    let first = message(owner_again.ok(send(general, "while you were away")).await);
    let second = message(owner_again.ok(send(general, "still away")).await);

    let mut resumed = TestClient::connect(&server, key(2)).await;
    resumed
        .send(Payload::Resume(proto::Resume {
            session_id: member_ready.session_id.clone(),
            resume_token: member_ready.resume_token.clone(),
            last_seq,
        }))
        .await;
    let resumed_payload = resumed.recv_payload().await;
    let mut replayed = Vec::new();
    while let Some(event) = resumed.queued_event() {
        replayed.push(event);
    }
    let live = message(owner_again.ok(send(general, "welcome back")).await);
    let live_seen = resumed
        .wait_for(|event| match event {
            Event::MessageCreate(create) => create.message.clone(),
            _ => None,
        })
        .await;

    let Payload::Resumed(proto::Resumed { replayed_events }) = resumed_payload else {
        panic!("expected Resumed, got {resumed_payload:?}");
    };
    assert_eq!(replayed_events, u64::try_from(replayed.len()).unwrap());
    let replayed_ids: Vec<i64> = replayed
        .iter()
        .filter_map(|(_, event)| match event {
            Event::MessageCreate(create) => Some(create.message.as_ref()?.id),
            _ => None,
        })
        .collect();
    assert_eq!(replayed_ids, [first.id, second.id]);
    let seqs: Vec<u64> = replayed.iter().map(|(seq, _)| *seq).collect();
    assert_eq!(
        seqs,
        (last_seq + 1..=last_seq + replayed_events).collect::<Vec<_>>()
    );
    assert_eq!(live_seen.id, live.id);
}

#[tokio::test]
async fn failed_resume_allows_identify_on_the_same_connection() {
    let server = TestServer::start().await;
    let token = server.claim_token();
    let mut client = TestClient::connect(&server, key(1)).await;

    client
        .send(Payload::Resume(proto::Resume {
            session_id: "missing".to_owned(),
            resume_token: vec![0; 32],
            last_seq: 0,
        }))
        .await;
    let error = match client.recv_payload().await {
        Payload::Error(error) => error,
        other => panic!("expected an error, got {other:?}"),
    };
    let ready = client.identify("Owner", None, Some(&token)).await;

    assert_eq!(code(&error), proto::ErrorCode::InvalidSession);
    assert!(ready.is_ok());
}

#[tokio::test]
async fn sending_too_fast_is_rate_limited() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let general = channel_id(&ready, "general");
    for n in 0..5 {
        owner.ok(send(general, &format!("burst {n}"))).await;
    }

    let error = owner.error(send(general, "one too many")).await;

    assert_eq!(code(&error), proto::ErrorCode::RateLimited);
    assert!(error.retry_after_ms.unwrap() > 0);
}

#[tokio::test]
async fn too_many_requests_are_rate_limited() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    for _ in 0..opencord_common::limits::REQUEST_RATE.burst {
        owner
            .ok(Request::FetchInvites(proto::FetchInvites {}))
            .await;
    }

    let error = owner
        .error(Request::FetchInvites(proto::FetchInvites {}))
        .await;

    assert_eq!(code(&error), proto::ErrorCode::RateLimited);
}

#[tokio::test]
async fn members_cannot_act_above_their_rank_or_grant_what_they_lack() {
    let Pair {
        mut owner,
        owner_ready,
        mut member,
        member_ready,
        server: _server,
    } = owner_and_member().await;
    let owner_id = self_id(&owner_ready);
    let member_id = self_id(&member_ready);

    let no_manage_roles = member
        .error(create_role("nope", Permissions::empty()))
        .await;
    let moderator = role(
        owner
            .ok(create_role(
                "moderator",
                Permissions::MANAGE_ROLES | Permissions::KICK_MEMBERS,
            ))
            .await,
    );
    owner
        .ok(Request::AddMemberRole(proto::AddMemberRole {
            user_id: member_id,
            role_id: moderator.id,
        }))
        .await;
    let kick_owner = member
        .error(Request::KickMember(proto::KickMember {
            user_id: owner_id,
            reason: None,
        }))
        .await;
    let grant_ban = member
        .error(create_role("banhammer", Permissions::BAN_MEMBERS))
        .await;
    let helper = role(
        member
            .ok(create_role("helper", Permissions::KICK_MEMBERS))
            .await,
    );
    let edit_own_rank = member
        .error(Request::UpdateRole(proto::UpdateRole {
            role_id: moderator.id,
            name: Some("supreme".to_owned()),
            ..Default::default()
        }))
        .await;
    let delete_everyone = owner
        .error(Request::DeleteRole(proto::DeleteRole {
            role_id: owner_ready.server.as_ref().unwrap().everyone_role_id,
        }))
        .await;

    assert_eq!(code(&no_manage_roles), proto::ErrorCode::Forbidden);
    assert_eq!(code(&kick_owner), proto::ErrorCode::Forbidden);
    assert_eq!(code(&grant_ban), proto::ErrorCode::Forbidden);
    assert_eq!(helper.permissions, Permissions::KICK_MEMBERS.bits());
    assert_eq!(helper.position, 1);
    assert_eq!(code(&edit_own_rank), proto::ErrorCode::Forbidden);
    assert_eq!(code(&delete_everyone), proto::ErrorCode::InvalidArgument);
}

#[tokio::test]
async fn new_roles_go_directly_above_everyone_and_can_be_reordered() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let a = role(owner.ok(create_role("a", Permissions::empty())).await);
    let b = role(owner.ok(create_role("b", Permissions::empty())).await);
    let c = role(owner.ok(create_role("c", Permissions::empty())).await);

    owner
        .ok(Request::ReorderRoles(proto::ReorderRoles {
            role_ids: vec![a.id, b.id, c.id],
        }))
        .await;
    let mut positions = std::collections::HashMap::new();
    for _ in 0..16 {
        let updated = owner
            .wait_for(|event| match event {
                Event::RoleUpdate(update) => update.role.clone(),
                _ => None,
            })
            .await;
        positions.insert(updated.id, updated.position);
        if positions.get(&a.id) == Some(&1) && positions.get(&c.id) == Some(&3) {
            break;
        }
    }

    assert_eq!(c.position, 1);
    assert_eq!(positions.get(&a.id), Some(&1));
    assert_eq!(positions.get(&c.id), Some(&3));
}

#[tokio::test]
async fn typing_and_presence_are_broadcast() {
    let Pair {
        mut owner,
        owner_ready,
        mut member,
        member_ready,
        server: _server,
    } = owner_and_member().await;
    let general = channel_id(&owner_ready, "general");
    let member_id = self_id(&member_ready);

    member
        .ok(Request::StartTyping(proto::StartTyping {
            channel_id: general,
        }))
        .await;
    let typing = owner
        .wait_for(|event| match event {
            Event::TypingStart(typing) => Some(typing.user_id),
            _ => None,
        })
        .await;
    member
        .ok(Request::UpdatePresence(proto::UpdatePresence {
            status: proto::PresenceStatus::Dnd as i32,
        }))
        .await;
    let status = owner
        .wait_for(|event| match event {
            Event::PresenceUpdate(update)
                if update.presence.as_ref()?.status == proto::PresenceStatus::Dnd as i32 =>
            {
                Some(update.presence.as_ref()?.user_id)
            }
            _ => None,
        })
        .await;

    assert_eq!(typing, member_id);
    assert_eq!(status, member_id);
}

#[tokio::test]
async fn channels_can_be_organized() {
    let Pair {
        mut owner,
        mut member,
        server: _server,
        ..
    } = owner_and_member().await;
    let category = channel(
        owner
            .ok(Request::CreateChannel(proto::CreateChannel {
                kind: proto::ChannelKind::Category as i32,
                name: "Projects".to_owned(),
                topic: None,
                parent_id: None,
            }))
            .await,
    );
    let inside = channel(
        owner
            .ok(Request::CreateChannel(proto::CreateChannel {
                kind: proto::ChannelKind::Text as i32,
                name: "Rust".to_owned(),
                topic: Some("crabs".to_owned()),
                parent_id: Some(category.id),
            }))
            .await,
    );
    let nested_category = owner
        .error(Request::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Category as i32,
            name: "Nested".to_owned(),
            topic: None,
            parent_id: Some(category.id),
        }))
        .await;
    let renamed = channel(
        owner
            .ok(Request::UpdateChannel(proto::UpdateChannel {
                channel_id: inside.id,
                name: Some("Rust Lang".to_owned()),
                topic: Some(String::new()),
                parent_id: None,
            }))
            .await,
    );
    owner
        .ok(Request::DeleteChannel(proto::DeleteChannel {
            channel_id: category.id,
        }))
        .await;
    let orphan = member
        .wait_for(|event| match event {
            Event::ChannelUpdate(update)
                if update.channel.as_ref()?.id == inside.id
                    && update.channel.as_ref()?.parent_id.is_none() =>
            {
                update.channel.clone()
            }
            _ => None,
        })
        .await;
    let forbidden = member.error(create_text_channel("mine")).await;

    assert_eq!(inside.parent_id, Some(category.id));
    assert_eq!(inside.topic.as_deref(), Some("crabs"));
    assert_eq!(code(&nested_category), proto::ErrorCode::InvalidArgument);
    assert_eq!(renamed.name, "rust-lang");
    assert_eq!(renamed.topic, None);
    assert_eq!(orphan.name, "rust-lang");
    assert_eq!(code(&forbidden), proto::ErrorCode::Forbidden);
}

#[tokio::test]
async fn server_profile_and_nickname_changes_are_broadcast() {
    let Pair {
        mut owner,
        mut member,
        member_ready,
        server: _server,
        ..
    } = owner_and_member().await;
    let member_id = self_id(&member_ready);

    let info = match owner
        .ok(Request::UpdateServer(proto::UpdateServer {
            name: Some("Renamed".to_owned()),
            description: Some("A place".to_owned()),
            open_join: None,
        }))
        .await
    {
        Response::Server(info) => info,
        other => panic!("expected server info, got {other:?}"),
    };
    let update = member
        .wait_for(|event| match event {
            Event::ServerUpdate(update) => update.server.clone(),
            _ => None,
        })
        .await;
    member
        .ok(Request::UpdateProfile(proto::UpdateProfile {
            display_name: "Renamed User".to_owned(),
        }))
        .await;
    member
        .ok(Request::UpdateNickname(proto::UpdateNickname {
            user_id: member_id,
            nickname: Some("Nick".to_owned()),
        }))
        .await;
    let nickname = owner
        .wait_for(|event| match event {
            Event::MemberUpdate(update) if update.member.as_ref()?.nickname.is_some() => {
                update.member.clone()
            }
            _ => None,
        })
        .await;
    let not_allowed = member
        .error(Request::UpdateServer(proto::UpdateServer {
            name: Some("Mine".to_owned()),
            ..Default::default()
        }))
        .await;

    assert_eq!(info.name, "Renamed");
    assert_eq!(update.description, "A place");
    assert_eq!(nickname.nickname.as_deref(), Some("Nick"));
    assert_eq!(nickname.user.unwrap().display_name, "Renamed User");
    assert_eq!(code(&not_allowed), proto::ErrorCode::Forbidden);
}

#[tokio::test]
async fn invites_can_be_listed_and_revoked() {
    let Pair {
        server,
        mut owner,
        mut member,
        ..
    } = owner_and_member().await;
    let member_code = invite(&mut member, None).await;

    let listed = match owner
        .ok(Request::FetchInvites(proto::FetchInvites {}))
        .await
    {
        Response::Invites(list) => list.invites,
        other => panic!("expected invites, got {other:?}"),
    };
    let member_list = member
        .error(Request::FetchInvites(proto::FetchInvites {}))
        .await;
    member
        .ok(Request::RevokeInvite(proto::RevokeInvite {
            code: member_code.clone(),
        }))
        .await;
    let mut stranger = TestClient::connect(&server, key(9)).await;
    let refused = stranger
        .identify("stranger", Some(&member_code), None)
        .await
        .unwrap_err();

    assert!(listed.iter().any(|invite| invite.code == member_code));
    assert_eq!(code(&member_list), proto::ErrorCode::Forbidden);
    assert_eq!(code(&refused), proto::ErrorCode::Unauthorized);
}

#[tokio::test]
async fn open_join_lets_anyone_in() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    owner
        .ok(Request::UpdateServer(proto::UpdateServer {
            open_join: Some(true),
            ..Default::default()
        }))
        .await;

    let mut stranger = TestClient::connect(&server, key(9)).await;

    assert!(stranger.identify("stranger", None, None).await.is_ok());
}

#[tokio::test]
async fn new_members_are_announced() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let code = invite(&mut owner, None).await;

    let (_member, member_ready) = join(&server, 2, &code).await;
    let joined = owner
        .wait_for(|event| match event {
            Event::MemberJoin(join) => join.member.clone(),
            _ => None,
        })
        .await;

    assert_eq!(joined.user.unwrap().id, self_id(&member_ready));
    assert_eq!(member_ready.members.len(), 2);
}
