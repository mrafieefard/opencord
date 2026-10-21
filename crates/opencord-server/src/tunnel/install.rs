//! Downloading cloudflared on the owner's request (plan §9.2): Cloudflare's
//! latest release for this machine, used only when its SHA-256 matches the
//! one Cloudflare lists in the release notes (and GitHub's own digest of the
//! file, when GitHub gives one).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::lock;
use crate::error::ApiError;
use crate::state::AppState;

/// cloudflared is about 40 MB; anything far bigger is not it.
const MAX_SIZE: u64 = 200 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(300);

/// Where the downloaded cloudflared lives.
pub fn target(data_dir: &Path) -> PathBuf {
    data_dir.join("bin").join("cloudflared")
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
}

/// The release file for this machine.
fn asset_name() -> Result<String, String> {
    if std::env::consts::OS != "linux" {
        return Err("Opencord downloads cloudflared only on Linux: install it yourself".into());
    }
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        "x86" => "386",
        other => return Err(format!("Cloudflare has no cloudflared for {other}")),
    };
    Ok(format!("cloudflared-linux-{arch}"))
}

/// `name: <hex>` from the release notes' checksum list.
fn listed_checksum(body: &str, name: &str) -> Option<String> {
    body.lines().find_map(|line| {
        let (file, sum) = line.trim().split_once(':')?;
        let sum = sum.trim();
        (file.trim() == name && sum.len() == 64 && sum.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| sum.to_ascii_lowercase())
    })
}

/// Starts the download; progress shows as `installing` in the status.
pub fn start(state: &Arc<AppState>) -> Result<(), ApiError> {
    {
        let mut inner = lock(&state.tunnel.inner);
        if inner.installing {
            return Err(ApiError::conflict(
                "cloudflared is already being downloaded",
            ));
        }
        inner.installing = true;
        inner.error = None;
    }
    state.network.changed(state);
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let result = download(&state).await;
        {
            let mut inner = lock(&state.tunnel.inner);
            inner.installing = false;
            if let Err(error) = &result {
                inner.error = Some(error.clone());
            }
        }
        match result {
            Ok(tag) => {
                tracing::info!(version = tag, "cloudflared was downloaded");
                super::detect(&state).await;
            }
            Err(error) => tracing::warn!(%error, "cloudflared could not be downloaded"),
        }
        state.network.changed(&state);
    });
    Ok(())
}

async fn download(state: &AppState) -> Result<String, String> {
    let name = asset_name()?;
    let http = state.network.http();
    let plain = |error: reqwest::Error| format!("the download failed: {error}");
    let release: Release = http
        .get(&state.config.tunnel.releases_api)
        .header("User-Agent", "opencord-server")
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(plain)?
        .json()
        .await
        .map_err(plain)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == name)
        .ok_or_else(|| format!("release {} has no {name}", release.tag_name))?;
    let listed = listed_checksum(&release.body, &name)
        .ok_or_else(|| format!("release {} lists no checksum for {name}", release.tag_name))?;
    if let Some(digest) = &asset.digest
        && digest.strip_prefix("sha256:").map(str::to_ascii_lowercase) != Some(listed.clone())
    {
        return Err("the checksums Cloudflare and GitHub give differ: not installed".into());
    }
    let mut response = http
        .get(&asset.browser_download_url)
        .header("User-Agent", "opencord-server")
        .timeout(TIMEOUT)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(plain)?;
    let mut hasher = Sha256::new();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(plain)? {
        if (bytes.len() + chunk.len()) as u64 > MAX_SIZE {
            return Err("the file is far bigger than cloudflared: not installed".into());
        }
        hasher.update(&chunk);
        bytes.extend_from_slice(&chunk);
    }
    if hex::encode(hasher.finalize()) != listed {
        return Err("the file's checksum is not the one Cloudflare lists: not installed".into());
    }
    let target = target(&state.config.server.data_dir);
    write_executable(&target, &bytes).map_err(|error| format!("could not save it: {error}"))?;
    Ok(release.tag_name)
}

fn write_executable(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = target.with_extension("partial");
    std::fs::write(&partial, bytes)?;
    // Only Linux downloads it (`asset_name`); elsewhere this is never called.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&partial, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checksum_is_read_from_the_release_notes() {
        let body = "### SHA256 Checksums:\n```\ncloudflared-linux-386: aa\n\
                    cloudflared-linux-amd64: D33FF2D14475178D2012C2C56BEBA87389AC5DED27649519F198A7D3134A99DB\n\
                    cloudflared-linux-amd64.deb: 9721287c28f78cb623e77c7b95a63feea87e909054d75666a0ec9eff20e5d548\n```";
        assert_eq!(
            listed_checksum(body, "cloudflared-linux-amd64").as_deref(),
            Some("d33ff2d14475178d2012c2c56beba87389ac5ded27649519f198a7d3134a99db")
        );
        assert_eq!(listed_checksum(body, "cloudflared-linux-386"), None);
        assert_eq!(listed_checksum(body, "cloudflared-linux-arm"), None);
    }
}
