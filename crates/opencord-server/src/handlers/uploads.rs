//! Gateway handlers for media uploads.

use opencord_common::blob::BlobKey;
use opencord_common::permissions::Permissions;
use opencord_common::upload::{
    self, MAX_UNFINISHED_PER_USER, PIECE_BYTES, TEMP_HEADROOM_BYTES, VOICE_FILENAME,
};
use opencord_proto::v1 as proto;
use proto::response::Result as Response;

use crate::attachments::temp;
use crate::db::attachments::{self, AttachmentRow};
use crate::error::ApiError;
use crate::handlers::Ctx;
use crate::handlers::messages::require_message_channel;
use crate::permissions::require_channel;
use crate::state::now_ms;

pub async fn create(ctx: &Ctx<'_>, request: proto::CreateUpload) -> Result<Response, ApiError> {
    // 1. kind is FILE or VOICE_MESSAGE [INVALID_ARGUMENT]
    let kind = match proto::AttachmentKind::try_from(request.kind) {
        Ok(proto::AttachmentKind::File) => proto::AttachmentKind::File,
        Ok(proto::AttachmentKind::VoiceMessage) => proto::AttachmentKind::VoiceMessage,
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
        if kind == proto::AttachmentKind::File
            && upload::is_blocked(&filename, &guild.upload_settings.blocked_extensions)
        {
            let ext = filename.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
            return Err(ApiError::file_type_blocked(ext));
        }

        // 6. FILE: size_bytes > guild.upload_limit(user, ceiling) [FILE_TOO_LARGE];
        //    VOICE: size_bytes > voice_max_bytes(max_seconds) [VOICE_MESSAGE_TOO_LONG]
        match kind {
            proto::AttachmentKind::File => {
                let limit = guild.upload_limit(ctx.user_id, ctx.state.attachments.max_file_bytes());
                if request.size_bytes > limit {
                    return Err(ApiError::file_too_large(limit));
                }
            }
            proto::AttachmentKind::VoiceMessage => {
                let max_seconds = guild.upload_settings.voice_message_max_seconds;
                let limit = upload::voice_max_bytes(max_seconds);
                if request.size_bytes > limit {
                    return Err(ApiError::voice_message_too_long(max_seconds));
                }
            }
            _ => unreachable!(),
        }
    }

    // 7. size_bytes == 0 is allowed for FILE only
    if kind != proto::AttachmentKind::File && request.size_bytes == 0 {
        return Err(ApiError::invalid_argument("voice messages cannot be empty"));
    }

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

    // 10. fs4::available_space(temp_dir) < size + 512 MiB [STORAGE_FULL]
    let temp_dir = ctx.state.attachments.temp_dir();
    tokio::fs::create_dir_all(temp_dir).await.map_err(|e| {
        tracing::error!(?e, "failed to create temp dir");
        ApiError::internal()
    })?;
    let required = request.size_bytes.saturating_add(TEMP_HEADROOM_BYTES);
    let available = fs4::available_space(temp_dir).map_err(|e| {
        tracing::warn!(?e, "failed to check available space");
        ApiError::storage_full()
    })?;
    if available < required {
        return Err(ApiError::storage_full());
    }

    // Generate ID, random key, seal key
    let id = ctx.state.ids.next_id();
    let key = BlobKey::random();
    let key_enc = ctx
        .state
        .certificates
        .secrets()
        .seal(&crate::attachments::key_label(id), key.as_bytes());

    let temp_path = ctx.state.attachments.temp_path(id);
    temp::create(&temp_path).await.map_err(|e| {
        tracing::error!(?e, "failed to create temp file");
        ApiError::internal()
    })?;

    let (active_storage_id, _) = ctx.state.attachments.active();
    let kind_str = match kind {
        proto::AttachmentKind::File => "file",
        proto::AttachmentKind::VoiceMessage => "voice",
        _ => "file",
    };
    let width = request.width.map(|w| (w as i64).clamp(1, 16_384));
    let height = request.height.map(|h| (h as i64).clamp(1, 16_384));
    let duration_ms = request.duration_ms.map(|d| (d as i64).clamp(1, 86_400_000));
    let now = now_ms();

    let row = AttachmentRow {
        id,
        uploader_id: ctx.user_id,
        channel_id: request.channel_id,
        message_id: None,
        position: 0,
        state: "uploading".to_owned(),
        kind: kind_str.to_owned(),
        filename,
        content_type: "application/octet-stream".to_owned(),
        size_bytes: request.size_bytes as i64,
        received_bytes: 0,
        storage_id: active_storage_id,
        key_enc,
        width,
        height,
        duration_ms,
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

    attachments::insert_upload(&mut conn, &row).await?;

    Ok(Response::Upload(proto::Upload {
        upload_id: id,
        piece_bytes: PIECE_BYTES,
    }))
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
            let temp_path = ctx.state.attachments.temp_path(row.id);
            let _ = tokio::fs::remove_file(temp_path).await;
            Ok(Response::Ack(proto::Ack {}))
        }
        _ => Err(ApiError::not_found("upload")),
    }
}
