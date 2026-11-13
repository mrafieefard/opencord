// SPDX-License-Identifier: MPL-2.0

//! Every event reaching plugins with its table (plan Task 17, design 2026-11-07 §10.4).

use opencord_proto::v1 as proto;
use serde_json::Value;

use crate::common::{
    TestClient, TestServer, channel_id, invite, join, media_token, owner, self_id,
};
use crate::support::{install, manifest, package, wait_for_log};

fn assert_ids_are_strings(val: &Value, path: &str) {
    match val {
        Value::Object(map) => {
            for (k, v) in map {
                let cur_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                if k == "id" || k.ends_with("_id") {
                    assert!(
                        v.is_string(),
                        "expected string for id key '{cur_path}', but got: {v:?}"
                    );
                }
                assert_ids_are_strings(v, &cur_path);
            }
        }
        Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                assert_ids_are_strings(v, &format!("{path}[{i}]"));
            }
        }
        _ => {}
    }
}

async fn plugin(
    server: &TestServer,
    owner: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    permissions: &[&str],
    source: &str,
) -> proto::Plugin {
    let zip = package(&[
        ("manifest.lua", &manifest(id, permissions)),
        ("server/main.luau", source),
    ]);
    install(server, owner, &media_token(ready), zip, permissions).await
}

#[tokio::test]
async fn ids_are_strings_in_every_payload() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    let all_permissions = [
        "read_messages",
        "read_channels",
        "read_roles",
        "read_members",
        "read_server",
        "read_voice",
        "read_music",
        "read_invites",
        "manage_messages",
        "manage_channels",
        "manage_roles",
        "kick_members",
        "ban_members",
        "create_invites",
        "play_soundboard",
        "control_music",
    ];

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "event-inspector",
        &all_permissions,
        r#"
        local event_names = {
            "message_create", "message_update", "message_delete", "typing_start",
            "channel_create", "channel_update", "channel_delete",
            "role_create", "role_update", "role_delete",
            "member_join", "member_leave", "member_update", "member_unban",
            "presence_update", "server_update", "voice_settings_update",
            "upload_settings_update", "voice_state_update", "stream_create",
            "stream_update", "stream_delete", "stream_viewers_update",
            "soundboard_play", "soundboard_sound_create", "soundboard_sound_update",
            "soundboard_sound_delete", "music_now_playing", "music_queue_update",
            "music_notice", "invite_create", "invite_revoke",
        }

        for _, name in ipairs(event_names) do
            opencord.on(name, function(payload)
                log.info("EVENT " .. name .. ": " .. json.encode(payload))
            end)
        end
        log.info("inspector ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "event-inspector", "inspector ready").await;

    // Send a message, edit it, delete it, start typing
    let send_res = owner
        .ok(proto::request::Kind::SendMessage(proto::SendMessage {
            channel_id: ch,
            content: "hello event".to_owned(),
            nonce: String::new(),
            attachment_ids: vec![],
        }))
        .await;

    let msg_id = match send_res {
        proto::response::Result::Message(m) => m.id,
        _ => panic!("expected message"),
    };

    owner
        .ok(proto::request::Kind::StartTyping(proto::StartTyping {
            channel_id: ch,
        }))
        .await;

    owner
        .ok(proto::request::Kind::EditMessage(proto::EditMessage {
            message_id: msg_id,
            content: "hello edited".to_owned(),
        }))
        .await;

    owner
        .ok(proto::request::Kind::DeleteMessage(proto::DeleteMessage {
            message_id: msg_id,
        }))
        .await;

    // Create channel, update channel, delete channel
    let ch_res = owner
        .ok(proto::request::Kind::CreateChannel(proto::CreateChannel {
            name: "event-ch".to_owned(),
            kind: proto::ChannelKind::Text as i32,
            topic: Some("testing".to_owned()),
            parent_id: None,
            plugin_id: None,
            type_key: None,
        }))
        .await;
    let new_ch_id = match ch_res {
        proto::response::Result::Channel(c) => c.id,
        _ => panic!("expected channel"),
    };

    owner
        .ok(proto::request::Kind::UpdateChannel(proto::UpdateChannel {
            channel_id: new_ch_id,
            name: Some("event-ch-renamed".to_owned()),
            topic: None,
            parent_id: None,
            bitrate: None,
            user_limit: None,
            text_in_voice: None,
        }))
        .await;

    owner
        .ok(proto::request::Kind::DeleteChannel(proto::DeleteChannel {
            channel_id: new_ch_id,
        }))
        .await;

    // Create role, update role, delete role
    let role_res = owner
        .ok(proto::request::Kind::CreateRole(proto::CreateRole {
            name: "event-role".to_owned(),
            color: 123,
            permissions: 0,
            hoist: false,
            mentionable: false,
            max_upload_bytes: None,
        }))
        .await;
    let new_role_id = match role_res {
        proto::response::Result::Role(r) => r.id,
        _ => panic!("expected role"),
    };

    owner
        .ok(proto::request::Kind::UpdateRole(proto::UpdateRole {
            role_id: new_role_id,
            name: Some("event-role-renamed".to_owned()),
            color: None,
            hoist: None,
            mentionable: None,
            permissions: None,
            max_upload_bytes: None,
        }))
        .await;

    owner
        .ok(proto::request::Kind::DeleteRole(proto::DeleteRole {
            role_id: new_role_id,
        }))
        .await;

    // Create invite, revoke invite
    let invite_res = owner
        .ok(proto::request::Kind::CreateInvite(proto::CreateInvite {
            max_uses: Some(5),
            expires_in_s: Some(3600),
            domain_id: None,
        }))
        .await;
    let code = match invite_res {
        proto::response::Result::Invite(inv) => inv.code,
        _ => panic!("expected invite"),
    };

    owner
        .ok(proto::request::Kind::RevokeInvite(proto::RevokeInvite {
            code: code.clone(),
        }))
        .await;

    // Member joins with an invite, updates nickname, gets kicked, gets banned, unbanned
    let invite2 = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &invite2).await;
    let member_id = self_id(&member_ready);

    member
        .ok(proto::request::Kind::UpdateNickname(
            proto::UpdateNickname {
                user_id: member_id,
                nickname: Some("new-nick".to_owned()),
            },
        ))
        .await;

    owner
        .ok(proto::request::Kind::KickMember(proto::KickMember {
            user_id: member_id,
            reason: Some("kick reason".to_owned()),
        }))
        .await;

    // Rejoin and ban
    let invite3 = invite(&mut owner, None).await;
    let (_member2, member2_ready) = join(&server, 3, &invite3).await;
    let member2_id = self_id(&member2_ready);

    owner
        .ok(proto::request::Kind::BanMember(proto::BanMember {
            user_id: member2_id,
            reason: Some("ban reason".to_owned()),
        }))
        .await;

    owner
        .ok(proto::request::Kind::UnbanMember(proto::UnbanMember {
            user_id: member2_id,
        }))
        .await;

    // Server settings updates
    owner
        .ok(proto::request::Kind::UpdateServer(proto::UpdateServer {
            name: Some("server-renamed".to_owned()),
            description: None,
            open_join: None,
        }))
        .await;

    owner
        .ok(proto::request::Kind::UpdateVoiceSettings(
            proto::UpdateVoiceSettings {
                sound_cooldown_s: Some(15),
                ..Default::default()
            },
        ))
        .await;

    owner
        .ok(proto::request::Kind::UpdateUploadSettings(
            proto::UpdateUploadSettings {
                blocked_extensions: Some(proto::BlockedExtensions {
                    extensions: vec!["exe".to_owned()],
                }),
            },
        ))
        .await;

    // Wait for the last event log
    wait_for_log(
        &mut owner,
        "event-inspector",
        "EVENT upload_settings_update:",
    )
    .await;

    // Fetch all logs
    let logs_res = owner
        .ok(proto::request::Kind::FetchPluginLog(
            proto::FetchPluginLog {
                plugin_id: "event-inspector".to_owned(),
                limit: 500,
                before: None,
            },
        ))
        .await;

    let lines = match logs_res {
        proto::response::Result::PluginLog(l) => l.lines,
        _ => panic!("expected plugin log"),
    };

    let mut events_map: std::collections::HashMap<String, Vec<Value>> =
        std::collections::HashMap::new();
    let mut event_count = 0;
    for line in lines {
        if let Some(json_str) = line.text.strip_prefix("EVENT ") {
            let (event_name, payload_json) = json_str.split_once(": ").unwrap();
            let parsed: Value = serde_json::from_str(payload_json)
                .unwrap_or_else(|e| panic!("invalid json in log '{line:?}': {e}"));
            assert_ids_are_strings(&parsed, event_name);
            events_map
                .entry(event_name.to_owned())
                .or_default()
                .push(parsed);
            event_count += 1;
        }
    }

    assert!(
        event_count >= 18,
        "expected at least 18 events logged, got {event_count}"
    );

    // Verify member_join payload
    let join_events = &events_map["member_join"];
    assert!(!join_events.is_empty());
    assert_eq!(join_events[0]["invite_code"], invite2);
    assert_eq!(join_events[0]["member"]["name"], "user2");

    // Verify member_leave (kicked)
    let leave_events = &events_map["member_leave"];
    assert!(leave_events.len() >= 2);
    let kicked = leave_events
        .iter()
        .find(|e| e["reason"] == "kicked")
        .expect("kicked event");
    assert_eq!(kicked["by"]["name"], "Owner");
    assert_eq!(kicked["user"]["name"], "new-nick");

    // Verify member_leave (banned)
    let banned = leave_events
        .iter()
        .find(|e| e["reason"] == "banned")
        .expect("banned event");
    assert_eq!(banned["by"]["name"], "Owner");
    assert_eq!(banned["ban_reason"], "ban reason");

    // Verify member_unban
    let unban_events = &events_map["member_unban"];
    assert_eq!(unban_events[0]["user_id"], member2_id.to_string());
    assert_eq!(unban_events[0]["by"]["name"], "Owner");

    // Verify invite_create
    let invite_creates = &events_map["invite_create"];
    assert_eq!(invite_creates[0]["code"], code);
    assert_eq!(invite_creates[0]["by"]["name"], "Owner");
    assert_eq!(invite_creates[0]["max_uses"], 5);

    // Verify invite_revoke
    let invite_revokes = &events_map["invite_revoke"];
    assert_eq!(invite_revokes[0]["code"], code);
    assert_eq!(invite_revokes[0]["by"]["name"], "Owner");

    // Verify channel_create, update, delete
    assert_eq!(
        events_map["channel_create"][0]["channel"]["name"],
        "event-ch"
    );
    assert_eq!(
        events_map["channel_update"][0]["channel"]["name"],
        "event-ch-renamed"
    );
    assert_eq!(
        events_map["channel_delete"][0]["channel_id"],
        new_ch_id.to_string()
    );

    // Verify role_create, update, delete
    assert_eq!(events_map["role_create"][0]["role"]["name"], "event-role");
    let new_role_id_str = new_role_id.to_string();
    let role_update = events_map["role_update"]
        .iter()
        .find(|e| e["role"]["id"] == new_role_id_str)
        .expect("role_update for created role");
    assert_eq!(role_update["role"]["name"], "event-role-renamed");
    assert_eq!(events_map["role_delete"][0]["role_id"], new_role_id_str);

    // Verify settings updates
    assert_eq!(
        events_map["server_update"][0]["server"]["name"],
        "server-renamed"
    );
    assert_eq!(
        events_map["voice_settings_update"][0]["settings"]["sound_cooldown_s"],
        15
    );
    assert_eq!(
        events_map["upload_settings_update"][0]["settings"]["blocked_extensions"],
        serde_json::json!(["exe"])
    );
}
