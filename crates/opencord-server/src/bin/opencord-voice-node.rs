//! `opencord-voice-node`: carries voice for an Opencord server from another
//! machine.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use opencord_common::address::format_fingerprint;
use opencord_server::cli;
use opencord_server::voice_node::{self, VoiceNodeConfig};
use tracing::info;

#[derive(Debug, Parser)]
#[command(
    name = "opencord-voice-node",
    version,
    about = "Voice node for an Opencord server"
)]
struct Cli {
    /// Config file. Created with defaults if it does not exist.
    #[arg(
        short,
        long,
        env = "OPENCORD_VOICE_NODE_CONFIG",
        default_value = "opencord-voice-node.toml"
    )]
    config: PathBuf,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the voice node. This is the default.
    Run,
    /// Write a new random shared secret to FILE. Copy the file to the main
    /// server too.
    GenerateSecret {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Run) {
        Command::Run => {
            let loaded = VoiceNodeConfig::load_or_create(&cli.config)?;
            cli::init_tracing(&loaded.config.log.filter);
            if loaded.created {
                info!(path = %cli.config.display(), "wrote a default config file");
            }
            let handle = voice_node::start(loaded.config).await?;
            info!(
                gateway = %handle.local_addr,
                endpoint = handle.endpoint(),
                udp_port = handle.udp_port(),
                fingerprint = %format_fingerprint(&handle.fingerprint),
                "voice node listening"
            );
            cli::shutdown_signal().await;
            info!("shutting down");
            handle.shutdown().await;
            Ok(())
        }
        Command::GenerateSecret { file } => {
            voice_node::generate_secret(&file)?;
            println!("wrote a new shared secret to {}", file.display());
            Ok(())
        }
    }
}
