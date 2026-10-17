//! Chat commands, Phase 3 L6 (plan §17): parsed and checked on the server,
//! the same permissions as the music panel, public results stored as
//! messages by who ran them, "only you" replies only to the sending
//! session, the Commands page, and the autocomplete rate limit.

mod common;

use common::music::*;
use common::*;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Response;

fn run(channel_id: i64, name: &str, options: &[(&str, &str)]) -> Kind {
    Kind::ExecuteCommand(proto::ExecuteCommand {
        channel_id,
        name: name.to_owned(),
        options: options
            .iter()
            .map(|(name, value)| proto::CommandOptionValue {
                name: (*name).to_owned(),
                value: (*value).to_owned(),
            })
            .collect(),
        nonce: "n1".to_owned(),
    })
}

fn result(response: Response) -> proto::CommandResult {
    match response {
        Response::CommandResult(result) => result,
        other => panic!("expected a command result, got {other:?}"),
    }
}

fn commands(response: Response) -> proto::Commands {
    match response {
        Response::Commands(commands) => commands,
        other => panic!("expected the commands, got {other:?}"),
    }
}

/// The server's text channel.
fn general_text(ready: &proto::Ready) -> i64 {
    channel_id(ready, "general")
}

async fn message_with_command(client: &mut TestClient) -> proto::Message {
    client
        .wait_for(|event| match event {
            Event::MessageCreate(create) => create
                .message
                .clone()
                .filter(|message| message.interaction.is_some()),
            _ => None,
        })
        .await
}

#[tokio::test]
async fn play_adds_music_and_posts_a_result_by_who_ran_it() {
    let Room {
        server,
        mut owner,
        mut member,
        voice,
        lavalink,
        ..
    } = room().await;
    let page = owner
        .ok(Kind::FetchCommandSettings(proto::FetchCommandSettings {}))
        .await;
    assert!(
        commands(page)
            .commands
            .iter()
            .any(|spec| spec.name == "play")
    );
    let text = {
        let (fresh, ready) = owner_again(&server).await;
        let id = general_text(&ready);
        let names: Vec<String> = ready
            .commands
            .unwrap()
            .commands
            .into_iter()
            .map(|spec| spec.name)
            .collect();
        assert!(names.contains(&"play".to_owned()) && names.contains(&"nowplaying".to_owned()));
        assert!(!names.contains(&"volume".to_owned()), "the app's own");
        fresh.close().await;
        id
    };

    let done = result(owner.ok(run(text, "play", &[("query", "song")])).await);
    assert!(done.message_id.is_some() && !done.ephemeral);
    fake_plays(&lavalink, voice, "song 1").await;
    let message = message_with_command(&mut member).await;
    assert_eq!(message.interaction.as_ref().unwrap().command, "play");
    assert_eq!(
        message.author_id,
        message.interaction.as_ref().unwrap().user_id
    );
    assert_eq!(
        message.content,
        "Added *song 1* — Fake Artist to the queue in General"
    );

    // Stored like other messages.
    let history = match owner
        .ok(Kind::FetchMessages(proto::FetchMessages {
            channel_id: text,
            before: None,
            limit: 10,
        }))
        .await
    {
        Response::Messages(list) => list.messages,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        history[0].interaction.as_ref().map(|i| i.command.as_str()),
        Some("play")
    );

    let paused = result(owner.ok(run(text, "pause", &[])).await);
    assert!(!paused.ephemeral);
    let session = session(owner.ok(snapshot_request(voice)).await);
    assert_eq!(session.state, proto::MusicPlayState::Paused as i32);
}

/// The owner signed in again on a second session, for its Ready.
async fn owner_again(server: &TestServer) -> (TestClient, proto::Ready) {
    let mut client = TestClient::connect(server, key(1)).await;
    let ready = client.identify("Owner", None, None).await.unwrap();
    (client, ready)
}

#[tokio::test]
async fn only_you_replies_go_to_the_sending_session_alone() {
    let Room {
        server,
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    let (mut other_device, ready) = owner_again(&server).await;
    let text = general_text(&ready);
    owner.ok(run(text, "play", &[("query", "song")])).await;
    fake_plays(&lavalink, voice, "song 1").await;

    let done = result(owner.ok(run(text, "np", &[])).await);
    assert!(done.ephemeral && done.message_id.is_none());
    let reply = owner
        .wait_for(|event| match event {
            Event::EphemeralMessage(message) => Some(message.clone()),
            _ => None,
        })
        .await;
    assert_eq!(reply.nonce, "n1");
    assert_eq!(
        reply.content,
        "Now playing in General: *song 1* — Fake Artist"
    );
    let queue = result(owner.ok(run(text, "queue", &[])).await);
    assert!(queue.ephemeral);
    // The other device of the same user hears nothing of either.
    let seen = other_device
        .collect_for(std::time::Duration::from_millis(500))
        .await;
    assert!(
        !seen
            .iter()
            .any(|event| matches!(event, Event::EphemeralMessage(_))),
        "{seen:?}"
    );
}

#[tokio::test]
async fn commands_need_the_same_permissions_as_the_panel() {
    let Room {
        server,
        mut owner,
        mut member,
        member_id,
        voice,
        everyone,
        lavalink: _lavalink,
        ..
    } = room().await;
    let (_, owner_ready) = owner_again(&server).await;
    let text = general_text(&owner_ready);

    // Not in voice: told to join, only them.
    let done = result(member.ok(run(text, "play", &[("query", "song")])).await);
    assert!(done.ephemeral);
    let reply = member
        .wait_for(|event| match event {
            Event::EphemeralMessage(message) => Some(message.content.clone()),
            _ => None,
        })
        .await;
    assert_eq!(reply, "Join a voice channel to use music commands");

    member.ok(join_voice(voice)).await;
    // Stop needs Manage music; another channel too.
    let error = member.error(run(text, "stop", &[])).await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);
    let error = member
        .error(run(text, "pause", &[("channel", &voice.to_string())]))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);

    // Without Add music, no /play either.
    owner
        .ok(Kind::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some(
                (opencord_common::permissions::Permissions::DEFAULT_EVERYONE
                    - opencord_common::permissions::Permissions::MUSIC_ADD)
                    .bits(),
            ),
            ..Default::default()
        }))
        .await;
    let error = member.error(run(text, "play", &[("query", "song")])).await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);
    let _ = member_id;

    // Unknown commands, options and values are refused by the server.
    for (name, options, expected) in [
        ("dance", vec![], proto::ErrorCode::CommandUnknown),
        (
            "volume",
            vec![("percent", "50")],
            proto::ErrorCode::CommandUnknown,
        ),
        ("seek", vec![], proto::ErrorCode::CommandInvalidOptions),
        (
            "repeat",
            vec![("mode", "always")],
            proto::ErrorCode::CommandInvalidOptions,
        ),
        (
            "remove",
            vec![("position", "x")],
            proto::ErrorCode::CommandInvalidOptions,
        ),
    ] {
        let error = owner.error(run(text, name, &options)).await;
        assert_eq!(code(&error), expected, "/{name}");
    }
}

#[tokio::test]
async fn the_commands_page_turns_commands_off_and_keeps_them_to_channels() {
    let Room {
        server,
        mut owner,
        mut member,
        voice,
        lavalink: _lavalink,
        ..
    } = room().await;
    let (_, ready) = owner_again(&server).await;
    let text = general_text(&ready);
    let page = commands(
        owner
            .ok(Kind::UpdateCommandSettings(proto::UpdateCommandSettings {
                name: "clear".to_owned(),
                enabled: Some(false),
                channel_ids: None,
                results_public: None,
            }))
            .await,
    );
    let clear = page
        .commands
        .iter()
        .find(|spec| spec.name == "clear")
        .unwrap();
    assert!(!clear.enabled);
    let update = member
        .wait_for(|event| match event {
            Event::CommandsUpdate(update) => update.commands.clone(),
            _ => None,
        })
        .await;
    assert!(!update.commands.iter().any(|spec| spec.name == "clear"));
    let error = owner.error(run(text, "clear", &[])).await;
    assert_eq!(code(&error), proto::ErrorCode::CommandDisabled);

    // Only in the voice channel's chat.
    owner
        .ok(Kind::UpdateCommandSettings(proto::UpdateCommandSettings {
            name: "shuffle".to_owned(),
            enabled: None,
            channel_ids: Some(proto::CommandChannels { ids: vec![voice] }),
            results_public: None,
        }))
        .await;
    let error = owner.error(run(text, "shuffle", &[])).await;
    assert_eq!(code(&error), proto::ErrorCode::CommandNotAllowedHere);

    // Results only to the user: nothing stored.
    owner
        .ok(Kind::UpdateCommandSettings(proto::UpdateCommandSettings {
            name: String::new(),
            enabled: None,
            channel_ids: None,
            results_public: Some(false),
        }))
        .await;
    let done = result(owner.ok(run(text, "repeat", &[("mode", "queue")])).await);
    assert!(done.ephemeral && done.message_id.is_none());

    // Members can't change the page.
    let error = member
        .error(Kind::FetchCommandSettings(proto::FetchCommandSettings {}))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);

    // Kept over a restart.
    let server = server.restart(|_| {}).await;
    let (owner, ready) = owner_again(&server).await;
    let names: Vec<String> = ready
        .commands
        .unwrap()
        .commands
        .into_iter()
        .map(|spec| spec.name)
        .collect();
    assert!(!names.contains(&"clear".to_owned()));
    owner.close().await;
}

#[tokio::test]
async fn play_suggestions_are_results_that_play_exactly_that_track() {
    let Room {
        server,
        mut owner,
        voice,
        lavalink,
        ..
    } = room().await;
    let (_, ready) = owner_again(&server).await;
    let text = general_text(&ready);
    let suggest = |value: &str| {
        Kind::AutocompleteCommand(proto::AutocompleteCommand {
            channel_id: text,
            name: "play".to_owned(),
            option: "query".to_owned(),
            value: value.to_owned(),
        })
    };
    let choices = match owner.ok(suggest("tune")).await {
        Response::CommandChoices(found) => found.choices,
        other => panic!("{other:?}"),
    };
    assert_eq!(choices.len(), 3);
    assert_eq!(choices[1].name, "tune 2 — Fake Artist · 3:00");
    owner
        .ok(run(text, "play", &[("query", &choices[1].value)]))
        .await;
    fake_plays(&lavalink, voice, "tune 2").await;

    // 5 a second.
    let mut limited = false;
    for _ in 0..8 {
        if let Response::Error(error) = owner.request(suggest("x")).await
            && code(&error) == proto::ErrorCode::RateLimited
        {
            limited = true;
        }
    }
    assert!(limited);
}

#[tokio::test]
async fn skip_command_acts_as_vote_skip_when_missing_control_and_vote_skip_enabled() {
    let Room {
        mut owner,
        mut member,
        everyone,
        voice,
        lavalink,
        server,
        ..
    } = room().await;
    let (_, owner_ready) = owner_again(&server).await;
    let text = general_text(&owner_ready);

    member.ok(join_voice(voice)).await;

    // Remove MUSIC_CONTROL from everyone.
    owner
        .ok(Kind::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some(
                (opencord_common::permissions::Permissions::DEFAULT_EVERYONE
                    - opencord_common::permissions::Permissions::MUSIC_CONTROL)
                    .bits(),
            ),
            ..Default::default()
        }))
        .await;

    owner.ok(run(text, "play", &[("query", "song 1")])).await;
    fake_plays(&lavalink, voice, "song 1 1").await;
    owner.ok(run(text, "play", &[("query", "song 2")])).await;

    // 1. Vote skip is off -> member running /skip gets error.
    let error = member.error(run(text, "skip", &[])).await;
    assert_eq!(code(&error), proto::ErrorCode::InvalidArgument);

    // 2. Enable vote skip.
    owner
        .ok(Kind::UpdateMusicSettings(proto::UpdateMusicSettings {
            vote_skip: Some(true),
            vote_skip_percent: Some(50),
            ..Default::default()
        }))
        .await;

    // 3. Member runs /skip -> votes to skip!
    let done = result(member.ok(run(text, "skip", &[])).await);
    assert!(done.message_id.is_some() && !done.ephemeral);

    // Track advances!
    fake_plays(&lavalink, voice, "song 2 1").await;
    let now = until(&mut owner, voice, "advanced to song 2", |s| playing(s, "song 2 1")).await;
    assert_eq!(title(now.now_playing.as_ref()), "song 2 1");
}
