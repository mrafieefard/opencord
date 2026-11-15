// SPDX-License-Identifier: MPL-2.0

//! Plugin channels: the channel kind, its migration and who sees it (plan
//! Task 7, design 2026-11-07 §14.2); channel types and whether a channel
//! can be opened (plan Task 8, §14.1 and §14.4).

use std::time::Duration;

use opencord_proto::v1 as proto;
use proto::PluginChannelState as State;
use proto::event::Kind as Event;
use proto::request::Kind;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{AssertSqlSafe, SqlitePool};

use crate::common::{TestClient, TestServer, invite, media_token, owner};
use crate::support::{install, log_lines, manifest, manifest_with, package, session, wait_for_log};

const FEATURE: &str = "plugin_channels";

/// The script of a plugin registering each of `keys` named "Tickets".
fn registering(keys: &[&str]) -> String {
    keys.iter()
        .map(|key| format!("ui.register_channel_type({key:?}, {{ name = \"Tickets\" }})\n"))
        .collect()
}

/// Installs plugin `id` at `version`, running `source`, through `client`
/// (an admin) whose Ready is `ready`; returns once it started.
async fn plugin_at(
    server: &TestServer,
    client: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    version: &str,
    source: &str,
) -> proto::Plugin {
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with(id, version, &["create_channel_types"], ""),
        ),
        ("server/main.luau", source),
        ("assets/desk.png", "not really a png"),
    ]);
    install(
        server,
        client,
        &media_token(ready),
        zip,
        &["create_channel_types"],
    )
    .await
}

/// [`plugin_at`] version 1.0.0 of `"tickets"`, registering `keys`.
async fn tickets(
    server: &TestServer,
    client: &mut TestClient,
    ready: &proto::Ready,
    keys: &[&str],
) -> proto::Plugin {
    plugin_at(
        server,
        client,
        ready,
        "tickets",
        "1.0.0",
        &registering(keys),
    )
    .await
}

fn plugin_channel(plugin_id: &str, type_key: &str) -> Kind {
    Kind::CreateChannel(proto::CreateChannel {
        kind: proto::ChannelKind::Plugin as i32,
        name: "Help".to_owned(),
        plugin_id: Some(plugin_id.to_owned()),
        type_key: Some(type_key.to_owned()),
        ..Default::default()
    })
}

fn set_running(running: bool) -> Kind {
    Kind::SetPluginRunning(proto::SetPluginRunning {
        plugin_id: "tickets".to_owned(),
        running,
    })
}

/// Waits up to 5 s for an event `pick` takes.
async fn within<T>(client: &mut TestClient, pick: impl FnMut(&Event) -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), client.wait_for(pick))
        .await
        .expect("the event never came")
}

/// The next list of channel types `client` is sent.
async fn types_update(client: &mut TestClient) -> Vec<proto::PluginChannelType> {
    within(client, |event| match event {
        Event::PluginChannelTypesUpdate(update) => Some(update.types.clone()),
        _ => None,
    })
    .await
}

/// The next `ChannelUpdate` of `channel_id` that `client` is sent.
async fn channel_update(client: &mut TestClient, channel_id: i64) -> proto::Channel {
    within(client, |event| match event {
        Event::ChannelUpdate(update) => update
            .channel
            .clone()
            .filter(|channel| channel.id == channel_id),
        _ => None,
    })
    .await
}

fn create_plugin_channel(name: &str) -> Kind {
    Kind::CreateChannel(proto::CreateChannel {
        kind: proto::ChannelKind::Plugin as i32,
        name: name.to_owned(),
        plugin_id: Some("tickets".to_owned()),
        type_key: Some("tickets".to_owned()),
        ..Default::default()
    })
}

fn create_text_channel(name: &str) -> Kind {
    Kind::CreateChannel(proto::CreateChannel {
        kind: proto::ChannelKind::Text as i32,
        name: name.to_owned(),
        ..Default::default()
    })
}

fn channel_of(result: proto::response::Result) -> proto::Channel {
    match result {
        proto::response::Result::Channel(channel) => channel,
        other => panic!("expected a channel, got {other:?}"),
    }
}

/// The channel id an event is about, if it's a channel event.
fn channel_event_id(event: &Event) -> Option<i64> {
    match event {
        Event::ChannelCreate(create) => create.channel.as_ref().map(|channel| channel.id),
        Event::ChannelUpdate(update) => update.channel.as_ref().map(|channel| channel.id),
        Event::ChannelDelete(delete) => Some(delete.channel_id),
        _ => None,
    }
}

/// Every event up to the `ChannelCreate` of `marker`, which a session
/// without the feature still gets: the events before it are the ones it was
/// sent about the plugin channel.
async fn events_until_created(client: &mut TestClient, marker: i64) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        let event = client.next_event().await;
        if matches!(&event, Event::ChannelCreate(_)) && channel_event_id(&event) == Some(marker) {
            return seen;
        }
        seen.push(event);
    }
}

// --- The migration -----------------------------------------------------

/// Each row of `table`, as text, with the columns `columns` (all of them
/// when empty), sorted.
async fn rows(pool: &SqlitePool, table: &str, columns: &[&str]) -> Vec<String> {
    let names: Vec<String> = if columns.is_empty() {
        sqlx::query_scalar(AssertSqlSafe(format!(
            "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
        )))
        .fetch_all(pool)
        .await
        .unwrap()
    } else {
        columns.iter().map(|name| (*name).to_owned()).collect()
    };
    let quoted: Vec<String> = names.iter().map(|name| format!("quote({name})")).collect();
    sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT {} FROM {table} ORDER BY 1",
        quoted.join(" || ',' || ")
    )))
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The migrations up to and including `version`.
fn migrations_through(version: i64) -> sqlx::migrate::Migrator {
    let all = sqlx::migrate!("./migrations");
    sqlx::migrate::Migrator {
        migrations: std::borrow::Cow::Owned(
            all.migrations
                .iter()
                .filter(|migration| migration.version <= version)
                .cloned()
                .collect(),
        ),
        ignore_missing: all.ignore_missing,
        locking: all.locking,
        create_schemas: all.create_schemas.clone(),
        no_tx: all.no_tx,
        table_name: all.table_name.clone(),
    }
}

const OLD_CHANNEL_COLUMNS: [&str; 10] = [
    "id",
    "kind",
    "name",
    "topic",
    "parent_id",
    "position",
    "created_at",
    "bitrate",
    "user_limit",
    "text_in_voice",
];

/// Tables whose rows the rebuild must keep exactly.
const KEPT: [&str; 7] = [
    "channel_overwrites",
    "messages",
    "attachments",
    "music_sessions",
    "music_local_songs",
    "music_local_song_owners",
    "blob_deletions",
];

#[tokio::test]
async fn the_rebuild_keeps_channels_overwrites_and_messages() {
    // Fails if the migration leaves foreign keys on while it drops the old
    // table (the cascade deletes every overwrite, message and music
    // session), forgets the attachments trigger, or doesn't turn foreign
    // keys back on.
    let dir = tempfile::tempdir().unwrap();
    let options = SqliteConnectOptions::new()
        .filename(dir.path().join("opencord.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    // One connection, so the checks below see the one the migration ran on.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    migrations_through(15).run(&pool).await.unwrap();
    for statement in [
        "INSERT INTO users (id, public_key, display_name, created_at)
         VALUES (1, X'01', 'Ann', 0), (2, X'02', 'Bo', 0)",
        "INSERT INTO members (user_id, joined_at) VALUES (1, 0), (2, 0)",
        "INSERT INTO channels (id, kind, name, topic, parent_id, position, created_at,
                               bitrate, user_limit, text_in_voice)
         VALUES (100, 'category', 'Talk', NULL, NULL, 0, 1, 64000, 0, TRUE),
                (101, 'text', 'general', 'Say hi', 100, 0, 2, 64000, 0, TRUE),
                (102, 'voice', 'Lounge', NULL, 100, 1, 3, 96000, 5, FALSE),
                (103, 'text', 'news', NULL, NULL, 1, 4, 64000, 0, TRUE)",
        "INSERT INTO channel_overwrites (channel_id, target_kind, target_id, allow, deny)
         VALUES (101, 'role', 7, 1, 2), (102, 'member', 2, 0, 1024), (100, 'role', 7, 0, 1)",
        "INSERT INTO messages (id, channel_id, author_id, content, edited_at)
         VALUES (500, 101, 1, 'hello', NULL), (501, 101, 2, 'hi', 9), (502, 102, 1, 'yo', NULL)",
        "INSERT INTO attachments (id, uploader_id, channel_id, message_id, position, state, kind,
                                  filename, size_bytes, storage_id, key_enc, created_at, updated_at)
         VALUES (600, 1, 101, 500, 0, 'sent', 'image', 'a.png', 10, 1, X'AA', 0, 0),
                (601, 2, 102, NULL, 0, 'music', 'music', 'song.ogg', 20, 1, X'BB', 0, 0)",
        "INSERT INTO music_sessions (channel_id, state_json, updated_at) VALUES (102, '{}', 5)",
        "INSERT INTO music_local_songs (id, sha256, size_bytes, filename, title, artist,
                                        duration_ms, uploader_id, created_at)
         VALUES (601, X'01', 20, 'song.ogg', 'Song', 'Band', 1000, 2, 0)",
        "INSERT INTO music_local_song_owners (song_id, user_id, added_at) VALUES (601, 2, 0)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let channels_before = rows(&pool, "channels", &OLD_CHANNEL_COLUMNS).await;
    let mut kept_before = Vec::new();
    for table in KEPT {
        kept_before.push(rows(&pool, table, &[]).await);
    }

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    assert_eq!(
        rows(&pool, "channels", &OLD_CHANNEL_COLUMNS).await,
        channels_before
    );
    for (table, before) in KEPT.iter().zip(&kept_before) {
        assert_eq!(&rows(&pool, table, &[]).await, before, "{table}");
    }
    assert_eq!(
        rows(&pool, "channels", &["plugin_id", "type_key"]).await,
        vec!["NULL,NULL"; 4]
    );
    let broken: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(broken, 0);
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(foreign_keys, 1, "foreign keys are on again");
    let leftovers: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE name LIKE '%channels_new%'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(leftovers, 0);

    // The references still hold: deleting a channel cascades, the trigger
    // still spares songs, and a category's children are let go.
    for statement in [
        "DELETE FROM channels WHERE id = 101",
        "DELETE FROM channels WHERE id = 102",
        "DELETE FROM channels WHERE id = 100",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    for (query, expected) in [
        ("SELECT COUNT(*) FROM messages", 0),
        ("SELECT COUNT(*) FROM channel_overwrites", 0),
        ("SELECT COUNT(*) FROM music_sessions", 0),
        ("SELECT COUNT(*) FROM attachments WHERE id = 600", 0),
        ("SELECT COUNT(*) FROM attachments WHERE id = 601", 1),
        (
            "SELECT COUNT(*) FROM channels WHERE id = 103 AND parent_id IS NULL",
            1,
        ),
    ] {
        let count: i64 = sqlx::query_scalar(query).fetch_one(&pool).await.unwrap();
        assert_eq!(count, expected, "{query}");
    }

    // The new kind, and its check both ways.
    sqlx::query(
        "INSERT INTO channels (id, kind, name, position, created_at, plugin_id, type_key)
         VALUES (110, 'plugin', 'Tickets', 2, 0, 'tickets', 'tickets')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for refused in [
        "INSERT INTO channels (id, kind, name, position, created_at)
         VALUES (111, 'plugin', 'Tickets', 3, 0)",
        "INSERT INTO channels (id, kind, name, position, created_at, plugin_id)
         VALUES (112, 'plugin', 'Tickets', 3, 0, 'tickets')",
        "INSERT INTO channels (id, kind, name, position, created_at, plugin_id, type_key)
         VALUES (113, 'text', 'chat', 3, 0, 'tickets', 'tickets')",
        "INSERT INTO channels (id, kind, name, position, created_at)
         VALUES (114, 'forum', 'chat', 3, 0)",
    ] {
        assert!(
            sqlx::query(refused).execute(&pool).await.is_err(),
            "{refused}"
        );
    }
}

#[tokio::test]
async fn the_upgrade_gives_everyone_start_activities_and_nothing_else() {
    // Fails if 0017 drops its WHERE (the second role gains the bit too) or
    // sets another bit than START_ACTIVITIES.
    use opencord_common::permissions::Permissions;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().in_memory(true))
        .await
        .unwrap();
    migrations_through(16).run(&pool).await.unwrap();
    let everyone = Permissions::VIEW_CHANNEL | Permissions::CONNECT;
    let speaker = Permissions::SPEAK;
    sqlx::query(
        "INSERT OR REPLACE INTO server_meta (key, value) VALUES ('everyone_role_id', '900')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO roles (id, name, position, permissions)
         VALUES (900, '@everyone', 0, ?), (901, 'Speakers', 1, ?)",
    )
    .bind(everyone.bits() as i64)
    .bind(speaker.bits() as i64)
    .execute(&pool)
    .await
    .unwrap();

    migrations_through(17).run(&pool).await.unwrap();

    let permissions = |id: i64| {
        let pool = pool.clone();
        async move {
            let bits: i64 = sqlx::query_scalar("SELECT permissions FROM roles WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
            Permissions::from_bits_retain(bits as u64)
        }
    };
    assert_eq!(
        permissions(900).await,
        everyone | Permissions::START_ACTIVITIES
    );
    assert_eq!(permissions(901).await, speaker);
}

// --- Creating one ------------------------------------------------------

#[tokio::test]
async fn a_plugin_channel_needs_its_plugin_and_type() {
    // Fails if CreateChannel stores a plugin channel without its plugin or
    // type (the database's check then fails as an internal error), takes
    // a malformed key, or lets another kind carry them.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    tickets(&server, &mut admin, &ready, &[&"k".repeat(40)]).await;
    let plugin = |plugin_id: Option<&str>, type_key: Option<&str>| {
        Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Plugin as i32,
            name: "Tickets".to_owned(),
            plugin_id: plugin_id.map(str::to_owned),
            type_key: type_key.map(str::to_owned),
            ..Default::default()
        })
    };

    for (request, message) in [
        (
            plugin(None, None),
            "a plugin channel needs its plugin_id and type_key",
        ),
        (
            plugin(Some("tickets"), None),
            "a plugin channel needs its plugin_id and type_key",
        ),
        (
            plugin(None, Some("tickets")),
            "a plugin channel needs its plugin_id and type_key",
        ),
        (
            plugin(Some("Tickets!"), Some("tickets")),
            "plugin_id is not a plugin id",
        ),
        (
            plugin(Some("tickets"), Some("Bad Key")),
            "type_key must be 1 to 40 of a-z, 0-9 and _",
        ),
        (
            plugin(Some("tickets"), Some(&"k".repeat(41))),
            "type_key must be 1 to 40 of a-z, 0-9 and _",
        ),
        (
            Kind::CreateChannel(proto::CreateChannel {
                kind: proto::ChannelKind::Text as i32,
                name: "chat".to_owned(),
                plugin_id: Some("tickets".to_owned()),
                type_key: Some("tickets".to_owned()),
                ..Default::default()
            }),
            "only plugin channels have a plugin_id and type_key",
        ),
    ] {
        let error = admin.error(request).await;
        assert_eq!(error.code, proto::ErrorCode::InvalidArgument as i32);
        assert_eq!(error.message, message);
    }

    let channel = channel_of(
        admin
            .ok(plugin(Some("tickets"), Some(&"k".repeat(40))))
            .await,
    );
    assert_eq!(channel.kind, proto::ChannelKind::Plugin as i32);
    assert_eq!(channel.name, "Tickets");
    assert_eq!(channel.plugin_id.as_deref(), Some("tickets"));
    assert_eq!(channel.type_key, Some("k".repeat(40)));
    assert_eq!(channel.plugin_state, State::Available as i32);
}

#[tokio::test]
async fn a_plugin_channel_takes_no_messages() {
    // Fails if sending, typing, reading history, running a command or
    // joining voice works in a plugin channel, as in a text channel.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    tickets(&server, &mut admin, &ready, &["tickets"]).await;
    let channel_id = channel_of(admin.ok(create_plugin_channel("Tickets")).await).id;

    for (request, message) in [
        (
            Kind::SendMessage(proto::SendMessage {
                channel_id,
                content: "hello".to_owned(),
                ..Default::default()
            }),
            "only text and voice channels have messages",
        ),
        (
            Kind::StartTyping(proto::StartTyping { channel_id }),
            "only text and voice channels have messages",
        ),
        (
            Kind::FetchMessages(proto::FetchMessages {
                channel_id,
                ..Default::default()
            }),
            "only text and voice channels have messages",
        ),
        (
            Kind::ExecuteCommand(proto::ExecuteCommand {
                channel_id,
                name: "play".to_owned(),
                ..Default::default()
            }),
            "only text and voice channels have messages",
        ),
        (
            Kind::UpdateVoiceState(proto::UpdateVoiceState {
                channel_id: Some(channel_id),
                ..Default::default()
            }),
            "that is not a voice channel",
        ),
    ] {
        let error = admin.error(request).await;
        assert_eq!(error.code, proto::ErrorCode::InvalidArgument as i32);
        assert_eq!(error.message, message);
    }
}

// --- Older apps --------------------------------------------------------

#[tokio::test]
async fn an_older_app_never_sees_a_plugin_channel() {
    // Fails if a session that didn't list "plugin_channels" gets a plugin
    // channel in Ready or in any channel event: created, updated,
    // reordered, an overwrite set, hidden and shown again, deleted.
    // Six sessions identify from one address.
    let server = TestServer::start_with(|config| config.gateway.identify_per_minute = 20).await;
    let (mut old_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    let code = invite(&mut admin, None).await;
    let (mut old_member, _) = session(&server, 2, Some(&code), &[]).await;
    let (mut new_member, _) = session(&server, 3, Some(&code), &[FEATURE]).await;
    let everyone = ready.server.as_ref().unwrap().everyone_role_id;
    tickets(&server, &mut admin, &ready, &["tickets"]).await;

    let plugin_id = channel_of(admin.ok(create_plugin_channel("Tickets")).await).id;
    admin
        .ok(Kind::UpdateChannel(proto::UpdateChannel {
            channel_id: plugin_id,
            name: Some("Support".to_owned()),
            ..Default::default()
        }))
        .await;
    admin
        .ok(Kind::ReorderChannels(proto::ReorderChannels {
            positions: vec![proto::ChannelPosition {
                channel_id: plugin_id,
                position: 7,
            }],
        }))
        .await;
    // Hidden from @everyone, then shown again: members see it go and come.
    admin
        .ok(Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: plugin_id,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: everyone,
                allow: 0,
                deny: opencord_common::permissions::Permissions::VIEW_CHANNEL.bits(),
            }),
        }))
        .await;
    admin
        .ok(Kind::DeleteChannelOverwrite(
            proto::DeleteChannelOverwrite {
                channel_id: plugin_id,
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: everyone,
            },
        ))
        .await;

    // Ready, while it exists.
    let (_later_old, old_ready) = session(&server, 2, None, &[]).await;
    let (_later_new, new_ready) = session(&server, 3, None, &[FEATURE]).await;
    assert!(!old_ready.channels.iter().any(|c| c.id == plugin_id));
    assert!(!old_ready.channel_permissions.contains_key(&plugin_id));
    let listed = new_ready
        .channels
        .iter()
        .find(|c| c.id == plugin_id)
        .expect("the feature session gets it in Ready");
    assert_eq!(listed.plugin_id.as_deref(), Some("tickets"));
    assert!(new_ready.channel_permissions.contains_key(&plugin_id));

    admin
        .ok(Kind::DeleteChannel(proto::DeleteChannel {
            channel_id: plugin_id,
        }))
        .await;
    let marker = channel_of(admin.ok(create_text_channel("marker")).await).id;

    for client in [&mut old_owner, &mut old_member] {
        let about_it: Vec<Event> = events_until_created(client, marker)
            .await
            .into_iter()
            .filter(|event| channel_event_id(event) == Some(plugin_id))
            .collect();
        assert!(about_it.is_empty(), "an older app got {about_it:?}");
    }
    // The same events reach an app that knows plugin channels.
    let kinds: Vec<&str> = events_until_created(&mut new_member, marker)
        .await
        .iter()
        .filter(|event| channel_event_id(event) == Some(plugin_id))
        .map(|event| match event {
            Event::ChannelCreate(_) => "create",
            Event::ChannelUpdate(_) => "update",
            Event::ChannelDelete(_) => "delete",
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        kinds,
        ["create", "update", "update", "delete", "create", "delete"]
    );
    old_owner.expect_silence(Duration::from_millis(200)).await;
}

// --- Keeping it --------------------------------------------------------

#[tokio::test]
async fn a_plugin_channel_round_trips_through_the_guild() {
    // Fails if loading the guild drops a plugin channel's plugin or type,
    // or reads it back as another kind.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    tickets(&server, &mut admin, &ready, &["desk"]).await;
    let category = channel_of(
        admin
            .ok(Kind::CreateChannel(proto::CreateChannel {
                kind: proto::ChannelKind::Category as i32,
                name: "Help".to_owned(),
                ..Default::default()
            }))
            .await,
    )
    .id;
    let created = channel_of(
        admin
            .ok(Kind::CreateChannel(proto::CreateChannel {
                kind: proto::ChannelKind::Plugin as i32,
                name: "Tickets".to_owned(),
                topic: Some("Ask us".to_owned()),
                parent_id: Some(category),
                plugin_id: Some("tickets".to_owned()),
                type_key: Some("desk".to_owned()),
            }))
            .await,
    );
    let created = channel_of(
        admin
            .ok(Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
                channel_id: created.id,
                overwrite: Some(proto::PermissionOverwrite {
                    target_kind: proto::OverwriteTarget::Role as i32,
                    target_id: ready.server.as_ref().unwrap().everyone_role_id,
                    allow: 0,
                    deny: opencord_common::permissions::Permissions::SEND_MESSAGES.bits(),
                }),
            }))
            .await,
    );
    drop(admin);
    drop(_owner);

    let server = server.restart(|_| {}).await;
    let (_admin, ready) = session(&server, 1, None, &[FEATURE]).await;

    let loaded = ready
        .channels
        .iter()
        .find(|channel| channel.id == created.id)
        .expect("the plugin channel is loaded");
    // The plugin starts again in the background, so it may not run yet.
    let unjudged = |channel: &proto::Channel| proto::Channel {
        plugin_state: 0,
        ..channel.clone()
    };
    assert_eq!(unjudged(loaded), unjudged(&created));
    assert_eq!(loaded.kind, proto::ChannelKind::Plugin as i32);
    assert_eq!(loaded.type_key.as_deref(), Some("desk"));
}

// --- Channel types (Task 8) ---------------------------------------------

#[tokio::test]
async fn a_registered_type_is_listed_to_feature_sessions() {
    // Fails if the list goes to apps that don't show plugin channels (in
    // Ready or as an update), or a type's fields are lost on the way.
    let server = TestServer::start_with(|config| config.gateway.identify_per_minute = 20).await;
    let (mut old_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    plugin_at(
        &server,
        &mut admin,
        &ready,
        "tickets",
        "1.0.0",
        r#"ui.register_channel_type("desk", {
            name = "Help desk", description = "Ask us", icon = "assets/desk.png",
        })"#,
    )
    .await;
    let expected = vec![proto::PluginChannelType {
        plugin_id: "tickets".to_owned(),
        type_key: "desk".to_owned(),
        name: "Help desk".to_owned(),
        description: "Ask us".to_owned(),
        icon: "assets/desk.png".to_owned(),
        plugin_name: "Test plugin".to_owned(),
    }];

    assert_eq!(types_update(&mut admin).await, expected);
    let (_later, new_ready) = session(&server, 1, None, &[FEATURE]).await;
    assert_eq!(new_ready.plugin_channel_types, expected);
    let (_older, old_ready) = session(&server, 1, None, &[]).await;
    assert!(old_ready.plugin_channel_types.is_empty());
    old_owner
        .assert_no_event(Duration::from_millis(200), |event| {
            matches!(event, Event::PluginChannelTypesUpdate(_))
        })
        .await;
}

#[tokio::test]
async fn a_channel_of_an_unregistered_type_is_refused() {
    // Fails if CreateChannel takes a plugin channel whose plugin doesn't
    // run or didn't register the type.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    let refused = |error: proto::Error| {
        assert_eq!(error.code, proto::ErrorCode::InvalidArgument as i32);
        assert_eq!(error.message, "that channel type isn't available");
    };

    refused(admin.error(plugin_channel("tickets", "desk")).await);
    tickets(&server, &mut admin, &ready, &["desk"]).await;
    refused(admin.error(plugin_channel("tickets", "other")).await);
    refused(admin.error(plugin_channel("elsewhere", "desk")).await);
    let created = channel_of(admin.ok(plugin_channel("tickets", "desk")).await);
    assert_eq!(created.plugin_state, State::Available as i32);

    admin.ok(set_running(false)).await;
    refused(admin.error(plugin_channel("tickets", "desk")).await);
}

#[tokio::test]
async fn stopping_the_plugin_makes_its_channels_unavailable_and_starting_brings_them_back() {
    // Fails if stopping leaves the channels available, starting doesn't
    // bring them back, or either changes the channel itself.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    tickets(&server, &mut admin, &ready, &["desk"]).await;
    let code = invite(&mut admin, None).await;
    let (mut member, _) = session(&server, 2, Some(&code), &[FEATURE]).await;
    let channel_id = channel_of(admin.ok(plugin_channel("tickets", "desk")).await).id;
    let created = channel_of(
        admin
            .ok(Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
                channel_id,
                overwrite: Some(proto::PermissionOverwrite {
                    target_kind: proto::OverwriteTarget::Role as i32,
                    target_id: ready.server.as_ref().unwrap().everyone_role_id,
                    allow: 0,
                    deny: opencord_common::permissions::Permissions::SEND_MESSAGES.bits(),
                }),
            }))
            .await,
    );
    assert_eq!(created.plugin_state, State::Available as i32);

    admin.ok(set_running(false)).await;
    assert!(types_update(&mut member).await.is_empty());
    let stopped = channel_update(&mut member, channel_id).await;
    assert_eq!(stopped.plugin_state, State::PluginStopped as i32);
    assert_eq!(
        proto::Channel {
            plugin_state: State::Available as i32,
            ..stopped
        },
        created
    );

    admin.ok(set_running(true)).await;
    assert_eq!(types_update(&mut member).await.len(), 1);
    assert_eq!(channel_update(&mut member, channel_id).await, created);
}

#[tokio::test]
async fn a_version_without_the_type_shows_type_gone() {
    // Fails if an update that drops a type leaves its channels available,
    // or shows them stopped on the way.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    tickets(&server, &mut admin, &ready, &["desk"]).await;
    let channel_id = channel_of(admin.ok(plugin_channel("tickets", "desk")).await).id;
    let code = invite(&mut admin, None).await;
    let (mut member, _) = session(&server, 2, Some(&code), &[FEATURE]).await;

    plugin_at(
        &server,
        &mut admin,
        &ready,
        "tickets",
        "2.0.0",
        &registering(&["queue"]),
    )
    .await;

    let types = types_update(&mut member).await;
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].type_key, "queue");
    assert_eq!(
        channel_update(&mut member, channel_id).await.plugin_state,
        State::TypeGone as i32
    );
}

#[tokio::test]
async fn a_restart_with_the_same_types_sends_nothing() {
    // Fails if a restart (by an admin, or after a crash) clears the types
    // while the new VM starts, sending "stopped" then "available" again.
    // 64 MiB is then more than the plugin may use.
    let server = TestServer::start_with(|config| config.plugins.memory_mb = 16).await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    let source = format!(
        r#"{}
        if not files.get("crashed") then
            files.put("crashed", "1")
            log.info("first run")
            -- Late enough for the member to join first.
            timer.after(1, function() local s = string.rep("x", 64 * 1024 * 1024) end)
        else
            log.info("run after the crash")
        end"#,
        registering(&["desk"])
    );
    plugin_at(&server, &mut admin, &ready, "tickets", "1.0.0", &source).await;
    let channel_id = channel_of(admin.ok(plugin_channel("tickets", "desk")).await).id;
    let code = invite(&mut admin, None).await;
    let (mut member, _) = session(&server, 2, Some(&code), &[FEATURE]).await;
    wait_for_log(&mut admin, "tickets", "run after the crash").await;
    admin
        .ok(Kind::RestartPlugin(proto::RestartPlugin {
            plugin_id: "tickets".to_owned(),
        }))
        .await;
    let marker = channel_of(admin.ok(create_text_channel("marker")).await).id;

    let sent: Vec<Event> = events_until_created(&mut member, marker)
        .await
        .into_iter()
        .filter(|event| {
            matches!(event, Event::PluginChannelTypesUpdate(_))
                || matches!(event, Event::ChannelUpdate(_))
                    && channel_event_id(event) == Some(channel_id)
        })
        .collect();
    assert!(sent.is_empty(), "a restart sent {sent:?}");
    let lines = log_lines(&mut admin, "tickets").await;
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("run after the crash"))
            .count(),
        2,
        "it restarted after the crash, then by the admin: {lines:#?}"
    );
}

#[tokio::test]
async fn the_plugin_hears_its_channels_created_and_deleted() {
    // Fails if the owning plugin doesn't hear of its channels without
    // read_channels, or another plugin with the same key hears of them.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    let listening = format!(
        r#"{}
        opencord.on("plugin_channel_create", function(event)
            log.info("created " .. event.channel.id .. " " .. event.channel.type_key)
        end)
        opencord.on("plugin_channel_delete", function(event)
            log.info("deleted " .. event.channel_id)
        end)"#,
        registering(&["desk"])
    );
    plugin_at(&server, &mut admin, &ready, "tickets", "1.0.0", &listening).await;
    plugin_at(&server, &mut admin, &ready, "other", "1.0.0", &listening).await;

    let channel_id = channel_of(admin.ok(plugin_channel("tickets", "desk")).await).id;
    wait_for_log(&mut admin, "tickets", &format!("created {channel_id} desk")).await;
    admin
        .ok(Kind::DeleteChannel(proto::DeleteChannel { channel_id }))
        .await;
    wait_for_log(&mut admin, "tickets", &format!("deleted {channel_id}")).await;

    tokio::time::sleep(Duration::from_millis(200)).await;
    let other = log_lines(&mut admin, "other").await;
    assert!(
        !other
            .iter()
            .any(|line| line.contains(&channel_id.to_string())),
        "{other:#?}"
    );
}

#[tokio::test]
async fn a_bad_key_icon_or_name_raises() {
    // Fails if a malformed registration is taken, raises another text, or
    // works without create_channel_types.
    let server = TestServer::start().await;
    let (mut admin, ready) = owner(&server).await;
    let source = r#"
        local function try(label, key, spec)
            local ok, err = pcall(ui.register_channel_type, key, spec)
            log.info(label .. ": " .. (ok and "ok" or err))
        end
        try("key", "Bad Key", { name = "Tickets" })
        try("long key", string.rep("k", 41), { name = "Tickets" })
        try("number key", 5, { name = "Tickets" })
        try("spec", "desk", "Tickets")
        try("no name", "desk", {})
        try("blank name", "desk", { name = "  " })
        try("long name", "desk", { name = string.rep("n", 61) })
        try("control name", "desk", { name = "a\nb" })
        try("long description", "desk", { name = "T", description = string.rep("d", 201) })
        try("svg icon", "desk", { name = "T", icon = "assets/desk.svg" })
        try("outside icon", "desk", { name = "T", icon = "client/desk.png" })
        try("missing icon", "desk", { name = "T", icon = "assets/gone.png" })
        try("good", "desk", { name = "T", icon = "assets/desk.png" })
        log.info("done")
    "#;
    plugin_at(&server, &mut admin, &ready, "tickets", "1.0.0", source).await;
    let zip = package(&[
        ("manifest.lua", &manifest("bare", &[])),
        (
            "server/main.luau",
            r#"local ok, err = pcall(ui.register_channel_type, "desk", { name = "T" })
            log.info("denied: " .. err)"#,
        ),
    ]);
    install(&server, &mut admin, &media_token(&ready), zip, &[]).await;
    wait_for_log(&mut admin, "tickets", "done").await;

    let lines = log_lines(&mut admin, "tickets").await;
    for expected in [
        "key: INVALID_ARGUMENT: key must be 1 to 40 of a-z, 0-9 and _",
        "long key: INVALID_ARGUMENT: key must be 1 to 40 of a-z, 0-9 and _",
        "number key: INVALID_ARGUMENT: key must be a string",
        "spec: INVALID_ARGUMENT: a channel type is a table such as { name = \"Tickets\" }",
        "no name: INVALID_ARGUMENT: name must be a string",
        "blank name: INVALID_ARGUMENT: name must be 1 to 60 characters, without control characters",
        "long name: INVALID_ARGUMENT: name must be 1 to 60 characters, without control characters",
        "control name: INVALID_ARGUMENT: name must be 1 to 60 characters, without control characters",
        "long description: INVALID_ARGUMENT: description must be at most 200 characters, without control characters",
        "svg icon: INVALID_ARGUMENT: icon must be a .png, .jpg, .jpeg or .webp file under assets/",
        "outside icon: INVALID_ARGUMENT: icon must be a .png, .jpg, .jpeg or .webp file under assets/",
        "missing icon: INVALID_ARGUMENT: icon assets/gone.png isn't in the plugin's package",
        "good: ok",
        "registered channel type desk",
    ] {
        assert!(
            lines.iter().any(|line| line == expected),
            "{expected:?} in {lines:#?}"
        );
    }
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("registered channel type"))
            .count(),
        1,
        "only the good one registered"
    );
    wait_for_log(
        &mut admin,
        "bare",
        "denied: PERMISSION_DENIED: the plugin doesn't have create_channel_types",
    )
    .await;
}

#[tokio::test]
async fn the_21st_type_raises() {
    // Fails if a plugin can register more than 20 types, or replacing one
    // of its 20 counts as a 21st.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;
    let source = r#"
        for i = 1, 20 do
            ui.register_channel_type("type_" .. i, { name = "Type " .. i })
        end
        local ok, err = pcall(ui.register_channel_type, "type_21", { name = "One more" })
        log.info("21st: " .. err)
        ui.register_channel_type("type_1", { name = "Renamed" })
        log.info("replaced")
    "#;
    plugin_at(&server, &mut admin, &ready, "tickets", "1.0.0", source).await;
    wait_for_log(&mut admin, "tickets", "replaced").await;

    wait_for_log(
        &mut admin,
        "tickets",
        "21st: PLUGIN_LIMIT: at most 20 channel types",
    )
    .await;
    let (_later, new_ready) = session(&server, 1, None, &[FEATURE]).await;
    let types = new_ready.plugin_channel_types;
    assert_eq!(types.len(), 20);
    assert!(
        types
            .iter()
            .any(|t| t.type_key == "type_1" && t.name == "Renamed")
    );
    assert!(!types.iter().any(|t| t.type_key == "type_21"));
}

#[tokio::test]
async fn a_plugin_creates_a_channel_of_its_own_type() {
    // Fails if is_registered_by misses starting types or channels.create fails for kind = "plugin".
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;

    let source = r#"
        ui.register_channel_type("desk", { name = "Help Desk" })
        local ch = channels.create({ kind = "plugin", type_key = "desk", name = "support-desk" })
        log.info("created channel " .. ch.name .. " id " .. ch.id .. " kind " .. ch.kind)
    "#;
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with(
                "own-creator",
                "1.0.0",
                &["create_channel_types", "manage_channels"],
                "",
            ),
        ),
        ("server/main.luau", source),
        ("assets/desk.png", "not really a png"),
    ]);
    install(
        &server,
        &mut admin,
        &media_token(&ready),
        zip,
        &["create_channel_types", "manage_channels"],
    )
    .await;

    wait_for_log(&mut admin, "own-creator", "created channel support-desk").await;

    // Verify channel is in the guild and available
    let (_later, new_ready) = session(&server, 1, None, &[FEATURE]).await;
    let ch = new_ready
        .channels
        .iter()
        .find(|c| c.name == "support-desk")
        .expect("channel should exist");
    assert_eq!(ch.kind, proto::ChannelKind::Plugin as i32);
    assert_eq!(ch.plugin_id.as_deref(), Some("own-creator"));
    assert_eq!(ch.type_key.as_deref(), Some("desk"));
    assert_eq!(ch.plugin_state, State::Available as i32);
}

#[tokio::test]
async fn another_plugins_type_is_refused() {
    // Fails if another plugin's type or unregistered type does not raise INVALID_ARGUMENT.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, ready) = session(&server, 1, None, &[FEATURE]).await;

    // Plugin 1 registers "desk"
    tickets(&server, &mut admin, &ready, &["desk"]).await;

    // Plugin 2 tries to create "desk" (registered by plugin 1) or an unknown type
    let source = r#"
        local ok, err = pcall(function()
            channels.create({ kind = "plugin", type_key = "desk", name = "rogue-desk" })
        end)
        log.info("other plugin type: " .. tostring(err))

        local ok2, err2 = pcall(function()
            channels.create({ kind = "plugin", type_key = "unregistered_type", name = "rogue-desk-2" })
        end)
        log.info("unknown type: " .. tostring(err2))
    "#;
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with("other-plugin", "1.0.0", &["manage_channels"], ""),
        ),
        ("server/main.luau", source),
    ]);
    install(
        &server,
        &mut admin,
        &media_token(&ready),
        zip,
        &["manage_channels"],
    )
    .await;

    wait_for_log(
        &mut admin,
        "other-plugin",
        "other plugin type: INVALID_ARGUMENT: that channel type isn't registered by this plugin",
    )
    .await;
    wait_for_log(
        &mut admin,
        "other-plugin",
        "unknown type: INVALID_ARGUMENT: that channel type isn't registered by this plugin",
    )
    .await;
}
