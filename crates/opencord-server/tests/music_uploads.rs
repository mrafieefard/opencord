//! Uploaded songs (design 2026-10-24): kept once per server by their hash,
//! owned by whoever uploaded or dropped them, found in search, removed by
//! owners and administrators, and played by the server itself with no
//! Lavalink node.

mod common;

use common::music::*;
use common::*;
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Response;
use sha2::{Digest, Sha256};

/// A 16-bit PCM WAV file of a tone at `hz`.
fn wav(seconds: f32, hz: f32) -> Vec<u8> {
    let rate = 48_000u32;
    let frames = (rate as f32 * seconds) as usize;
    let mut data = Vec::new();
    for n in 0..frames {
        let t = n as f32 / rate as f32;
        let value = (0.4 * (std::f32::consts::TAU * hz * t).sin() * 32_767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&rate.to_le_bytes());
    file.extend_from_slice(&(rate * 2).to_le_bytes());
    file.extend_from_slice(&2u16.to_le_bytes());
    file.extend_from_slice(&16u16.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());
    file.extend_from_slice(&data);
    file
}

struct Studio {
    server: TestServer,
    owner: TestClient,
    owner_token: String,
    member: TestClient,
    member_token: String,
    voice: i64,
    everyone: i64,
    /// The text channel, for commands.
    text: i64,
}

/// A server with no Lavalink node; the owner and a member in its voice
/// channel.
async fn studio() -> Studio {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let voice = channel_id(&ready, "General");
    owner.ok(join_voice(voice)).await;
    let code = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &code).await;
    member.ok(join_voice(voice)).await;
    Studio {
        owner_token: media_token(&ready),
        member_token: media_token(&member_ready),
        everyone: ready.server.as_ref().unwrap().everyone_role_id,
        text: channel_id(&ready, "general"),
        server,
        owner,
        member,
        voice,
    }
}

/// Uploads `bytes` as a song; the error when it failed.
async fn upload(
    server: &TestServer,
    client: &mut TestClient,
    token: &str,
    voice: i64,
    filename: &str,
    bytes: &[u8],
) -> Result<(), proto::Error> {
    let upload_id = match client
        .ok(Kind::CreateUpload(proto::CreateUpload {
            channel_id: voice,
            filename: filename.to_owned(),
            size_bytes: bytes.len() as u64,
            kind: proto::AttachmentKind::Music.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await
    {
        Response::Upload(upload) => upload.upload_id,
        other => panic!("expected an upload, got {other:?}"),
    };
    let auth = format!("Bearer {token}");
    let path = format!("/media/uploads/{upload_id}");
    let (status, _) = server
        .send_http(
            "PUT",
            &path,
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            bytes,
        )
        .await;
    assert_eq!(status, 200);
    let (status, _) = server
        .send_http(
            "POST",
            &format!("{path}/finish"),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 202);
    client
        .wait_for(|event| match event {
            proto::event::Kind::UploadReady(ready)
                if ready.attachment.as_ref().map(|a| a.id) == Some(upload_id) =>
            {
                Some(Ok(()))
            }
            proto::event::Kind::UploadFailed(failed) if failed.upload_id == upload_id => {
                Some(Err(failed.error.clone().unwrap_or_default()))
            }
            _ => None,
        })
        .await
}

/// What the server answers for a dropped file.
async fn find(
    client: &mut TestClient,
    voice: i64,
    bytes: &[u8],
) -> Option<proto::MusicSearchResult> {
    match client
        .ok(Kind::MusicFindLocal(proto::MusicFindLocal {
            channel_id: voice,
            sha256: Sha256::digest(bytes).to_vec(),
            size_bytes: bytes.len() as u64,
        }))
        .await
    {
        Response::MusicLocalMatch(found) => found.result,
        other => panic!("expected a match, got {other:?}"),
    }
}

async fn search_results(
    client: &mut TestClient,
    voice: i64,
    query: &str,
) -> Vec<proto::MusicSearchResult> {
    match client
        .ok(Kind::MusicSearch(proto::MusicSearch {
            channel_id: voice,
            query: query.to_owned(),
            source: String::new(),
        }))
        .await
    {
        Response::MusicSearchResults(results) => results.results,
        other => panic!("expected results, got {other:?}"),
    }
}

async fn every_song(client: &mut TestClient) -> Vec<proto::LocalSong> {
    match client
        .ok(Kind::FetchLocalSongs(proto::FetchLocalSongs::default()))
        .await
    {
        Response::LocalSongs(songs) => songs.songs,
        other => panic!("expected songs, got {other:?}"),
    }
}

#[tokio::test]
async fn a_song_plays_on_a_server_without_lavalink_and_the_queue_moves_on() {
    let Studio {
        server,
        mut owner,
        owner_token,
        voice,
        ..
    } = studio().await;
    let song = wav(2.0, 440.0);
    upload(&server, &mut owner, &owner_token, voice, "tone.wav", &song)
        .await
        .unwrap();

    let found = find(&mut owner, voice, &song)
        .await
        .expect("the song is kept");
    let track = found.track.clone().unwrap();
    assert_eq!(track.title, "tone");
    assert_eq!(track.source_display_name, "Local");
    assert_eq!(track.duration_ms, 2_000);
    assert!(track.local_song_id > 0);
    assert!(track.removable);

    owner.ok(enqueue(voice, vec![found.result_id])).await;
    until(&mut owner, voice, "playing the song", |session| {
        playing(session, "tone") && !session.waiting_for_node
    })
    .await;
    let done = until(&mut owner, voice, "done with the song", |session| {
        session.now_playing.is_none() && session.history.len() == 1
    })
    .await;
    assert_eq!(title(done.history.first()), "tone");
}

#[tokio::test]
async fn the_same_file_is_kept_once_and_whoever_drops_it_owns_it_too() {
    let Studio {
        server,
        mut owner,
        owner_token,
        mut member,
        member_token,
        voice,
        ..
    } = studio().await;
    let song = wav(1.5, 330.0);
    assert!(find(&mut member, voice, &song).await.is_none());
    upload(&server, &mut owner, &owner_token, voice, "a.wav", &song)
        .await
        .unwrap();

    // Without access to every song, the member finds only their own.
    assert!(search_results(&mut member, voice, "a").await.is_empty());
    let dropped = find(&mut member, voice, &song)
        .await
        .expect("known by its hash");
    assert!(dropped.track.unwrap().removable);
    let theirs = search_results(&mut member, voice, "a").await;
    assert_eq!(theirs.len(), 1);

    // Uploaded again anyway: still one song.
    upload(
        &server,
        &mut member,
        &member_token,
        voice,
        "again.wav",
        &song,
    )
    .await
    .unwrap();
    assert_eq!(every_song(&mut owner).await.len(), 1);
}

#[tokio::test]
async fn access_to_all_songs_shows_everyone_s_but_removes_none_of_them() {
    let Studio {
        server,
        mut owner,
        owner_token,
        mut member,
        voice,
        everyone,
        ..
    } = studio().await;
    let song = wav(1.5, 550.0);
    upload(
        &server,
        &mut owner,
        &owner_token,
        voice,
        "shared.wav",
        &song,
    )
    .await
    .unwrap();
    owner
        .ok(Kind::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some(
                (Permissions::DEFAULT_EVERYONE | Permissions::MUSIC_ALL_LOCAL_SONGS).bits(),
            ),
            ..Default::default()
        }))
        .await;

    let results = search_results(&mut member, voice, "shared").await;
    assert_eq!(results.len(), 1);
    let track = results[0].track.clone().unwrap();
    assert!(!track.removable);
    let error = member
        .error(Kind::RemoveLocalSong(proto::RemoveLocalSong {
            song_id: track.local_song_id,
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);
    let error = member
        .error(Kind::FetchLocalSongs(proto::FetchLocalSongs::default()))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::Forbidden);
    assert_eq!(every_song(&mut owner).await.len(), 1);
}

#[tokio::test]
async fn a_song_goes_when_its_last_owner_or_an_administrator_removes_it() {
    let Studio {
        server,
        mut owner,
        mut member,
        member_token,
        voice,
        ..
    } = studio().await;
    let first = wav(1.5, 200.0);
    upload(
        &server,
        &mut member,
        &member_token,
        voice,
        "first.wav",
        &first,
    )
    .await
    .unwrap();
    let first_id = find(&mut owner, voice, &first)
        .await
        .unwrap()
        .track
        .unwrap()
        .local_song_id;

    // Both own it: the member's removal leaves it for the owner.
    member
        .ok(Kind::RemoveLocalSong(proto::RemoveLocalSong {
            song_id: first_id,
        }))
        .await;
    assert_eq!(every_song(&mut owner).await.len(), 1);
    assert!(search_results(&mut member, voice, "first").await.is_empty());
    owner
        .ok(Kind::RemoveLocalSong(proto::RemoveLocalSong {
            song_id: first_id,
        }))
        .await;
    assert!(every_song(&mut owner).await.is_empty());

    // An administrator removes a member's song for everyone.
    let second = wav(1.5, 250.0);
    upload(
        &server,
        &mut member,
        &member_token,
        voice,
        "second.wav",
        &second,
    )
    .await
    .unwrap();
    let second_id = every_song(&mut owner).await[0].id;
    owner
        .ok(Kind::RemoveLocalSong(proto::RemoveLocalSong {
            song_id: second_id,
        }))
        .await;
    assert!(every_song(&mut owner).await.is_empty());
    assert!(find(&mut member, voice, &second).await.is_none());
}

#[tokio::test]
async fn what_is_not_a_song_is_refused() {
    let Studio {
        server,
        mut owner,
        owner_token,
        voice,
        ..
    } = studio().await;
    let error = owner
        .error(Kind::CreateUpload(proto::CreateUpload {
            channel_id: voice,
            filename: "notes.txt".into(),
            size_bytes: 10,
            kind: proto::AttachmentKind::Music.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert_eq!(code(&error), proto::ErrorCode::InvalidArgument);

    let error = upload(
        &server,
        &mut owner,
        &owner_token,
        voice,
        "fake.mp3",
        b"this is only text pretending to be a song",
    )
    .await
    .unwrap_err();
    assert_eq!(code(&error), proto::ErrorCode::InvalidArgument);
    assert!(every_song(&mut owner).await.is_empty());
}

#[tokio::test]
async fn a_queued_song_that_was_removed_is_skipped_with_a_notice() {
    let Studio {
        server,
        mut owner,
        owner_token,
        voice,
        ..
    } = studio().await;
    let long = wav(3.0, 100.0);
    let gone = wav(1.5, 150.0);
    upload(&server, &mut owner, &owner_token, voice, "long.wav", &long)
        .await
        .unwrap();
    upload(&server, &mut owner, &owner_token, voice, "gone.wav", &gone)
        .await
        .unwrap();
    let long = find(&mut owner, voice, &long).await.unwrap();
    let gone = find(&mut owner, voice, &gone).await.unwrap();
    let gone_id = gone.track.as_ref().unwrap().local_song_id;
    owner
        .ok(enqueue(voice, vec![long.result_id, gone.result_id]))
        .await;
    until(&mut owner, voice, "playing the first", |session| {
        playing(session, "long")
    })
    .await;
    owner
        .ok(Kind::RemoveLocalSong(proto::RemoveLocalSong {
            song_id: gone_id,
        }))
        .await;
    owner
        .wait_for(|event| match event {
            proto::event::Kind::MusicNotice(notice)
                if notice.text == "This song was removed from the server" =>
            {
                Some(())
            }
            _ => None,
        })
        .await;
}

fn command(channel_id: i64, name: &str, options: &[(&str, &str)]) -> Kind {
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

/// The next message a command posted.
async fn command_message(client: &mut TestClient) -> String {
    client
        .wait_for(|event| match event {
            proto::event::Kind::MessageCreate(create) => create
                .message
                .clone()
                .filter(|message| message.interaction.is_some())
                .map(|message| message.content),
            _ => None,
        })
        .await
}

async fn play_suggestions(client: &mut TestClient, text: i64, value: &str) -> Vec<String> {
    match client
        .ok(Kind::AutocompleteCommand(proto::AutocompleteCommand {
            channel_id: text,
            name: "play".to_owned(),
            option: "query".to_owned(),
            value: value.to_owned(),
        }))
        .await
    {
        Response::CommandChoices(found) => found
            .choices
            .into_iter()
            .map(|choice| choice.name)
            .collect(),
        other => panic!("expected choices, got {other:?}"),
    }
}

#[tokio::test]
async fn play_finds_uploaded_songs_without_lavalink_and_says_they_are_local() {
    let Studio {
        server,
        mut owner,
        owner_token,
        voice,
        text,
        ..
    } = studio().await;
    let song = wav(2.0, 600.0);
    upload(
        &server,
        &mut owner,
        &owner_token,
        voice,
        "Lantern.wav",
        &song,
    )
    .await
    .unwrap();

    let suggestions = play_suggestions(&mut owner, text, "lant").await;
    assert_eq!(suggestions, ["Lantern — Owner · 0:02 · Local"]);

    owner
        .ok(command(text, "play", &[("query", "lantern")]))
        .await;
    assert_eq!(
        command_message(&mut owner).await,
        "Added *Lantern* — Owner (Local) to the queue in General"
    );
    until(&mut owner, voice, "playing the song", |session| {
        playing(session, "Lantern")
    })
    .await;
}

#[tokio::test]
async fn play_takes_an_uploaded_song_titled_as_asked_over_lavalink() {
    let Room {
        server,
        mut owner,
        voice,
        lavalink,
        media_token,
        ..
    } = room().await;
    let (_, ready) = owner_again(&server).await;
    let text = channel_id(&ready, "general");
    let song = wav(2.0, 700.0);
    upload(&server, &mut owner, &media_token, voice, "mine.wav", &song)
        .await
        .unwrap();

    // Lavalink's first, then the uploaded song, marked.
    let suggestions = play_suggestions(&mut owner, text, "mine").await;
    assert_eq!(suggestions.len(), 4);
    assert_eq!(suggestions[0], "mine 1 — Fake Artist · 3:00");
    assert_eq!(suggestions[3], "mine — Owner · 0:02 · Local");

    owner.ok(command(text, "play", &[("query", "Mine")])).await;
    assert_eq!(
        command_message(&mut owner).await,
        "Added *mine* — Owner (Local) to the queue in General"
    );
    // Words that are no uploaded song's title get Lavalink's first.
    owner.ok(command(text, "play", &[("query", "min")])).await;
    fake_plays(&lavalink, voice, "min 1").await;
}
