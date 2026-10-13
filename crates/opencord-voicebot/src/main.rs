//! `opencord-voicebot`: joins a voice channel, plays a tone, can send a
//! synthetic camera and watch others', play soundboard sounds, and reports
//! what it hears and sees. For checking that a server's voice and video
//! work end to end.

use std::process::ExitCode;
use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use opencord_core::api::types::{
    CoreEventPayload, ScreenShareRequest, ScreenShareResolution, StreamSourceKind,
};
use opencord_voicebot::{Heard, VoiceSession, Voicebot};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Invite link, or host:port of a server the bot may join.
    #[arg(long)]
    server: String,
    /// Owner claim token, to claim a fresh server.
    #[arg(long)]
    claim: Option<String>,
    #[arg(long, default_value = "Voicebot")]
    name: String,
    /// Voice channel to join.
    #[arg(long, default_value = "General")]
    channel: String,
    /// Tone to play, in Hz; 0 stays silent.
    #[arg(long, default_value_t = 440.0)]
    tone: f32,
    /// Send a camera of real H.264 (a moving test scene in 180p, 360p and
    /// 720p layers), as the app's camera would.
    #[arg(long)]
    camera: bool,
    /// Send the synthetic test pattern instead: H.264-shaped pictures with
    /// checksums, to check that every picture arrives intact.
    #[arg(long, conflicts_with = "camera")]
    test_pattern: bool,
    /// Watch everyone's video in tiles this many pixels tall.
    #[arg(long, value_name = "HEIGHT")]
    watch: Option<u32>,
    /// Share a screen of real H.264 (the moving scene as a 1920×1080
    /// screen) at a quality such as 720p30 or 1080p60, within the server's
    /// maximum.
    #[arg(long, value_name = "QUALITY", value_parser = screen_quality)]
    screen: Option<ScreenShareRequest>,
    /// Watch every screen share in the channel (opt-in, plan §9.5); with
    /// --watch, their video comes too.
    #[arg(long)]
    watch_streams: bool,
    /// With --screen, share a stereo tone as the screen's sound: LEFT,RIGHT
    /// in Hz (plan §10.5).
    #[arg(long, value_name = "LEFT,RIGHT", value_parser = tone_pair, requires = "screen")]
    screen_tone: Option<(f32, f32)>,
    /// Report how loud these frequencies (Hz, comma-separated) were in the
    /// last seconds of each voice and screen share sound heard.
    #[arg(long, value_name = "HZ,HZ", value_delimiter = ',')]
    listen_for: Vec<f32>,
    /// With --watch-streams, stop (and report) once a stream watched ends.
    #[arg(long, requires = "watch_streams")]
    until_stream_ends: bool,
    /// Add an audio file (MP3, WAV, FLAC, OGG, M4A) to the soundboard
    /// before joining; needs Manage soundboard (the owner has it).
    #[arg(long, value_name = "FILE")]
    upload_sound: Option<String>,
    /// The uploaded sound's name.
    #[arg(long, default_value = "Voicebot", requires = "upload_sound")]
    sound_name: String,
    /// Play this soundboard sound once joined: server:<sha256>,
    /// default:<id>, or "uploaded" for the file from --upload-sound.
    #[arg(long, value_name = "REF")]
    sound: Option<String>,
    /// Play --sound again this often, in seconds.
    #[arg(long, value_name = "SECONDS", requires = "sound")]
    sound_every: Option<u64>,
    #[arg(long, default_value_t = 10)]
    seconds: u64,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Args::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("opencord-voicebot: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(args: Args) -> anyhow::Result<()> {
    let mut bot = Voicebot::connect(&args.server, args.claim, &args.name).await?;
    let uploaded = match &args.upload_sound {
        Some(path) => {
            let sound = bot.upload_sound(path, &args.sound_name).await?;
            println!("uploaded {} as server:{}", sound.name, sound.sha256);
            Some(format!("server:{}", sound.sha256))
        }
        None => None,
    };
    let sound = match args.sound.as_deref() {
        Some("uploaded") => Some(uploaded.context("--sound uploaded needs --upload-sound")?),
        other => other.map(str::to_owned),
    };
    let channel = bot.voice_channel(&args.channel)?;
    let session = bot.join(channel).await?;
    println!("joined {} as user {}", args.channel, bot.user_id());
    let duration = Duration::from_secs(args.seconds);
    if args.tone > 0.0 {
        session.play_tone(args.tone, duration);
    }
    // Track ids are unique in a channel.
    let track_id = format!("voicebot-camera-{}", bot.user_id());
    if args.camera {
        publish_camera(&session, &track_id).await?;
    }
    if args.test_pattern {
        session.publish_camera(&track_id).await?;
    }
    if let Some(height) = args.watch {
        session.watch(height, true);
    }
    if let Some(mut request) = args.screen {
        request.has_audio = args.screen_tone.is_some();
        go_live(&bot, &session, channel, request).await?;
        if let Some((left, right)) = args.screen_tone {
            share_tone(&session, left, right, duration);
        }
    }
    let self_id = bot.user_id();
    let mut watched: Vec<String> = Vec::new();
    if args.watch_streams {
        let streams: Vec<String> = bot
            .ready
            .streams
            .iter()
            .filter(|stream| stream.channel_id == channel && stream.user_id != self_id)
            .map(|stream| stream.stream_key.clone())
            .collect();
        for key in streams {
            watch_stream(&bot, &key).await;
            watched.push(key);
        }
    }
    let started = tokio::time::Instant::now();
    let deadline = started + duration;
    // Without --sound-every, the sound plays once.
    let mut next_sound = sound.as_ref().map(|_| started);
    loop {
        let sound_due = async {
            match next_sound {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            () = tokio::time::sleep_until(deadline) => break,
            () = sound_due => {
                if let Some(sound) = &sound {
                    match bot.play_sound(channel, sound).await {
                        Ok(()) => println!("played {sound}"),
                        Err(error) => eprintln!("voicebot: could not play {sound}: {error:#}"),
                    }
                }
                next_sound = args
                    .sound_every
                    .map(|every| tokio::time::Instant::now() + Duration::from_secs(every));
            }
            payload = bot.next_event() => match payload {
                Some(CoreEventPayload::StreamCreate(stream))
                    if args.watch_streams
                        && stream.channel_id == channel
                        && stream.user_id != self_id =>
                {
                    watch_stream(&bot, &stream.stream_key).await;
                    watched.push(stream.stream_key.clone());
                }
                Some(CoreEventPayload::StreamDelete { stream_key, .. })
                    if args.until_stream_ends && watched.contains(&stream_key) =>
                {
                    println!("stream {stream_key} ended");
                    break;
                }
                Some(CoreEventPayload::VoiceChannelEffect(effect)) => println!(
                    "sound {} ({}) from user {}, {} ms after start",
                    effect.sound_ref,
                    effect.name,
                    effect.user_id,
                    started.elapsed().as_millis(),
                ),
                Some(_) => {}
                None => break,
            },
        }
    }
    let tones = |heard: &Heard| -> String {
        let mut levels = Vec::new();
        for channel in 0..heard.channels {
            for frequency in &args.listen_for {
                let side = match (heard.channels, channel) {
                    (1, _) => "",
                    (_, 0) => "left ",
                    _ => "right ",
                };
                levels.push(format!(
                    "{side}{frequency} Hz {:.1} dBFS",
                    heard.tone_dbfs(channel, *frequency)
                ));
            }
        }
        match levels.is_empty() {
            true => String::new(),
            false => format!("; {}", levels.join(", ")),
        }
    };
    for (user, heard) in session.heard() {
        println!(
            "user {user}: {} packets, {} decoded, peak {:.1} dBFS{}",
            heard.packets,
            heard.decoded,
            heard.peak_dbfs,
            tones(&heard)
        );
    }
    for (user, heard) in session.stream_heard() {
        println!(
            "stream sound from user {user}: {} packets, {} decoded, peak {:.1} dBFS{}",
            heard.packets,
            heard.decoded,
            heard.peak_dbfs,
            tones(&heard)
        );
    }
    for (user, seen) in session.seen() {
        let [low, middle, high] = seen.layers;
        println!(
            "user {user}: {} pictures, {} intact, {} decoded, {} keyframes, {:.0} kbit/s; by layer {low}/{middle}/{high}",
            seen.frames,
            seen.intact,
            seen.decoded,
            seen.keyframes,
            seen.bytes as f64 * 8.0 / duration.as_secs_f64() / 1000.0,
        );
    }
    Ok(())
}

async fn watch_stream(bot: &Voicebot, key: &str) {
    match bot.client.stream_watch(key, true).await {
        Ok(()) => println!("watching {key}"),
        Err(error) => eprintln!("voicebot: could not watch {key}: {error}"),
    }
}

/// "1000,2400": a frequency for each side.
fn tone_pair(text: &str) -> Result<(f32, f32), String> {
    let (left, right) = text
        .split_once(',')
        .ok_or("two frequencies, like 1000,2400")?;
    let parse = |hz: &str| {
        hz.trim()
            .parse::<f32>()
            .map_err(|_| format!("not a frequency: {hz}"))
    };
    Ok((parse(left)?, parse(right)?))
}

#[cfg(target_os = "linux")]
fn share_tone(session: &VoiceSession, left: f32, right: f32, duration: Duration) {
    session.share_tone(left, right, duration);
}

#[cfg(not(target_os = "linux"))]
fn share_tone(_session: &VoiceSession, _left: f32, _right: f32, _duration: Duration) {}

/// "720p30", "1080p60", "source60"...: a preset and a frame rate.
fn screen_quality(text: &str) -> Result<ScreenShareRequest, String> {
    let presets = [
        ("source", ScreenShareResolution::Source),
        ("1440p", ScreenShareResolution::P1440),
        ("1080p", ScreenShareResolution::P1080),
        ("720p", ScreenShareResolution::P720),
        ("480p", ScreenShareResolution::P480),
    ];
    let (resolution, fps) = presets
        .into_iter()
        .find_map(|(name, resolution)| Some((resolution, text.strip_prefix(name)?)))
        .ok_or("a quality is like 720p30 or source60")?;
    let fps = fps
        .parse()
        .map_err(|_| format!("no frame rate in {text}"))?;
    Ok(ScreenShareRequest {
        resolution,
        fps,
        has_audio: false,
    })
}

/// Goes live on the server, then sends the screen.
#[cfg(target_os = "linux")]
async fn go_live(
    bot: &Voicebot,
    session: &VoiceSession,
    channel: i64,
    request: ScreenShareRequest,
) -> anyhow::Result<()> {
    use opencord_common::video::ScreenPreset;
    use opencord_media::video::layers::ScreenShape;

    let stream = bot
        .client
        .create_stream(&bot.server_key, channel, StreamSourceKind::Screen, request)
        .await?;
    let preset = match request.resolution {
        ScreenShareResolution::P480 => ScreenPreset::P480,
        ScreenShareResolution::P720 => ScreenPreset::P720,
        ScreenShareResolution::P1080 => ScreenPreset::P1080,
        ScreenShareResolution::P1440 => ScreenPreset::P1440,
        ScreenShareResolution::Source => ScreenPreset::Source,
    };
    let shape = ScreenShape {
        max_pixels: preset.max_pixels(),
        fps: request.fps,
    };
    let track_id = format!("voicebot-screen-{}", bot.user_id());
    session.publish_encoded_screen(&track_id, shape).await?;
    println!("live as {}", stream.stream_key);
    Ok(())
}

#[cfg(not(target_os = "linux"))]
async fn go_live(
    _bot: &Voicebot,
    _session: &VoiceSession,
    _channel: i64,
    _request: ScreenShareRequest,
) -> anyhow::Result<()> {
    anyhow::bail!("an encoded screen needs Linux for now")
}

#[cfg(target_os = "linux")]
async fn publish_camera(session: &VoiceSession, track_id: &str) -> anyhow::Result<()> {
    session.publish_encoded_camera(track_id).await
}

#[cfg(not(target_os = "linux"))]
async fn publish_camera(_session: &VoiceSession, _track_id: &str) -> anyhow::Result<()> {
    anyhow::bail!("an encoded camera needs Linux for now; try --test-pattern")
}
