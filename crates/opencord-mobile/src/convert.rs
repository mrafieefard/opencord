//! The core's types as the records in `types.rs`.

use opencord_common::permissions::Permissions;
use opencord_core::api::types as core;

use crate::ids::id;
use crate::types::*;

fn opt_id(value: Option<i64>) -> Option<String> {
    value.map(id)
}

/// Milliseconds, sizes and positions: exact up to 2^53.
fn number(value: i64) -> f64 {
    value as f64
}

fn unsigned(value: u64) -> f64 {
    value as f64
}

pub(crate) fn server_entry(server: core::Server) -> ServerEntry {
    ServerEntry {
        key: server.key,
        name: server.name,
        host: server.host,
        port: server.port,
        user_id: opt_id(server.user_id),
        fingerprint: server.fingerprint,
    }
}

pub(crate) fn add_result(outcome: core::AddServerOutcome) -> AddServerResult {
    match outcome {
        core::AddServerOutcome::Added(server) => AddServerResult::Added {
            server: server_entry(server),
        },
        core::AddServerOutcome::NeedsTrust {
            address,
            fingerprint,
        } => AddServerResult::NeedsTrust {
            address,
            fingerprint,
        },
    }
}

impl From<core::FailureReason> for FailureKind {
    fn from(reason: core::FailureReason) -> Self {
        match reason {
            core::FailureReason::Rejected => Self::Rejected,
            core::FailureReason::Kicked => Self::Kicked,
            core::FailureReason::Banned => Self::Banned,
            core::FailureReason::FingerprintChanged => Self::FingerprintChanged,
            core::FailureReason::Incompatible => Self::Incompatible,
        }
    }
}

pub(crate) fn connection(state: core::ConnectionState) -> ConnectionRecord {
    match state {
        core::ConnectionState::Connecting => ConnectionRecord::Connecting,
        core::ConnectionState::Connected => ConnectionRecord::Connected,
        core::ConnectionState::Reconnecting {
            attempt,
            retry_in_ms,
        } => ConnectionRecord::Reconnecting {
            attempt,
            retry_in_ms,
        },
        core::ConnectionState::Failed {
            reason,
            message,
            expected_fingerprint,
            presented_fingerprint,
        } => ConnectionRecord::Failed {
            reason: reason.into(),
            message,
            expected_fingerprint,
            presented_fingerprint,
        },
    }
}

pub(crate) fn user(user: core::User) -> UserRecord {
    UserRecord {
        id: id(user.id),
        display_name: user.display_name,
        avatar_hash: user.avatar_hash,
        fingerprint: user.fingerprint,
    }
}

pub(crate) fn member(member: core::Member) -> MemberRecord {
    MemberRecord {
        user: user(member.user),
        nickname: member.nickname,
        role_ids: member.role_ids.into_iter().map(id).collect(),
    }
}

pub(crate) fn channel(channel: core::Channel) -> ChannelRecord {
    ChannelRecord {
        id: id(channel.id),
        kind: match channel.kind {
            core::ChannelKind::Text => ChannelKind::Text,
            core::ChannelKind::Voice => ChannelKind::Voice,
            core::ChannelKind::Category => ChannelKind::Category,
        },
        name: channel.name,
        topic: channel.topic,
        parent_id: opt_id(channel.parent_id),
        position: channel.position,
        user_limit: channel.user_limit,
        text_in_voice: channel.text_in_voice,
    }
}

/// Permission bits as the core's API carries them (an `i64` holding the
/// `u64` flags).
fn permissions(bits: i64) -> Permissions {
    Permissions::from_bits_truncate(u64::from_ne_bytes(bits.to_ne_bytes()))
}

pub(crate) fn access(channel_id: i64, bits: i64) -> ChannelAccess {
    let granted = permissions(bits);
    let admin = granted.contains(Permissions::ADMINISTRATOR);
    let has = |flag: Permissions| admin || granted.contains(flag);
    ChannelAccess {
        channel_id: id(channel_id),
        view: has(Permissions::VIEW_CHANNEL),
        read_history: has(Permissions::READ_HISTORY),
        send: has(Permissions::SEND_MESSAGES),
        attach: has(Permissions::ATTACH_FILES),
        connect: has(Permissions::CONNECT),
        speak: has(Permissions::SPEAK),
        manage_messages: has(Permissions::MANAGE_MESSAGES),
        music_add: has(Permissions::MUSIC_ADD),
        music_control: has(Permissions::MUSIC_CONTROL),
        music_manage_queue: has(Permissions::MUSIC_MANAGE_QUEUE),
        music_manage: has(Permissions::MUSIC_MANAGE),
        use_soundboard: has(Permissions::USE_SOUNDBOARD),
        use_external_sounds: has(Permissions::USE_EXTERNAL_SOUNDS),
    }
}

pub(crate) fn access_list(channels: Vec<core::ChannelPermissions>) -> Vec<ChannelAccess> {
    channels
        .into_iter()
        .map(|entry| access(entry.channel_id, entry.permissions))
        .collect()
}

pub(crate) fn snapshot(server_key: String, ready: core::ReadySnapshot) -> ServerSnapshot {
    ServerSnapshot {
        server_key,
        self_user: user(ready.self_user),
        name: ready.server.name,
        description: ready.server.description,
        owner_id: opt_id(ready.server.owner_id),
        icon_hash: ready.server.icon_hash,
        channels: ready.channels.into_iter().map(channel).collect(),
        access: access_list(ready.channel_permissions),
        members: ready.members.into_iter().map(member).collect(),
        voice_states: ready.voice_states.into_iter().map(voice_state).collect(),
        voice_enabled: ready.voice_enabled,
        soundboard: soundboard_settings(&ready.voice_settings),
        soundboard_sounds: ready
            .soundboard_sounds
            .into_iter()
            .map(soundboard_sound)
            .collect(),
    }
}

pub(crate) fn soundboard_settings(settings: &core::VoiceSettings) -> SoundboardSettingsRecord {
    SoundboardSettingsRecord {
        enabled: settings.soundboard_enabled,
        allow_default_sounds: settings.allow_default_sounds,
        allow_external_sounds: settings.allow_external_sounds,
        cooldown_ms: settings.sound_cooldown_s.saturating_mul(1000),
    }
}

pub(crate) fn soundboard_sound(sound: core::SoundboardSound) -> SoundboardSoundRecord {
    SoundboardSoundRecord {
        id: id(sound.id),
        name: sound.name,
        emoji: sound.emoji,
        volume: sound.volume,
        sha256: sound.sha256,
        size_bytes: sound.size_bytes,
        duration_ms: sound.duration_ms,
        uploader_id: opt_id(sound.uploader_id),
        created_at_ms: number(sound.created_at_ms),
    }
}

/// Back to the core's, for playing another server's sound here.
pub(crate) fn core_soundboard_sound(
    sound: SoundboardSoundRecord,
) -> Result<core::SoundboardSound, crate::MobileError> {
    Ok(core::SoundboardSound {
        id: crate::parse_id(&sound.id)?,
        name: sound.name,
        emoji: sound.emoji,
        volume: sound.volume,
        sha256: sound.sha256,
        size_bytes: sound.size_bytes,
        duration_ms: sound.duration_ms,
        uploader_id: sound
            .uploader_id
            .as_deref()
            .map(crate::parse_id)
            .transpose()?,
        created_at_ms: sound.created_at_ms as i64,
    })
}

pub(crate) fn sound_effect(effect: core::SoundEffect) -> SoundEffectRecord {
    SoundEffectRecord {
        channel_id: id(effect.channel_id),
        user_id: id(effect.user_id),
        sound_ref: effect.sound_ref,
        name: effect.name,
        emoji: effect.emoji,
        started_at_ms: number(effect.started_at_ms),
        duration_ms: effect.duration_ms,
    }
}

pub(crate) fn message(message: core::Message) -> MessageRecord {
    MessageRecord {
        id: id(message.id),
        channel_id: id(message.channel_id),
        author_id: id(message.author_id),
        content: message.content,
        created_at_ms: number(message.created_at_ms),
        edited_at_ms: message.edited_at_ms.map(number),
        nonce: message.nonce,
        command: message.command,
        attachments: message.attachments.into_iter().map(attachment).collect(),
    }
}

fn attachment(attachment: core::Attachment) -> AttachmentRecord {
    AttachmentRecord {
        id: id(attachment.id),
        filename: attachment.filename,
        content_type: attachment.content_type,
        size_bytes: unsigned(attachment.size_bytes),
        kind: match attachment.kind {
            core::AttachmentKind::File => AttachmentKind::File,
            core::AttachmentKind::Image => AttachmentKind::Image,
            core::AttachmentKind::Video => AttachmentKind::Video,
            core::AttachmentKind::Audio => AttachmentKind::Audio,
            core::AttachmentKind::VoiceMessage => AttachmentKind::VoiceMessage,
        },
        width: attachment.width,
        height: attachment.height,
        duration_ms: attachment.duration_ms,
        has_thumbnail: attachment.has_thumbnail,
        thumbnail_width: attachment.thumbnail_width,
        thumbnail_height: attachment.thumbnail_height,
        animated: attachment.animated,
    }
}

pub(crate) fn variant(variant: AttachmentVariant) -> core::AttachmentVariant {
    match variant {
        AttachmentVariant::Original => core::AttachmentVariant::Original,
        AttachmentVariant::Thumbnail => core::AttachmentVariant::Thumbnail,
    }
}

pub(crate) fn voice_state(state: core::VoiceState) -> VoiceStateRecord {
    VoiceStateRecord {
        user_id: id(state.user_id),
        channel_id: opt_id(state.channel_id),
        this_device: state.this_device,
        self_mute: state.self_mute,
        self_deaf: state.self_deaf,
        server_mute: state.server_mute,
        server_deaf: state.server_deaf,
    }
}

pub(crate) fn voice_link(state: core::VoiceConnectionState) -> VoiceLinkState {
    match state {
        core::VoiceConnectionState::AwaitingEndpoint => VoiceLinkState::AwaitingEndpoint,
        core::VoiceConnectionState::Authenticating => VoiceLinkState::Authenticating,
        core::VoiceConnectionState::RtcConnecting => VoiceLinkState::RtcConnecting,
        core::VoiceConnectionState::Connected => VoiceLinkState::Connected,
        core::VoiceConnectionState::Reconnecting => VoiceLinkState::Reconnecting,
        core::VoiceConnectionState::NoRoute => VoiceLinkState::NoRoute,
        core::VoiceConnectionState::Disconnected { reason } => {
            VoiceLinkState::Disconnected { reason }
        }
    }
}

pub(crate) fn speaking(change: core::SpeakingChange) -> SpeakingChangeRecord {
    SpeakingChangeRecord {
        user_id: id(change.user_id),
        speaking: change.speaking,
    }
}

pub(crate) fn upload_phase(phase: core::UploadPhase) -> UploadPhaseRecord {
    match phase {
        core::UploadPhase::Preparing => UploadPhaseRecord::Preparing,
        core::UploadPhase::Uploading => UploadPhaseRecord::Uploading,
        core::UploadPhase::Processing => UploadPhaseRecord::Processing,
        core::UploadPhase::Sending => UploadPhaseRecord::Sending,
        core::UploadPhase::Done => UploadPhaseRecord::Done,
        core::UploadPhase::Failed { message } => UploadPhaseRecord::Failed { message },
        core::UploadPhase::Cancelled => UploadPhaseRecord::Cancelled,
    }
}

pub(crate) fn outgoing_file(file: OutgoingFileRecord) -> core::OutgoingFile {
    core::OutgoingFile {
        path: Some(file.path),
        bytes: None,
        filename: file.filename,
        staged_id: None,
    }
}

fn track(track: core::MusicTrack) -> MusicTrackRecord {
    MusicTrackRecord {
        title: track.title,
        artist: track.artist,
        duration_ms: unsigned(track.duration_ms),
        is_stream: track.is_stream,
        is_seekable: track.is_seekable,
        source: track.source,
        source_display_name: track.source_display_name,
        uri: track.uri,
        artwork_id: track.artwork_id,
        local_song_id: (track.local_song_id != 0).then(|| id(track.local_song_id)),
        removable: track.removable,
    }
}

fn queue_item(item: core::MusicQueueItem) -> MusicQueueItemRecord {
    MusicQueueItemRecord {
        item_id: item.item_id,
        track: track(item.track),
        requester_id: id(item.requester_id),
        added_at_ms: number(item.added_at_ms),
    }
}

pub(crate) fn repeat(repeat: core::MusicRepeat) -> MusicRepeat {
    match repeat {
        core::MusicRepeat::Off => MusicRepeat::Off,
        core::MusicRepeat::Queue => MusicRepeat::Queue,
        core::MusicRepeat::Track => MusicRepeat::Track,
    }
}

pub(crate) fn core_repeat(repeat: MusicRepeat) -> core::MusicRepeat {
    match repeat {
        MusicRepeat::Off => core::MusicRepeat::Off,
        MusicRepeat::Queue => core::MusicRepeat::Queue,
        MusicRepeat::Track => core::MusicRepeat::Track,
    }
}

pub(crate) fn placement(placement: MusicPlacement) -> core::MusicPlacement {
    match placement {
        MusicPlacement::End => core::MusicPlacement::End,
        MusicPlacement::Next => core::MusicPlacement::Next,
        MusicPlacement::Now => core::MusicPlacement::Now,
    }
}

pub(crate) fn action(action: MusicAction) -> core::MusicAction {
    match action {
        MusicAction::Pause => core::MusicAction::Pause,
        MusicAction::Resume => core::MusicAction::Resume,
        MusicAction::Next => core::MusicAction::Next,
        MusicAction::Previous => core::MusicAction::Previous,
        MusicAction::Seek => core::MusicAction::Seek,
        MusicAction::Stop => core::MusicAction::Stop,
    }
}

pub(crate) fn session(session: core::MusicSession) -> MusicSessionRecord {
    MusicSessionRecord {
        channel_id: id(session.channel_id),
        version: unsigned(session.version),
        state: match session.state {
            core::MusicPlayState::Idle => MusicPlayState::Idle,
            core::MusicPlayState::Loading => MusicPlayState::Loading,
            core::MusicPlayState::Playing => MusicPlayState::Playing,
            core::MusicPlayState::Paused => MusicPlayState::Paused,
        },
        now_playing: session.now_playing.map(queue_item),
        queue: session.queue.into_iter().map(queue_item).collect(),
        history: session.history.into_iter().map(queue_item).collect(),
        position_ms: unsigned(session.position_ms),
        anchor_ms: number(session.anchor_ms),
        repeat: repeat(session.repeat),
        shuffle: session.shuffle,
        waiting_for_node: session.waiting_for_node,
        paused_empty: session.paused_empty,
        skip_votes: session.skip_votes.into_iter().map(id).collect(),
        skip_votes_needed: session.skip_votes_needed,
    }
}

pub(crate) fn now_playing(now: core::MusicNowPlaying) -> MusicNowPlayingRecord {
    MusicNowPlayingRecord {
        channel_id: id(now.channel_id),
        title: now.title,
        artist: now.artist,
        artwork_id: now.artwork_id,
        playing: now.playing,
        ended: now.ended,
        source_display_name: now.source_display_name,
    }
}

pub(crate) fn music_settings(settings: core::MusicSettings) -> MusicSettingsRecord {
    MusicSettingsRecord {
        supported: settings.supported,
        enabled: settings.enabled,
        configured: settings.configured,
        has_nodes: settings.has_nodes,
        local_songs: settings.local_songs,
        available_sources: settings.available_sources,
        default_source: settings.default_source,
        allow_links: settings.allow_links,
        vote_skip: settings.vote_skip,
    }
}

pub(crate) fn search_results(results: core::MusicSearchResults) -> MusicSearchResultsRecord {
    MusicSearchResultsRecord {
        results: results
            .results
            .into_iter()
            .map(|result| MusicSearchResultRecord {
                result_id: result.result_id,
                track: track(result.track),
            })
            .collect(),
        playlist: results.playlist.map(|playlist| MusicPlaylistRecord {
            name: playlist.name,
            result_ids: playlist.result_ids,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(flags: Permissions) -> i64 {
        i64::from_ne_bytes(flags.bits().to_ne_bytes())
    }

    #[test]
    fn access_follows_the_permission_bits() {
        let view = access(1, bits(Permissions::VIEW_CHANNEL));
        assert!(view.view);
        assert!(!view.read_history && !view.send && !view.connect && !view.music_add);

        let chat = access(
            1,
            bits(
                Permissions::VIEW_CHANNEL
                    | Permissions::READ_HISTORY
                    | Permissions::SEND_MESSAGES
                    | Permissions::ATTACH_FILES,
            ),
        );
        assert!(chat.view && chat.read_history && chat.send && chat.attach);
        assert!(!chat.connect && !chat.manage_messages);

        let music = access(
            1,
            bits(
                Permissions::MUSIC_ADD
                    | Permissions::MUSIC_CONTROL
                    | Permissions::MUSIC_MANAGE_QUEUE
                    | Permissions::MUSIC_MANAGE,
            ),
        );
        assert!(
            music.music_add
                && music.music_control
                && music.music_manage_queue
                && music.music_manage
        );
        assert!(!music.view);

        let admin = access(7, bits(Permissions::ADMINISTRATOR));
        assert_eq!(admin.channel_id, "7");
        assert!(
            admin.view
                && admin.read_history
                && admin.send
                && admin.attach
                && admin.connect
                && admin.speak
                && admin.manage_messages
                && admin.music_add
                && admin.music_control
                && admin.music_manage_queue
                && admin.music_manage
                && admin.use_soundboard
                && admin.use_external_sounds
        );

        let sounds = access(1, bits(Permissions::USE_SOUNDBOARD));
        assert!(sounds.use_soundboard && !sounds.use_external_sounds);
    }

    #[test]
    fn a_sound_crosses_and_comes_back_whole() {
        let sound = core::SoundboardSound {
            id: 9_007_199_254_740_993,
            name: "Airhorn".to_owned(),
            emoji: Some("📯".to_owned()),
            volume: 80,
            sha256: "ab".repeat(32),
            size_bytes: 12_345,
            duration_ms: 1_500,
            uploader_id: Some(42),
            created_at_ms: 1_761_500_000_123,
        };
        let record = soundboard_sound(sound.clone());
        assert_eq!(record.id, "9007199254740993");
        assert_eq!(record.uploader_id.as_deref(), Some("42"));
        assert_eq!(core_soundboard_sound(record).unwrap(), sound);
    }

    #[test]
    fn the_cooldown_crosses_in_milliseconds() {
        let settings = core::VoiceSettings {
            screen_share_max_resolution: core::ScreenShareResolution::P1080,
            screen_share_max_fps: 30,
            max_stream_viewers: 10,
            camera_allowed: true,
            max_camera_participants: 10,
            max_voice_bitrate: 64_000,
            afk_channel_id: None,
            afk_timeout_s: 300,
            soundboard_enabled: true,
            allow_default_sounds: false,
            allow_external_sounds: true,
            sound_cooldown_s: 3,
            max_sounds: 48,
        };
        assert_eq!(
            soundboard_settings(&settings),
            SoundboardSettingsRecord {
                enabled: true,
                allow_default_sounds: false,
                allow_external_sounds: true,
                cooldown_ms: 3_000,
            }
        );
    }

    fn core_track(local_song_id: i64) -> core::MusicTrack {
        core::MusicTrack {
            title: "t".to_owned(),
            artist: "a".to_owned(),
            duration_ms: 1000,
            is_stream: false,
            is_seekable: true,
            source: "local".to_owned(),
            source_display_name: "Local".to_owned(),
            uri: String::new(),
            artwork_id: String::new(),
            local_song_id,
            removable: false,
        }
    }

    #[test]
    fn numbers_and_ids_cross_as_planned() {
        let converted = message(core::Message {
            id: i64::MAX,
            channel_id: 10,
            author_id: 1001,
            content: "hi".to_owned(),
            created_at_ms: 1_761_500_000_123,
            edited_at_ms: None,
            nonce: Some("n".to_owned()),
            command: None,
            attachments: vec![core::Attachment {
                id: 5,
                filename: "big.bin".to_owned(),
                content_type: "application/octet-stream".to_owned(),
                size_bytes: 3_000_000_000,
                kind: core::AttachmentKind::File,
                width: None,
                height: None,
                duration_ms: None,
                waveform: Vec::new(),
                has_thumbnail: false,
                thumbnail_width: None,
                thumbnail_height: None,
                animated: false,
            }],
        });
        assert_eq!(converted.id, "9223372036854775807");
        assert_eq!(converted.channel_id, "10");
        assert_eq!(converted.created_at_ms, 1_761_500_000_123.0);
        assert_eq!(converted.attachments[0].size_bytes, 3e9);

        assert_eq!(track(core_track(0)).local_song_id, None);
        assert_eq!(track(core_track(7)).local_song_id, Some("7".to_owned()));
    }
}
