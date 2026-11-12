//! Installing, updating and removing plugins (design 2026-11-07 §9).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use opencord_common::permissions::Permissions;
use opencord_plugins::manifest::{self, ConfigField, ConfigKind, Manifest};
use opencord_plugins::package::Package;
use opencord_plugins::signing::{
    self, Incoming, Pin, PublicKey, Rotation, SignatureState, SigningError, UpdateDecision,
};
use opencord_proto::v1 as proto;

use super::permissions::{PluginPermission, may_grant};
use super::review::{self, PendingReview};
use super::{PluginEvent, identity};
use crate::db::plugins::{self as rows, ConfigValueRow, PermissionRow, PluginRow};
use crate::error::ApiError;
use crate::guild::Guild;
use crate::permissions::require_base;
use crate::state::{AppState, Audience, now_ms};

/// The longest `text` or `secret` setting.
const TEXT_MAX_CHARS: usize = 2_000;

fn package_invalid(text: impl Into<String>) -> ApiError {
    ApiError::new(proto::ErrorCode::PluginPackageInvalid, text)
}

fn signature_invalid(error: SigningError) -> ApiError {
    ApiError::new(proto::ErrorCode::PluginSignatureInvalid, error.to_string())
}

fn io_failed(error: std::io::Error) -> ApiError {
    tracing::error!(%error, "a plugin's files couldn't be written");
    ApiError::internal()
}

pub fn require_manager(state: &AppState, user_id: i64) -> Result<(), ApiError> {
    require_base(&state.guild(), user_id, Permissions::MANAGE_PLUGINS).map(|_| ())
}

/// A package that passed every check.
struct Checked {
    package: Package,
    manifest: Manifest,
    signature: SignatureState,
    rotation: Option<Rotation>,
    icon: Option<Vec<u8>>,
}

/// Reads and checks an uploaded zip. Hashes every file: run it off the
/// async threads.
fn check_package(zip: &[u8]) -> Result<Checked, ApiError> {
    let package = Package::read_zip(zip).map_err(|error| package_invalid(error.to_string()))?;
    let source = package
        .get("manifest.lua")
        .ok_or_else(|| package_invalid("manifest.lua is missing"))?;
    let source = std::str::from_utf8(source)
        .map_err(|_| package_invalid("manifest.lua isn't UTF-8 text"))?;
    let manifest =
        manifest::evaluate(source).map_err(|error| package_invalid(error.to_string()))?;
    manifest::check_files(&manifest, &package)
        .map_err(|error| package_invalid(error.to_string()))?;
    let signature = signing::verify(&package, &manifest).map_err(signature_invalid)?;
    let rotation = signing::rotation(&package).map_err(signature_invalid)?;
    let icon = manifest
        .icon
        .as_deref()
        .and_then(|path| package.get(path))
        .map(<[u8]>::to_vec);
    Ok(Checked {
        package,
        manifest,
        signature,
        rotation,
        icon,
    })
}

/// The plugin's row, if installed, and the key it is pinned to: the
/// installed version's, or the one an uninstall that kept the data left.
pub(super) async fn pinned(
    state: &AppState,
    id: &str,
) -> Result<(Option<PluginRow>, Pin), ApiError> {
    let mut conn = state.db.acquire().await?;
    let row = rows::plugin(&mut conn, id).await?;
    let key = match &row {
        Some(row) => row.public_key.clone(),
        None => rows::leftover(&mut conn, id).await?.flatten(),
    };
    let pin = match key {
        None => Pin::None,
        Some(bytes) => Pin::Key(PublicKey(<[u8; 32]>::try_from(bytes).map_err(|_| {
            tracing::error!(plugin = %id, "a plugin's pinned key isn't 32 bytes");
            ApiError::internal()
        })?)),
    };
    Ok((row, pin))
}

/// The pin after installing what came, or why it may not replace what's
/// there.
pub(super) fn decide(pin: Pin, review: &PendingReview) -> Result<Pin, ApiError> {
    let incoming = match review.signature {
        SignatureState::Signed(key) => Incoming::Signed(key),
        SignatureState::Unsigned if review.dev => Incoming::Dev,
        SignatureState::Unsigned => Incoming::Unsigned,
    };
    match signing::update_decision(&pin, &incoming, review.rotation.as_ref()) {
        UpdateDecision::Allow { pin } => Ok(pin),
        UpdateDecision::Refuse(text) => {
            Err(ApiError::new(proto::ErrorCode::PluginNotSameAuthor, text))
        }
    }
}

pub(super) async fn check_room(state: &AppState) -> Result<(), ApiError> {
    let max = state.config.plugins.max_plugins;
    let count = rows::count_plugins(&mut *state.db.acquire().await?).await?;
    if count >= i64::from(max) {
        return Err(ApiError::new(
            proto::ErrorCode::PluginLimit,
            format!("this server already has {max} plugins"),
        ));
    }
    Ok(())
}

/// `POST /plugins/packages`, once the caller and the body are known: the
/// zip checked and waiting for review.
pub async fn upload(
    state: &Arc<AppState>,
    user_id: i64,
    zip: Vec<u8>,
) -> Result<proto::PluginReview, ApiError> {
    let (mut pending, package) = read_package(zip, false).await?;
    let (row, pin) = pinned(state, &pending.manifest.id).await?;
    decide(pin, &pending)?;
    if row.is_none() {
        check_room(state).await?;
    }
    if let (Some(repo), SignatureState::Signed(key)) = (&pending.manifest.github, pending.signature)
    {
        pending.github_check = super::github::check(repo, &key).await;
    }
    let pending = hold_for_review(state, pending, package).await?;
    review::to_proto(state, &pending, user_id).await
}

/// Checks a zip off the async threads, and makes its icon a profile
/// picture: a review with no folder or expiry yet. A dev push (`dev`) is
/// never signed, whatever its files say.
pub(super) async fn read_package(
    zip: Vec<u8>,
    dev: bool,
) -> Result<(PendingReview, Package), ApiError> {
    let checked = tokio::task::spawn_blocking(move || check_package(&zip))
        .await
        .map_err(|_| ApiError::internal())??;
    let icon =
        match checked.icon {
            Some(bytes) => Some(super::super::avatars::normalize(bytes).await.map_err(
                |error| package_invalid(format!("the icon isn't a picture: {}", error.message)),
            )?),
            None => None,
        };
    let (signature, rotation) = if dev {
        (SignatureState::Unsigned, None)
    } else {
        (checked.signature, checked.rotation)
    };
    let pending = PendingReview {
        id: review::new_id(),
        manifest: checked.manifest,
        signature,
        rotation,
        github_check: proto::GithubCheck::Unspecified,
        icon,
        folder: PathBuf::new(),
        expires_at_ms: 0,
        dev,
    };
    Ok((pending, checked.package))
}

/// Writes the review's files where reviews wait, and keeps it.
pub(super) async fn hold_for_review(
    state: &AppState,
    mut pending: PendingReview,
    package: Package,
) -> Result<Arc<PendingReview>, ApiError> {
    pending.folder = review::folder(state).join(&pending.id);
    write_package(package, pending.folder.clone()).await?;
    pending.expires_at_ms = state.plugins.reviews.expires_at_ms();
    let pending = state.plugins.reviews.insert(pending);
    tracing::info!(plugin = %pending.manifest.id, dev = pending.dev, "a plugin is waiting for review");
    Ok(pending)
}

/// Writes the package's files into `folder`, or nothing.
pub(super) async fn write_package(package: Package, folder: PathBuf) -> Result<(), ApiError> {
    let target = folder.clone();
    let written = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        for path in package.paths() {
            let file = target.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(file, package.get(path).unwrap_or_default())?;
        }
        Ok(())
    })
    .await
    .map_err(|_| ApiError::internal())?;
    if let Err(error) = written {
        review::remove_folder(&folder).await;
        return Err(io_failed(error));
    }
    Ok(())
}

/// The permissions in `names`, each one the manifest asks for and the
/// caller may grant, or already `held`: keeping a grant isn't granting.
fn check_grants(
    state: &AppState,
    caller: i64,
    manifest: &Manifest,
    names: &[String],
    held: &HashSet<String>,
) -> Result<HashSet<PluginPermission>, ApiError> {
    let guild = state.guild();
    names
        .iter()
        .map(|name| {
            match PluginPermission::from_name(name).filter(|_| manifest.permissions.contains(name))
            {
                Some(permission)
                    if held.contains(name) || may_grant(&guild, caller, permission) =>
                {
                    Ok(permission)
                }
                _ => Err(ApiError::forbidden(format!("you can't grant {name}"))),
            }
        })
        .collect()
}

/// The rows for `granted`: ones already held keep who granted them.
fn grant_rows(
    granted: &HashSet<PluginPermission>,
    old: &[PermissionRow],
    caller: i64,
    now: i64,
) -> Vec<PermissionRow> {
    let mut names: Vec<&str> = granted.iter().map(|permission| permission.name()).collect();
    names.sort_unstable();
    names
        .into_iter()
        .map(|name| {
            old.iter()
                .find(|row| row.name == name)
                .cloned()
                .unwrap_or_else(|| PermissionRow {
                    name: name.to_owned(),
                    granted_by: caller,
                    granted_at: now,
                })
        })
        .collect()
}

pub(crate) fn config_purpose(plugin_id: &str, key: &str) -> String {
    format!("plugin-config:{plugin_id}:{key}")
}

/// The settings after applying `submitted` to `stored`: an empty value
/// unsets a setting, except a secret's, which keeps the stored one.
/// Settings the manifest no longer has are dropped; every required one
/// must be set.
pub(super) fn merge_config(
    state: &AppState,
    manifest: &Manifest,
    stored: &[ConfigValueRow],
    submitted: &[proto::PluginConfigValue],
) -> Result<Vec<ConfigValueRow>, ApiError> {
    let fields: HashMap<&str, &ConfigField> = manifest
        .config
        .iter()
        .map(|field| (field.key.as_str(), field))
        .collect();
    let mut values: HashMap<String, ConfigValueRow> = stored
        .iter()
        .filter(|row| fields.contains_key(row.key.as_str()))
        .map(|row| (row.key.clone(), row.clone()))
        .collect();
    {
        let guild = state.guild();
        for value in submitted {
            let field = fields.get(value.key.as_str()).ok_or_else(|| {
                ApiError::invalid_argument(format!("{} isn't a setting of this plugin", value.key))
            })?;
            if value.value.is_empty() {
                if field.kind != ConfigKind::Secret {
                    values.remove(&value.key);
                }
                continue;
            }
            check_value(&guild, field, &value.value)?;
            let row = if field.kind == ConfigKind::Secret {
                ConfigValueRow {
                    key: field.key.clone(),
                    value: state.certificates.secrets().seal(
                        &config_purpose(&manifest.id, &field.key),
                        value.value.as_bytes(),
                    ),
                    sealed: true,
                }
            } else {
                ConfigValueRow {
                    key: field.key.clone(),
                    value: value.value.clone().into_bytes(),
                    sealed: false,
                }
            };
            values.insert(field.key.clone(), row);
        }
    }
    if let Some(field) = manifest
        .config
        .iter()
        .find(|field| field.required && !values.contains_key(&field.key))
    {
        return Err(ApiError::invalid_argument(format!(
            "{} is required",
            field.label
        )));
    }
    let mut rows: Vec<ConfigValueRow> = values.into_values().collect();
    rows.sort_unstable_by(|a, b| a.key.cmp(&b.key));
    Ok(rows)
}

fn check_value(guild: &Guild, field: &ConfigField, value: &str) -> Result<(), ApiError> {
    let id = || {
        (value.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| value.parse::<i64>().ok())
            .flatten()
    };
    let label = &field.label;
    let (fits, problem) = match field.kind {
        ConfigKind::Number => (
            value.parse::<f64>().is_ok_and(f64::is_finite),
            format!("{label} must be a number"),
        ),
        ConfigKind::Boolean => (
            value == "true" || value == "false",
            format!("{label} must be true or false"),
        ),
        ConfigKind::Choice => (
            field.choices.iter().any(|choice| choice == value),
            format!("{label} must be one of its choices"),
        ),
        ConfigKind::Role => (
            id().is_some_and(|id| guild.roles.contains_key(&id)),
            format!("{label} must be a role of this server"),
        ),
        ConfigKind::Channel => (
            id().is_some_and(|id| guild.channels.contains_key(&id)),
            format!("{label} must be a channel of this server"),
        ),
        ConfigKind::User => (
            id().is_some_and(|id| guild.members.contains_key(&id)),
            format!("{label} must be a member of this server"),
        ),
        ConfigKind::Text | ConfigKind::Secret => (
            value.chars().count() <= TEXT_MAX_CHARS,
            format!("{label} is at most 2,000 characters"),
        ),
    };
    if fits {
        Ok(())
    } else {
        Err(ApiError::invalid_argument(problem))
    }
}

pub(super) fn saved_manifest(row: &PluginRow) -> Result<Manifest, ApiError> {
    serde_json::from_str(&row.manifest).map_err(|error| {
        tracing::error!(plugin = %row.id, %error, "a plugin's saved manifest is unreadable");
        ApiError::internal()
    })
}

/// The installed plugin as `viewer` sees it.
pub(super) async fn shown(
    state: &AppState,
    id: &str,
    viewer: i64,
) -> Result<proto::Plugin, ApiError> {
    super::describe(state, id, Some(viewer))
        .await?
        .ok_or_else(|| ApiError::not_found("plugin"))
}

/// Starts the plugin its row describes.
pub(super) async fn start(state: &Arc<AppState>, row: &PluginRow) -> Result<(), String> {
    let installed = super::installed(state, row.clone()).await?;
    state
        .plugins
        .start(state, installed)
        .await
        .map_err(|error| error.0)
}

/// What a confirmed review writes.
struct Install {
    row: PluginRow,
    license_accepted_by: Option<i64>,
    granted: HashSet<PluginPermission>,
    grants: Vec<PermissionRow>,
    config: Vec<ConfigValueRow>,
    icon_hash: Option<String>,
}

/// The version an update replaces, as it was.
struct Previous {
    row: PluginRow,
    manifest: Manifest,
    license_accepted_by: Option<i64>,
    grants: Vec<PermissionRow>,
    config: Vec<ConfigValueRow>,
    avatar_hash: Option<String>,
}

/// `ConfirmPluginReview`: installs or updates the plugin, and starts it.
pub async fn confirm(
    state: &Arc<AppState>,
    caller: i64,
    request: proto::ConfirmPluginReview,
) -> Result<proto::Plugin, ApiError> {
    require_manager(state, caller)?;
    let pending = state
        .plugins
        .reviews
        .get(&request.review_id)
        .ok_or_else(|| ApiError::not_found("review"))?;
    let manifest = &pending.manifest;
    let id = manifest.id.clone();
    let _changes = state.plugins.changes.lock().await;
    // Read under the lock: another admin may have installed, updated or
    // removed it since the upload.
    require_manager(state, caller)?;
    let (row, pin) = pinned(state, &id).await?;
    let previous = match row {
        Some(row) => {
            let mut conn = state.db.acquire().await?;
            let avatar_hash = state
                .guild()
                .plugin_users
                .get(&row.user_id)
                .and_then(|user| user.avatar_hash.clone());
            Some(Previous {
                manifest: saved_manifest(&row)?,
                license_accepted_by: rows::license_accepted_by(&mut conn, &id).await?,
                grants: rows::permission_rows(&mut conn, &id).await?,
                config: rows::config_values(&mut conn, &id).await?,
                avatar_hash,
                row,
            })
        }
        None => None,
    };
    let held: HashSet<String> = previous
        .iter()
        .flat_map(|previous| previous.grants.iter().map(|grant| grant.name.clone()))
        .collect();
    let granted = check_grants(state, caller, manifest, &request.granted, &held)?;
    let stored = previous
        .as_ref()
        .map_or(&[][..], |previous| &previous.config);
    let config = merge_config(state, manifest, stored, &request.config)?;
    let needs_acceptance = review::license_needs_acceptance(&manifest.license);
    if needs_acceptance && !request.accept_license {
        return Err(ApiError::invalid_argument("the licence must be accepted"));
    }
    let pin = decide(pin, &pending)?;
    if previous.is_none() {
        check_room(state).await?;
    }
    let pending = state
        .plugins
        .reviews
        .take(&request.review_id)
        .ok_or_else(|| ApiError::not_found("review"))?;
    review::removed(state, pending.id.clone());

    let now = now_ms();
    let folder = format!("v{now}");
    let plugin_dir = state.config.plugins_dir().join(&id);
    let version_dir = plugin_dir.join(&folder);
    let moved = async {
        tokio::fs::create_dir_all(&plugin_dir).await?;
        tokio::fs::rename(&pending.folder, &version_dir).await
    };
    if let Err(error) = moved.await {
        review::remove_folder(&pending.folder).await;
        return Err(io_failed(error));
    }
    let icon_hash = match pending.icon.clone() {
        Some(png) => match crate::avatars::store(state, png).await {
            Ok(hash) => Some(hash),
            Err(error) => {
                review::remove_folder(&version_dir).await;
                return Err(error);
            }
        },
        None => None,
    };
    let manifest_json = serde_json::to_string(&pending.manifest).map_err(|error| {
        tracing::error!(plugin = %id, %error, "a manifest couldn't be saved");
        ApiError::internal()
    })?;
    let old_grants = previous
        .as_ref()
        .map_or(&[][..], |previous| &previous.grants);
    let install = Install {
        row: PluginRow {
            id: id.clone(),
            user_id: 0,
            role_id: 0,
            version: pending.manifest.version.to_string(),
            folder,
            manifest: manifest_json,
            signature: match (pending.signature, pending.dev) {
                (SignatureState::Signed(_), _) => "signed",
                (SignatureState::Unsigned, true) => "dev",
                (SignatureState::Unsigned, false) => "unsigned",
            }
            .to_owned(),
            public_key: match pin {
                Pin::Key(key) => Some(key.0.to_vec()),
                Pin::None => None,
            },
            github_check: match pending.github_check {
                proto::GithubCheck::Verified => "verified",
                proto::GithubCheck::Mismatch => "mismatch",
                proto::GithubCheck::Failed => "failed",
                proto::GithubCheck::Unspecified => "none",
            }
            .to_owned(),
            running: true,
            crashed_reason: None,
            installed_at: previous
                .as_ref()
                .map_or(now, |previous| previous.row.installed_at),
            updated_at: now,
        },
        license_accepted_by: needs_acceptance.then_some(caller),
        grants: grant_rows(&granted, old_grants, caller, now),
        granted,
        config,
        icon_hash,
    };
    let result = match previous {
        None => install_new(state, &pending.manifest, caller, install).await,
        Some(previous) => update(state, &pending.manifest, install, previous).await,
    };
    if result.is_err() {
        review::remove_folder(&version_dir).await;
    }
    result?;
    super::announce(state, &id).await;
    shown(state, &id, caller).await
}

/// A first install: its user and role, its rows, then its start. One that
/// fails to start stays installed and stopped, with the error shown.
async fn install_new(
    state: &Arc<AppState>,
    manifest: &Manifest,
    caller: i64,
    install: Install,
) -> Result<(), ApiError> {
    let identity = identity::ensure(
        state,
        &manifest.id,
        &manifest.name,
        install.icon_hash.as_deref(),
        caller,
        &install.granted,
    )
    .await?;
    let row = PluginRow {
        user_id: identity.user_id,
        role_id: identity.role_id,
        ..install.row
    };
    let written = async {
        let mut tx = state.db.begin().await?;
        rows::insert_plugin(&mut tx, &row, install.license_accepted_by).await?;
        rows::replace_permissions(&mut tx, &row.id, &install.grants).await?;
        rows::replace_config(&mut tx, &row.id, &install.config).await?;
        rows::delete_leftover(&mut tx, &row.id).await?;
        tx.commit().await
    };
    if let Err(error) = written.await {
        // Without its row the plugin isn't installed: its role goes too.
        if let Err(retired) = identity::retire(state, &row.id).await {
            tracing::error!(plugin = %row.id, error = %retired.message, "a half-installed plugin's role stayed");
        }
        return Err(error.into());
    }
    tracing::info!(plugin = %row.id, "a plugin was installed");
    if let Err(error) = start(state, &row).await {
        tracing::warn!(plugin = %row.id, %error, "a new plugin failed to start");
        rows::set_running(&mut *state.db.acquire().await?, &row.id, false).await?;
    }
    Ok(())
}

/// An update: the new version replaces the old, and must start. If it
/// doesn't, the old version is put back and started again; its folder
/// goes only once the new version runs.
async fn update(
    state: &Arc<AppState>,
    manifest: &Manifest,
    install: Install,
    previous: Previous,
) -> Result<(), ApiError> {
    let row = PluginRow {
        user_id: previous.row.user_id,
        role_id: previous.row.role_id,
        ..install.row
    };
    let mut tx = state.db.begin().await?;
    rows::update_plugin(&mut tx, &row, install.license_accepted_by).await?;
    rows::replace_permissions(&mut tx, &row.id, &install.grants).await?;
    rows::replace_config(&mut tx, &row.id, &install.config).await?;
    tx.commit().await?;
    let started = async {
        identity::rename(state, &row.id, &manifest.name, install.icon_hash.as_deref())
            .await
            .map_err(|error| error.message)?;
        identity::set_permissions(state, &row.id, &install.granted)
            .await
            .map_err(|error| error.message)?;
        start(state, &row).await
    };
    match started.await {
        Ok(()) => {
            let old = state
                .config
                .plugins_dir()
                .join(&row.id)
                .join(&previous.row.folder);
            if previous.row.folder != row.folder {
                review::remove_folder(&old).await;
            }
            tracing::info!(plugin = %row.id, "a plugin was updated");
            super::views::scripts_updated(state, &row.id);
            Ok(())
        }
        Err(error) => {
            let message = format!("the new version failed to start: {error}");
            tracing::warn!(plugin = %row.id, %error, "an update failed to start; rolling back");
            roll_back(state, previous).await;
            if let Some(handle) = state.plugins.get(&row.id) {
                handle.log().push(proto::PluginLogLevel::Error, &message);
            }
            super::announce(state, &row.id).await;
            Err(ApiError::new(proto::ErrorCode::PluginUnavailable, message))
        }
    }
}

/// Puts the previous version's row, permissions, config, name and role
/// back, and starts it if it was running. Failures are logged: the admin
/// already gets the update's error, and the plugin page shows the state.
async fn roll_back(state: &Arc<AppState>, previous: Previous) {
    let id = previous.row.id.clone();
    state.plugins.stop(state, &id).await;
    let restored = async {
        let mut tx = state.db.begin().await?;
        rows::update_plugin(&mut tx, &previous.row, previous.license_accepted_by).await?;
        rows::replace_permissions(&mut tx, &id, &previous.grants).await?;
        rows::replace_config(&mut tx, &id, &previous.config).await?;
        tx.commit().await
    };
    if let Err(error) = restored.await {
        return tracing::error!(plugin = %id, %error, "could not put a plugin's old version back");
    }
    let granted: HashSet<PluginPermission> = previous
        .grants
        .iter()
        .filter_map(|grant| PluginPermission::from_name(&grant.name))
        .collect();
    let identity = async {
        identity::rename(
            state,
            &id,
            &previous.manifest.name,
            previous.avatar_hash.as_deref(),
        )
        .await?;
        identity::set_permissions(state, &id, &granted).await
    };
    if let Err(error) = identity.await {
        tracing::error!(plugin = %id, error = %error.message, "could not put a plugin's old role back");
    }
    if previous.row.running
        && previous.row.crashed_reason.is_none()
        && let Err(error) = start(state, &previous.row).await
    {
        tracing::warn!(plugin = %id, %error, "a plugin's old version failed to start again");
    }
}

/// `UninstallPlugin`: stops it and removes its role and row. With
/// `delete_data` everything of it goes; without, its database, files and
/// log stay for a reinstall with the same id and key (its fetched images
/// don't).
pub async fn uninstall(
    state: &Arc<AppState>,
    caller: i64,
    request: proto::UninstallPlugin,
) -> Result<(), ApiError> {
    require_manager(state, caller)?;
    let id = request.plugin_id;
    let _changes = state.plugins.changes.lock().await;
    let row = rows::plugin(&mut *state.db.acquire().await?, &id)
        .await?
        .ok_or_else(|| ApiError::not_found("plugin"))?;
    if !request.delete_data {
        // Written first, so the pin can't be lost between the two.
        rows::insert_leftover(
            &mut *state.db.acquire().await?,
            &id,
            row.public_key.as_deref(),
            now_ms(),
        )
        .await?;
    }
    state.plugins.remove(state, &id).await;
    if let Err(error) = identity::retire(state, &id).await {
        if !request.delete_data
            && let Err(undo) = rows::delete_leftover(&mut *state.db.acquire().await?, &id).await
        {
            tracing::error!(plugin = %id, error = %undo, "a plugin that wasn't removed kept a leftover row");
        }
        return Err(error);
    }
    let dir = state.config.plugins_dir().join(&id);
    if request.delete_data {
        let mut conn = state.db.acquire().await?;
        rows::delete_plugin_files(&mut conn, &id).await?;
        rows::delete_leftover(&mut conn, &id).await?;
        review::remove_folder(&dir).await;
    } else {
        remove_versions(&dir).await;
        // Images fetched for members aren't the plugin's data.
        review::remove_folder(&dir.join(super::images::CACHE_FOLDER)).await;
    }
    forget_command_settings(state, &id).await?;
    tracing::info!(plugin = %id, delete_data = request.delete_data, "a plugin was uninstalled");
    state.broadcast(
        proto::event::Kind::PluginRemove(proto::PluginRemove { plugin_id: id }),
        Audience::Holding(Permissions::MANAGE_PLUGINS),
    );
    Ok(())
}

/// An uninstalled plugin's commands are off the Commands page for good, so
/// what was set for them goes too. Stopping a plugin keeps it.
async fn forget_command_settings(state: &Arc<AppState>, id: &str) -> Result<(), ApiError> {
    let _writes = state.write_lock().await;
    let settings = state.commands.settings();
    let left = crate::commands::remove_settings_for_plugin(settings.clone(), id);
    if left != settings {
        let mut conn = state.db.acquire().await?;
        state.commands.save(&mut conn, left).await?;
    }
    Ok(())
}

/// Deletes the version folders (`v<ms>`) in a plugin's folder.
async fn remove_versions(dir: &Path) {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => return tracing::warn!(%error, "could not list a plugin's folder"),
    };
    loop {
        match entries.next_entry().await {
            Ok(Some(entry)) => {
                if is_version_folder(&entry.file_name().to_string_lossy()) {
                    review::remove_folder(&entry.path()).await;
                }
            }
            Ok(None) => return,
            Err(error) => return tracing::warn!(%error, "could not list a plugin's folder"),
        }
    }
}

fn is_version_folder(name: &str) -> bool {
    name.strip_prefix('v')
        .is_some_and(|ms| !ms.is_empty() && ms.bytes().all(|byte| byte.is_ascii_digit()))
}

async fn installed_row(state: &AppState, id: &str) -> Result<PluginRow, ApiError> {
    rows::plugin(&mut *state.db.acquire().await?, id)
        .await?
        .ok_or_else(|| ApiError::not_found("plugin"))
}

/// `SetPluginPermissions`: newly granted ones need the caller to hold what
/// they need; any may be removed.
pub async fn set_permissions(
    state: &Arc<AppState>,
    caller: i64,
    request: proto::SetPluginPermissions,
) -> Result<proto::Plugin, ApiError> {
    require_manager(state, caller)?;
    let id = request.plugin_id;
    let _changes = state.plugins.changes.lock().await;
    let row = installed_row(state, &id).await?;
    let manifest = saved_manifest(&row)?;
    let old = rows::permission_rows(&mut *state.db.acquire().await?, &id).await?;
    let held: HashSet<String> = old.iter().map(|grant| grant.name.clone()).collect();
    let granted = check_grants(state, caller, &manifest, &request.granted, &held)?;
    let grants = grant_rows(&granted, &old, caller, now_ms());
    let mut tx = state.db.begin().await?;
    rows::replace_permissions(&mut tx, &id, &grants).await?;
    tx.commit().await?;
    identity::set_permissions(state, &id, &granted).await?;
    if let Some(handle) = state.plugins.get(&id) {
        handle.set_granted(granted);
        let admin = crate::music::display_name(state, caller);
        let now: HashSet<&str> = grants.iter().map(|grant| grant.name.as_str()).collect();
        for name in now.iter().filter(|name| !held.contains(**name)) {
            handle.log().push(
                proto::PluginLogLevel::Action,
                &format!("{admin} granted {name}"),
            );
        }
        for name in held.iter().filter(|name| !now.contains(name.as_str())) {
            handle.log().push(
                proto::PluginLogLevel::Action,
                &format!("{admin} revoked {name}"),
            );
        }
        if held.contains("register_commands")
            && !now.contains("register_commands")
            && state.commands.remove_for_plugin(&id)
        {
            crate::commands::announce(state);
        }
    }
    super::announce(state, &id).await;
    shown(state, &id, caller).await
}

pub(crate) fn build_config_values(
    state: &AppState,
    plugin_id: &str,
    manifest_fields: &[ConfigField],
    stored: &[ConfigValueRow],
) -> HashMap<String, super::ConfigValue> {
    let mut map = HashMap::new();
    for field in manifest_fields {
        if let Some(row) = stored.iter().find(|r| r.key == field.key) {
            let val_str = String::from_utf8_lossy(&row.value);
            let config_val = match field.kind {
                ConfigKind::Number => val_str.parse::<f64>().ok().map(super::ConfigValue::Number),
                ConfigKind::Boolean => Some(super::ConfigValue::Boolean(val_str == "true")),
                ConfigKind::Role => Some(super::ConfigValue::Role(val_str.into_owned())),
                ConfigKind::Channel => Some(super::ConfigValue::Channel(val_str.into_owned())),
                ConfigKind::User => Some(super::ConfigValue::User(val_str.into_owned())),
                ConfigKind::Secret => {
                    let purpose = config_purpose(plugin_id, &field.key);
                    match state
                        .certificates
                        .secrets()
                        .open_string(&purpose, &row.value)
                    {
                        Ok(opened) => Some(super::ConfigValue::Secret(opened)),
                        Err(err) => {
                            tracing::warn!(
                                plugin = %plugin_id,
                                key = %field.key,
                                %err,
                                "could not open plugin secret"
                            );
                            None
                        }
                    }
                }
                ConfigKind::Text | ConfigKind::Choice => {
                    Some(super::ConfigValue::Text(val_str.into_owned()))
                }
            };
            if let Some(cv) = config_val {
                map.insert(field.key.clone(), cv);
            }
        }
    }
    map
}

/// `SetPluginConfig`, then `settings_change` with the keys sent.
pub async fn set_config(
    state: &Arc<AppState>,
    caller: i64,
    request: proto::SetPluginConfig,
) -> Result<proto::Plugin, ApiError> {
    require_manager(state, caller)?;
    let id = request.plugin_id;
    let _changes = state.plugins.changes.lock().await;
    let row = installed_row(state, &id).await?;
    let manifest = saved_manifest(&row)?;
    let stored = rows::config_values(&mut *state.db.acquire().await?, &id).await?;
    let config = merge_config(state, &manifest, &stored, &request.config)?;
    let mut tx = state.db.begin().await?;
    rows::replace_config(&mut tx, &id, &config).await?;
    tx.commit().await?;
    let mut changed: Vec<&str> = request
        .config
        .iter()
        .map(|value| value.key.as_str())
        .collect();
    changed.sort_unstable();
    changed.dedup();

    if let Some(handle) = state.plugins.get(&id) {
        let map = build_config_values(state, &id, &manifest.config, &config);
        handle.update_config(map);
    }

    state.plugins.deliver(
        &id,
        PluginEvent {
            name: "settings_change".to_owned(),
            payload: serde_json::json!({ "changed": changed }),
            reply: None,
            command: None,
        },
    );
    super::announce(state, &id).await;
    shown(state, &id, caller).await
}

/// `SetPluginRunning`: the admin's switch.
pub async fn set_running(
    state: &Arc<AppState>,
    caller: i64,
    request: proto::SetPluginRunning,
) -> Result<proto::Plugin, ApiError> {
    require_manager(state, caller)?;
    let id = request.plugin_id;
    let _changes = state.plugins.changes.lock().await;
    let row = installed_row(state, &id).await?;
    rows::set_running(&mut *state.db.acquire().await?, &id, request.running).await?;
    if request.running {
        if let Err(error) = start(state, &row).await {
            tracing::info!(plugin = %id, %error, "a plugin failed to start");
        }
    } else {
        state.plugins.stop(state, &id).await;
    }
    super::announce(state, &id).await;
    shown(state, &id, caller).await
}

/// `RestartPlugin`: stopped, if running, and started.
pub async fn restart(
    state: &Arc<AppState>,
    caller: i64,
    request: proto::RestartPlugin,
) -> Result<proto::Plugin, ApiError> {
    set_running(
        state,
        caller,
        proto::SetPluginRunning {
            plugin_id: request.plugin_id,
            running: true,
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_version_folders_are_version_folders() {
        assert!(is_version_folder("v1762560000000"));
        assert!(!is_version_folder("v"));
        assert!(!is_version_folder("files"));
        assert!(!is_version_folder("data.sqlite"));
        assert!(!is_version_folder("vfiles"));
    }
}
