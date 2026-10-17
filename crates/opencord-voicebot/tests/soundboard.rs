//! Phase 2 V8 acceptance: a soundboard sound played in a voice channel is
//! ready to play at every listener within 150 ms of the others (plan §16,
//! V8). Each listener takes it through the same path the app does: the
//! server's VoiceChannelEffect, then the sound from the cache its
//! background sync filled, decoded for the mixer.

use std::time::{Duration, Instant};

use opencord_core::api::types::CoreEventPayload;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use opencord_voicebot::Voicebot;

const LISTENERS: usize = 3;
/// Plan §16, V8: all listeners hear a sound within this of each other.
const SPREAD: Duration = Duration::from_millis(150);

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
        config.music.audio_udp_port = 0;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }
}

/// A 16-bit mono WAV of a 440 Hz tone.
fn tone_wav(dir: &std::path::Path, seconds: f32) -> String {
    let rate = 44_100u32;
    let mut data = Vec::new();
    for n in 0..(rate as f32 * seconds) as u32 {
        let t = n as f32 / rate as f32;
        let value = (0.3 * (std::f32::consts::TAU * 440.0 * t).sin() * 32_767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    for field in [16u32, 1 | (1 << 16), rate, rate * 2, 2 | (16 << 16)] {
        file.extend_from_slice(&field.to_le_bytes());
    }
    file.extend_from_slice(b"data");
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());
    file.extend_from_slice(&data);
    let path = dir.join("tone.wav");
    std::fs::write(&path, file).unwrap();
    path.to_string_lossy().into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn every_listener_has_a_played_sound_ready_within_150_ms_of_the_others() {
    let server = TestServer::start().await;
    let owner = Voicebot::connect(
        &server.address(),
        server.handle.claim_token.clone(),
        "Owner",
    )
    .await
    .unwrap();
    let invite = owner
        .client
        .create_invite(&owner.server_key, None, None, None)
        .await
        .unwrap();
    let mut listeners = Vec::new();
    for n in 0..LISTENERS {
        let bot = Voicebot::connect(&invite.link, None, &format!("Listener {n}"))
            .await
            .unwrap();
        listeners.push(bot);
    }
    let files = tempfile::tempdir().unwrap();
    let sound = owner
        .upload_sound(&tone_wav(files.path(), 2.0), "Tone")
        .await
        .unwrap();
    let sound_ref = format!("server:{}", sound.sha256);
    let general = owner.voice_channel("General").unwrap();
    let _owner_voice = owner.join(general).await.unwrap();
    let mut voices = Vec::new();
    for bot in &mut listeners {
        bot.wait_for(|payload| match payload {
            CoreEventPayload::SoundboardSoundCreate(created) => Some(created.id),
            _ => None,
        })
        .await
        .unwrap();
        voices.push(bot.join(general).await.unwrap());
    }
    // The background sync fetches new sounds before anyone plays them.
    tokio::time::sleep(Duration::from_secs(1)).await;

    let waiting: Vec<_> = listeners
        .into_iter()
        .map(|mut bot| {
            tokio::spawn(async move {
                let effect = bot
                    .wait_for(|payload| match payload {
                        CoreEventPayload::VoiceChannelEffect(effect) => Some(effect.clone()),
                        _ => None,
                    })
                    .await
                    .unwrap();
                let (samples, volume) = bot
                    .client
                    .soundboard_samples(&bot.server_key, &effect.sound_ref)
                    .await
                    .unwrap();
                (Instant::now(), effect, samples.len(), volume)
            })
        })
        .collect();
    let played = Instant::now();
    owner.play_sound(general, &sound_ref).await.unwrap();
    let mut ready = Vec::new();
    for task in waiting {
        let (at, effect, samples, volume) = task.await.unwrap();
        assert_eq!(effect.sound_ref, sound_ref);
        assert_eq!(effect.user_id, owner.user_id());
        assert_eq!(effect.duration_ms, 2_000);
        assert_eq!((samples, volume), (96_000, 100), "two seconds at 48 kHz");
        ready.push(at - played);
    }

    let first = ready.iter().min().unwrap();
    let last = ready.iter().max().unwrap();
    println!("ready after {ready:?}; spread {:?}", *last - *first);
    assert!(*last - *first <= SPREAD, "{ready:?}");
}
