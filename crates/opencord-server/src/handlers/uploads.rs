//! Gateway handlers for media uploads.

use opencord_common::blob::BlobKey;
use opencord_common::permissions::Permissions;
use opencord_common::upload::{
    self, MAX_UNFINISHED_PER_USER, PIECE_BYTES, TEMP_HEADROOM_BYTES, VOICE_FILENAME,
};
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::response::Result as Response;

use crate::attachments::settings::UploadSettings;
use crate::attachments::temp;
use crate::attachments::view_uploads::ViewUpload;
use crate::db::attachments::{self, AttachmentRow};
use crate::error::ApiError;
use crate::guild::Guild;
use crate::handlers::Ctx;
use crate::handlers::messages::require_message_channel;
use crate::permissions::{require_base, require_channel};
use crate::plugins::permissions::PluginPermission;
use crate::state::{AppState, Audience, now_ms};
use sqlx::SqliteConnection;

pub async fn create(ctx: &Ctx<'_>, request: proto::CreateUpload) -> Result<Response, ApiError> {
    if let Some(view_id) = request.view_id {
        return view_upload(ctx, &request, view_id).await;
    }
    // 1. kind is FILE, VOICE_MESSAGE or MUSIC [INVALID_ARGUMENT]
    let kind = match proto::AttachmentKind::try_from(request.kind) {
        Ok(proto::AttachmentKind::File) => proto::AttachmentKind::File,
        Ok(proto::AttachmentKind::VoiceMessage) => proto::AttachmentKind::VoiceMessage,
        Ok(proto::AttachmentKind::Music) => {
            let filename = music_upload(ctx, &request)?;
            return start(ctx, &request, "music", filename)
                .await
                .map(Response::Upload);
        }
        _ => return Err(ApiError::invalid_argument("invalid attachment kind")),
    };

    // 2. require_channel(VIEW_CHANNEL | SEND_MESSAGES) plus the text-or-voice-with-chat rule
    require_message_channel(
        ctx,
        request.channel_id,
        Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
    )?;

    // 4. filename = sanitize_filename(..); for VOICE the filename is always voice-message.ogg
    let filename = match kind {
        proto::AttachmentKind::File => upload::sanitize_filename(&request.filename),
        proto::AttachmentKind::VoiceMessage => VOICE_FILENAME.to_owned(),
        _ => unreachable!(),
    };

    // 3. FILE needs ATTACH_FILES; VOICE needs SEND_VOICE_MESSAGES [FORBIDDEN]
    {
        let guild = ctx.state.guild();
        let needed_perm = match kind {
            proto::AttachmentKind::File => Permissions::ATTACH_FILES,
            proto::AttachmentKind::VoiceMessage => Permissions::SEND_VOICE_MESSAGES,
            _ => unreachable!(),
        };
        require_channel(&guild, ctx.user_id, request.channel_id, needed_perm)?;

        // 5. FILE: blocked extension [FILE_TYPE_BLOCKED]
        // 6. FILE: size_bytes > guild.upload_limit(user, ceiling) [FILE_TOO_LARGE];
        //    VOICE: size_bytes > voice_max_bytes(max_seconds) [VOICE_MESSAGE_TOO_LONG]
        match kind {
            proto::AttachmentKind::File => {
                check_file(ctx, &guild, &filename, request.size_bytes)?;
            }
            proto::AttachmentKind::VoiceMessage => {
                let limit = upload::voice_max_bytes(upload::VOICE_MAX_SECONDS);
                if request.size_bytes > limit {
                    return Err(ApiError::voice_message_too_long(upload::VOICE_MAX_SECONDS));
                }
            }
            _ => unreachable!(),
        }
    }

    // 7. size_bytes == 0 is allowed for FILE only
    if kind != proto::AttachmentKind::File && request.size_bytes == 0 {
        return Err(ApiError::invalid_argument("voice messages cannot be empty"));
    }

    let kind_str = match kind {
        proto::AttachmentKind::VoiceMessage => "voice",
        _ => "file",
    };
    start(ctx, &request, kind_str, filename)
        .await
        .map(Response::Upload)
}

/// A file's extension isn't blocked [FILE_TYPE_BLOCKED] and its size is
/// within the member's limit [FILE_TOO_LARGE].
pub(crate) fn check_file_for(
    guild: &Guild,
    user_id: i64,
    ceiling: u64,
    filename: &str,
    size: u64,
) -> Result<(), ApiError> {
    let blocked = &guild.upload_settings.blocked_extensions;
    if let Some(entry) = blocked
        .iter()
        .find(|entry| upload::is_blocked(filename, std::slice::from_ref(*entry)))
    {
        return Err(ApiError::file_type_blocked(entry));
    }
    let limit = guild.upload_limit(user_id, ceiling);
    if size > limit {
        return Err(ApiError::file_too_large(limit));
    }
    Ok(())
}

fn check_file(ctx: &Ctx<'_>, guild: &Guild, filename: &str, size: u64) -> Result<(), ApiError> {
    check_file_for(
        guild,
        ctx.user_id,
        ctx.state.attachments.max_file_bytes(),
        filename,
        size,
    )
}

/// A file for a plugin view's picker (plan M2 Task 12, design 2026-11-07
/// §13.3): the session's own open view, of a plugin that takes uploads, in
/// a channel where the member may attach files; the file's checks are a
/// message file's. The upload is the member's, with channel id 0, and its
/// view is kept in `view_uploads`.
async fn view_upload(
    ctx: &Ctx<'_>,
    request: &proto::CreateUpload,
    view_id: i64,
) -> Result<Response, ApiError> {
    if request.channel_id != 0 {
        return Err(ApiError::invalid_argument(
            "an upload is for a channel or a view, not both",
        ));
    }
    if request.kind != proto::AttachmentKind::File as i32 {
        return Err(ApiError::invalid_argument("a view takes files only"));
    }
    let session = ctx.session.ok_or_else(|| ApiError::not_found("view"))?;
    let view = ctx
        .state
        .plugins
        .views
        .info(view_id)
        .filter(|view| view.session_id == session.id)
        .ok_or_else(|| ApiError::not_found("view"))?;
    let takes = ctx
        .state
        .plugins
        .get(&view.plugin_id)
        .is_some_and(|handle| handle.holds(PluginPermission::AcceptUploads));
    if !takes {
        return Err(ApiError::forbidden("this plugin doesn't take uploads"));
    }
    let filename = upload::sanitize_filename(&request.filename);
    {
        let guild = ctx.state.guild();
        require_channel(
            &guild,
            ctx.user_id,
            view.channel_id,
            Permissions::ATTACH_FILES,
        )?;
        check_file(ctx, &guild, &filename, request.size_bytes)?;
    }
    let created = start(ctx, request, "file", filename).await?;
    ctx.state.attachments.view_uploads().insert(
        created.upload_id,
        ViewUpload {
            plugin_id: view.plugin_id,
            view_id,
            user_id: ctx.user_id,
        },
    );
    Ok(Response::Upload(created))
}

/// A song for the music player (design 2026-10-24): one who may add music
/// to the voice channel and attach files there, a file the player takes,
/// within their upload limit. Returns its filename.
fn music_upload(ctx: &Ctx<'_>, request: &proto::CreateUpload) -> Result<String, ApiError> {
    super::music::may_act(
        ctx,
        request.channel_id,
        Permissions::MUSIC_ADD | Permissions::ATTACH_FILES,
    )?;
    let filename = upload::sanitize_filename(&request.filename);
    if !crate::music::local::is_song_file(&filename) {
        return Err(ApiError::invalid_argument(
            "The music player takes MP3, FLAC, M4A, AAC, Ogg and WAV files",
        ));
    }
    if request.size_bytes == 0 {
        return Err(ApiError::invalid_argument("the file is empty"));
    }
    let guild = ctx.state.guild();
    let limit = guild.upload_limit(ctx.user_id, ctx.state.attachments.max_file_bytes());
    if request.size_bytes > limit {
        return Err(ApiError::file_too_large(limit));
    }
    Ok(filename)
}

pub(crate) struct NewUpload {
    pub uploader_id: i64,
    pub channel_id: i64,
    pub kind: &'static str,
    pub filename: String,
    pub size_bytes: u64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
}

pub(crate) async fn insert_new(
    state: &AppState,
    conn: &mut SqliteConnection,
    new: NewUpload,
) -> Result<AttachmentRow, ApiError> {
    // 10. fs4::available_space(temp_dir) < size + 512 MiB [STORAGE_FULL]
    let temp_dir = state.attachments.temp_dir();
    tokio::fs::create_dir_all(temp_dir).await.map_err(|e| {
        tracing::error!(?e, "failed to create temp dir");
        ApiError::internal()
    })?;
    let required = new.size_bytes.saturating_add(TEMP_HEADROOM_BYTES);
    let available = fs4::available_space(temp_dir).map_err(|e| {
        tracing::warn!(?e, "failed to check available space");
        ApiError::storage_full()
    })?;
    if available < required {
        return Err(ApiError::storage_full());
    }

    // Generate ID, random key, seal key
    let id = state.ids.next_id();
    let key = BlobKey::random();
    let key_enc = state
        .certificates
        .secrets()
        .seal(&crate::attachments::key_label(id), key.as_bytes());

    let temp_path = state.attachments.temp_path(id);
    temp::create(&temp_path).await.map_err(|e| {
        tracing::error!(?e, "failed to create temp file");
        ApiError::internal()
    })?;

    let active_storage_id = state.attachments.active_id();
    let now = now_ms();

    let row = AttachmentRow {
        id,
        uploader_id: new.uploader_id,
        channel_id: new.channel_id,
        message_id: None,
        position: 0,
        state: "uploading".to_owned(),
        kind: new.kind.to_owned(),
        filename: new.filename,
        content_type: "application/octet-stream".to_owned(),
        size_bytes: new.size_bytes as i64,
        received_bytes: 0,
        storage_id: active_storage_id,
        key_enc,
        width: new.width,
        height: new.height,
        duration_ms: new.duration_ms,
        animated: false,
        waveform: None,
        has_thumbnail: false,
        thumbnail_width: None,
        thumbnail_height: None,
        error_code: None,
        error_message: None,
        created_at: now,
        updated_at: now,
    };

    attachments::insert_upload(conn, &row).await?;
    Ok(row)
}

/// Checks what every upload needs, then makes its row and temp file.
async fn start(
    ctx: &Ctx<'_>,
    request: &proto::CreateUpload,
    kind_str: &'static str,
    filename: String,
) -> Result<proto::Upload, ApiError> {
    // 8. rate_limits.check_upload(user) [RATE_LIMITED]
    ctx.state
        .rate_limits
        .check_upload(ctx.user_id)
        .map_err(ApiError::rate_limited)?;

    // 9. count_unfinished >= 10 [CONFLICT "finish or cancel your other uploads first"]
    let mut conn = ctx.state.db.acquire().await?;
    let unfinished = attachments::count_unfinished(&mut conn, ctx.user_id).await?;
    if unfinished >= MAX_UNFINISHED_PER_USER as i64 {
        return Err(ApiError::conflict(
            "finish or cancel your other uploads first",
        ));
    }

    let width = request.width.map(|w| (w as i64).clamp(1, 16_384));
    let height = request.height.map(|h| (h as i64).clamp(1, 16_384));
    let duration_ms = request.duration_ms.map(|d| (d as i64).clamp(1, 86_400_000));

    let row = insert_new(
        ctx.state,
        &mut conn,
        NewUpload {
            uploader_id: ctx.user_id,
            channel_id: request.channel_id,
            kind: kind_str,
            filename,
            size_bytes: request.size_bytes,
            width,
            height,
            duration_ms,
        },
    )
    .await?;

    Ok(proto::Upload {
        upload_id: row.id,
        piece_bytes: PIECE_BYTES,
    })
}

pub async fn cancel(ctx: &Ctx<'_>, request: proto::CancelUpload) -> Result<Response, ApiError> {
    let mut conn = ctx.state.db.acquire().await?;
    let row = attachments::find(&mut conn, request.upload_id)
        .await?
        .ok_or_else(|| ApiError::not_found("upload"))?;
    if row.uploader_id != ctx.user_id {
        return Err(ApiError::not_found("upload"));
    }
    match row.state.as_str() {
        "uploading" | "processing" | "ready" | "failed" => {
            attachments::delete(&mut conn, row.id).await?;
            ctx.state.attachments.view_uploads().forget(row.id);
            let temp_path = ctx.state.attachments.temp_path(row.id);
            let _ = tokio::fs::remove_file(temp_path).await;
            Ok(Response::Ack(proto::Ack {}))
        }
        _ => Err(ApiError::not_found("upload")),
    }
}

pub async fn update_settings(
    ctx: &Ctx<'_>,
    request: proto::UpdateUploadSettings,
) -> Result<Response, ApiError> {
    let _writes = ctx.state.write_lock().await;
    let current = {
        let guild = ctx.state.guild();
        require_base(&guild, ctx.user_id, Permissions::MANAGE_SERVER)?;
        guild.upload_settings.clone()
    };
    let blocked_extensions = match &request.blocked_extensions {
        Some(list) => {
            if let Some(bad) = list
                .extensions
                .iter()
                .find(|entry| upload::normalize_extension(entry).is_err())
            {
                let shown: String = bad.trim().chars().take(30).collect();
                return Err(ApiError::invalid_argument(format!(
                    "\"{shown}\" isn't a file extension"
                )));
            }
            upload::normalize_blocked_list(&list.extensions)?
        }
        None => current.blocked_extensions,
    };
    let updated = UploadSettings { blocked_extensions };
    updated.save(&mut *ctx.state.db.acquire().await?).await?;
    ctx.state.guild_mut().upload_settings = updated.clone();
    let settings = updated.to_proto(ctx.state.attachments.max_file_bytes());
    ctx.state.broadcast(
        Event::UploadSettingsUpdate(proto::UploadSettingsUpdate {
            settings: Some(settings.clone()),
        }),
        Audience::Everyone,
    );
    Ok(Response::UploadSettings(settings))
}
