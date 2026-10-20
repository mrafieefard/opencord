//! What the owner's storage page shows, and the checks its requests share.

use std::collections::HashMap;

use opencord_proto::v1 as proto;

use super::S3Settings;
use crate::db;
use crate::db::storage::StorageRow;
use crate::error::ApiError;
use crate::state::{AppState, Audience};

/// Every backend with its usage, the move's progress and the temp folder's
/// free space.
pub async fn snapshot(state: &AppState) -> Result<proto::Storage, ApiError> {
    let mut conn = state.db.acquire().await?;
    let rows = db::storage::list(&mut conn).await?;
    let usage: HashMap<i64, (i64, i64)> = db::attachments::usage(&mut conn)
        .await?
        .into_iter()
        .map(|(storage_id, files, bytes)| (storage_id, (files, bytes)))
        .collect();
    let backends = rows
        .iter()
        .map(|row| {
            let (files, bytes) = usage.get(&row.id).copied().unwrap_or((0, 0));
            backend_to_proto(row, files, bytes)
        })
        .collect();
    let temp_free_bytes = match fs4::available_space(state.attachments.temp_dir()) {
        Ok(bytes) => bytes,
        Err(error) => {
            // Shown on the page only; nothing is decided from it.
            tracing::warn!(%error, "the temp folder's free space couldn't be read");
            0
        }
    };
    Ok(proto::Storage {
        backends,
        r#move: state.attachments.mover().progress(),
        temp_free_bytes,
    })
}

fn backend_to_proto(row: &StorageRow, files: i64, bytes: i64) -> proto::StorageBackend {
    let is_s3 = row.kind == "s3";
    proto::StorageBackend {
        id: row.id,
        kind: if is_s3 {
            proto::StorageKind::S3
        } else {
            proto::StorageKind::Local
        }
        .into(),
        name: row.name.clone(),
        active: row.active,
        s3: is_s3.then(|| proto::S3Config {
            endpoint: row.endpoint.clone().unwrap_or_default(),
            region: row.region.clone().unwrap_or_default(),
            bucket: row.bucket.clone().unwrap_or_default(),
            key_prefix: row.key_prefix.clone(),
            path_style: row.path_style,
            allow_http: row.allow_http,
            access_key_id: mask(row.access_key_id.as_deref().unwrap_or_default()),
            secret_access_key: String::new(),
        }),
        has_secret: row.secret_enc.is_some(),
        file_count: u64::try_from(files).unwrap_or(0),
        bytes: u64::try_from(bytes).unwrap_or(0),
        last_error: row.last_error.clone(),
    }
}

/// "••••WXYZ": only the last 4 characters, and none of a key that short.
pub fn mask(access_key_id: &str) -> String {
    let chars: Vec<char> = access_key_id.chars().collect();
    if chars.len() <= 4 {
        return "••••".to_owned();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("••••{tail}")
}

/// Sends the owner the storage page's new state.
pub async fn broadcast(state: &AppState) -> Result<proto::Storage, ApiError> {
    let storage = snapshot(state).await?;
    let owner_id = state.guild().meta.owner_id;
    if let Some(owner_id) = owner_id {
        state.broadcast(
            proto::event::Kind::StorageUpdate(proto::StorageUpdate {
                storage: Some(storage.clone()),
            }),
            Audience::User(owner_id),
        );
    }
    Ok(storage)
}

pub fn valid_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(ApiError::invalid_argument(
            "a storage name is 1 to 100 characters on one line",
        ));
    }
    Ok(name.to_owned())
}

/// Checked, trimmed settings. `stored_secret` fills in an empty secret.
/// Bucket settings from a request. Editing a saved bucket, `saved` fills
/// in the keys left empty: the access key, and the secret while the access
/// key stays the same (a new access key needs its own secret).
pub fn valid_s3(
    config: &proto::S3Config,
    saved: Option<&S3Settings>,
) -> Result<S3Settings, ApiError> {
    let field = |value: &str, what: &str, max: usize| {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > max {
            return Err(ApiError::invalid_argument(format!(
                "the {what} is 1 to {max} characters"
            )));
        }
        Ok(value.to_owned())
    };
    let access_key_id = match (config.access_key_id.trim(), saved) {
        ("", Some(saved)) => saved.access_key_id.clone(),
        (key, _) => field(key, "access key", 200)?,
    };
    let secret_access_key = match (config.secret_access_key.trim(), saved) {
        ("", Some(saved)) if saved.access_key_id == access_key_id => {
            saved.secret_access_key.clone()
        }
        ("", Some(_)) => {
            return Err(ApiError::invalid_argument(
                "a new access key needs its secret key",
            ));
        }
        (secret, _) => field(secret, "secret key", 200)?,
    };
    let key_prefix = config.key_prefix.trim();
    if key_prefix.chars().count() > 200 {
        return Err(ApiError::invalid_argument(
            "the key prefix is at most 200 characters",
        ));
    }
    Ok(S3Settings {
        endpoint: field(&config.endpoint, "endpoint", 500)?,
        region: field(&config.region, "region", 100)?,
        bucket: field(&config.bucket, "bucket name", 100)?,
        key_prefix: key_prefix.to_owned(),
        path_style: config.path_style,
        allow_http: config.allow_http,
        access_key_id,
        secret_access_key,
    })
}

/// The first failed step, for a STORAGE_UNAVAILABLE message and `last_error`.
pub fn test_failure(test: &proto::StorageTest) -> Option<String> {
    test.steps
        .iter()
        .find(|step| !step.ok)
        .map(|step| format!("the bucket test failed at “{}”: {}", step.name, step.detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_keys_show_only_their_last_four_characters() {
        assert_eq!(mask("AKIAIOSFODNN7EXAMPLE"), "••••MPLE");
        assert_eq!(mask("abcd"), "••••");
        assert_eq!(mask(""), "••••");
    }

    fn saved() -> S3Settings {
        S3Settings {
            endpoint: "https://s3.example.com".into(),
            region: "auto".into(),
            bucket: "files".into(),
            key_prefix: String::new(),
            path_style: false,
            allow_http: false,
            access_key_id: "AKIASAVED".into(),
            secret_access_key: "saved-secret".into(),
        }
    }

    /// An edit keeps the keys it leaves empty; a new access key needs its
    /// own secret, a new secret alone keeps the access key.
    #[test]
    fn an_edit_keeps_the_saved_keys_it_leaves_empty() {
        let config = proto::S3Config {
            endpoint: " https://s3.example.com ".into(),
            region: "auto".into(),
            bucket: "other".into(),
            ..Default::default()
        };
        let settings = valid_s3(&config, Some(&saved())).unwrap();
        assert_eq!(settings.access_key_id, "AKIASAVED");
        assert_eq!(settings.secret_access_key, "saved-secret");
        assert_eq!(settings.endpoint, "https://s3.example.com");
        assert_eq!(settings.bucket, "other");
        assert!(valid_s3(&config, None).is_err(), "adding needs both keys");

        let new_secret = proto::S3Config {
            secret_access_key: "rotated".into(),
            ..config.clone()
        };
        let settings = valid_s3(&new_secret, Some(&saved())).unwrap();
        assert_eq!(settings.access_key_id, "AKIASAVED");
        assert_eq!(settings.secret_access_key, "rotated");

        let new_key = proto::S3Config {
            access_key_id: "AKIANEW".into(),
            ..config.clone()
        };
        assert!(valid_s3(&new_key, Some(&saved())).is_err());
        let both = proto::S3Config {
            secret_access_key: "new-secret".into(),
            ..new_key
        };
        let settings = valid_s3(&both, Some(&saved())).unwrap();
        assert_eq!(
            (
                settings.access_key_id.as_str(),
                settings.secret_access_key.as_str()
            ),
            ("AKIANEW", "new-secret")
        );
    }
}
