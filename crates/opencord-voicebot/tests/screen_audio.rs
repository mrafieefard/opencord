//! Phase 2 V7: a screen share's sound reaches those watching it, in
//! stereo, apart from the streamer's voice, and nobody else (plan §9.5,
//! §10.5). A bot talks at 440 Hz and shares a screen whose sound is
//! 1 kHz left and 2.4 kHz right; one bot watches, one does not. It needs
//! a video codec, so it skips where this system has none yet.

use std::time::Duration;

use opencord_common::video::ScreenPreset;
use opencord_core::api::types::{ScreenShareRequest, ScreenShareResolution, StreamSourceKind};
use opencord_media::video::layers::ScreenShape;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use opencord_voicebot::Voicebot;

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.voice.udp_port = 0;
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }
}

/// Heard: above this. Absent: below that.
const HEARD: f32 = -25.0;
const ABSENT: f32 = -55.0;

#[tokio::test(flavor = "multi_thread")]
async fn a_screens_sound_reaches_its_watchers_in_stereo_apart_from_the_voice() {
    if !opencord_media::video::codec::supported() {
        eprintln!("skipped: no video codec on this system yet");
        return;
    }
    let server = TestServer::start().await;
    let streamer = Voicebot::connect(
        &server.address(),
        server.handle.claim_token.clone(),
        "Streamer",
    )
    .await
    .unwrap();
    let invite = streamer
        .client
        .create_invite(&streamer.server_key, None, None)
        .await
        .unwrap();
    let watcher = Voicebot::connect(&invite.link, None, "Watcher")
        .await
        .unwrap();
    let other = Voicebot::connect(&invite.link, None, "Other")
        .await
        .unwrap();
    let general = streamer.voice_channel("General").unwrap();
    let streaming = streamer.join(general).await.unwrap();
    let watching = watcher.join(general).await.unwrap();
    let not_watching = other.join(general).await.unwrap();

    let request = ScreenShareRequest {
        resolution: ScreenShareResolution::P720,
        fps: 30,
        has_audio: true,
    };
    let stream = streamer
        .client
        .create_stream(
            &streamer.server_key,
            general,
            StreamSourceKind::Screen,
            request,
        )
        .await
        .unwrap();
    let shape = ScreenShape {
        max_pixels: ScreenPreset::P720.max_pixels(),
        fps: 30,
    };
    streaming
        .publish_encoded_screen("screen-streamer", shape)
        .await
        .unwrap();
    let length = Duration::from_secs(8);
    streaming.play_tone(440.0, length);
    let _sound = streaming.share_tone(1000.0, 2400.0, length);
    watcher
        .client
        .stream_watch(&stream.stream_key, true)
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(6)).await;

    let id = streamer.user_id();
    let voice = &watching.heard()[&id];
    let sound = &watching.stream_heard()[&id];
    let others_voice = &not_watching.heard()[&id];
    println!(
        "watcher: voice 440 {:.1}, stream left 1000 {:.1} / 2400 {:.1}, right 2400 {:.1} / 1000 {:.1}, {} packets; other: voice 440 {:.1}, stream {:?}",
        voice.tone_dbfs(0, 440.0),
        sound.tone_dbfs(0, 1000.0),
        sound.tone_dbfs(0, 2400.0),
        sound.tone_dbfs(1, 2400.0),
        sound.tone_dbfs(1, 1000.0),
        sound.decoded,
        others_voice.tone_dbfs(0, 440.0),
        not_watching
            .stream_heard()
            .get(&id)
            .map(|heard| heard.packets),
    );
    // The watcher: the voice once, and the stream's sound in stereo.
    assert!(voice.tone_dbfs(0, 440.0) > HEARD);
    assert!(
        voice.tone_dbfs(0, 1000.0) < ABSENT,
        "no stream in the voice"
    );
    assert!(sound.decoded > 150, "{} frames", sound.decoded);
    assert!(sound.tone_dbfs(0, 1000.0) > HEARD && sound.tone_dbfs(0, 2400.0) < ABSENT);
    assert!(sound.tone_dbfs(1, 2400.0) > HEARD && sound.tone_dbfs(1, 1000.0) < ABSENT);
    assert!(sound.tone_dbfs(0, 440.0) < ABSENT, "no voice in the stream");
    // Not watching: the voice, no stream sound.
    assert!(others_voice.tone_dbfs(0, 440.0) > HEARD);
    assert!(!not_watching.stream_heard().contains_key(&id));
}
