// SPDX-License-Identifier: MPL-2.0

//! Plugin voice API: voice, soundboard, music (plan Task 16, design 2026-11-07 §10.3, §10.9).

use opencord_proto::v1 as proto;
use prost::Message as _;

use crate::common::fake_lavalink::FakeLavalink;
use crate::common::music::{PASSWORD, add_node};
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

fn sound_file(seconds: f64, claimed: f64) -> Vec<u8> {
    let samples = (seconds * 48_000.0) as u64;
    let packets: Vec<Vec<u8>> = (0..samples.div_ceil(960))
        .map(|n| vec![31 << 3, n as u8])
        .collect();
    opencord_common::sound::ogg::write(1, 0, &packets, (claimed * 48_000.0) as u64)
}

async fn upload_sound(server: &TestServer, token: &str, name: &str) -> proto::SoundboardSound {
    let body = sound_file(1.0, 1.0);
    let authorization = format!("Bearer {token}");
    let path = format!("/media/sounds?name={name}&emoji=%F0%9F%94%94&volume=80");
    let (status, bytes) = server
        .send_http("POST", &path, &[("Authorization", &authorization)], &body)
        .await;
    assert_eq!(status, 201);
    let resp = proto::Response::decode(bytes.as_slice()).unwrap();
    match resp.result.unwrap() {
        proto::response::Result::SoundboardSound(sound) => sound,
        other => panic!("expected SoundboardSound, got {other:?}"),
    }
}

#[tokio::test]
async fn each_voice_call_needs_its_permission() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "voice-unauthorized",
        &[],
        r#"
        local function test(name, fn)
            local ok, err = pcall(fn)
            log.info(name .. ": " .. tostring(ok) .. " " .. tostring(err))
        end

        test("voice.states", function() voice.states() end)
        test("voice.move", function() voice.move("1", "2") end)
        test("voice.disconnect", function() voice.disconnect("1") end)
        test("voice.mute", function() voice.mute("1", true) end)
        test("voice.deafen", function() voice.deafen("1", true) end)
        test("soundboard.play", function() soundboard.play("1", "1") end)
        test("music.state", function() music.state("1") end)
        test("music.add", function() music.add("1", "song") end)
        test("music.skip", function() music.skip("1") end)
        test("music.pause", function() music.pause("1") end)
        test("music.resume", function() music.resume("1") end)
        log.info("permission checks done")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "voice-unauthorized", "permission checks done").await;

    let logs = owner
        .ok(proto::request::Kind::FetchPluginLog(
            proto::FetchPluginLog {
                plugin_id: "voice-unauthorized".to_owned(),
                limit: 100,
                before: None,
            },
        ))
        .await;

    let lines = match logs {
        proto::response::Result::PluginLog(l) => l.lines,
        _ => panic!("expected log"),
    };

    let log_text = lines
        .into_iter()
        .map(|l| l.text)
        .collect::<Vec<_>>()
        .join("\n");

    let expected = [
        "voice.states: false PERMISSION_DENIED: the plugin doesn't have read_voice",
        "voice.move: false PERMISSION_DENIED: the plugin doesn't have move_members",
        "voice.disconnect: false PERMISSION_DENIED: the plugin doesn't have move_members",
        "voice.mute: false PERMISSION_DENIED: the plugin doesn't have mute_members",
        "voice.deafen: false PERMISSION_DENIED: the plugin doesn't have deafen_members",
        "soundboard.play: false PERMISSION_DENIED: the plugin doesn't have play_soundboard",
        "music.state: false PERMISSION_DENIED: the plugin doesn't have read_music",
        "music.add: false PERMISSION_DENIED: the plugin doesn't have control_music",
        "music.skip: false PERMISSION_DENIED: the plugin doesn't have control_music",
        "music.pause: false PERMISSION_DENIED: the plugin doesn't have control_music",
        "music.resume: false PERMISSION_DENIED: the plugin doesn't have control_music",
    ];

    for exp in expected {
        assert!(
            log_text.contains(exp),
            "expected log to contain '{exp}', but log was:\n{log_text}"
        );
    }
}

#[tokio::test]
async fn a_plugin_plays_a_sound_without_being_in_the_call() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let voice_ch = channel_id(&ready, "General");
    let token = media_token(&ready);
    let sound = upload_sound(&server, &token, "bell").await;

    // Owner connects to voice
    owner
        .ok(proto::request::Kind::UpdateVoiceState(
            proto::UpdateVoiceState {
                channel_id: Some(voice_ch),
                ..Default::default()
            },
        ))
        .await;

    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "sound-player",
        &["play_soundboard"],
        &format!(
            r#"
            local ok, err = pcall(function()
                soundboard.play("{voice_ch}", "{sound_id}")
            end)
            log.info("sound play: " .. tostring(ok) .. " " .. tostring(err))
            "#,
            sound_id = sound.id
        ),
    )
    .await;

    wait_for_log(&mut owner, "sound-player", "sound play: true nil").await;

    // Owner in voice channel should receive VoiceChannelEffect
    let effect = loop {
        match owner.next_event().await {
            proto::event::Kind::VoiceChannelEffect(effect) => break effect,
            _ => continue,
        }
    };
    assert_eq!(effect.channel_id, voice_ch);
    assert_eq!(effect.user_id, p.user_id);
    assert_eq!(effect.name, "bell");
}

#[tokio::test]
async fn the_cooldown_applies_to_plugins() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let voice_ch = channel_id(&ready, "General");
    let token = media_token(&ready);
    let sound = upload_sound(&server, &token, "chime").await;

    // Owner connects to voice
    owner
        .ok(proto::request::Kind::UpdateVoiceState(
            proto::UpdateVoiceState {
                channel_id: Some(voice_ch),
                ..Default::default()
            },
        ))
        .await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "cooldown-tester",
        &["play_soundboard"],
        &format!(
            r#"
            soundboard.play("{voice_ch}", "{sound_id}")
            local ok, err = pcall(function()
                soundboard.play("{voice_ch}", "{sound_id}")
            end)
            log.info("second play: " .. tostring(ok) .. " " .. tostring(err))
            "#,
            sound_id = sound.id
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "cooldown-tester",
        "second play: false SOUND_COOLDOWN",
    )
    .await;
}

#[tokio::test]
async fn members_still_need_to_be_in_the_call() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let voice_ch = channel_id(&ready, "General");
    let token = media_token(&ready);
    let sound = upload_sound(&server, &token, "horn").await;
    let sound_ref = format!("server:{}", hex::encode(&sound.sha256));

    let invite_code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &invite_code).await;

    // Member is NOT in the voice call, tries to play a sound
    let res = member
        .error(proto::request::Kind::PlaySoundboardSound(
            proto::PlaySoundboardSound {
                channel_id: voice_ch,
                sound_ref,
                ..Default::default()
            },
        ))
        .await;

    assert_eq!(res.code, proto::ErrorCode::VoiceNotConnected as i32);
}

#[tokio::test]
async fn a_track_added_by_a_plugin_names_the_plugin_user() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    add_node(&mut owner, "Home", &lavalink).await;
    let voice_ch = channel_id(&ready, "General");

    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "dj-plugin",
        &["control_music", "read_music"],
        &format!(
            r#"
            music.add("{voice_ch}", "lofi hip hop")
            local state = music.state("{voice_ch}")
            log.info("music added, queue size: " .. #state.queue)
            local req_id = (state.now_playing and state.now_playing.requester_id) or (state.queue[1] and state.queue[1].requester_id)
            log.info("requester id: " .. tostring(req_id))
            "#,
        ),
    )
    .await;

    let expected_user_id = p.user_id.to_string();
    wait_for_log(
        &mut owner,
        "dj-plugin",
        &format!("requester id: {expected_user_id}"),
    )
    .await;
}

#[tokio::test]
async fn happy_paths_for_voice_soundboard_music() {
    let lavalink = FakeLavalink::start(PASSWORD).await;
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    add_node(&mut owner, "Home", &lavalink).await;
    let voice1 = channel_id(&ready, "General");

    // Create a second voice channel
    let ch2_res = owner
        .ok(proto::request::Kind::CreateChannel(proto::CreateChannel {
            name: "Lounge".to_owned(),
            kind: proto::ChannelKind::Voice as i32,
            topic: None,
            parent_id: None,
        }))
        .await;
    let voice2 = match ch2_res {
        proto::response::Result::Channel(ch) => ch.id,
        _ => panic!("expected channel"),
    };

    let invite_code = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &invite_code).await;
    let member_id = self_id(&member_ready);

    // Member connects to voice1
    member
        .ok(proto::request::Kind::UpdateVoiceState(
            proto::UpdateVoiceState {
                channel_id: Some(voice1),
                ..Default::default()
            },
        ))
        .await;

    let token = media_token(&ready);
    let sound = upload_sound(&server, &token, "ding").await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "voice-all",
        &[
            "read_voice",
            "move_members",
            "mute_members",
            "deafen_members",
            "play_soundboard",
            "read_music",
            "control_music",
        ],
        &format!(
            r#"
            -- 1. voice.states
            local states = voice.states()
            log.info("states count: " .. #states)
            local states_v1 = voice.states("{voice1}")
            log.info("v1 states count: " .. #states_v1)

            -- 2. voice.mute
            voice.mute("{member_id}", true)
            log.info("muted member")

            -- 3. voice.deafen
            voice.deafen("{member_id}", true)
            log.info("deafened member")

            -- 4. voice.move
            voice.move("{member_id}", "{voice2}")
            log.info("moved member")

            -- 5. soundboard.play
            soundboard.play("{voice2}", "{sound_id}")
            log.info("played sound")

            -- 6. music.add
            music.add("{voice2}", "test track")
            log.info("added music")

            -- 7. music.state
            local mstate = music.state("{voice2}")
            log.info("music state version: " .. tostring(mstate.version ~= nil))

            -- 8. music.pause & music.resume
            music.pause("{voice2}")
            log.info("paused music")
            music.resume("{voice2}")
            log.info("resumed music")

            -- 9. music.skip
            music.skip("{voice2}")
            log.info("skipped music")

            -- 10. voice.disconnect
            voice.disconnect("{member_id}")
            log.info("disconnected member")
            "#,
            sound_id = sound.id
        ),
    )
    .await;

    wait_for_log(&mut owner, "voice-all", "disconnected member").await;
}
