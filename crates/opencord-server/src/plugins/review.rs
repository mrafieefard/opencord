//! Uploaded plugins waiting for an admin's review (design 2026-11-07 §9.1,
//! §9.2). A review's files wait in `data/plugins/.reviews/<review_id>/`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use opencord_plugins::limits::PENDING_REVIEW_TIME;
use opencord_plugins::manifest::Manifest;
use opencord_plugins::signing::{Rotation, SignatureState};
use opencord_proto::v1 as proto;

use super::permissions::{PluginPermission, may_grant};
use crate::error::ApiError;
use crate::random;
use crate::state::{AppState, Audience, now_ms};

/// The licences that need no accepting: open-source SPDX ids.
const OPEN_LICENSES: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "MPL-2.0",
    "Zlib",
    "Unlicense",
    "CC0-1.0",
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "LGPL-2.1-only",
    "LGPL-2.1-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "AGPL-3.0-only",
    "AGPL-3.0-or-later",
];

/// How often expired reviews are swept.
pub const SWEEP_EVERY: Duration = Duration::from_secs(60);

pub fn license_needs_acceptance(license: &str) -> bool {
    !OPEN_LICENSES.contains(&license)
}

/// A checked package waiting for an admin.
pub struct PendingReview {
    pub id: String,
    pub manifest: Manifest,
    pub signature: SignatureState,
    /// Its checked `KEY-ROTATION`, if any.
    pub rotation: Option<Rotation>,
    pub github_check: proto::GithubCheck,
    /// The icon as a profile picture, already normalized.
    pub icon: Option<Vec<u8>>,
    /// Where its files wait.
    pub folder: PathBuf,
    pub expires_at_ms: i64,
    pub dev: bool,
}

/// The reviews waiting. **An entry leaves** when confirmed or discarded
/// ([`Reviews::take`]), replaced by a newer dev push ([`Reviews::take_dev`]), or once
/// expired ([`Reviews::sweep`], every [`SWEEP_EVERY`]); its folder goes
/// with it.
#[derive(Default)]
pub struct Reviews {
    entries: Mutex<HashMap<String, Arc<PendingReview>>>,
    /// Added to the clock; moved only by tests.
    offset_ms: AtomicI64,
}

impl Reviews {
    /// The time reviews expire by.
    pub fn now_ms(&self) -> i64 {
        now_ms() + self.offset_ms.load(Ordering::Relaxed)
    }

    /// For tests: reviews see the time as `by` later from now on.
    pub fn move_clock(&self, by: Duration) {
        let by = i64::try_from(by.as_millis()).unwrap_or(i64::MAX);
        self.offset_ms.fetch_add(by, Ordering::Relaxed);
    }

    pub fn expires_at_ms(&self) -> i64 {
        self.now_ms() + PENDING_REVIEW_TIME.as_millis() as i64
    }

    pub fn insert(&self, review: PendingReview) -> Arc<PendingReview> {
        let review = Arc::new(review);
        self.lock().insert(review.id.clone(), review.clone());
        review
    }

    /// The review, unless it expired.
    pub fn get(&self, id: &str) -> Option<Arc<PendingReview>> {
        let now = self.now_ms();
        self.lock()
            .get(id)
            .filter(|review| review.expires_at_ms > now)
            .cloned()
    }

    /// Takes the review out, unless it expired. Its folder is the caller's.
    pub fn take(&self, id: &str) -> Option<Arc<PendingReview>> {
        let now = self.now_ms();
        let mut entries = self.lock();
        if entries.get(id)?.expires_at_ms <= now {
            return None;
        }
        entries.remove(id)
    }

    /// Takes out the plugin's dev reviews other than `keep`, expired or
    /// not. Their folders are the caller's.
    pub fn take_dev(&self, plugin_id: &str, keep: &str) -> Vec<Arc<PendingReview>> {
        let mut entries = self.lock();
        let ids: Vec<String> = entries
            .values()
            .filter(|review| review.dev && review.manifest.id == plugin_id && review.id != keep)
            .map(|review| review.id.clone())
            .collect();
        ids.iter().filter_map(|id| entries.remove(id)).collect()
    }

    /// The reviews that haven't expired, oldest first.
    pub fn all(&self) -> Vec<Arc<PendingReview>> {
        let now = self.now_ms();
        let mut reviews: Vec<Arc<PendingReview>> = self
            .lock()
            .values()
            .filter(|review| review.expires_at_ms > now)
            .cloned()
            .collect();
        reviews.sort_unstable_by_key(|review| review.expires_at_ms);
        reviews
    }

    /// Removes the expired reviews and their folders; their ids.
    pub async fn sweep(&self) -> Vec<String> {
        let now = self.now_ms();
        let expired: Vec<Arc<PendingReview>> = {
            let mut entries = self.lock();
            let ids: Vec<String> = entries
                .values()
                .filter(|review| review.expires_at_ms <= now)
                .map(|review| review.id.clone())
                .collect();
            ids.iter().filter_map(|id| entries.remove(id)).collect()
        };
        for review in &expired {
            remove_folder(&review.folder).await;
        }
        expired.iter().map(|review| review.id.clone()).collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<PendingReview>>> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A new review id: 16 random bytes, base32 lowercase.
pub fn new_id() -> String {
    data_encoding::BASE32_NOPAD
        .encode(&random::bytes::<16>())
        .to_ascii_lowercase()
}

/// Where reviews' files wait.
pub fn folder(state: &AppState) -> PathBuf {
    state.config.plugins_dir().join(".reviews")
}

/// Deletes a folder that may not exist. A folder left behind only takes
/// space, so a failure is logged and the caller goes on.
pub async fn remove_folder(folder: &Path) {
    match tokio::fs::remove_dir_all(folder).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, "could not delete a plugin folder"),
    }
}

/// Sweeps expired reviews every [`SWEEP_EVERY`] until shutdown.
pub async fn sweep_reviews(state: Arc<AppState>) {
    let mut every = tokio::time::interval(SWEEP_EVERY);
    loop {
        tokio::select! {
            _ = every.tick() => {}
            () = state.shutdown.cancelled() => return,
        }
        for id in state.plugins.reviews.sweep().await {
            removed(&state, id);
        }
    }
}

/// Tells the admins a review is gone.
pub fn removed(state: &AppState, review_id: String) {
    state.broadcast(
        proto::event::Kind::PluginReviewRemove(proto::PluginReviewRemove { review_id }),
        Audience::Holding(opencord_common::permissions::Permissions::MANAGE_PLUGINS),
    );
}

/// The review as `viewer` sees it. For an update, the permissions the
/// installed version holds keep their state; the others start granted
/// where `viewer` may grant them.
pub async fn to_proto(
    state: &AppState,
    review: &PendingReview,
    viewer: i64,
) -> Result<proto::PluginReview, ApiError> {
    let installed = super::describe(state, &review.manifest.id, Some(viewer)).await?;
    let held: HashMap<&str, bool> = installed
        .iter()
        .flat_map(|plugin| &plugin.permissions)
        .map(|grant| (grant.name.as_str(), grant.granted))
        .collect();
    let permissions = {
        let guild = state.guild();
        review
            .manifest
            .permissions
            .iter()
            .map(|name| {
                let grantable = PluginPermission::from_name(name)
                    .is_some_and(|permission| may_grant(&guild, viewer, permission));
                proto::PluginPermissionGrant {
                    name: name.clone(),
                    granted: held.get(name.as_str()).copied().unwrap_or(grantable),
                    grantable,
                }
            })
            .collect()
    };
    let new_permissions = match &installed {
        Some(_) => review
            .manifest
            .permissions
            .iter()
            .filter(|name| !held.contains_key(name.as_str()))
            .cloned()
            .collect(),
        None => Vec::new(),
    };
    let (signature, key_fingerprint) = match review.signature {
        SignatureState::Unsigned if review.dev => (proto::PluginSignature::Dev, String::new()),
        SignatureState::Unsigned => (proto::PluginSignature::Unsigned, String::new()),
        SignatureState::Signed(key) => (proto::PluginSignature::Signed, key.fingerprint()),
    };
    Ok(proto::PluginReview {
        review_id: review.id.clone(),
        plugin: Some(proto::Plugin {
            signature: signature as i32,
            key_fingerprint,
            github_check: review.github_check as i32,
            permissions,
            ..super::manifest_plugin(&review.manifest)
        }),
        installed,
        new_permissions,
        license_needs_acceptance: license_needs_acceptance(&review.manifest.license),
        expires_at_ms: review.expires_at_ms,
        dev: review.dev,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_licenses_need_no_acceptance() {
        assert!(!license_needs_acceptance("MIT"));
        assert!(!license_needs_acceptance("AGPL-3.0-or-later"));
        assert!(license_needs_acceptance("All rights reserved"));
        assert!(license_needs_acceptance("https://example.com/licence"));
        // Ids are matched exactly, as SPDX writes them.
        assert!(license_needs_acceptance("mit"));
    }

    #[test]
    fn review_ids_are_26_lowercase_base32_characters() {
        let id = new_id();
        assert_eq!(id.len(), 26);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c))
        );
        assert_ne!(id, new_id());
    }
}
