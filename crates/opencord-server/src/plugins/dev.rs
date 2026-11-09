//! Dev tokens, dev pushes and the log long-poll (design 2026-11-07 §17.3,
//! §19).

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use axum::http::HeaderMap;
use axum::http::header::AUTHORIZATION;
use opencord_common::address::format_fingerprint;
use opencord_common::permissions::Permissions;
use opencord_plugins::limits::{
    DEV_TOKEN_DEFAULT_DAYS, DEV_TOKEN_MAX_DAYS, DEV_TOKEN_MIN_DAYS, LOG_LINES,
};
use opencord_plugins::manifest::{self, Manifest};
use opencord_plugins::package::Package;
use opencord_proto::v1 as proto;
use sha2::{Digest, Sha256};
use tokio::sync::broadcast::error::RecvError;
use tokio::time::Instant;

use super::install::{self, require_manager};
use super::logbook::PluginLog;
use super::permissions::PluginPermission;
use super::review::{self, PendingReview};
use super::{PluginHandle, identity};
use crate::db::plugins::{self as rows, ConfigValueRow, DevTokenRow, PermissionRow, PluginRow};
use crate::error::ApiError;
use crate::random;
use crate::state::{AppState, Audience, now_ms};

const SECRET_PREFIX: &str = "ocdev_";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// How long a log long-poll waits for new lines.
const LOG_WAIT: Duration = Duration::from_secs(25);
/// How often a long-poll looks again for a plugin that hasn't run yet.
const LOG_LOOK_EVERY: Duration = Duration::from_secs(1);

/// Dev tokens' clock.
#[derive(Default)]
pub struct DevTokens {
    /// Added to the clock; moved only by tests.
    offset_ms: AtomicI64,
}

impl DevTokens {
    /// The time tokens expire by.
    pub fn now_ms(&self) -> i64 {
        now_ms() + self.offset_ms.load(Ordering::Relaxed)
    }

    /// For tests: tokens see the time as `by` later from now on.
    pub fn move_clock(&self, by: Duration) {
        let by = i64::try_from(by.as_millis()).unwrap_or(i64::MAX);
        self.offset_ms.fetch_add(by, Ordering::Relaxed);
    }
}

/// `"ocdev_"` and 32 random bytes in lowercase base32.
fn new_secret() -> String {
    let random = data_encoding::BASE32_NOPAD
        .encode(&random::bytes::<32>())
        .to_ascii_lowercase();
    format!("{SECRET_PREFIX}{random}")
}

fn hash(secret: &str) -> [u8; 32] {
    Sha256::digest(secret.as_bytes()).into()
}

/// Whether two hashes are equal, looking at every byte whichever differ.
fn same_hash(a: &[u8], b: &[u8]) -> bool {
    let differ = a.iter().zip(b).fold(0u8, |differ, (x, y)| differ | (x ^ y));
    a.len() == b.len() && std::hint::black_box(differ) == 0
}

fn token_proto(row: DevTokenRow) -> proto::PluginDevToken {
    proto::PluginDevToken {
        id: row.id,
        plugin_id: row.plugin_id,
        created_by: row.created_by,
        created_at_ms: row.created_at,
        expires_at_ms: row.expires_at,
        last_used_at_ms: row.last_used_at,
    }
}

/// `CreatePluginDevToken`: the secret, shown this once.
pub async fn create(
    state: &AppState,
    caller: i64,
    request: proto::CreatePluginDevToken,
) -> Result<proto::CreatedPluginDevToken, ApiError> {
    require_manager(state, caller)?;
    if !manifest::valid_id(&request.plugin_id) {
        return Err(ApiError::invalid_argument(
            "plugin_id must be 3 to 40 lowercase letters, digits and dashes, \
             starting and ending with a letter or digit",
        ));
    }
    let days = match request.days {
        0 => DEV_TOKEN_DEFAULT_DAYS,
        days if (DEV_TOKEN_MIN_DAYS..=DEV_TOKEN_MAX_DAYS).contains(&days) => days,
        _ => {
            return Err(ApiError::invalid_argument(format!(
                "days must be {DEV_TOKEN_MIN_DAYS} to {DEV_TOKEN_MAX_DAYS}"
            )));
        }
    };
    let secret = new_secret();
    let now = state.plugins.dev_tokens.now_ms();
    let row = DevTokenRow {
        id: state.ids.next_id(),
        plugin_id: request.plugin_id,
        created_by: caller,
        created_at: now,
        expires_at: now + i64::from(days) * DAY_MS,
        last_used_at: None,
    };
    rows::insert_dev_token(&mut *state.db.acquire().await?, &row, &hash(&secret)).await?;
    tracing::info!(plugin = %row.plugin_id, token = row.id, days, "a dev token was created");
    Ok(proto::CreatedPluginDevToken {
        token: Some(token_proto(row)),
        secret,
        server_fingerprint: format_fingerprint(&state.fingerprint),
    })
}

/// `RevokePluginDevToken`.
pub async fn revoke(
    state: &AppState,
    caller: i64,
    request: proto::RevokePluginDevToken,
) -> Result<(), ApiError> {
    require_manager(state, caller)?;
    if !rows::delete_dev_token(&mut *state.db.acquire().await?, request.token_id).await? {
        return Err(ApiError::not_found("dev token"));
    }
    tracing::info!(token = request.token_id, "a dev token was revoked");
    Ok(())
}

/// `FetchPluginDevTokens`: every token, without secrets.
pub async fn fetch(state: &AppState, caller: i64) -> Result<proto::PluginDevTokens, ApiError> {
    require_manager(state, caller)?;
    let tokens = rows::dev_tokens(&mut *state.db.acquire().await?).await?;
    Ok(proto::PluginDevTokens {
        tokens: tokens.into_iter().map(token_proto).collect(),
    })
}

/// A dev token that checked out for one plugin.
pub struct DevToken {
    pub id: i64,
    pub plugin_id: String,
    pub created_by: i64,
}

/// The bearer token in `headers`, if it names `plugin_id`, hasn't expired,
/// and the member who made it may still manage plugins.
pub async fn authorize(
    state: &AppState,
    headers: &HeaderMap,
    plugin_id: &str,
) -> Result<DevToken, ApiError> {
    let secret = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::unauthorized("a dev token is needed"))?
        .trim();
    let refused = || ApiError::forbidden("the dev token isn't valid for this plugin");
    if !manifest::valid_id(plugin_id) {
        return Err(refused());
    }
    let wanted = hash(secret);
    let now = state.plugins.dev_tokens.now_ms();
    let tokens = rows::dev_token_hashes(&mut *state.db.acquire().await?, plugin_id).await?;
    // Each is compared in full, so the time taken says nothing of the hash.
    let found = tokens.into_iter().fold(None, |found, row| {
        if same_hash(&row.token_hash, &wanted) {
            Some(row)
        } else {
            found
        }
    });
    let row = found
        .filter(|row| row.expires_at > now)
        .ok_or_else(refused)?;
    if require_manager(state, row.created_by).is_err() {
        return Err(ApiError::forbidden(
            "the dev token's creator can no longer manage plugins",
        ));
    }
    Ok(DevToken {
        id: row.id,
        plugin_id: plugin_id.to_owned(),
        created_by: row.created_by,
    })
}

/// Counts a push against its token's rate, and marks the token used.
pub async fn count_push(state: &AppState, token: &DevToken) -> Result<(), ApiError> {
    state
        .rate_limits
        .check_dev_push(token.id)
        .map_err(ApiError::rate_limited)?;
    let now = state.plugins.dev_tokens.now_ms();
    rows::set_dev_token_used(&mut *state.db.acquire().await?, token.id, now).await?;
    Ok(())
}

/// What a dev push did.
pub enum Pushed {
    /// It waits for an admin.
    Review(Box<proto::PluginReview>),
    /// It was installed and restarted, or crashed starting.
    Installed(Box<proto::Plugin>),
}

/// `PUT /plugins/dev/{id}`, once the token, its rate and the body are
/// known.
pub async fn push(
    state: &Arc<AppState>,
    token: &DevToken,
    zip: Vec<u8>,
) -> Result<Pushed, ApiError> {
    let (pending, package) = install::read_package(zip, true).await?;
    if pending.manifest.id != token.plugin_id {
        return Err(ApiError::forbidden(format!(
            "the dev token is for {}, not {}",
            token.plugin_id, pending.manifest.id
        )));
    }
    let _changes = state.plugins.changes.lock().await;
    let (row, pin) = install::pinned(state, &token.plugin_id).await?;
    install::decide(pin, &pending)?;
    let Some(row) = row else {
        install::check_room(state).await?;
        return wait_for_review(state, token, pending, package).await;
    };
    let installed = install::saved_manifest(&row)?;
    let stored = rows::config_values(&mut *state.db.acquire().await?, &row.id).await?;
    if asks_for_more(&installed, &pending.manifest, &stored) {
        return wait_for_review(state, token, pending, package).await;
    }
    install_dev(state, token, row, &stored, pending, package)
        .await
        .map(|plugin| Pushed::Installed(Box::new(plugin)))
}

/// Whether `new` asks for a permission or a domain `installed` didn't ask
/// for, or has a required setting with no value stored.
fn asks_for_more(installed: &Manifest, new: &Manifest, stored: &[ConfigValueRow]) -> bool {
    let permission = new
        .permissions
        .iter()
        .any(|name| !installed.permissions.contains(name));
    let domain = new
        .domains
        .iter()
        .any(|domain| !installed.domains.contains(domain));
    let setting = new
        .config
        .iter()
        .any(|field| field.required && !stored.iter().any(|value| value.key == field.key));
    permission || domain || setting
}

/// Holds the push for review in place of the plugin's earlier dev review,
/// and tells the admins.
async fn wait_for_review(
    state: &AppState,
    token: &DevToken,
    pending: PendingReview,
    package: Package,
) -> Result<Pushed, ApiError> {
    let pending = install::hold_for_review(state, pending, package).await?;
    for old in state
        .plugins
        .reviews
        .take_dev(&token.plugin_id, &pending.id)
    {
        review::remove_folder(&old.folder).await;
        review::removed(state, old.id.clone());
    }
    let shown = review::to_proto(state, &pending, token.created_by).await?;
    state.broadcast(
        proto::event::Kind::PluginReviewUpdate(Box::new(proto::PluginReviewUpdate {
            review: Some(shown.clone()),
        })),
        Audience::Holding(Permissions::MANAGE_PLUGINS),
    );
    Ok(Pushed::Review(Box::new(shown)))
}

/// Installs a push that asks for nothing new as a dev version, keeping the
/// grants and settings that still apply, and restarts it. A version that
/// fails to start stays, crashed with its error: dev mode shows the
/// developer what broke rather than rolling back.
async fn install_dev(
    state: &Arc<AppState>,
    token: &DevToken,
    old: PluginRow,
    stored: &[ConfigValueRow],
    pending: PendingReview,
    package: Package,
) -> Result<proto::Plugin, ApiError> {
    let id = old.id.clone();
    let manifest = &pending.manifest;
    let config = install::merge_config(state, manifest, stored, &[])?;
    let (grants, license_accepted_by) = {
        let mut conn = state.db.acquire().await?;
        (
            rows::permission_rows(&mut conn, &id).await?,
            rows::license_accepted_by(&mut conn, &id).await?,
        )
    };
    let grants: Vec<PermissionRow> = grants
        .into_iter()
        .filter(|grant| manifest.permissions.contains(&grant.name))
        .collect();
    let granted: HashSet<PluginPermission> = grants
        .iter()
        .filter_map(|grant| PluginPermission::from_name(&grant.name))
        .collect();
    let manifest_json = serde_json::to_string(manifest).map_err(|error| {
        tracing::error!(plugin = %id, %error, "a manifest couldn't be saved");
        ApiError::internal()
    })?;
    let now = now_ms();
    let row = PluginRow {
        version: manifest.version.to_string(),
        folder: format!("v{now}"),
        manifest: manifest_json,
        signature: "dev".to_owned(),
        public_key: None,
        github_check: "none".to_owned(),
        running: true,
        crashed_reason: None,
        updated_at: now,
        ..old.clone()
    };
    let plugin_dir = state.config.plugins_dir().join(&id);
    let version_dir = plugin_dir.join(&row.folder);
    install::write_package(package, version_dir.clone()).await?;
    let saved = async {
        let icon_hash = match pending.icon.clone() {
            Some(png) => Some(crate::avatars::store(state, png).await?),
            None => None,
        };
        let mut tx = state.db.begin().await?;
        rows::update_plugin(&mut tx, &row, license_accepted_by).await?;
        rows::replace_permissions(&mut tx, &id, &grants).await?;
        rows::replace_config(&mut tx, &id, &config).await?;
        tx.commit().await?;
        Ok::<_, ApiError>(icon_hash)
    };
    let icon_hash = match saved.await {
        Ok(icon_hash) => icon_hash,
        Err(error) => {
            review::remove_folder(&version_dir).await;
            return Err(error);
        }
    };
    identity::rename(state, &id, &manifest.name, icon_hash.as_deref()).await?;
    identity::set_permissions(state, &id, &granted).await?;
    match install::start(state, &row).await {
        Ok(()) => {
            tracing::info!(plugin = %id, "a dev version was installed");
            super::views::scripts_updated(state, &id);
        }
        Err(error) => {
            tracing::info!(plugin = %id, %error, "a dev version failed to start");
            crashed(state, &id, &error).await?;
        }
    }
    if old.folder != row.folder {
        review::remove_folder(&plugin_dir.join(&old.folder)).await;
    }
    super::announce(state, &id).await;
    install::shown(state, &id, token.created_by).await
}

/// Shows a dev version that failed to start as crashed, with why. It stays
/// off until the next push, or until an admin starts it.
async fn crashed(state: &AppState, id: &str, error: &str) -> Result<(), ApiError> {
    // A start that failed before its VM ran leaves the old one running.
    state.plugins.stop(state, id).await;
    super::save_crashed_reason(state, id, Some(error)).await?;
    if let Some(handle) = state.plugins.get(id) {
        handle.set_status(proto::PluginState::Crashed, Some(error.to_owned()));
    }
    Ok(())
}

/// `GET /plugins/dev/{id}/log`: the plugin's lines after `after`, oldest
/// first, waiting up to [`LOG_WAIT`] for some; none if nothing came.
pub async fn wait_for_log(
    state: &AppState,
    plugin_id: &str,
    after: u64,
) -> Vec<proto::PluginLogLine> {
    let deadline = Instant::now() + LOG_WAIT;
    loop {
        if let Some(handle) = state.plugins.get(plugin_id) {
            return watch_log(state, &handle, after, deadline).await;
        }
        // Not run since the server started, or still waiting for review.
        let look = deadline.min(Instant::now() + LOG_LOOK_EVERY);
        tokio::select! {
            () = tokio::time::sleep_until(look) => {}
            () = state.shutdown.cancelled() => return Vec::new(),
        }
        if Instant::now() >= deadline {
            return Vec::new();
        }
    }
}

async fn watch_log(
    state: &AppState,
    handle: &PluginHandle,
    after: u64,
    deadline: Instant,
) -> Vec<proto::PluginLogLine> {
    let mut watcher = handle.log().subscribe();
    loop {
        let lines = lines_after(handle.log(), after);
        if !lines.is_empty() {
            return lines;
        }
        tokio::select! {
            received = watcher.recv() => match received {
                // A new line, or lines missed: read the log again.
                Ok(_) | Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return Vec::new(),
            },
            () = tokio::time::sleep_until(deadline) => return Vec::new(),
            () = state.shutdown.cancelled() => return Vec::new(),
        }
    }
}

/// The log's lines after `after`. A log whose newest line is older than
/// `after` started again with the server, so all of it is new.
fn lines_after(log: &PluginLog, after: u64) -> Vec<proto::PluginLogLine> {
    let lines = log.lines(None, LOG_LINES);
    let restarted = lines.last().is_some_and(|line| line.seq < after);
    lines
        .into_iter()
        .filter(|line| restarted || line.seq > after)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_ocdev_and_52_base32_characters() {
        let secret = new_secret();
        let random = secret.strip_prefix("ocdev_").unwrap();
        assert_eq!(random.len(), 52);
        assert!(
            random
                .chars()
                .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c))
        );
        assert_ne!(secret, new_secret());
    }

    #[test]
    fn hashes_compare_whole() {
        let a = hash("ocdev_a");
        let mut b = a;
        b[31] ^= 1;
        assert!(same_hash(&a, &a));
        assert!(!same_hash(&a, &b));
        assert!(!same_hash(&a, &a[..31]));
    }

    #[tokio::test]
    async fn a_restarted_log_is_new_from_its_first_line() {
        let log = PluginLog::new("test", None);
        log.push(proto::PluginLogLevel::Info, "one");
        log.push(proto::PluginLogLevel::Info, "two");

        let after_one: Vec<u64> = lines_after(&log, 1).iter().map(|line| line.seq).collect();
        let after_two = lines_after(&log, 2);
        let restarted: Vec<u64> = lines_after(&log, 40).iter().map(|line| line.seq).collect();

        assert_eq!(after_one, vec![2]);
        assert!(after_two.is_empty());
        assert_eq!(restarted, vec![1, 2]);
    }
}
