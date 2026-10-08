//! `opencord-voicebot`: joins a voice channel, plays a tone, can send a
//! synthetic camera and watch others', and reports what it hears and
//! sees. For checking that a server's voice and video work end to end.

use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use opencord_voicebot::Voicebot;

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
    /// Send a synthetic three-layer camera (180p, 360p, 720p).
    #[arg(long)]
    camera: bool,
    /// Watch everyone's video in tiles this many pixels tall.
    #[arg(long, value_name = "HEIGHT")]
    watch: Option<u32>,
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
    let bot = Voicebot::connect(&args.server, args.claim, &args.name).await?;
    let channel = bot.voice_channel(&args.channel)?;
    let session = bot.join(channel).await?;
    println!("joined {} as user {}", args.channel, bot.user_id());
    let duration = Duration::from_secs(args.seconds);
    if args.tone > 0.0 {
        session.play_tone(args.tone, duration);
    }
    if args.camera {
        // Track ids are unique in a channel.
        let track_id = format!("voicebot-camera-{}", bot.user_id());
        session.publish_camera(&track_id).await?;
    }
    if let Some(height) = args.watch {
        session.watch(height);
    }
    tokio::time::sleep(duration).await;
    for (user, heard) in session.heard() {
        println!(
            "user {user}: {} packets, {} decoded, peak {:.1} dBFS",
            heard.packets, heard.decoded, heard.peak_dbfs
        );
    }
    for (user, seen) in session.seen() {
        let [low, middle, high] = seen.layers;
        println!(
            "user {user}: {} pictures, {} intact, {} keyframes, {:.0} kbit/s; by layer {low}/{middle}/{high}",
            seen.frames,
            seen.intact,
            seen.keyframes,
            seen.bytes as f64 * 8.0 / duration.as_secs_f64() / 1000.0,
        );
    }
    Ok(())
}
