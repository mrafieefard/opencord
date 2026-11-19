//! The core's types as the records in `types.rs`.

use opencord_common::permissions::Permissions;
use opencord_core::api::types as core;

use crate::ids::id;
use crate::private_types::*;
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
        public_key_hex: user.public_key_hex,
    }
}

pub(crate) fn member(member: core::Member) -> MemberRecord {
    MemberRecord {
        user: user(member.user),
        nickname: member.nickname,
        role_ids: member.role_ids.into_iter().map(id).collect(),
        joined_at_ms: number(member.joined_at_ms),
    }
}

/// Every channel the phone can show; `Option` for kinds it may not know
/// yet.
pub(crate) fn channel(channel: core::Channel) -> Option<ChannelRecord> {
    let plugin = match (channel.plugin_id, channel.type_key) {
        (Some(plugin_id), Some(type_key)) => Some(PluginChannelRecord {
            plugin_id,
            type_key,
            state: plugin_channel_state(
                channel
                    .plugin_state
                    .unwrap_or(core::PluginChannelState::PluginStopped),
            ),
        }),
        _ => None,
    };
    Some(ChannelRecord {
        id: id(channel.id),
        kind: match channel.kind {
            core::ChannelKind::Text => ChannelKind::Text,
            core::ChannelKind::Voice => ChannelKind::Voice,
            core::ChannelKind::Category => ChannelKind::Category,
            core::ChannelKind::Plugin => ChannelKind::Plugin,
        },
        name: channel.name,
        topic: channel.topic,
        parent_id: opt_id(channel.parent_id),
        position: channel.position,
        user_limit: channel.user_limit,
        text_in_voice: channel.text_in_voice,
        plugin,
    })
}

fn plugin_channel_state(state: core::PluginChannelState) -> PluginChannelState {
    match state {
        core::PluginChannelState::Available => PluginChannelState::Available,
        core::PluginChannelState::PluginStopped => PluginChannelState::PluginStopped,
        core::PluginChannelState::TypeGone => PluginChannelState::TypeGone,
    }
}

pub(crate) fn plugin_channel_type(kind: core::PluginChannelType) -> PluginChannelTypeRecord {
    PluginChannelTypeRecord {
        plugin_id: kind.plugin_id,
        type_key: kind.type_key,
        name: kind.name,
        description: kind.description,
        icon: kind.icon,
        plugin_name: kind.plugin_name,
    }
}

pub(crate) fn plugin_activity(activity: core::PluginActivity) -> PluginActivityRecord {
    PluginActivityRecord {
        plugin_id: activity.plugin_id,
        activity_key: activity.activity_key,
        name: activity.name,
        description: activity.description,
        icon: activity.icon,
        plugin_name: activity.plugin_name,
    }
}

pub(crate) fn activity_session(session: core::ActivitySession) -> ActivitySessionRecord {
    ActivitySessionRecord {
        id: id(session.id),
        channel_id: id(session.channel_id),
        plugin_id: session.plugin_id,
        activity_key: session.activity_key,
        name: session.name,
        icon: session.icon,
        plugin_name: session.plugin_name,
        starter_id: id(session.starter_id),
        started_at_ms: session.started_at_ms,
        viewer_ids: session.viewer_ids.into_iter().map(id).collect(),
    }
}

pub(crate) fn activity_end_reason(reason: core::ActivityEndReason) -> ActivityEndReason {
    match reason {
        core::ActivityEndReason::Ended => ActivityEndReason::Ended,
        core::ActivityEndReason::Plugin => ActivityEndReason::Plugin,
        core::ActivityEndReason::CallEmpty => ActivityEndReason::CallEmpty,
        core::ActivityEndReason::PluginStopped => ActivityEndReason::PluginStopped,
        core::ActivityEndReason::Unavailable => ActivityEndReason::Unavailable,
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
        send_voice_messages: has(Permissions::SEND_VOICE_MESSAGES),
        start_activities: has(Permissions::START_ACTIVITIES),
        manage_channel: has(Permissions::MANAGE_CHANNELS),
    }
}

pub(crate) fn access_list(channels: Vec<core::ChannelPermissions>) -> Vec<ChannelAccess> {
    channels
        .into_iter()
        .map(|entry| access(entry.channel_id, entry.permissions))
        .collect()
}

pub(crate) fn server_access(bits: i64) -> ServerAccess {
    let granted = permissions(bits);
    let admin = granted.contains(Permissions::ADMINISTRATOR);
    let has = |flag: Permissions| admin || granted.contains(flag);
    ServerAccess {
        manage_server: has(Permissions::MANAGE_SERVER),
        manage_channels: has(Permissions::MANAGE_CHANNELS),
        manage_roles: has(Permissions::MANAGE_ROLES),
        kick_members: has(Permissions::KICK_MEMBERS),
        ban_members: has(Permissions::BAN_MEMBERS),
        create_invite: has(Permissions::CREATE_INVITE),
        manage_nicknames: has(Permissions::MANAGE_NICKNAMES),
        change_nickname: has(Permissions::CHANGE_NICKNAME),
        manage_messages: has(Permissions::MANAGE_MESSAGES),
        mention_everyone: has(Permissions::MENTION_EVERYONE),
        manage_plugins: has(Permissions::MANAGE_PLUGINS),
        manage_soundboard: has(Permissions::MANAGE_SOUNDBOARD),
        music_manage: has(Permissions::MUSIC_MANAGE),
        mute_members: has(Permissions::MUTE_MEMBERS),
        deafen_members: has(Permissions::DEAFEN_MEMBERS),
        move_members: has(Permissions::MOVE_MEMBERS),
        priority_speaker: has(Permissions::PRIORITY_SPEAKER),
        video: has(Permissions::VIDEO),
        screenshare: has(Permissions::SCREENSHARE),
        administrator: granted.contains(Permissions::ADMINISTRATOR),
    }
}

pub(crate) fn role(r: core::Role) -> RoleRecord {
    RoleRecord {
        id: id(r.id),
        name: r.name,
        color: r.color,
        position: r.position,
        hoist: r.hoist,
        mentionable: r.mentionable,
        max_upload_bytes: r.max_upload_bytes.map(unsigned),
        plugin_id: r.plugin_id,
        access: server_access(r.permissions),
    }
}

pub(crate) fn presence_status(s: core::PresenceStatus) -> PresenceStatus {
    match s {
        core::PresenceStatus::Online => PresenceStatus::Online,
        core::PresenceStatus::Idle => PresenceStatus::Idle,
        core::PresenceStatus::Dnd => PresenceStatus::Dnd,
        core::PresenceStatus::Offline => PresenceStatus::Offline,
    }
}

pub(crate) fn presence_status_to_core(s: PresenceStatus) -> core::PresenceStatus {
    match s {
        PresenceStatus::Online => core::PresenceStatus::Online,
        PresenceStatus::Idle => core::PresenceStatus::Idle,
        PresenceStatus::Dnd => core::PresenceStatus::Dnd,
        PresenceStatus::Offline => core::PresenceStatus::Offline,
    }
}

pub(crate) fn presence(p: core::Presence) -> PresenceRecord {
    PresenceRecord {
        user_id: id(p.user_id),
        status: presence_status(p.status),
    }
}

pub(crate) fn snapshot(server_key: String, ready: core::ReadySnapshot) -> ServerSnapshot {
    ServerSnapshot {
        server_key,
        self_user: user(ready.self_user),
        name: ready.server.name,
        description: ready.server.description,
        owner_id: opt_id(ready.server.owner_id),
        icon_hash: ready.server.icon_hash,
        channels: ready.channels.into_iter().filter_map(channel).collect(),
        access: access_list(ready.channel_permissions),
        roles: ready.roles.into_iter().map(role).collect(),
        presences: ready.presences.into_iter().map(presence).collect(),
        server_access: server_access(ready.server_permissions),
        members: ready.members.into_iter().map(member).collect(),
        voice_states: ready.voice_states.into_iter().map(voice_state).collect(),
        voice_enabled: ready.voice_enabled,
        soundboard: soundboard_settings(&ready.voice_settings),
        soundboard_sounds: ready
            .soundboard_sounds
            .into_iter()
            .map(soundboard_sound)
            .collect(),
        voice_message_max_seconds: ready.upload_settings.voice_message_max_seconds,
        plugin_users: ready.plugin_users.into_iter().map(plugin_user).collect(),
        commands: command_list(ready.commands),
        plugin_channel_types: ready
            .plugin_channel_types
            .into_iter()
            .map(plugin_channel_type)
            .collect(),
        plugin_activities: ready
            .plugin_activities
            .into_iter()
            .map(plugin_activity)
            .collect(),
        activity_sessions: ready
            .activity_sessions
            .into_iter()
            .map(activity_session)
            .collect(),
    }
}

pub(crate) fn command_run_state(state: core::CommandRunState) -> CommandRunStateRecord {
    match state {
        core::CommandRunState::Answered => CommandRunStateRecord::Answered,
        core::CommandRunState::Thinking => CommandRunStateRecord::Thinking,
        core::CommandRunState::NoAnswer => CommandRunStateRecord::NoAnswer,
    }
}

pub(crate) fn command_choice(choice: core::CommandChoice) -> CommandChoiceRecord {
    CommandChoiceRecord {
        name: choice.name,
        value: choice.value,
        artwork_id: choice.artwork_id,
    }
}

fn command_option(option: core::CommandOptionSpec) -> CommandOptionRecord {
    CommandOptionRecord {
        name: option.name,
        description: option.description,
        kind: match option.kind {
            core::CommandOptionKind::Text => CommandOptionKindRecord::Text,
            core::CommandOptionKind::Integer => CommandOptionKindRecord::Integer,
            core::CommandOptionKind::Boolean => CommandOptionKindRecord::Boolean,
            core::CommandOptionKind::Choice => CommandOptionKindRecord::Choice,
            core::CommandOptionKind::User => CommandOptionKindRecord::User,
            core::CommandOptionKind::Channel => CommandOptionKindRecord::Channel,
        },
        required: option.required,
        autocomplete: option.autocomplete,
        choices: option.choices.into_iter().map(command_choice).collect(),
        min: number(option.min),
        max: number(option.max),
    }
}

/// The music permissions a command needs, named as in `ChannelAccess`.
fn command_needs(bits: i64) -> Vec<String> {
    let bits = Permissions::from_bits_truncate(bits as u64);
    [
        (Permissions::MUSIC_ADD, "musicAdd"),
        (Permissions::MUSIC_CONTROL, "musicControl"),
        (Permissions::MUSIC_MANAGE_QUEUE, "musicManageQueue"),
        (Permissions::MUSIC_MANAGE, "musicManage"),
    ]
    .into_iter()
    .filter(|(permission, _)| bits.contains(*permission))
    .map(|(_, name)| name.to_owned())
    .collect()
}

fn command(spec: core::CommandSpec) -> CommandRecord {
    let source = if spec.feature.starts_with("plugin:") {
        spec.feature.clone()
    } else {
        String::new()
    };
    CommandRecord {
        needs: command_needs(spec.permissions),
        name: spec.name,
        description: spec.description,
        options: spec.options.into_iter().map(command_option).collect(),
        feature: spec.feature,
        source,
        permission_summary: spec.permission_summary,
        aliases: spec.aliases,
        enabled: spec.enabled,
        channel_ids: spec.channel_ids.into_iter().map(id).collect(),
    }
}

pub(crate) fn command_list(list: core::CommandList) -> CommandListRecord {
    CommandListRecord {
        commands: list.commands.into_iter().map(command).collect(),
        results_public: list.results_public,
    }
}

pub(crate) fn ephemeral_note(message: core::EphemeralMessage) -> EphemeralNoteRecord {
    EphemeralNoteRecord {
        channel_id: id(message.channel_id),
        nonce: message.nonce,
        content: message.content,
        state: command_run_state(message.state),
    }
}

pub(crate) fn plugin_user(u: core::PluginUser) -> PluginUserRecord {
    PluginUserRecord {
        user_id: id(u.user_id),
        plugin_id: u.plugin_id,
        name: u.name,
        avatar_hash: if u.avatar_hash.is_empty() {
            None
        } else {
            Some(u.avatar_hash)
        },
    }
}

pub(crate) fn voice_draft(draft: core::VoiceDraft) -> VoiceDraftRecord {
    VoiceDraftRecord {
        draft_id: unsigned(draft.draft_id),
        duration_ms: draft.duration_ms,
        waveform: draft.waveform.into_iter().map(u16::from).collect(),
    }
}

pub(crate) fn playback_item(item: core::PlaybackItem) -> PlaybackItemRecord {
    match item {
        core::PlaybackItem::Attachment {
            server_key,
            attachment_id,
        } => PlaybackItemRecord::Attachment {
            server_key,
            attachment_id: id(attachment_id),
        },
        core::PlaybackItem::Draft { draft_id } => PlaybackItemRecord::Draft {
            draft_id: unsigned(draft_id),
        },
    }
}

/// A draft's id back from JavaScript: a whole number, not negative.
pub(crate) fn draft_id(value: f64) -> Result<u64, crate::MobileError> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= 9_007_199_254_740_991.0
    {
        Ok(value as u64)
    } else {
        Err(crate::MobileError::InvalidInput {
            message: format!("not a voice draft: {value}"),
        })
    }
}

pub(crate) fn core_playback_item(
    item: PlaybackItemRecord,
) -> Result<core::PlaybackItem, crate::MobileError> {
    Ok(match item {
        PlaybackItemRecord::Attachment {
            server_key,
            attachment_id,
        } => core::PlaybackItem::Attachment {
            server_key,
            attachment_id: crate::parse_id(&attachment_id)?,
        },
        PlaybackItemRecord::Draft { draft_id: value } => core::PlaybackItem::Draft {
            draft_id: draft_id(value)?,
        },
    })
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
        interaction: message.command.map(|command| MessageInteractionRecord {
            user_id: id(message.command_user_id.unwrap_or(message.author_id)),
            command,
            state: command_run_state(message.command_state),
        }),
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
        waveform: attachment.waveform.into_iter().map(u16::from).collect(),
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

/// The phone's three audio choices on top of the core's defaults, which
/// every phone call ran on before P1: nothing else about a call's audio
/// changes. Volumes are held to 0–2; one that isn't a number is 1.0.
pub(crate) fn core_audio_settings(settings: PhoneAudioSettings) -> core::AudioSettings {
    core::AudioSettings {
        noise_suppression: core_noise_suppression(settings.noise_suppression),
        input_volume: volume(settings.input_volume),
        output_volume: volume(settings.output_volume),
        ..core::AudioSettings::default()
    }
}

fn volume(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 2.0)
    } else {
        1.0
    }
}

pub(crate) fn core_noise_suppression(mode: NoiseSuppression) -> core::NoiseSuppressionMode {
    match mode {
        NoiseSuppression::Off => core::NoiseSuppressionMode::Off,
        NoiseSuppression::Standard => core::NoiseSuppressionMode::Standard,
        NoiseSuppression::High => core::NoiseSuppressionMode::High,
    }
}

pub(crate) fn core_cue(cue: VoiceCueKind) -> core::VoiceCue {
    match cue {
        VoiceCueKind::Join => core::VoiceCue::Join,
        VoiceCueKind::Leave => core::VoiceCue::Leave,
        VoiceCueKind::UserJoin => core::VoiceCue::UserJoin,
        VoiceCueKind::UserLeave => core::VoiceCue::UserLeave,
        VoiceCueKind::Mute => core::VoiceCue::Mute,
        VoiceCueKind::Unmute => core::VoiceCue::Unmute,
        VoiceCueKind::Deafen => core::VoiceCue::Deafen,
        VoiceCueKind::Undeafen => core::VoiceCue::Undeafen,
        VoiceCueKind::StreamStart => core::VoiceCue::StreamStart,
        VoiceCueKind::StreamStop => core::VoiceCue::StreamStop,
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

pub(crate) fn invite(invite: core::Invite) -> InviteRecord {
    InviteRecord {
        code: invite.code,
        link: invite.link,
        web_link: invite.web_link,
        domain_id: opt_id(invite.domain_id),
        created_by: opt_id(invite.created_by),
        created_at_ms: number(invite.created_at_ms),
        max_uses: invite.max_uses,
        uses: invite.uses,
        expires_at_ms: invite.expires_at_ms.map(number),
    }
}

// Private messages (phone P3). Each enum maps variant for variant, by name,
// both ways.

macro_rules! same_variants {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        impl From<core::$name> for $name {
            fn from(value: core::$name) -> Self {
                match value {
                    $(core::$name::$variant => Self::$variant,)+
                }
            }
        }

        impl From<$name> for core::$name {
            fn from(value: $name) -> Self {
                match value {
                    $($name::$variant => Self::$variant,)+
                }
            }
        }
    };
}

same_variants!(FriendStatus {
    None,
    Outgoing,
    Incoming,
    Friends
});
same_variants!(ConversationStatus {
    None,
    Request,
    Accepted
});
same_variants!(PrivateVerification {
    Unverified,
    Verified,
    Mismatch
});
same_variants!(PrivateMessageKind {
    Text,
    Divider,
    Moved,
    Undecryptable,
    KeysChanged,
    NewDevice,
    UnacceptedKeys
});
same_variants!(PrivateSendState {
    Received,
    Sending,
    Sent,
    Failed
});
same_variants!(OwnDeviceState { Other, Offering });

pub(crate) fn private_opened(opened: core::PrivateOpened) -> PrivateOpenedRecord {
    PrivateOpenedRecord {
        started_fresh: opened.started_fresh,
        keys_reset: opened.keys_reset,
    }
}

pub(crate) fn private_contact(contact: core::PrivateContact) -> PrivateContactRecord {
    PrivateContactRecord {
        peer: contact.peer,
        name: contact.name,
        friend: contact.friend.into(),
        request_spam: contact.request_spam,
        blocked: contact.blocked,
        conversation: contact.conversation.into(),
        relay_server_key: contact.relay_server_key,
        last_activity_ms: contact.last_activity_ms.map(number),
        unread: contact.unread,
        muted: contact.muted,
        hidden: contact.hidden,
        preview: contact.preview,
        verification: contact.verification.into(),
        new_keys: contact.new_keys,
        reachable: contact.reachable,
    }
}

pub(crate) fn private_message(message: core::PrivateMessage) -> PrivateMessageRecord {
    PrivateMessageRecord {
        id: message.id,
        peer: message.peer,
        outgoing: message.outgoing,
        kind: message.kind.into(),
        content: message.content,
        server_key: message.server_key,
        sent_at_ms: number(message.sent_at_ms),
        edited_at_ms: message.edited_at_ms.map(number),
        state: message.state.into(),
    }
}

pub(crate) fn key_words(words: core::KeyWords) -> KeyWordsRecord {
    KeyWordsRecord {
        words: words.words,
        keys: words.keys,
    }
}

pub(crate) fn own_device(device: core::OwnDevice) -> OwnDeviceRecord {
    OwnDeviceRecord {
        device_id: device.device_id,
        state: device.state.into(),
    }
}

pub(crate) fn own_devices(devices: Vec<core::OwnDevice>, has_chats: bool) -> OwnDevicesRecord {
    OwnDevicesRecord {
        devices: devices.into_iter().map(own_device).collect(),
        has_chats,
    }
}

pub(crate) fn shared_server(server: core::SharedServer) -> SharedServerRecord {
    SharedServerRecord {
        server_key: server.server_key,
        name: server.name,
        connected: server.connected,
        private_messages: server.private_messages,
        friends_only: server.friends_only,
        peer_user_id: id(server.peer_user_id),
    }
}

pub(crate) fn privacy_settings(settings: core::PrivacySettingsData) -> PrivacySettingsRecord {
    PrivacySettingsRecord {
        allow_friend_requests: settings.allow_friend_requests,
        allow_message_requests: settings.allow_message_requests,
    }
}

pub(crate) fn core_privacy_settings(settings: PrivacySettingsRecord) -> core::PrivacySettingsData {
    core::PrivacySettingsData {
        allow_friend_requests: settings.allow_friend_requests,
        allow_message_requests: settings.allow_message_requests,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_channel(state: core::PluginChannelState) -> core::Channel {
        core::Channel {
            id: 40,
            kind: core::ChannelKind::Plugin,
            name: "help-desk".to_owned(),
            topic: None,
            parent_id: Some(3),
            position: 2,
            overwrites: Vec::new(),
            bitrate: 0,
            user_limit: 0,
            text_in_voice: false,
            plugin_id: Some("tickets".to_owned()),
            type_key: Some("tickets".to_owned()),
            plugin_state: Some(state),
        }
    }

    // Fails if a plugin channel is dropped again, or loses its plugin, type
    // or state on the way to JavaScript.
    #[test]
    fn a_plugin_channel_is_kept_with_its_state() {
        for (core_state, state) in [
            (
                core::PluginChannelState::Available,
                PluginChannelState::Available,
            ),
            (
                core::PluginChannelState::PluginStopped,
                PluginChannelState::PluginStopped,
            ),
            (
                core::PluginChannelState::TypeGone,
                PluginChannelState::TypeGone,
            ),
        ] {
            let record = channel(plugin_channel(core_state)).expect("a plugin channel is kept");
            assert_eq!(record.kind, ChannelKind::Plugin);
            assert_eq!(record.id, "40");
            assert_eq!(record.parent_id.as_deref(), Some("3"));
            assert_eq!(
                record.plugin,
                Some(PluginChannelRecord {
                    plugin_id: "tickets".to_owned(),
                    type_key: "tickets".to_owned(),
                    state,
                })
            );
        }
    }

    // Fails if an ordinary channel gains plugin fields.
    #[test]
    fn a_text_channel_has_no_plugin() {
        let mut text = plugin_channel(core::PluginChannelState::Available);
        text.kind = core::ChannelKind::Text;
        text.plugin_id = None;
        text.type_key = None;
        text.plugin_state = None;
        assert_eq!(channel(text).unwrap().plugin, None);
    }

    // Fails if starting activities or managing the channel doesn't follow
    // its bit (the Activities button and End activity on the phone).
    #[test]
    fn access_names_start_activities_and_manage_channel() {
        let none = access(1, bits(Permissions::VIEW_CHANNEL));
        assert!(!none.start_activities && !none.manage_channel);
        let start = access(1, bits(Permissions::START_ACTIVITIES));
        assert!(start.start_activities && !start.manage_channel);
        let manage = access(1, bits(Permissions::MANAGE_CHANNELS));
        assert!(manage.manage_channel && !manage.start_activities);
        let admin = access(1, bits(Permissions::ADMINISTRATOR));
        assert!(admin.start_activities && admin.manage_channel);
    }

    fn bits(flags: Permissions) -> i64 {
        i64::from_ne_bytes(flags.bits().to_ne_bytes())
    }

    // Fails if ADMINISTRATOR doesn't grant every server-wide permission flag.
    #[test]
    fn administrator_grants_every_server_flag() {
        let access = server_access(bits(Permissions::ADMINISTRATOR));
        assert!(
            access.manage_server
                && access.manage_channels
                && access.manage_roles
                && access.kick_members
                && access.ban_members
                && access.create_invite
                && access.manage_nicknames
                && access.change_nickname
                && access.manage_messages
                && access.mention_everyone
                && access.manage_plugins
                && access.manage_soundboard
                && access.music_manage
                && access.mute_members
                && access.deafen_members
                && access.move_members
                && access.priority_speaker
                && access.video
                && access.screenshare
                && access.administrator
        );
    }

    // Fails if server_access doesn't map permission bits to the right fields, or leaks administrator.
    #[test]
    fn server_access_follows_the_permission_bits() {
        let access = server_access(bits(Permissions::KICK_MEMBERS | Permissions::MANAGE_ROLES));
        assert!(access.kick_members);
        assert!(access.manage_roles);
        assert!(
            !access.manage_server
                && !access.manage_channels
                && !access.ban_members
                && !access.create_invite
                && !access.manage_nicknames
                && !access.change_nickname
                && !access.manage_messages
                && !access.mention_everyone
                && !access.manage_plugins
                && !access.manage_soundboard
                && !access.music_manage
                && !access.mute_members
                && !access.deafen_members
                && !access.move_members
                && !access.priority_speaker
                && !access.video
                && !access.screenshare
                && !access.administrator
        );
    }

    // Fails if a role loses its permissions, id as a string, or server access mapping.
    #[test]
    fn a_role_crosses_with_its_bits() {
        let role = core::Role {
            id: 42,
            name: "Moderator".to_owned(),
            color: 0x5865f2,
            position: 3,
            permissions: bits(Permissions::BAN_MEMBERS),
            hoist: true,
            mentionable: true,
            max_upload_bytes: Some(25_000_000),
            plugin_id: Some("moderation".to_owned()),
        };
        let record = super::role(role);
        assert_eq!(record.id, "42");
        assert_eq!(record.name, "Moderator");
        assert_eq!(record.color, 0x5865f2);
        assert_eq!(record.position, 3);
        assert!(record.hoist);
        assert!(record.mentionable);
        assert_eq!(record.max_upload_bytes, Some(25_000_000.0));
        assert_eq!(record.plugin_id.as_deref(), Some("moderation"));
        assert!(record.access.ban_members);
        assert!(
            !record.access.manage_server
                && !record.access.manage_channels
                && !record.access.manage_roles
                && !record.access.kick_members
                && !record.access.create_invite
                && !record.access.administrator
        );
    }

    // Fails if ReadySnapshot drops roles, presences, or server_access when converting to ServerSnapshot.
    #[test]
    fn the_snapshot_carries_roles_presences_and_server_access() {
        let ready = core::ReadySnapshot {
            self_user: core::User {
                id: 1,
                public_key_hex: "01".repeat(32),
                display_name: "Self".to_owned(),
                avatar_hash: None,
                fingerprint: "ABCD-EFGH-IJKL-MNOP".to_owned(),
            },
            server: core::ServerInfo {
                server_id_hex: "02".repeat(16),
                name: "Test Guild".to_owned(),
                description: "Guild desc".to_owned(),
                owner_id: Some(1),
                open_join: false,
                everyone_role_id: 1,
                icon_hash: None,
            },
            channels: Vec::new(),
            roles: vec![
                core::Role {
                    id: 10,
                    name: "Moderator".to_owned(),
                    color: 0x5865f2,
                    position: 1,
                    permissions: bits(Permissions::KICK_MEMBERS),
                    hoist: true,
                    mentionable: true,
                    max_upload_bytes: Some(10_000_000),
                    plugin_id: None,
                },
                core::Role {
                    id: 11,
                    name: "Admin".to_owned(),
                    color: 0,
                    position: 2,
                    permissions: bits(Permissions::ADMINISTRATOR),
                    hoist: false,
                    mentionable: false,
                    max_upload_bytes: None,
                    plugin_id: None,
                },
            ],
            members: Vec::new(),
            presences: vec![core::Presence {
                user_id: 1,
                status: core::PresenceStatus::Online,
            }],
            server_permissions: bits(Permissions::MANAGE_SERVER),
            channel_permissions: Vec::new(),
            voice_enabled: false,
            voice_states: Vec::new(),
            streams: Vec::new(),
            voice_settings: core::VoiceSettings {
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
            },
            soundboard_sounds: Vec::new(),
            addresses: core::ServerAddresses {
                primary: None,
                domains: Vec::new(),
                ip: None,
            },
            commands: core::CommandList {
                commands: Vec::new(),
                results_public: false,
            },
            upload_settings: core::UploadSettings {
                blocked_extensions: Vec::new(),
                voice_message_max_seconds: 60,
                max_file_bytes: 10_000_000,
                max_attachments: 10,
            },
            private_messages: false,
            private_settings: core::PrivateSettings {
                enabled: false,
                friends_only: false,
                allowed_by_config: true,
            },
            plugin_users: Vec::new(),
            plugin_channel_types: Vec::new(),
            plugin_activities: Vec::new(),
            activity_sessions: Vec::new(),
        };

        let snapshot = snapshot("srv".to_owned(), ready);
        assert_eq!(snapshot.roles.len(), 2);
        assert_eq!(snapshot.roles[0].id, "10");
        assert_eq!(snapshot.roles[1].id, "11");
        assert_eq!(snapshot.presences.len(), 1);
        assert_eq!(snapshot.presences[0].user_id, "1");
        assert_eq!(snapshot.presences[0].status, PresenceStatus::Online);
        assert!(snapshot.server_access.manage_server);
        assert!(!snapshot.server_access.administrator);
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
    fn a_playback_item_crosses_and_comes_back() {
        let attachment = core::PlaybackItem::Attachment {
            server_key: "s".to_owned(),
            attachment_id: 9_007_199_254_740_993,
        };
        assert_eq!(
            core_playback_item(playback_item(attachment.clone())).unwrap(),
            attachment
        );
        let draft = core::PlaybackItem::Draft { draft_id: 7 };
        assert_eq!(
            core_playback_item(playback_item(draft.clone())).unwrap(),
            draft
        );
    }

    #[test]
    fn a_draft_id_is_a_whole_number() {
        assert_eq!(draft_id(3.0).unwrap(), 3);
        for bad in [-1.0, 1.5, f64::NAN, f64::INFINITY] {
            assert!(draft_id(bad).is_err(), "{bad} was taken");
        }
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

    fn spec(feature: &str, permissions: i64) -> core::CommandSpec {
        core::CommandSpec {
            name: "roll".to_owned(),
            description: "d".to_owned(),
            options: vec![core::CommandOptionSpec {
                name: "sides".to_owned(),
                description: "o".to_owned(),
                kind: core::CommandOptionKind::Integer,
                required: true,
                autocomplete: true,
                choices: vec![core::CommandChoice {
                    name: "six".to_owned(),
                    value: "6".to_owned(),
                    artwork_id: String::new(),
                }],
                min: 2,
                max: 100,
            }],
            feature: feature.to_owned(),
            permission_summary: "Anyone".to_owned(),
            aliases: vec!["r".to_owned()],
            permissions,
            enabled: true,
            channel_ids: vec![7],
        }
    }

    #[test]
    fn a_commands_list_crosses_with_its_source_and_needs() {
        let bits = (Permissions::MUSIC_CONTROL | Permissions::SEND_MESSAGES).bits() as i64;

        let list = command_list(core::CommandList {
            commands: vec![spec("plugin:dice", 0), spec("music", bits)],
            results_public: false,
        });

        assert!(!list.results_public);
        assert_eq!(list.commands[0].source, "plugin:dice");
        assert_eq!(list.commands[0].needs, Vec::<String>::new());
        assert_eq!(list.commands[1].source, "");
        assert_eq!(list.commands[1].needs, ["musicControl"]);
        assert_eq!(list.commands[0].channel_ids, ["7"]);
        let option = &list.commands[0].options[0];
        assert_eq!(option.kind, CommandOptionKindRecord::Integer);
        assert_eq!((option.min, option.max), (2.0, 100.0));
        assert_eq!(option.choices[0].value, "6");
    }

    #[test]
    fn an_ephemeral_keeps_its_nonce_and_state() {
        let note = ephemeral_note(core::EphemeralMessage {
            channel_id: 9,
            nonce: "n".to_owned(),
            content: "thinking".to_owned(),
            state: core::CommandRunState::Thinking,
        });

        assert_eq!(note.channel_id, "9");
        assert_eq!(note.nonce, "n");
        assert_eq!(note.state, CommandRunStateRecord::Thinking);
    }

    #[test]
    fn a_commands_message_says_who_ran_it_and_its_state() {
        let converted = message(core::Message {
            id: 1,
            channel_id: 2,
            author_id: 3,
            content: "x".to_owned(),
            created_at_ms: 0,
            edited_at_ms: None,
            nonce: None,
            command: Some("roll".to_owned()),
            command_user_id: Some(4),
            command_state: core::CommandRunState::NoAnswer,
            attachments: Vec::new(),
        });

        assert_eq!(
            converted.interaction,
            Some(MessageInteractionRecord {
                user_id: "4".to_owned(),
                command: "roll".to_owned(),
                state: CommandRunStateRecord::NoAnswer,
            })
        );
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
            command_user_id: None,
            command_state: core::CommandRunState::Answered,
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

        let core_member = core::Member {
            user: core::User {
                id: 42,
                public_key_hex: "PK".to_owned(),
                display_name: "Alice".to_owned(),
                avatar_hash: None,
                fingerprint: "FP".to_owned(),
            },
            nickname: Some("Ali".to_owned()),
            role_ids: vec![1, 2],
            joined_at_ms: 1_791_374_400_000,
        };
        let converted_member = member(core_member);
        assert_eq!(converted_member.user.id, "42");
        assert_eq!(converted_member.nickname.as_deref(), Some("Ali"));
        assert_eq!(converted_member.role_ids, vec!["1", "2"]);
        assert_eq!(converted_member.joined_at_ms, 1_791_374_400_000.0);
    }

    fn phone_audio(input_volume: f32, output_volume: f32) -> PhoneAudioSettings {
        PhoneAudioSettings {
            noise_suppression: NoiseSuppression::Standard,
            input_volume,
            output_volume,
        }
    }

    // Fails if the phone's untouched choices change anything about a call's
    // audio: every phone call ran on the core's defaults before P1.
    #[test]
    fn untouched_settings_equal_the_core_default() {
        assert_eq!(
            core_audio_settings(phone_audio(1.0, 1.0)),
            core::AudioSettings::default()
        );
    }

    // Fails if a volume past 200 %, below 0 or not a number reaches the
    // call, or if the three choices don't land where they belong.
    #[test]
    fn volumes_clamp() {
        let high = core_audio_settings(phone_audio(5.0, 2.5));
        assert_eq!((high.input_volume, high.output_volume), (2.0, 2.0));
        let low = core_audio_settings(phone_audio(-1.0, -0.1));
        assert_eq!((low.input_volume, low.output_volume), (0.0, 0.0));
        let odd = core_audio_settings(phone_audio(f32::NAN, f32::INFINITY));
        assert_eq!((odd.input_volume, odd.output_volume), (1.0, 1.0));
        let chosen = core_audio_settings(PhoneAudioSettings {
            noise_suppression: NoiseSuppression::High,
            input_volume: 0.5,
            output_volume: 1.5,
        });
        assert_eq!(
            chosen,
            core::AudioSettings {
                noise_suppression: core::NoiseSuppressionMode::High,
                input_volume: 0.5,
                output_volume: 1.5,
                ..core::AudioSettings::default()
            }
        );
        assert_eq!(
            core_noise_suppression(NoiseSuppression::Off),
            core::NoiseSuppressionMode::Off
        );
    }

    // Fails if a sound plays as another one.
    #[test]
    fn every_cue_maps_to_the_core() {
        let pairs = [
            (VoiceCueKind::Join, core::VoiceCue::Join),
            (VoiceCueKind::Leave, core::VoiceCue::Leave),
            (VoiceCueKind::UserJoin, core::VoiceCue::UserJoin),
            (VoiceCueKind::UserLeave, core::VoiceCue::UserLeave),
            (VoiceCueKind::Mute, core::VoiceCue::Mute),
            (VoiceCueKind::Unmute, core::VoiceCue::Unmute),
            (VoiceCueKind::Deafen, core::VoiceCue::Deafen),
            (VoiceCueKind::Undeafen, core::VoiceCue::Undeafen),
            (VoiceCueKind::StreamStart, core::VoiceCue::StreamStart),
            (VoiceCueKind::StreamStop, core::VoiceCue::StreamStop),
        ];
        for (phone, expected) in pairs {
            assert_eq!(core_cue(phone), expected, "{phone:?}");
        }
    }

    // Fails if `invite` drops the link or does not map the fields.
    #[test]
    fn an_invite_crosses_with_its_link() {
        let record = invite(core::Invite {
            code: "AbC123xyz".to_string(),
            link: "opencord://chat.example.org:7711/invite/AbC123xyz#fp=abcdef".to_string(),
            web_link: Some("https://chat.example.org/invite/AbC123xyz".to_string()),
            domain_id: Some(42),
            created_by: Some(101),
            created_at_ms: 1_700_000_000_123,
            max_uses: Some(5),
            uses: 1,
            expires_at_ms: Some(1_700_100_000_456),
        });
        assert_eq!(record.code, "AbC123xyz");
        assert_eq!(
            record.link,
            "opencord://chat.example.org:7711/invite/AbC123xyz#fp=abcdef"
        );
        assert_eq!(
            record.web_link.as_deref(),
            Some("https://chat.example.org/invite/AbC123xyz")
        );
        assert_eq!(record.domain_id.as_deref(), Some("42"));
        assert_eq!(record.created_by.as_deref(), Some("101"));
        assert_eq!(record.created_at_ms, 1_700_000_000_123.0);
        assert_eq!(record.max_uses, Some(5));
        assert_eq!(record.uses, 1);
        assert_eq!(record.expires_at_ms, Some(1_700_100_000_456.0));
    }

    // Fails if `domain_id` or `created_by` are not converted to decimal strings.
    #[test]
    fn ids_in_an_invite_are_strings() {
        let record = invite(core::Invite {
            code: "code".to_string(),
            link: "link".to_string(),
            web_link: None,
            domain_id: Some(9_000_000_000_000_000_001),
            created_by: Some(9_000_000_000_000_000_002),
            created_at_ms: 0,
            max_uses: None,
            uses: 0,
            expires_at_ms: None,
        });
        assert_eq!(record.domain_id.as_deref(), Some("9000000000000000001"));
        assert_eq!(record.created_by.as_deref(), Some("9000000000000000002"));
    }

    fn round_trips<C, M>(pairs: &[(C, M)])
    where
        C: Copy + PartialEq + std::fmt::Debug + From<M>,
        M: Copy + PartialEq + std::fmt::Debug + From<C>,
    {
        for &(core_value, mobile_value) in pairs {
            assert_eq!(M::from(core_value), mobile_value);
            assert_eq!(C::from(mobile_value), core_value);
        }
    }

    // Fails if a private enum's variant maps to another variant, either way:
    // a contact shown as verified when it isn't, or a failed send shown as
    // sent.
    #[test]
    fn every_private_enum_round_trips() {
        round_trips(&[
            (core::FriendStatus::None, FriendStatus::None),
            (core::FriendStatus::Outgoing, FriendStatus::Outgoing),
            (core::FriendStatus::Incoming, FriendStatus::Incoming),
            (core::FriendStatus::Friends, FriendStatus::Friends),
        ]);
        round_trips(&[
            (core::ConversationStatus::None, ConversationStatus::None),
            (
                core::ConversationStatus::Request,
                ConversationStatus::Request,
            ),
            (
                core::ConversationStatus::Accepted,
                ConversationStatus::Accepted,
            ),
        ]);
        round_trips(&[
            (
                core::PrivateVerification::Unverified,
                PrivateVerification::Unverified,
            ),
            (
                core::PrivateVerification::Verified,
                PrivateVerification::Verified,
            ),
            (
                core::PrivateVerification::Mismatch,
                PrivateVerification::Mismatch,
            ),
        ]);
        round_trips(&[
            (core::PrivateMessageKind::Text, PrivateMessageKind::Text),
            (
                core::PrivateMessageKind::Divider,
                PrivateMessageKind::Divider,
            ),
            (core::PrivateMessageKind::Moved, PrivateMessageKind::Moved),
            (
                core::PrivateMessageKind::Undecryptable,
                PrivateMessageKind::Undecryptable,
            ),
            (
                core::PrivateMessageKind::KeysChanged,
                PrivateMessageKind::KeysChanged,
            ),
            (
                core::PrivateMessageKind::NewDevice,
                PrivateMessageKind::NewDevice,
            ),
            (
                core::PrivateMessageKind::UnacceptedKeys,
                PrivateMessageKind::UnacceptedKeys,
            ),
        ]);
        round_trips(&[
            (core::PrivateSendState::Received, PrivateSendState::Received),
            (core::PrivateSendState::Sending, PrivateSendState::Sending),
            (core::PrivateSendState::Sent, PrivateSendState::Sent),
            (core::PrivateSendState::Failed, PrivateSendState::Failed),
        ]);
        round_trips(&[
            (core::OwnDeviceState::Other, OwnDeviceState::Other),
            (core::OwnDeviceState::Offering, OwnDeviceState::Offering),
        ]);

        // And inside the records that carry them.
        let contact = private_contact(core::PrivateContact {
            peer: "ab".repeat(32),
            name: Some("Kai".to_owned()),
            friend: core::FriendStatus::Incoming,
            request_spam: true,
            blocked: false,
            conversation: core::ConversationStatus::Request,
            relay_server_key: Some("a.example:7700".to_owned()),
            last_activity_ms: Some(1_700_000_000_123),
            unread: 3,
            muted: true,
            hidden: false,
            preview: Some("hi".to_owned()),
            verification: core::PrivateVerification::Mismatch,
            new_keys: true,
            reachable: false,
        });
        assert_eq!(
            contact,
            PrivateContactRecord {
                peer: "ab".repeat(32),
                name: Some("Kai".to_owned()),
                friend: FriendStatus::Incoming,
                request_spam: true,
                blocked: false,
                conversation: ConversationStatus::Request,
                relay_server_key: Some("a.example:7700".to_owned()),
                last_activity_ms: Some(1_700_000_000_123.0),
                unread: 3,
                muted: true,
                hidden: false,
                preview: Some("hi".to_owned()),
                verification: PrivateVerification::Mismatch,
                new_keys: true,
                reachable: false,
            }
        );
        let message = private_message(core::PrivateMessage {
            id: "0f".repeat(16),
            peer: "ab".repeat(32),
            outgoing: true,
            kind: core::PrivateMessageKind::KeysChanged,
            content: String::new(),
            server_key: None,
            sent_at_ms: 1_700_000_000_456,
            edited_at_ms: Some(1_700_000_000_789),
            state: core::PrivateSendState::Failed,
        });
        assert_eq!(message.kind, PrivateMessageKind::KeysChanged);
        assert_eq!(message.state, PrivateSendState::Failed);
        assert_eq!(message.sent_at_ms, 1_700_000_000_456.0);
        assert_eq!(message.edited_at_ms, Some(1_700_000_000_789.0));
        assert_eq!(message.id, "0f".repeat(16));
        let devices = own_devices(
            vec![core::OwnDevice {
                device_id: "cd".repeat(16),
                state: core::OwnDeviceState::Offering,
            }],
            true,
        );
        assert_eq!(
            devices,
            OwnDevicesRecord {
                devices: vec![OwnDeviceRecord {
                    device_id: "cd".repeat(16),
                    state: OwnDeviceState::Offering,
                }],
                has_chats: true,
            }
        );
        let settings = PrivacySettingsRecord {
            allow_friend_requests: false,
            allow_message_requests: true,
        };
        assert_eq!(privacy_settings(core_privacy_settings(settings)), settings);
    }

    // Fails if a peer's user id crosses as a number, which JavaScript reads
    // as a `bigint` (or loses digits of).
    #[test]
    fn a_shared_server_user_id_is_a_string() {
        let record = shared_server(core::SharedServer {
            server_key: "a.example:7700".to_owned(),
            name: "Home".to_owned(),
            connected: true,
            private_messages: true,
            friends_only: false,
            peer_user_id: 9_000_000_000_000_000_003,
        });
        assert_eq!(record.peer_user_id, "9000000000000000003");
        assert_eq!(record.server_key, "a.example:7700");
        assert!(record.private_messages && !record.friends_only);
    }
}
