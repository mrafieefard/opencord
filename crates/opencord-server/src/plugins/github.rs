//! "Verified via GitHub" (design 2026-11-07 §9.4): a signed plugin's
//! repository publishes its key in `.opencord/plugin.pub`.

use std::time::Duration;

use opencord_plugins::manifest::GithubRepo;
use opencord_plugins::signing::PublicKey;
use opencord_proto::v1 as proto;

const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_BYTES: usize = 4 * 1024;

/// Fetches the repository's published key and compares it with `key`.
pub async fn check(repo: &GithubRepo, key: &PublicKey) -> proto::GithubCheck {
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/HEAD/.opencord/plugin.pub",
        repo.owner, repo.repo
    );
    match tokio::time::timeout(TIMEOUT, fetch(&url)).await {
        Ok(Ok(Some(text))) => judge(&text, key),
        Ok(Ok(None)) => proto::GithubCheck::Mismatch,
        Ok(Err(error)) => {
            tracing::info!(%error, "could not check a plugin's key on GitHub");
            proto::GithubCheck::Failed
        }
        Err(_) => {
            tracing::info!("checking a plugin's key on GitHub timed out");
            proto::GithubCheck::Failed
        }
    }
}

/// The published file's text; `None` when the repository has none, or
/// something larger than a key.
async fn fetch(url: &str) -> Result<Option<String>, String> {
    let client = crate::tls::http_client().map_err(|error| error.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| error.to_string())?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!("GitHub answered {}", response.status()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_BYTES {
            return Ok(None);
        }
    }
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

fn judge(text: &str, key: &PublicKey) -> proto::GithubCheck {
    if text.trim() == key.to_string() {
        proto::GithubCheck::Verified
    } else {
        proto::GithubCheck::Mismatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_key_must_be_the_plugins() {
        let key = PublicKey([7; 32]);
        let other = PublicKey([8; 32]);
        assert_eq!(
            judge(&format!("  {key}\n"), &key),
            proto::GithubCheck::Verified
        );
        assert_eq!(
            judge(&other.to_string(), &key),
            proto::GithubCheck::Mismatch
        );
        assert_eq!(judge("", &key), proto::GithubCheck::Mismatch);
    }
}
