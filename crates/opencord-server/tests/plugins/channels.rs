// SPDX-License-Identifier: MPL-2.0

//! Plugin channels: the channel kind, its migration and who sees it (plan
//! Task 7, design 2026-11-07 §14.2).

use std::time::Duration;

use opencord_proto::v1 as proto;
use proto::envelope::Payload;
use proto::event::Kind as Event;
use proto::request::Kind;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{AssertSqlSafe, SqlitePool};

use crate::common::{TestClient, TestServer, invite, key, owner};

const FEATURE: &str = "plugin_channels";

/// A session for `seed`'s key, listing `features`. The key must already be
/// a member, or `invite` lets it join.
async fn session(
    server: &TestServer,
    seed: u8,
    invite: Option<&str>,
    features: &[&str],
) -> (TestClient, proto::Ready) {
    let mut client = TestClient::connect(server, key(seed)).await;
    let mut identify = client.identify_message("Member", invite, None);
    identify.features = features.iter().map(|name| (*name).to_owned()).collect();
    client.send(Payload::Identify(identify)).await;
    match client.recv_payload().await {
        Payload::Ready(ready) => (client, *ready),
        other => panic!("expected Ready, got {other:?}"),
    }
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

// --- Creating one ------------------------------------------------------

#[tokio::test]
async fn a_plugin_channel_needs_its_plugin_and_type() {
    // Fails if CreateChannel stores a plugin channel without its plugin or
    // type (the database's check then fails as an internal error), takes
    // a malformed key, or lets another kind carry them.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, _) = session(&server, 1, None, &[FEATURE]).await;
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
    assert_eq!(
        channel.plugin_state,
        proto::PluginChannelState::PluginStopped as i32
    );
}

#[tokio::test]
async fn a_plugin_channel_takes_no_messages() {
    // Fails if sending, typing, reading history, running a command or
    // joining voice works in a plugin channel, as in a text channel.
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, _) = session(&server, 1, None, &[FEATURE]).await;
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
    assert_eq!(loaded, &created);
    assert_eq!(loaded.kind, proto::ChannelKind::Plugin as i32);
    assert_eq!(loaded.type_key.as_deref(), Some("desk"));
}
