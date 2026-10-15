//! `opencord-voice-load`: fills a server with voicebots for the voice
//! node's load test (Phase 2 plan §16.1): channels of people, some talking,
//! some with cameras, one sharing a screen, everyone watching every video.
//! The video is the synthetic test pattern at each layer's full bitrate, so
//! the bots spend their CPU on the network, not on encoding. Prints `ready`
//! once everyone is in, then holds for --seconds and prints what arrived.
//!
//! The server must allow this many sign-ins from one address: run it with
//! OPENCORD_IDENTIFY_PER_MINUTE set high.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Context as _;
use clap::Parser;
use opencord_common::video::ScreenPreset;
use opencord_core::api::types::{
    ChannelKind, CoreEventPayload, ScreenShareRequest, ScreenShareResolution, StreamSourceKind,
    VoiceSettingsChanges,
};
use opencord_media::video::layers::{ScreenShape, screen_layers};
use opencord_voicebot::Voicebot;
use tokio::task::JoinSet;
use tokio::time::Instant;

#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// host:port of a fresh server to claim.
    #[arg(long)]
    server: String,
    /// Its owner claim token.
    #[arg(long)]
    claim: String,
    #[arg(long, default_value_t = 10)]
    channels: usize,
    #[arg(long, default_value_t = 10)]
    people: usize,
    /// People in each channel who talk all the time.
    #[arg(long, default_value_t = 2)]
    talkers: usize,
    /// People in each channel with a camera.
    #[arg(long, default_value_t = 2)]
    cameras: usize,
    /// Screen shares in each channel (0 or 1), 1080p30.
    #[arg(long, default_value_t = 1)]
    streams: usize,
    /// Tile height everyone watches every video at.
    #[arg(long, default_value_t = 360)]
    watch: u32,
    /// How long to hold everyone in, once they are.
    #[arg(long, default_value_t = 60)]
    seconds: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Arc::new(Args::parse());
    let owner = Voicebot::connect(&args.server, Some(args.claim.clone()), "Owner").await?;
    // Shares at 1080p (the server allows 720p unless told).
    owner
        .client
        .update_voice_settings(
            &owner.server_key,
            VoiceSettingsChanges {
                screen_share_max_resolution: Some(ScreenShareResolution::P1080),
                ..VoiceSettingsChanges::default()
            },
        )
        .await
        .context("raising the screen share cap")?;
    let mut channels = Vec::new();
    for index in 1..=args.channels {
        let channel = owner
            .client
            .create_channel(
                &owner.server_key,
                ChannelKind::Voice,
                format!("Load {index}"),
                None,
                None,
            )
            .await
            .context("creating a channel")?;
        channels.push(channel.id);
    }
    let invite = owner
        .client
        .create_invite(&owner.server_key, None, None)
        .await?
        .link;

    let total = args.channels * args.people;
    let joined = Arc::new(AtomicUsize::new(0));
    let everyone_in = Arc::new(tokio::sync::Notify::new());
    let mut bots = JoinSet::new();
    for (index, channel) in channels.iter().copied().enumerate() {
        for seat in 0..args.people {
            let (args, invite) = (Arc::clone(&args), invite.clone());
            let (joined, everyone_in) = (Arc::clone(&joined), Arc::clone(&everyone_in));
            bots.spawn(async move {
                let name = format!("L{index}-{seat}");
                let result = bot(&args, &invite, channel, seat, &name, || {
                    if joined.fetch_add(1, Ordering::SeqCst) + 1 == total {
                        everyone_in.notify_one();
                    }
                })
                .await;
                (name, result)
            });
            // A few at a time: the server's gateway and the node's DTLS.
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    tokio::select! {
        () = everyone_in.notified() => println!("ready: {total} people in {} channels", args.channels),
        Some(Ok((name, Err(error)))) = bots.join_next() => anyhow::bail!("{name} failed: {error:#}"),
    }
    let mut frames = 0u64;
    let mut packets = 0u64;
    while let Some(done) = bots.join_next().await {
        let (name, result) = done?;
        let (seen, heard) = result.with_context(|| name.clone())?;
        frames += seen;
        packets += heard;
    }
    println!("done: {frames} pictures and {packets} audio packets received in all");
    drop(owner);
    Ok(())
}

/// One person: joins, talks, shows a camera or shares a screen as their
/// seat says, watches everything, holds, and reports pictures and audio
/// packets received.
async fn bot(
    args: &Args,
    invite: &str,
    channel: i64,
    seat: usize,
    name: &str,
    joined: impl FnOnce(),
) -> anyhow::Result<(u64, u64)> {
    let mut bot = Voicebot::connect(invite, None, name).await?;
    let session = bot.join(channel).await?;
    let hold = Duration::from_secs(args.seconds) + Duration::from_secs(120);
    if seat < args.talkers {
        session.play_tone(300.0 + 40.0 * seat as f32, hold);
    }
    let cameras = args.talkers..args.talkers + args.cameras;
    if cameras.contains(&seat) {
        session
            .publish_camera(&format!("load-camera-{}", bot.user_id()))
            .await?;
    }
    if args.streams > 0 && seat == cameras.end {
        let request = ScreenShareRequest {
            resolution: ScreenShareResolution::P1080,
            fps: 30,
            has_audio: false,
        };
        bot.client
            .create_stream(&bot.server_key, channel, StreamSourceKind::Screen, request)
            .await?;
        let shape = ScreenShape {
            max_pixels: ScreenPreset::P1080.max_pixels(),
            fps: 30,
        };
        session
            .publish_screen_pattern(
                &format!("load-screen-{}", bot.user_id()),
                screen_layers(1920, 1080, shape),
            )
            .await?;
    }
    session.watch(args.watch, false);
    joined();

    // Watch every stream in the channel as it starts, until the end.
    let self_id = bot.user_id();
    let deadline = Instant::now() + Duration::from_secs(args.seconds) + Duration::from_secs(60);
    loop {
        let event = tokio::select! {
            event = bot.next_event() => event,
            () = tokio::time::sleep_until(deadline) => break,
        };
        match event {
            Some(CoreEventPayload::StreamCreate(stream))
                if stream.channel_id == channel && stream.user_id != self_id =>
            {
                bot.client.stream_watch(&stream.stream_key, true).await?;
            }
            Some(_) => {}
            None => break,
        }
    }
    let frames = session.seen().values().map(|seen| seen.frames).sum();
    let packets = session.heard().values().map(|heard| heard.packets).sum();
    Ok((frames, packets))
}
