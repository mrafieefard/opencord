//! Command-line interface.

use std::path::PathBuf;

use anyhow::Context as _;
use clap::{Parser, Subcommand};
use opencord_common::address::{Fingerprint, InviteLink, ServerAddress};

use crate::bootstrap::{self, BootstrapError};
use crate::config::Config;
use crate::db::meta::ServerMeta;
use crate::db::{self, invites};
use crate::state::now_ms;
use crate::tls;

#[derive(Debug, Parser)]
#[command(
    name = "opencord-server",
    version,
    about = "Self-hostable Opencord community server"
)]
pub struct Cli {
    /// Config file. Created with defaults if it does not exist.
    #[arg(short, long, env = "OPENCORD_CONFIG", default_value = "opencord.toml")]
    pub config: PathBuf,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum Command {
    /// Run the server. This is the default.
    Run,
    /// Print the fingerprint of the server's TLS certificate.
    Fingerprint,
    /// Manage invites.
    Invite {
        #[command(subcommand)]
        action: InviteCommand,
    },
    /// Issue a new owner claim token. Whoever uses it becomes the owner,
    /// replacing the current one.
    ResetClaimToken,
}

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum InviteCommand {
    /// Create an invite and print its link.
    Create {
        /// How many people can join with it. Unlimited if omitted.
        #[arg(long)]
        max_uses: Option<u32>,
        /// Seconds until it expires. Never expires if omitted.
        #[arg(long, value_name = "SECONDS")]
        expires_in: Option<u32>,
    },
}

pub fn fingerprint(config: &Config) -> anyhow::Result<Fingerprint> {
    let tls = tls::load_or_generate(
        &config.tls,
        &config.server.data_dir.join("tls"),
        std::slice::from_ref(&config.server.public_host),
    )?;
    Ok(tls.fingerprint)
}

pub async fn create_invite(
    config: &Config,
    max_uses: Option<u32>,
    expires_in: Option<u32>,
) -> anyhow::Result<InviteLink> {
    let pool = open_database(config).await?;
    let mut conn = pool.acquire().await?;
    ServerMeta::load(&mut conn)
        .await?
        .ok_or(BootstrapError::NotInitialized)?;
    let invite = invites::create(&mut conn, None, max_uses, expires_in, now_ms()).await?;
    Ok(InviteLink {
        address: ServerAddress {
            host: config.server.public_host.to_ascii_lowercase(),
            port: config.public_port(),
        },
        code: invite.code,
        fingerprint: Some(fingerprint(config)?),
    })
}

pub async fn reset_claim_token(config: &Config) -> anyhow::Result<String> {
    let pool = open_database(config).await?;
    Ok(bootstrap::reset_claim_token(&pool).await?)
}

async fn open_database(config: &Config) -> anyhow::Result<sqlx::SqlitePool> {
    let data_dir = &config.server.data_dir;
    std::fs::create_dir_all(data_dir)
        .with_context(|| format!("could not create {}", data_dir.display()))?;
    db::connect(&data_dir.join(db::DATABASE_FILE))
        .await
        .context("could not open the database")
}

#[cfg(test)]
mod tests {
    use opencord_common::snowflake::SnowflakeGenerator;

    use super::*;

    fn config(dir: &tempfile::TempDir) -> Config {
        let mut config = Config::default();
        config.server.data_dir = dir.path().to_owned();
        config.server.public_host = "Chat.Example.com".to_owned();
        config
    }

    #[test]
    fn parses_subcommands() {
        let cli = Cli::try_parse_from([
            "opencord-server",
            "--config",
            "/etc/opencord.toml",
            "invite",
            "create",
            "--max-uses",
            "3",
        ])
        .unwrap();

        assert_eq!(cli.config, PathBuf::from("/etc/opencord.toml"));
        assert_eq!(
            cli.command,
            Some(Command::Invite {
                action: InviteCommand::Create {
                    max_uses: Some(3),
                    expires_in: None
                }
            })
        );
        assert_eq!(
            Cli::try_parse_from(["opencord-server"]).unwrap().command,
            None
        );
    }

    #[tokio::test]
    async fn creates_an_invite_link_for_the_public_address() {
        let dir = tempfile::tempdir().unwrap();
        let config = config(&dir);
        let pool = open_database(&config).await.unwrap();
        bootstrap::initialize(&pool, "Test", &SnowflakeGenerator::new(0).unwrap(), 0)
            .await
            .unwrap();

        let link = create_invite(&config, Some(2), None).await.unwrap();

        assert_eq!(link.address.host, "chat.example.com");
        assert_eq!(link.address.port, 7710);
        assert_eq!(link.fingerprint, Some(fingerprint(&config).unwrap()));
        let stored = invites::find(&mut pool.acquire().await.unwrap(), &link.code)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.max_uses, Some(2));
    }

    #[tokio::test]
    async fn commands_need_a_started_server() {
        let dir = tempfile::tempdir().unwrap();

        assert!(create_invite(&config(&dir), None, None).await.is_err());
        assert!(reset_claim_token(&config(&dir)).await.is_err());
    }
}
