use std::process::ExitCode;

use clap::Parser;
use opencord_common::address::format_fingerprint;
use opencord_server::cli::{self, Cli, Command, InviteCommand};
use opencord_server::config::Config;
use opencord_server::server;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

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
    let loaded = Config::load_or_create(&cli.config, |name| std::env::var(name).ok())?;
    let config = loaded.config;
    match cli.command.unwrap_or(Command::Run) {
        Command::Run => {
            init_tracing(&config.log.filter);
            if loaded.created {
                info!(path = %cli.config.display(), "wrote a default config file");
            }
            serve(config).await
        }
        Command::Fingerprint => {
            println!("{}", format_fingerprint(&cli::fingerprint(&config)?));
            Ok(())
        }
        Command::Invite {
            action:
                InviteCommand::Create {
                    max_uses,
                    expires_in,
                },
        } => {
            println!(
                "{}",
                cli::create_invite(&config, max_uses, expires_in).await?
            );
            Ok(())
        }
        Command::ResetClaimToken => {
            println!("{}", cli::reset_claim_token(&config).await?);
            Ok(())
        }
    }
}

async fn serve(config: Config) -> anyhow::Result<()> {
    let public_address = format!("{}:{}", config.server.public_host, config.public_port());
    let handle = server::start(config).await?;
    info!(address = %handle.local_addr, "listening");
    info!(fingerprint = %format_fingerprint(&handle.fingerprint), "TLS certificate");
    if let Some(token) = &handle.claim_token {
        warn!(
            address = %public_address,
            claim_token = %token,
            "this server has no owner yet: add it in the app with this claim token to become the owner",
        );
    }
    if let Some(link) = &handle.invite_link {
        info!(invite = %link, "share this invite link");
    }
    shutdown_signal().await;
    info!("shutting down");
    handle.shutdown().await;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

fn init_tracing(filter: &str) {
    let filter = EnvFilter::try_new(filter).unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
