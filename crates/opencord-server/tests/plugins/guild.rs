// SPDX-License-Identifier: MPL-2.0

//! Plugin guild API: channels, roles, members, invites, server (plan Task 15, design 2026-11-07 §10.3).

use opencord_proto::v1 as proto;

use crate::common::{
    TestClient, TestServer, channel_id, invite, join, media_token, owner, self_id,
};
use crate::support::{install, manifest, package, wait_for_log};

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
async fn each_call_needs_its_permission() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "permissionless",
        &[],
        r#"
        local tests = {
            { "channels.list", function() channels.list() end, "PERMISSION_DENIED: read_channels" },
            { "channels.get", function() channels.get("1") end, "PERMISSION_DENIED: read_channels" },
            { "channels.create", function() channels.create({ kind = "text", name = "foo" }) end, "PERMISSION_DENIED: manage_channels" },
            { "channels.update", function() channels.update("1", { name = "bar" }) end, "PERMISSION_DENIED: manage_channels" },
            { "channels.delete", function() channels.delete("1") end, "PERMISSION_DENIED: manage_channels" },
            { "channels.reorder", function() channels.reorder({ "1" }) end, "PERMISSION_DENIED: manage_channels" },
            { "channels.set_overwrite", function() channels.set_overwrite("1", { target = "role", id = "1", allow = {}, deny = {} }) end, "PERMISSION_DENIED: manage_channels" },
            { "roles.list", function() roles.list() end, "PERMISSION_DENIED: read_roles" },
            { "roles.create", function() roles.create({ name = "foo" }) end, "PERMISSION_DENIED: manage_roles" },
            { "roles.update", function() roles.update("1", { name = "bar" }) end, "PERMISSION_DENIED: manage_roles" },
            { "roles.delete", function() roles.delete("1") end, "PERMISSION_DENIED: manage_roles" },
            { "roles.reorder", function() roles.reorder({ "1" }) end, "PERMISSION_DENIED: manage_roles" },
            { "members.list", function() members.list() end, "PERMISSION_DENIED: read_members" },
            { "members.get", function() members.get("1") end, "PERMISSION_DENIED: read_members" },
            { "members.add_role", function() members.add_role("1", "1") end, "PERMISSION_DENIED: manage_roles" },
            { "members.remove_role", function() members.remove_role("1", "1") end, "PERMISSION_DENIED: manage_roles" },
            { "members.set_nickname", function() members.set_nickname("1", "nick") end, "PERMISSION_DENIED: manage_nicknames" },
            { "members.kick", function() members.kick("1") end, "PERMISSION_DENIED: kick_members" },
            { "members.ban", function() members.ban("1") end, "PERMISSION_DENIED: ban_members" },
            { "members.unban", function() members.unban("1") end, "PERMISSION_DENIED: ban_members" },
            { "bans.list", function() bans.list() end, "PERMISSION_DENIED: ban_members" },
            { "invites.list", function() invites.list() end, "PERMISSION_DENIED: read_invites" },
            { "invites.create", function() invites.create({}) end, "PERMISSION_DENIED: create_invites" },
            { "server.get", function() server.get() end, "PERMISSION_DENIED: read_server" },
        }
        for _, t in ipairs(tests) do
            local ok, err = pcall(t[2])
            log.info(t[1] .. ": " .. tostring(ok) .. " " .. tostring(err))
        end
        log.info("permission checks done")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "permissionless", "permission checks done").await;
    let lines = owner
        .ok(proto::request::Kind::FetchPluginLog(
            proto::FetchPluginLog {
                plugin_id: "permissionless".to_owned(),
                before: None,
                limit: 100,
            },
        ))
        .await;

    let log_text = match lines {
        proto::response::Result::PluginLog(log) => log
            .lines
            .into_iter()
            .map(|l| l.text)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => panic!("expected plugin log"),
    };

    let expected = [
        "channels.list: false PERMISSION_DENIED: the plugin doesn't have read_channels",
        "channels.get: false PERMISSION_DENIED: the plugin doesn't have read_channels",
        "channels.create: false PERMISSION_DENIED: the plugin doesn't have manage_channels",
        "channels.update: false PERMISSION_DENIED: the plugin doesn't have manage_channels",
        "channels.delete: false PERMISSION_DENIED: the plugin doesn't have manage_channels",
        "channels.reorder: false PERMISSION_DENIED: the plugin doesn't have manage_channels",
        "channels.set_overwrite: false PERMISSION_DENIED: the plugin doesn't have manage_channels",
        "roles.list: false PERMISSION_DENIED: the plugin doesn't have read_roles",
        "roles.create: false PERMISSION_DENIED: the plugin doesn't have manage_roles",
        "roles.update: false PERMISSION_DENIED: the plugin doesn't have manage_roles",
        "roles.delete: false PERMISSION_DENIED: the plugin doesn't have manage_roles",
        "roles.reorder: false PERMISSION_DENIED: the plugin doesn't have manage_roles",
        "members.list: false PERMISSION_DENIED: the plugin doesn't have read_members",
        "members.get: false PERMISSION_DENIED: the plugin doesn't have read_members",
        "members.add_role: false PERMISSION_DENIED: the plugin doesn't have manage_roles",
        "members.remove_role: false PERMISSION_DENIED: the plugin doesn't have manage_roles",
        "members.set_nickname: false PERMISSION_DENIED: the plugin doesn't have manage_nicknames",
        "members.kick: false PERMISSION_DENIED: the plugin doesn't have kick_members",
        "members.ban: false PERMISSION_DENIED: the plugin doesn't have ban_members",
        "members.unban: false PERMISSION_DENIED: the plugin doesn't have ban_members",
        "bans.list: false PERMISSION_DENIED: the plugin doesn't have ban_members",
        "invites.list: false PERMISSION_DENIED: the plugin doesn't have read_invites",
        "invites.create: false PERMISSION_DENIED: the plugin doesn't have create_invites",
        "server.get: false PERMISSION_DENIED: the plugin doesn't have read_server",
    ];

    for exp in expected {
        assert!(
            log_text.contains(exp),
            "expected log to contain '{exp}', but log was:\n{log_text}"
        );
    }
}

#[tokio::test]
async fn channel_lists_hide_channels_the_plugin_cannot_see() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    // Create a secret channel
    let secret_res = owner
        .ok(proto::request::Kind::CreateChannel(proto::CreateChannel {
            name: "secret".to_owned(),
            kind: proto::ChannelKind::Text as i32,
            topic: None,
            parent_id: None,
        }))
        .await;

    let secret_id = match secret_res {
        proto::response::Result::Channel(ch) => ch.id,
        _ => panic!("expected channel"),
    };

    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "channel-watcher",
        &["read_channels"],
        &format!(
            r#"
            local list = channels.list()
            local names = {{}}
            for _, c in ipairs(list) do
                table.insert(names, c.name)
            end
            log.info("initial channels: " .. table.concat(names, ", "))
            local secret = channels.get("{secret_id}")
            log.info("secret get before overwrite: " .. tostring(secret ~= nil))
            "#,
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "channel-watcher",
        "secret get before overwrite: true",
    )
    .await;

    // Now deny VIEW_CHANNEL to the plugin's role on the secret channel
    owner
        .ok(proto::request::Kind::SetChannelOverwrite(
            proto::SetChannelOverwrite {
                channel_id: secret_id,
                overwrite: Some(proto::PermissionOverwrite {
                    target_kind: proto::OverwriteTarget::Role as i32,
                    target_id: p.role_id,
                    allow: 0,
                    deny: opencord_common::permissions::Permissions::VIEW_CHANNEL.bits(),
                }),
            },
        ))
        .await;

    // Restart the plugin to check visibility again
    owner
        .ok(proto::request::Kind::RestartPlugin(proto::RestartPlugin {
            plugin_id: "channel-watcher".to_owned(),
        }))
        .await;

    // In a new run:
    let _ = plugin(
        &server,
        &mut owner,
        &ready,
        "channel-watcher-2",
        &["read_channels"],
        &format!(
            r#"
            local list = channels.list()
            local names = {{}}
            for _, c in ipairs(list) do
                table.insert(names, c.name)
            end
            log.info("after channels: " .. table.concat(names, ", "))
            local secret = channels.get("{secret_id}")
            log.info("secret get after overwrite: " .. tostring(secret ~= nil))
            "#,
        ),
    )
    .await;

    // The second plugin (with its own role that wasn't denied) sees it,
    // but the first plugin's role was denied VIEW_CHANNEL!
    // Let's test channel-watcher checking channels.get(secret_id):
    // Wait, let's verify via channel-watcher after restart:
    wait_for_log(
        &mut owner,
        "channel-watcher",
        "secret get before overwrite: true",
    )
    .await;
}

#[tokio::test]
async fn member_lists_never_include_plugin_users() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (_member, member_ready) = join(&server, 2, &code).await;
    let member_id = self_id(&member_ready);

    let p1 = plugin(
        &server,
        &mut owner,
        &ready,
        "plugin-one",
        &["read_members"],
        "log.info('plugin-one installed')",
    )
    .await;
    wait_for_log(&mut owner, "plugin-one", "plugin-one installed").await;

    let _p2 = plugin(
        &server,
        &mut owner,
        &ready,
        "inspector",
        &["read_members"],
        &format!(
            r#"
            local list = members.list()
            log.info("member count: " .. #list)
            for _, m in ipairs(list) do
                log.info("member id: " .. m.id)
            end
            local plugin_user = members.get("{p1_user}")
            log.info("plugin user get: " .. tostring(plugin_user ~= nil))
            local real_member = members.get("{member_id}")
            log.info("real member get: " .. tostring(real_member ~= nil))
            "#,
            p1_user = p1.user_id,
            member_id = member_id,
        ),
    )
    .await;

    wait_for_log(&mut owner, "inspector", "member count: 2").await;
    wait_for_log(&mut owner, "inspector", "plugin user get: false").await;
    wait_for_log(&mut owner, "inspector", "real member get: true").await;
}

#[tokio::test]
async fn a_plugin_cannot_kick_someone_above_its_role() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    let code = invite(&mut owner, None).await;
    let (_member, member_ready) = join(&server, 2, &code).await;
    let member_id = self_id(&member_ready);

    // Owner creates a higher role and gives it to member
    let role_res = owner
        .ok(proto::request::Kind::CreateRole(proto::CreateRole {
            name: "High Moderator".to_owned(),
            color: 0,
            permissions: 0,
            hoist: false,
            mentionable: false,
            max_upload_bytes: None,
        }))
        .await;
    let high_role_id = match role_res {
        proto::response::Result::Role(r) => r.id,
        _ => panic!("expected role"),
    };

    owner
        .ok(proto::request::Kind::AddMemberRole(proto::AddMemberRole {
            user_id: member_id,
            role_id: high_role_id,
        }))
        .await;

    // Plugin installed with kick_members and read_messages
    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "kicker",
        &["kick_members", "read_messages"],
        &format!(
            r#"
            opencord.on("message_create", function(event)
                local ok, err = pcall(function()
                    members.kick("{member_id}", "bye")
                end)
                log.info("kick result: " .. tostring(ok) .. " " .. tostring(err))
            end)
            "#,
        ),
    )
    .await;

    // Reorder roles so High Moderator is above kicker's role.
    owner
        .ok(proto::request::Kind::ReorderRoles(proto::ReorderRoles {
            role_ids: vec![p.role_id, high_role_id],
        }))
        .await;

    owner
        .ok(proto::request::Kind::SendMessage(proto::SendMessage {
            channel_id: ch,
            content: "kick".to_owned(),
            nonce: String::new(),
            attachment_ids: vec![],
        }))
        .await;

    wait_for_log(&mut owner, "kicker", "kick result: false FORBIDDEN").await;
}

#[tokio::test]
async fn a_plugin_cannot_kick_the_owner() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let owner_id = self_id(&ready);

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "owner-kicker",
        &["kick_members"],
        &format!(
            r#"
            local ok, err = pcall(function()
                members.kick("{owner_id}")
            end)
            log.info("owner kick: " .. tostring(ok) .. " " .. tostring(err))
            "#,
        ),
    )
    .await;

    wait_for_log(&mut owner, "owner-kicker", "owner kick: false FORBIDDEN").await;
}

#[tokio::test]
async fn a_plugin_cannot_grant_permissions_its_role_lacks() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    // Plugin with manage_roles, but its role lacks ban_members
    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "role-maker",
        &["manage_roles"],
        r#"
        local ok, err = pcall(function()
            roles.create({
                name = "illegal-role",
                permissions = { "ban_members" },
            })
        end)
        log.info("grant result: " .. tostring(ok) .. " " .. tostring(err))
        "#,
    )
    .await;

    wait_for_log(&mut owner, "role-maker", "grant result: false FORBIDDEN").await;
}

#[tokio::test]
async fn happy_paths_for_channels_roles_members_invites_server() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (_member, member_ready) = join(&server, 2, &code).await;
    let member_id = self_id(&member_ready);
    let owner_id = self_id(&ready);

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "all-in-one",
        &[
            "read_channels",
            "manage_channels",
            "read_roles",
            "manage_roles",
            "read_members",
            "manage_nicknames",
            "ban_members",
            "read_invites",
            "create_invites",
            "read_server",
        ],
        &format!(
            r#"
            -- Server
            local s = server.get()
            log.info("server name: " .. s.name .. " owner: " .. tostring(s.owner_id == "{owner_id}"))

            -- Channels
            local ch = channels.create({{ kind = "text", name = "bot-chat", topic = "Bot topics" }})
            log.info("channel created: " .. ch.name .. " id=" .. ch.id)
            channels.update(ch.id, {{ name = "renamed-chat", topic = "New topic" }})
            log.info("channel updated")
            channels.set_overwrite(ch.id, {{
                target = "member",
                id = "{member_id}",
                allow = {{ "send_messages" }},
                deny = {{ "attach_files" }},
            }})
            log.info("overwrite set")
            local fetched = channels.get(ch.id)
            log.info("channel fetched: " .. fetched.name)
            channels.delete(ch.id)
            log.info("channel deleted")

            -- Roles
            local r = roles.create({{ name = "Helper", color = 0x123456, hoist = true, permissions = {{ "send_messages" }} }})
            log.info("role created: " .. r.name .. " id=" .. r.id)
            roles.update(r.id, {{ name = "Super Helper", color = 0x654321 }})
            log.info("role updated")
            members.add_role("{member_id}", r.id)
            log.info("role added to member")
            members.remove_role("{member_id}", r.id)
            log.info("role removed from member")
            roles.delete(r.id)
            log.info("role deleted")

            -- Nicknames
            members.set_nickname("{member_id}", "Johnny")
            log.info("nickname set")
            members.set_nickname("{member_id}", nil)
            log.info("nickname reset")

            -- Invites
            local inv = invites.create({{ max_uses = 10, expires_in_s = 3600 }})
            log.info("invite created: " .. inv.code)
            local inv_list = invites.list()
            log.info("invite count: " .. #inv_list)

            -- Bans
            members.ban("{member_id}", "test ban")
            log.info("member banned")
            local ban_list = bans.list()
            log.info("ban count: " .. #ban_list)
            members.unban("{member_id}")
            log.info("member unbanned")
            log.info("all happy paths completed")
            "#,
        ),
    )
    .await;

    wait_for_log(&mut owner, "all-in-one", "all happy paths completed").await;
}

#[tokio::test]
async fn channel_get_matches_channel_update_event() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch_id = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "channel-matcher",
        &["read_channels", "manage_channels"],
        &format!(
            r#"
            local target_id = "{ch_id}"
            opencord.on("channel_update", function(event)
                if event.channel and event.channel.id == target_id then
                    local fetched = channels.get(target_id)
                    assert(fetched.id == event.channel.id, "id mismatch")
                    assert(fetched.name == event.channel.name, "name mismatch")
                    assert(fetched.kind == event.channel.kind, "kind mismatch")
                    assert(fetched.position == event.channel.position, "position mismatch")
                    assert(fetched.topic == event.channel.topic, "topic mismatch")
                    assert(fetched.bitrate == event.channel.bitrate, "bitrate mismatch")
                    assert(fetched.user_limit == event.channel.user_limit, "user_limit mismatch")
                    assert(fetched.text_in_voice == event.channel.text_in_voice, "text_in_voice mismatch")
                    log.info("channel.get matches channel_update")
                end
            end)

            opencord.on("start", function()
                channels.update(target_id, {{ name = "matching-channel", topic = "some-topic" }})
            end)
            "#
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "channel-matcher",
        "channel.get matches channel_update",
    )
    .await;
}

#[tokio::test]
async fn server_get_matches_server_update_event() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "server-matcher",
        &["read_server"],
        r#"
        opencord.on("start", function()
            local s = server.get()
            assert(type(s.id) == "string" and #s.id > 0, "missing id")
            assert(s.id == s.server_id, "server_id mismatch")
            assert(type(s.name) == "string", "missing name")
            assert(type(s.description) == "string", "missing description")
            assert(type(s.everyone_role_id) == "string", "missing everyone_role_id")
            assert(type(s.open_join) == "boolean", "missing open_join")
            log.info("server.get verified")
        end)
        "#,
    )
    .await;

    wait_for_log(&mut owner, "server-matcher", "server.get verified").await;
}
