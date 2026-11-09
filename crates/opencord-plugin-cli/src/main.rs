use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "opencord-plugin", about = "Opencord plugin development tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a starter plugin in a new folder
    Init {
        /// Plugin ID
        id: String,
    },
    /// Generate an Ed25519 signing key
    Keygen {
        /// Output path for the private key
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Pack a plugin folder into a zip archive
    Pack {
        /// Plugin directory to pack
        folder: Option<PathBuf>,
        /// Path to Ed25519 private key
        #[arg(long)]
        key: Option<PathBuf>,
        /// Output zip file path
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Write a KEY-ROTATION statement handing the plugin over to a new key
    RotateKey {
        /// Path to the old private key
        #[arg(long)]
        old: PathBuf,
        /// Path to the new private key
        #[arg(long)]
        new: PathBuf,
        /// Plugin folder
        folder: Option<PathBuf>,
    },
    /// Watch a plugin directory and push changes to an Opencord server in dev mode
    Dev {
        /// Server address as host:port
        #[arg(long)]
        server: String,
        /// Expected TLS certificate fingerprint (hex)
        #[arg(long)]
        fingerprint: Option<String>,
        /// Dev token (ocdev_...)
        #[arg(long)]
        token: String,
        /// Plugin folder
        folder: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { id } => {
            if let Err(e) = opencord_plugin_cli::init::init(&id, std::path::Path::new(".")) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Keygen { out } => {
            if let Err(e) = opencord_plugin_cli::keys::keygen(out.as_deref()) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Pack { folder, key, out } => {
            let folder = folder.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = opencord_plugin_cli::pack::pack(&folder, key.as_deref(), out.as_deref())
            {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::RotateKey { old, new, folder } => {
            let folder = folder.unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = opencord_plugin_cli::pack::rotate_key(&old, &new, &folder) {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Dev {
            server,
            fingerprint,
            token,
            folder,
        } => {
            let folder = folder.unwrap_or_else(|| PathBuf::from("."));
            let opts = opencord_plugin_cli::dev::DevOptions {
                server: &server,
                fingerprint: fingerprint.as_deref(),
                token: &token,
                folder: &folder,
            };
            if let Err(e) = opencord_plugin_cli::dev::run_dev(opts).await {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }
}
