use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use crate::attachments::{
    Backend, S3Settings, S3Storage, manage, s3_settings, storage_secret_label,
};
use crate::db;
use crate::db::storage::StorageRow;
use crate::error::ApiError;
use crate::handlers::Ctx;
use crate::secrets::Secrets;
use crate::state::{AppState, now_ms};

fn require_owner(ctx: &Ctx<'_>) -> Result<(), ApiError> {
    if ctx.state.guild().is_owner(ctx.user_id) {
        Ok(())
    } else {
        Err(ApiError::forbidden("only the owner manages storage"))
    }
}

/// A saved S3 row's settings. This fails only if the secrets file changed.
fn saved_settings(ctx: &Ctx<'_>, row: &StorageRow) -> Result<S3Settings, ApiError> {
    s3_settings(row, ctx.state.certificates.secrets()).map_err(|error| {
        tracing::error!(storage_id = row.id, %error, "a storage's settings couldn't be read");
        ApiError::internal()
    })
}

/// Builds the bucket and runs its test. A failed step is STORAGE_UNAVAILABLE.
async fn tested(ctx: &Ctx<'_>, settings: &S3Settings) -> Result<S3Storage, ApiError> {
    let storage = S3Storage::new(settings, ctx.state.attachments.http().clone())
        .map_err(ApiError::invalid_argument)?;
    let test = storage.test(ctx.state.attachments.temp_dir()).await;
    match manage::test_failure(&test) {
        Some(failure) => Err(ApiError::storage_unavailable(failure)),
        None => Ok(storage),
    }
}

/// `row` with the bucket's settings and its secret sealed.
fn with_settings(row: StorageRow, settings: &S3Settings, secrets: &Secrets) -> StorageRow {
    let secret_enc = secrets.seal(
        &storage_secret_label(row.id),
        settings.secret_access_key.as_bytes(),
    );
    StorageRow {
        endpoint: Some(settings.endpoint.clone()),
        region: Some(settings.region.clone()),
        bucket: Some(settings.bucket.clone()),
        key_prefix: settings.key_prefix.clone(),
        path_style: settings.path_style,
        allow_http: settings.allow_http,
        access_key_id: Some(settings.access_key_id.clone()),
        secret_enc: Some(secret_enc),
        last_error: None,
        ..row
    }
}

/// Files already at the old place would be lost by moving `row` there:
/// stored, queued for deletion, or being written by an upload or a move.
async fn check_place_change(
    ctx: &Ctx<'_>,
    row: &StorageRow,
    settings: &S3Settings,
) -> Result<(), ApiError> {
    let same_place = row.endpoint.as_deref() == Some(&settings.endpoint)
        && row.bucket.as_deref() == Some(&settings.bucket)
        && row.key_prefix == settings.key_prefix;
    if same_place {
        return Ok(());
    }
    if ctx.state.attachments.mover().is_running() {
        return Err(ApiError::conflict(
            "wait for the move to finish, or stop it, first",
        ));
    }
    let mut conn = ctx.state.db.acquire().await?;
    let count_on = db::attachments::count_on(&mut conn, row.id).await?;
    let count_blobs = db::storage::count_blobs(&mut conn, row.id).await?;
    if count_on + count_blobs > 0 {
        return Err(ApiError::conflict(
            "this storage holds files; add a new storage and move them there instead",
        ));
    }
    Ok(())
}

async fn answer_storage(state: &AppState) -> Result<Response, ApiError> {
    Ok(Response::Storage(manage::broadcast(state).await?))
}

pub async fn get(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    Ok(Response::Storage(manage::snapshot(ctx.state).await?))
}

pub async fn add(ctx: &Ctx<'_>, request: proto::AddStorage) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let name = manage::valid_name(&request.name)?;
    let Some(config) = request.s3 else {
        return Err(ApiError::invalid_argument(
            "the bucket's settings are missing",
        ));
    };
    let settings = manage::valid_s3(&config, None)?;
    let storage = tested(ctx, &settings).await?;
    let _writes = ctx.state.write_lock().await;
    let id = ctx.state.ids.next_id();
    let row = with_settings(
        StorageRow {
            id,
            kind: "s3".into(),
            name,
            endpoint: None,
            region: None,
            bucket: None,
            key_prefix: String::new(),
            path_style: false,
            allow_http: false,
            access_key_id: None,
            secret_enc: None,
            active: false,
            last_error: None,
            created_at: now_ms(),
        },
        &settings,
        ctx.state.certificates.secrets(),
    );
    let mut conn = ctx.state.db.acquire().await?;
    db::storage::insert(&mut conn, &row).await?;
    ctx.state
        .attachments
        .set_backend(id, Backend::S3(Box::new(storage)));
    answer_storage(ctx.state).await
}

pub async fn update(ctx: &Ctx<'_>, request: proto::UpdateStorage) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let id = request.storage_id;
    let row = {
        let mut conn = ctx.state.db.acquire().await?;
        db::storage::find(&mut conn, id)
            .await?
            .ok_or_else(|| ApiError::not_found("storage"))?
    };
    let name = request
        .name
        .as_deref()
        .map(manage::valid_name)
        .transpose()?;

    let (new_settings, new_storage) = if let Some(config) = request.s3 {
        if row.kind != "s3" {
            return Err(ApiError::invalid_argument(
                "this server's own storage has no bucket settings",
            ));
        }
        let stored = if config.secret_access_key.trim().is_empty() {
            Some(saved_settings(ctx, &row)?.secret_access_key)
        } else {
            None
        };
        let settings = manage::valid_s3(&config, stored)?;
        // Checked again under the lock; this one spares a bucket test.
        check_place_change(ctx, &row, &settings).await?;
        let storage = tested(ctx, &settings).await?;
        (Some(settings), Some(storage))
    } else {
        (None, None)
    };

    let _writes = ctx.state.write_lock().await;
    let current = {
        let mut conn = ctx.state.db.acquire().await?;
        db::storage::find(&mut conn, id)
            .await?
            .ok_or_else(|| ApiError::not_found("storage"))?
    };
    if let Some(settings) = &new_settings {
        check_place_change(ctx, &current, settings).await?;
    }
    let mut updated_row = StorageRow {
        name: name.unwrap_or(current.name.clone()),
        ..current
    };
    if let Some(settings) = &new_settings {
        updated_row = with_settings(updated_row, settings, ctx.state.certificates.secrets());
    }
    {
        let mut conn = ctx.state.db.acquire().await?;
        db::storage::update(&mut conn, &updated_row).await?;
    }
    if let Some(storage) = new_storage {
        ctx.state
            .attachments
            .set_backend(id, Backend::S3(Box::new(storage)));
    }
    answer_storage(ctx.state).await
}

pub async fn test(ctx: &Ctx<'_>, request: proto::TestStorage) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let Some(target) = request.target else {
        return Err(ApiError::invalid_argument(
            "name a storage or give its settings",
        ));
    };
    match target {
        proto::test_storage::Target::S3(config) => {
            let settings = manage::valid_s3(&config, None)?;
            let storage = S3Storage::new(&settings, ctx.state.attachments.http().clone())
                .map_err(ApiError::invalid_argument)?;
            let test = storage.test(ctx.state.attachments.temp_dir()).await;
            Ok(Response::StorageTest(test))
        }
        proto::test_storage::Target::StorageId(id) => {
            let row = {
                let mut conn = ctx.state.db.acquire().await?;
                db::storage::find(&mut conn, id)
                    .await?
                    .ok_or_else(|| ApiError::not_found("storage"))?
            };
            if row.kind != "s3" {
                return Err(ApiError::invalid_argument("only S3 storage can be tested"));
            }
            let settings = saved_settings(ctx, &row)?;
            let storage = S3Storage::new(&settings, ctx.state.attachments.http().clone())
                .map_err(ApiError::invalid_argument)?;
            let test = storage.test(ctx.state.attachments.temp_dir()).await;
            {
                let _writes = ctx.state.write_lock().await;
                let mut conn = ctx.state.db.acquire().await?;
                db::storage::set_last_error(&mut conn, id, manage::test_failure(&test).as_deref())
                    .await?;
            }
            manage::broadcast(ctx.state).await?;
            Ok(Response::StorageTest(test))
        }
    }
}

pub async fn activate(
    ctx: &Ctx<'_>,
    request: proto::ActivateStorage,
) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let id = request.storage_id;
    let row = {
        let mut conn = ctx.state.db.acquire().await?;
        db::storage::find(&mut conn, id)
            .await?
            .ok_or_else(|| ApiError::not_found("storage"))?
    };
    if row.active {
        return answer_storage(ctx.state).await;
    }
    // Only a test: startup, add and update keep the loaded backend in step
    // with the row, and replacing it here could undo an update made meanwhile.
    if row.kind == "s3" {
        let settings = saved_settings(ctx, &row)?;
        let storage = S3Storage::new(&settings, ctx.state.attachments.http().clone())
            .map_err(ApiError::invalid_argument)?;
        let test = storage.test(ctx.state.attachments.temp_dir()).await;
        if let Some(failure) = manage::test_failure(&test) {
            {
                let _writes = ctx.state.write_lock().await;
                let mut conn = ctx.state.db.acquire().await?;
                db::storage::set_last_error(&mut conn, id, Some(&failure)).await?;
            }
            manage::broadcast(ctx.state).await?;
            return Err(ApiError::storage_unavailable(failure));
        }
    }

    let _writes = ctx.state.write_lock().await;
    if ctx.state.attachments.mover().is_running() {
        return Err(ApiError::conflict(
            "wait for the move to finish, or stop it, first",
        ));
    }
    {
        let mut conn = ctx.state.db.acquire().await?;
        db::storage::find(&mut conn, id)
            .await?
            .ok_or_else(|| ApiError::not_found("storage"))?;
        db::storage::set_active(&mut conn, id).await?;
    }
    ctx.state.attachments.set_active(id);
    answer_storage(ctx.state).await
}

pub async fn remove(ctx: &Ctx<'_>, request: proto::RemoveStorage) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let id = request.storage_id;
    if id == 1 {
        return Err(ApiError::invalid_argument(
            "this server's own storage can't be removed",
        ));
    }
    let _writes = ctx.state.write_lock().await;
    let mut conn = ctx.state.db.acquire().await?;
    let row = db::storage::find(&mut conn, id)
        .await?
        .ok_or_else(|| ApiError::not_found("storage"))?;
    if row.active {
        return Err(ApiError::conflict("activate another storage first"));
    }
    let count_on = db::attachments::count_on(&mut conn, id).await?;
    if count_on > 0 {
        return Err(ApiError::conflict(format!(
            "{count_on} files are still stored there; move them first"
        )));
    }
    let count_blobs = db::storage::count_blobs(&mut conn, id).await?;
    if count_blobs > 0 {
        return Err(ApiError::conflict(format!(
            "{count_blobs} files there are still waiting to be deleted; try again later"
        )));
    }
    db::storage::delete(&mut conn, id).await?;
    ctx.state.attachments.remove_backend(id);
    answer_storage(ctx.state).await
}

pub async fn start_move(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    let _writes = ctx.state.write_lock().await;
    crate::attachments::move_job::start(ctx.state).await?;
    answer_storage(ctx.state).await
}

pub async fn stop_move(ctx: &Ctx<'_>) -> Result<Response, ApiError> {
    require_owner(ctx)?;
    ctx.state.attachments.mover().stop();
    Ok(Response::Storage(manage::snapshot(ctx.state).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secret_is_sealed() {
        let secrets = Secrets::from_key(&[1; 32]);
        let settings = S3Settings {
            endpoint: "https://s3.example.com".into(),
            region: "auto".into(),
            bucket: "files".into(),
            key_prefix: "prefix/".into(),
            path_style: true,
            allow_http: false,
            access_key_id: "AKIAEXAMPLE".into(),
            secret_access_key: "very-secret-key-12345".into(),
        };
        let row = StorageRow {
            id: 2,
            kind: "s3".into(),
            name: "S3".into(),
            endpoint: None,
            region: None,
            bucket: None,
            key_prefix: String::new(),
            path_style: false,
            allow_http: false,
            access_key_id: None,
            secret_enc: None,
            active: false,
            last_error: None,
            created_at: 0,
        };
        let with = with_settings(row, &settings, &secrets);
        let secret_enc = with.secret_enc.as_ref().unwrap();
        assert!(
            !secret_enc
                .windows(settings.secret_access_key.len())
                .any(|w| w == settings.secret_access_key.as_bytes())
        );
        let unsealed = s3_settings(&with, &secrets).unwrap();
        assert_eq!(unsealed.secret_access_key, settings.secret_access_key);
    }
}
