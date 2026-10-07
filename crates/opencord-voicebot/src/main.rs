//! `opencord-voicebot`: joins a voice channel, plays a tone and reports
//! what it hears. For checking that a server's voice works end to end.

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
    tokio::time::sleep(duration).await;
    for (user, heard) in session.heard() {
        println!(
            "user {user}: {} packets, {} decoded, peak {:.1} dBFS",
            heard.packets, heard.decoded, heard.peak_dbfs
        );
    }
    Ok(())
}
