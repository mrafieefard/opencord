//! Events, pulled by JavaScript in batches (design §5.4).
//!
//! A callback from a Rust thread into JavaScript would block that thread
//! until JavaScript returns, so the app instead loops on `next_events`.
//! Each JavaScript start (a reload too) calls `events_attach` first: a
//! newer generation releases a call still waiting under an older one, so
//! the stream is never held by a JavaScript context that is gone.
//!
//! Three sources feed the one queue: the core's server events, voice
//! media's, and private messages' once `private_open` has run.

use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use opencord_core::api::types::{
    self as core, CoreEvent, CoreEventPayload, MediaEvent, PrivateEvent,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{Mutex, Notify};

use crate::convert;
use crate::ids::id;
use crate::private_types::{OwnDeviceRecord, PrivateContactRecord, PrivateMessageRecord};
use crate::types::*;

/// The most one batch holds.
pub const BATCH_MAX: u32 = 1024;

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileEvent {
    /// The commands this user can use changed.
    CommandsUpdated {
        server_key: String,
        commands: CommandListRecord,
    },
    /// A reply only this device sees.
    EphemeralReceived {
        server_key: String,
        note: EphemeralNoteRecord,
    },
    Connection {
        server_key: String,
        state: ConnectionRecord,
    },
    /// Replaces everything known of that server.
    Ready {
        snapshot: ServerSnapshot,
    },
    MessageCreated {
        server_key: String,
        message: MessageRecord,
    },
    MessageUpdated {
        server_key: String,
        message: MessageRecord,
    },
    MessageDeleted {
        server_key: String,
        channel_id: String,
        message_id: String,
    },
    ChannelUpserted {
        server_key: String,
        channel: ChannelRecord,
    },
    ChannelDeleted {
        server_key: String,
        channel_id: String,
    },
    RoleUpserted {
        server_key: String,
        role: RoleRecord,
    },
    RoleDeleted {
        server_key: String,
        role_id: String,
    },
    PresenceChanged {
        server_key: String,
        presence: PresenceRecord,
    },
    /// The user's own permissions changed.
    AccessChanged {
        server_key: String,
        access: Vec<ChannelAccess>,
        server_access: ServerAccess,
    },
    MemberUpserted {
        server_key: String,
        member: MemberRecord,
    },
    MemberLeft {
        server_key: String,
        user_id: String,
    },
    Typing {
        server_key: String,
        channel_id: String,
        user_id: String,
    },
    ServerInfoChanged {
        server_key: String,
        name: String,
        icon_hash: Option<String>,
    },
    VoiceStateChanged {
        server_key: String,
        state: VoiceStateRecord,
    },
    /// How this device's voice connection is doing.
    VoiceLink {
        server_key: String,
        channel_id: String,
        state: VoiceLinkState,
    },
    Speaking {
        server_key: String,
        channel_id: String,
        changes: Vec<SpeakingChangeRecord>,
    },
    /// An audio device failed, or a missing one was replaced by the
    /// default (`fell_back`, with its name as the message).
    AudioProblem {
        output: bool,
        fell_back: bool,
        message: String,
    },
    /// The microphone heard speech while muted (at most every 30 s).
    SpeakingWhileMuted,
    /// The microphone's level in dBFS while the level meter is on, at the
    /// core's rate.
    InputLevel {
        dbfs: f32,
    },
    /// High noise suppression is too heavy for this device; the core uses
    /// Standard instead.
    NoiseSuppressionFellBack,
    MusicSession {
        server_key: String,
        session: MusicSessionRecord,
    },
    MusicNowPlaying {
        server_key: String,
        now_playing: MusicNowPlayingRecord,
    },
    MusicNotice {
        server_key: String,
        channel_id: String,
        text: String,
    },
    MusicSettingsChanged {
        server_key: String,
        settings: MusicSettingsRecord,
    },
    SoundboardSettingsChanged {
        server_key: String,
        settings: SoundboardSettingsRecord,
    },
    SoundboardSoundUpserted {
        server_key: String,
        sound: SoundboardSoundRecord,
    },
    SoundboardSoundDeleted {
        server_key: String,
        sound_id: String,
    },
    VoiceMessageLimitChanged {
        server_key: String,
        max_seconds: u32,
    },
    /// The microphone's level while recording a voice message, 0–1,
    /// about every 33 ms.
    VoiceRecordLevel {
        level: f32,
    },
    /// Recording stopped by itself: at the server's maximum length, or the
    /// microphone went away.
    VoiceRecordStopped {
        max_length: bool,
    },
    Playback {
        item: PlaybackItemRecord,
        position_ms: u32,
        duration_ms: u32,
        playing: bool,
        buffering: bool,
    },
    PlaybackEnded {
        item: PlaybackItemRecord,
    },
    PlaybackFailed {
        item: PlaybackItemRecord,
        message: String,
    },
    /// Someone played a sound in a voice channel; this device plays it
    /// itself when it is in that channel.
    SoundPlayed {
        server_key: String,
        effect: SoundEffectRecord,
    },
    MusicUpload {
        server_key: String,
        channel_id: String,
        upload_id: String,
        filename: String,
        sent_bytes: f64,
        total_bytes: f64,
        phase: UploadPhaseRecord,
    },
    /// A file of a message sent with `message_send_files`.
    SendProgress {
        server_key: String,
        nonce: String,
        index: u32,
        sent_bytes: f64,
        total_bytes: f64,
        phase: UploadPhaseRecord,
    },
    Notice {
        server_key: String,
        key: String,
        title: String,
        body: String,
    },
    PluginUsersUpdated {
        server_key: String,
        plugin_users: Vec<PluginUserRecord>,
    },
    /// The channel types the server's running plugins register now.
    PluginChannelTypesUpdated {
        server_key: String,
        types: Vec<PluginChannelTypeRecord>,
    },
    /// The activities the server's running plugins register now.
    PluginActivitiesUpdated {
        server_key: String,
        activities: Vec<PluginActivityRecord>,
    },
    ActivitySessionCreated {
        server_key: String,
        session: ActivitySessionRecord,
    },
    /// Who has it open changed.
    ActivitySessionUpdated {
        server_key: String,
        session: ActivitySessionRecord,
    },
    ActivitySessionDeleted {
        server_key: String,
        session_id: String,
        /// The call it ran in.
        channel_id: String,
        reason: ActivityEndReason,
    },
    /// Private messages and friends, from `private_open` on.
    Private {
        event: PrivateEventRecord,
    },
}

/// The core's `PrivateEvent`, one variant each, with the same names.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum PrivateEventRecord {
    /// This person's other devices with other chat keys, and whether this
    /// device has any chats.
    OwnDevicesChanged {
        devices: Vec<OwnDeviceRecord>,
        has_chats: bool,
    },
    ContactChanged {
        contact: PrivateContactRecord,
    },
    ContactRemoved {
        peer: String,
    },
    MessageAdded {
        message: PrivateMessageRecord,
    },
    MessageChanged {
        message: PrivateMessageRecord,
    },
    MessageDeleted {
        peer: String,
        message_id: String,
    },
    Typing {
        peer: String,
    },
    FriendRequestReceived {
        peer: String,
        spam: bool,
    },
}

enum Incoming {
    Core(Box<CoreEvent>),
    Media(MediaEvent),
    Private(PrivateEvent),
}

struct Receivers {
    core: UnboundedReceiver<CoreEvent>,
    media: Option<UnboundedReceiver<MediaEvent>>,
    /// Fed by the forwarder `EventPump::connect_private` hands out; the
    /// pump keeps its sender, so this never ends.
    private: UnboundedReceiver<PrivateEvent>,
}

impl Receivers {
    /// The next event from any stream; `None` once the core's has ended.
    async fn recv(&mut self) -> Option<Incoming> {
        loop {
            let media_open = self.media.is_some();
            tokio::select! {
                event = self.core.recv() => return event.map(|event| Incoming::Core(Box::new(event))),
                Some(event) = self.private.recv() => return Some(Incoming::Private(event)),
                event = async {
                    match self.media.as_mut() {
                        Some(media) => media.recv().await,
                        None => None,
                    }
                }, if media_open => match event {
                    Some(event) => return Some(Incoming::Media(event)),
                    // Media has stopped for good; carry on with the others.
                    None => self.media = None,
                },
            }
        }
    }

    fn try_recv(&mut self) -> Option<Incoming> {
        if let Ok(event) = self.core.try_recv() {
            return Some(Incoming::Core(Box::new(event)));
        }
        if let Some(event) = self.media.as_mut().and_then(|media| media.try_recv().ok()) {
            return Some(Incoming::Media(event));
        }
        self.private.try_recv().ok().map(Incoming::Private)
    }
}

/// The core's event streams, handed out in batches to one waiter at a
/// time.
pub struct EventPump {
    receivers: Mutex<Receivers>,
    generation: AtomicU64,
    attached: Notify,
    /// Into `Receivers::private`. A sender of its own, so connecting the
    /// core's private stream never waits for the lock a waiting `next`
    /// holds.
    private_in: UnboundedSender<PrivateEvent>,
    /// Whether the core's private stream has been taken.
    private_connected: AtomicBool,
}

impl EventPump {
    pub fn new(
        core: UnboundedReceiver<CoreEvent>,
        media: Option<UnboundedReceiver<MediaEvent>>,
    ) -> Self {
        let (private_in, private) = unbounded_channel();
        Self {
            receivers: Mutex::new(Receivers {
                core,
                media,
                private,
            }),
            generation: AtomicU64::new(0),
            attached: Notify::new(),
            private_in,
            private_connected: AtomicBool::new(false),
        }
    }

    /// The task that feeds the core's private stream into the queue, the
    /// first time only: `take` (`Client::private_events`) is called once
    /// per process, since taking the stream again silences the receiver
    /// taken first, and every JavaScript start (a reload too) opens again.
    /// `None` when it is connected already.
    pub fn connect_private(
        &self,
        take: impl FnOnce() -> UnboundedReceiver<PrivateEvent>,
    ) -> Option<impl Future<Output = ()> + Send + 'static> {
        if self.private_connected.swap(true, Ordering::SeqCst) {
            return None;
        }
        let mut from = take();
        let into = self.private_in.clone();
        Some(async move {
            while let Some(event) = from.recv().await {
                if into.send(event).is_err() {
                    tracing::info!("the event pump is gone; private events stop");
                    return;
                }
            }
            tracing::warn!("the core's private event stream ended");
        })
    }

    /// Starts a new generation: a call waiting under an older one returns
    /// an empty batch at once.
    pub fn attach(&self) -> u64 {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.attached.notify_waiters();
        generation
    }

    /// Waits for at least one event, then returns what is ready, up to
    /// `max`. Empty only when a newer generation has attached, or the core
    /// has stopped.
    pub async fn next(&self, generation: u64, max: u32) -> Vec<MobileEvent> {
        let max = max.clamp(1, BATCH_MAX) as usize;
        // Registered before the generation is read, so an `attach` between
        // the two can't be missed.
        let superseded = self.attached.notified();
        tokio::pin!(superseded);
        superseded.as_mut().enable();
        if self.generation.load(Ordering::SeqCst) != generation {
            return Vec::new();
        }
        let mut receivers = tokio::select! {
            receivers = self.receivers.lock() => receivers,
            () = &mut superseded => return Vec::new(),
        };
        let mut batch = Vec::new();
        while batch.is_empty() {
            if self.generation.load(Ordering::SeqCst) != generation {
                return Vec::new();
            }
            let first = tokio::select! {
                event = receivers.recv() => event,
                () = &mut superseded => return Vec::new(),
            };
            let Some(first) = first else {
                return Vec::new();
            };
            batch.extend(convert_incoming(first));
            while batch.len() < max {
                let Some(next) = receivers.try_recv() else {
                    break;
                };
                batch.extend(convert_incoming(next));
            }
        }
        batch
    }
}

fn convert_incoming(incoming: Incoming) -> Option<MobileEvent> {
    match incoming {
        Incoming::Core(event) => core_event(*event),
        Incoming::Media(event) => media_event(event),
        Incoming::Private(event) => Some(MobileEvent::Private {
            event: private_event(event),
        }),
    }
}

/// A private event as JavaScript sees it; every kind reaches it.
pub(crate) fn private_event(event: PrivateEvent) -> PrivateEventRecord {
    match event {
        PrivateEvent::OwnDevicesChanged { devices, has_chats } => {
            let own = convert::own_devices(devices, has_chats);
            PrivateEventRecord::OwnDevicesChanged {
                devices: own.devices,
                has_chats: own.has_chats,
            }
        }
        PrivateEvent::ContactChanged(contact) => PrivateEventRecord::ContactChanged {
            contact: convert::private_contact(contact),
        },
        PrivateEvent::ContactRemoved { peer } => PrivateEventRecord::ContactRemoved { peer },
        PrivateEvent::MessageAdded(message) => PrivateEventRecord::MessageAdded {
            message: convert::private_message(message),
        },
        PrivateEvent::MessageChanged(message) => PrivateEventRecord::MessageChanged {
            message: convert::private_message(message),
        },
        PrivateEvent::MessageDeleted { peer, message_id } => {
            PrivateEventRecord::MessageDeleted { peer, message_id }
        }
        PrivateEvent::Typing { peer } => PrivateEventRecord::Typing { peer },
        PrivateEvent::FriendRequestReceived { peer, spam } => {
            PrivateEventRecord::FriendRequestReceived { peer, spam }
        }
    }
}

/// The core's event as JavaScript sees it, or `None` for the kinds the
/// phone apps don't use yet. No catch-all arm: a new core event fails to
/// compile here until it is mapped or listed as dropped.
pub(crate) fn core_event(event: CoreEvent) -> Option<MobileEvent> {
    let server_key = event.server_key;
    Some(match event.payload {
        CoreEventPayload::ConnectionState(state) => MobileEvent::Connection {
            server_key,
            state: convert::connection(state),
        },
        CoreEventPayload::Ready(ready) => MobileEvent::Ready {
            snapshot: convert::snapshot(server_key, ready),
        },
        CoreEventPayload::MessageCreate(message) => MobileEvent::MessageCreated {
            server_key,
            message: convert::message(message),
        },
        CoreEventPayload::MessageUpdate(message) => MobileEvent::MessageUpdated {
            server_key,
            message: convert::message(message),
        },
        CoreEventPayload::MessageDelete {
            channel_id,
            message_id,
        } => MobileEvent::MessageDeleted {
            server_key,
            channel_id: id(channel_id),
            message_id: id(message_id),
        },
        CoreEventPayload::ChannelCreate(channel) | CoreEventPayload::ChannelUpdate(channel) => {
            MobileEvent::ChannelUpserted {
                server_key,
                channel: convert::channel(channel)?,
            }
        }
        CoreEventPayload::PluginChannelTypesUpdate(types) => {
            MobileEvent::PluginChannelTypesUpdated {
                server_key,
                types: types
                    .into_iter()
                    .map(convert::plugin_channel_type)
                    .collect(),
            }
        }
        CoreEventPayload::PluginActivitiesUpdate(activities) => {
            MobileEvent::PluginActivitiesUpdated {
                server_key,
                activities: activities
                    .into_iter()
                    .map(convert::plugin_activity)
                    .collect(),
            }
        }
        CoreEventPayload::ActivitySessionCreate(session) => MobileEvent::ActivitySessionCreated {
            server_key,
            session: convert::activity_session(session),
        },
        CoreEventPayload::ActivitySessionUpdate(session) => MobileEvent::ActivitySessionUpdated {
            server_key,
            session: convert::activity_session(session),
        },
        CoreEventPayload::ActivitySessionDelete {
            session_id,
            channel_id,
            reason,
        } => MobileEvent::ActivitySessionDeleted {
            server_key,
            session_id: id(session_id),
            channel_id: id(channel_id),
            reason: convert::activity_end_reason(reason),
        },
        CoreEventPayload::ChannelDelete { channel_id } => MobileEvent::ChannelDeleted {
            server_key,
            channel_id: id(channel_id),
        },
        CoreEventPayload::PermissionsUpdate {
            server_permissions,
            channel_permissions,
        } => MobileEvent::AccessChanged {
            server_key,
            access: convert::access_list(channel_permissions),
            server_access: convert::server_access(server_permissions),
        },
        CoreEventPayload::RoleCreate(role) | CoreEventPayload::RoleUpdate(role) => {
            MobileEvent::RoleUpserted {
                server_key,
                role: convert::role(role),
            }
        }
        CoreEventPayload::RoleDelete { role_id } => MobileEvent::RoleDeleted {
            server_key,
            role_id: id(role_id),
        },
        CoreEventPayload::PresenceUpdate(presence) => MobileEvent::PresenceChanged {
            server_key,
            presence: convert::presence(presence),
        },
        CoreEventPayload::MemberJoin(member) | CoreEventPayload::MemberUpdate(member) => {
            MobileEvent::MemberUpserted {
                server_key,
                member: convert::member(member),
            }
        }
        CoreEventPayload::MemberLeave { user_id } => MobileEvent::MemberLeft {
            server_key,
            user_id: id(user_id),
        },
        CoreEventPayload::TypingStart {
            channel_id,
            user_id,
        } => MobileEvent::Typing {
            server_key,
            channel_id: id(channel_id),
            user_id: id(user_id),
        },
        CoreEventPayload::ServerUpdate(info) => MobileEvent::ServerInfoChanged {
            server_key,
            name: info.name,
            icon_hash: info.icon_hash,
        },
        CoreEventPayload::VoiceStateUpdate(state) => MobileEvent::VoiceStateChanged {
            server_key,
            state: convert::voice_state(state),
        },
        CoreEventPayload::MusicSession(session) => MobileEvent::MusicSession {
            server_key,
            session: convert::session(session),
        },
        CoreEventPayload::MusicNowPlaying(now) => MobileEvent::MusicNowPlaying {
            server_key,
            now_playing: convert::now_playing(now),
        },
        CoreEventPayload::MusicNotice(notice) => MobileEvent::MusicNotice {
            server_key,
            channel_id: id(notice.channel_id),
            text: notice.text,
        },
        CoreEventPayload::MusicSettingsUpdate(settings) => MobileEvent::MusicSettingsChanged {
            server_key,
            settings: convert::music_settings(settings),
        },
        CoreEventPayload::MusicUploadProgress {
            channel_id,
            upload_id,
            filename,
            sent_bytes,
            total_bytes,
            phase,
        } => MobileEvent::MusicUpload {
            server_key,
            channel_id: id(channel_id),
            upload_id,
            filename,
            sent_bytes: sent_bytes as f64,
            total_bytes: total_bytes as f64,
            phase: convert::upload_phase(phase),
        },
        CoreEventPayload::UploadProgress {
            nonce,
            file_index,
            sent_bytes,
            total_bytes,
            phase,
            ..
        } => MobileEvent::SendProgress {
            server_key,
            nonce,
            index: file_index,
            sent_bytes: sent_bytes as f64,
            total_bytes: total_bytes as f64,
            phase: convert::upload_phase(phase),
        },
        CoreEventPayload::ServerNotice(notice) => MobileEvent::Notice {
            server_key,
            key: notice.key,
            title: notice.title,
            body: notice.body,
        },
        CoreEventPayload::VoiceSettingsUpdate(settings) => MobileEvent::SoundboardSettingsChanged {
            server_key,
            settings: convert::soundboard_settings(&settings),
        },
        CoreEventPayload::SoundboardSoundCreate(sound)
        | CoreEventPayload::SoundboardSoundUpdate(sound) => MobileEvent::SoundboardSoundUpserted {
            server_key,
            sound: convert::soundboard_sound(sound),
        },
        CoreEventPayload::SoundboardSoundDelete { sound_id } => {
            MobileEvent::SoundboardSoundDeleted {
                server_key,
                sound_id: id(sound_id),
            }
        }
        CoreEventPayload::VoiceChannelEffect(effect) => MobileEvent::SoundPlayed {
            server_key,
            effect: convert::sound_effect(effect),
        },
        CoreEventPayload::UploadSettingsUpdate(settings) => MobileEvent::VoiceMessageLimitChanged {
            server_key,
            max_seconds: settings.voice_message_max_seconds,
        },
        CoreEventPayload::CommandsUpdate(list) => MobileEvent::CommandsUpdated {
            server_key,
            commands: convert::command_list(list),
        },
        CoreEventPayload::EphemeralMessage(message) => MobileEvent::EphemeralReceived {
            server_key,
            note: convert::ephemeral_note(message),
        },
        CoreEventPayload::PluginUsersUpdate(users) => MobileEvent::PluginUsersUpdated {
            server_key,
            plugin_users: users.into_iter().map(convert::plugin_user).collect(),
        },
        CoreEventPayload::StreamCreate(_)
        | CoreEventPayload::StreamUpdate(_)
        | CoreEventPayload::StreamDelete { .. }
        | CoreEventPayload::StreamViewersUpdate { .. }
        | CoreEventPayload::ServerAddressesUpdate(_)
        | CoreEventPayload::NetworkStatus(_)
        | CoreEventPayload::LavalinkNodeStatus { .. }
        | CoreEventPayload::LavalinkNodeRemoved { .. }
        | CoreEventPayload::LavalinkNodeEvent(_)
        | CoreEventPayload::LavalinkTestProgress(_)
        | CoreEventPayload::PrivateSettingsUpdate(_)
        | CoreEventPayload::StorageUpdate(_)
        | CoreEventPayload::VoiceRejoinFailed { .. }
        | CoreEventPayload::StagedUploadProgress { .. }
        | CoreEventPayload::DownloadProgress { .. }
        | CoreEventPayload::PluginUpdate(_)
        | CoreEventPayload::PluginRemove { .. }
        | CoreEventPayload::PluginReviewUpdate(_)
        | CoreEventPayload::PluginReviewRemove { .. }
        | CoreEventPayload::PluginLogAppend { .. } => return None,
    })
}

/// Voice media's event, or `None` for the kinds the phone apps don't use.
pub(crate) fn media_event(event: MediaEvent) -> Option<MobileEvent> {
    Some(match event {
        MediaEvent::ConnectionState {
            server_key,
            channel_id,
            state,
        } => MobileEvent::VoiceLink {
            server_key,
            channel_id: id(channel_id),
            state: convert::voice_link(state),
        },
        MediaEvent::DeviceFellBack { output, device } => MobileEvent::AudioProblem {
            output,
            fell_back: true,
            message: device,
        },
        MediaEvent::DeviceFailed { output, message } => MobileEvent::AudioProblem {
            output,
            fell_back: false,
            message,
        },
        MediaEvent::Speaking {
            server_key,
            channel_id,
            changes,
        } => MobileEvent::Speaking {
            server_key,
            channel_id: id(channel_id),
            changes: changes.into_iter().map(convert::speaking).collect(),
        },
        MediaEvent::SpeakingWhileMuted => MobileEvent::SpeakingWhileMuted,
        MediaEvent::InputLevel { dbfs } => MobileEvent::InputLevel { dbfs },
        MediaEvent::NoiseSuppressionFellBack => MobileEvent::NoiseSuppressionFellBack,
        MediaEvent::VoiceRecordLevel { level } => MobileEvent::VoiceRecordLevel { level },
        MediaEvent::VoiceRecordStopped { reason } => MobileEvent::VoiceRecordStopped {
            max_length: reason == core::VoiceStopReason::MaxLength,
        },
        MediaEvent::PlaybackUpdate {
            item,
            position_ms,
            duration_ms,
            playing,
            buffering,
        } => MobileEvent::Playback {
            item: convert::playback_item(item),
            position_ms,
            duration_ms,
            playing,
            buffering,
        },
        MediaEvent::PlaybackEnded { item } => MobileEvent::PlaybackEnded {
            item: convert::playback_item(item),
        },
        MediaEvent::PlaybackFailed { item, message } => MobileEvent::PlaybackFailed {
            item: convert::playback_item(item),
            message,
        },
        MediaEvent::DevicesChanged(_)
        | MediaEvent::HotkeyPressed { .. }
        | MediaEvent::VideoTrackAdded { .. }
        | MediaEvent::VideoTrackRemoved { .. }
        | MediaEvent::VideoSizeChanged { .. }
        | MediaEvent::CameraStopped { .. }
        | MediaEvent::ScreenShareStopped { .. }
        | MediaEvent::ConnectionStats { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use opencord_core::api::types::{Message, SpeakingChange};
    use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

    use super::*;

    struct Fed {
        pump: Arc<EventPump>,
        core: UnboundedSender<CoreEvent>,
        media: UnboundedSender<MediaEvent>,
        /// Stands in for the core's private stream, connected as
        /// `private_open` connects it.
        private: UnboundedSender<PrivateEvent>,
    }

    fn fed() -> Fed {
        let (core, core_rx) = unbounded_channel();
        let (media, media_rx) = unbounded_channel();
        let (private, private_rx) = unbounded_channel();
        let pump = Arc::new(EventPump::new(core_rx, Some(media_rx)));
        let forward = pump
            .connect_private(|| private_rx)
            .expect("the first connection is taken");
        tokio::spawn(forward);
        Fed {
            pump,
            core,
            media,
            private,
        }
    }

    fn contact(peer: &str) -> core::PrivateContact {
        core::PrivateContact {
            peer: peer.to_owned(),
            name: Some("Kai".to_owned()),
            friend: core::FriendStatus::Friends,
            request_spam: false,
            blocked: false,
            conversation: core::ConversationStatus::Accepted,
            relay_server_key: Some("s".to_owned()),
            last_activity_ms: Some(1_700_000_000_123),
            unread: 2,
            muted: true,
            hidden: false,
            preview: Some("hi".to_owned()),
            verification: core::PrivateVerification::Verified,
            new_keys: true,
            reachable: true,
        }
    }

    fn private_message(id: &str, state: core::PrivateSendState) -> core::PrivateMessage {
        core::PrivateMessage {
            id: id.to_owned(),
            peer: "aa".to_owned(),
            outgoing: true,
            kind: core::PrivateMessageKind::Text,
            content: "hello".to_owned(),
            server_key: None,
            sent_at_ms: 1_700_000_000_456,
            edited_at_ms: Some(1_700_000_000_789),
            state,
        }
    }

    /// Batches under `generation` until `count` events came, or panics.
    async fn pull(pump: &EventPump, generation: u64, count: usize) -> Vec<MobileEvent> {
        let mut events = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), async {
            while events.len() < count {
                let batch = pump.next(generation, 64).await;
                assert!(!batch.is_empty(), "the stream ended");
                events.extend(batch);
            }
        })
        .await
        .unwrap_or_else(|_| panic!("got {events:?}, not {count} events"));
        events
    }

    // Fails if a private event is dropped on its way to the queue, or a
    // field is lost or changed on the way (times as numbers, enums as the
    // core's).
    #[tokio::test]
    async fn private_events_reach_javascript() {
        let fed = fed();
        let generation = fed.pump.attach();
        let sent = vec![
            PrivateEvent::OwnDevicesChanged {
                devices: vec![core::OwnDevice {
                    device_id: "d1".to_owned(),
                    state: core::OwnDeviceState::Offering,
                }],
                has_chats: true,
            },
            PrivateEvent::ContactChanged(contact("aa")),
            PrivateEvent::ContactRemoved {
                peer: "bb".to_owned(),
            },
            PrivateEvent::MessageAdded(private_message("m1", core::PrivateSendState::Sending)),
            PrivateEvent::MessageChanged(private_message("m1", core::PrivateSendState::Sent)),
            PrivateEvent::MessageDeleted {
                peer: "aa".to_owned(),
                message_id: "m1".to_owned(),
            },
            PrivateEvent::Typing {
                peer: "aa".to_owned(),
            },
            PrivateEvent::FriendRequestReceived {
                peer: "cc".to_owned(),
                spam: true,
            },
        ];
        for event in sent {
            fed.private.send(event).unwrap();
        }
        let message = |state| PrivateMessageRecord {
            id: "m1".to_owned(),
            peer: "aa".to_owned(),
            outgoing: true,
            kind: crate::private_types::PrivateMessageKind::Text,
            content: "hello".to_owned(),
            server_key: None,
            sent_at_ms: 1_700_000_000_456.0,
            edited_at_ms: Some(1_700_000_000_789.0),
            state,
        };
        let expected: Vec<_> = [
            PrivateEventRecord::OwnDevicesChanged {
                devices: vec![OwnDeviceRecord {
                    device_id: "d1".to_owned(),
                    state: crate::private_types::OwnDeviceState::Offering,
                }],
                has_chats: true,
            },
            PrivateEventRecord::ContactChanged {
                contact: PrivateContactRecord {
                    peer: "aa".to_owned(),
                    name: Some("Kai".to_owned()),
                    friend: crate::private_types::FriendStatus::Friends,
                    request_spam: false,
                    blocked: false,
                    conversation: crate::private_types::ConversationStatus::Accepted,
                    relay_server_key: Some("s".to_owned()),
                    last_activity_ms: Some(1_700_000_000_123.0),
                    unread: 2,
                    muted: true,
                    hidden: false,
                    preview: Some("hi".to_owned()),
                    verification: crate::private_types::PrivateVerification::Verified,
                    new_keys: true,
                    reachable: true,
                },
            },
            PrivateEventRecord::ContactRemoved {
                peer: "bb".to_owned(),
            },
            PrivateEventRecord::MessageAdded {
                message: message(crate::private_types::PrivateSendState::Sending),
            },
            PrivateEventRecord::MessageChanged {
                message: message(crate::private_types::PrivateSendState::Sent),
            },
            PrivateEventRecord::MessageDeleted {
                peer: "aa".to_owned(),
                message_id: "m1".to_owned(),
            },
            PrivateEventRecord::Typing {
                peer: "aa".to_owned(),
            },
            PrivateEventRecord::FriendRequestReceived {
                peer: "cc".to_owned(),
                spam: true,
            },
        ]
        .into_iter()
        .map(|event| MobileEvent::Private { event })
        .collect();
        assert_eq!(pull(&fed.pump, generation, 8).await, expected);
    }

    // Fails if the private stream ignores the generation rule: a call
    // left waiting by a reload would take events the new start never sees.
    #[tokio::test]
    async fn an_old_generation_gets_no_private_events() {
        let fed = fed();
        let old = fed.pump.attach();
        let new = fed.pump.attach();
        fed.private
            .send(PrivateEvent::Typing {
                peer: "aa".to_owned(),
            })
            .unwrap();
        // Let the forwarder put it in the queue first.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let stale = tokio::time::timeout(Duration::from_millis(100), fed.pump.next(old, 10))
            .await
            .expect("a stale generation waited");
        assert!(stale.is_empty());
        assert_eq!(
            pull(&fed.pump, new, 1).await,
            vec![MobileEvent::Private {
                event: PrivateEventRecord::Typing {
                    peer: "aa".to_owned()
                },
            }]
        );
    }

    /// The peers of the contacts in `events`' `ContactChanged`s.
    fn changed_peers(events: &[MobileEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|event| match event {
                MobileEvent::Private {
                    event: PrivateEventRecord::ContactChanged { contact },
                } => Some(contact.peer.clone()),
                _ => None,
            })
            .collect()
    }

    // Fails if a second `private_open` (a JavaScript reload) takes the
    // core's private stream again: the first receiver then hears nothing
    // more, and what it had queued is lost. The real core and its real
    // database: blocking someone sends `ContactChanged` with no server.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_reload_keeps_the_private_stream() {
        let dir = tempfile::tempdir().unwrap();
        let mobile = crate::Mobile::start(
            &dir.path().join("data"),
            &dir.path().join("cache"),
            tokio::runtime::Handle::current(),
            false,
        )
        .unwrap();
        mobile.client().set_identity(
            opencord_core::identity::Identity::generate(),
            "Ava".to_owned(),
        );
        let first_peer = "aa".repeat(32);
        let second_peer = "bb".repeat(32);

        mobile.attach();
        mobile.private_open([7; 32]).await.unwrap();
        mobile.client().private_block(&first_peer).await.unwrap();

        // A JavaScript reload: a new generation, and boot opens again.
        let generation = mobile.attach();
        let opened = mobile.private_open([7; 32]).await.unwrap();
        assert!(!opened.started_fresh);
        mobile.client().private_block(&second_peer).await.unwrap();

        let mut events = Vec::new();
        tokio::time::timeout(Duration::from_secs(10), async {
            while changed_peers(&events).len() < 2 {
                events.extend(mobile.next_events(generation, 64).await);
            }
        })
        .await
        .unwrap_or_else(|_| panic!("got {:?}", changed_peers(&events)));
        assert_eq!(changed_peers(&events), [first_peer, second_peer]);
    }

    fn message(id: i64) -> CoreEvent {
        CoreEvent {
            server_key: "s".to_owned(),
            payload: CoreEventPayload::MessageCreate(Message {
                id,
                channel_id: 10,
                author_id: 1,
                content: format!("m{id}"),
                created_at_ms: 0,
                edited_at_ms: None,
                nonce: None,
                command: None,
                command_user_id: None,
                command_state: core::CommandRunState::Answered,
                attachments: Vec::new(),
            }),
        }
    }

    fn message_id(event: &MobileEvent) -> &str {
        match event {
            MobileEvent::MessageCreated { message, .. } => &message.id,
            other => panic!("expected a message, got {other:?}"),
        }
    }

    fn session(viewers: Vec<i64>) -> core::ActivitySession {
        core::ActivitySession {
            id: 900,
            channel_id: 20,
            plugin_id: "paint".to_owned(),
            activity_key: "paint".to_owned(),
            name: "Paint together".to_owned(),
            icon: "assets/icon.png".to_owned(),
            plugin_name: "Shared Paint".to_owned(),
            starter_id: 7,
            started_at_ms: 1_700_000_000_000,
            viewer_ids: viewers,
        }
    }

    fn incoming(payload: CoreEventPayload) -> MobileEvent {
        core_event(CoreEvent {
            server_key: "s".to_owned(),
            payload,
        })
        .expect("the event reaches JavaScript")
    }

    // Fails if one of the plugin events is dropped again, or a field is lost
    // or changed on the way (ids as strings, the reason as the core's).
    #[test]
    fn the_plugin_events_reach_javascript() {
        let types = incoming(CoreEventPayload::PluginChannelTypesUpdate(vec![
            core::PluginChannelType {
                plugin_id: "tickets".to_owned(),
                type_key: "tickets".to_owned(),
                name: "Tickets".to_owned(),
                description: "Open and track support tickets".to_owned(),
                icon: "assets/ticket.png".to_owned(),
                plugin_name: "Tickets".to_owned(),
            },
        ]));
        assert_eq!(
            types,
            MobileEvent::PluginChannelTypesUpdated {
                server_key: "s".to_owned(),
                types: vec![PluginChannelTypeRecord {
                    plugin_id: "tickets".to_owned(),
                    type_key: "tickets".to_owned(),
                    name: "Tickets".to_owned(),
                    description: "Open and track support tickets".to_owned(),
                    icon: "assets/ticket.png".to_owned(),
                    plugin_name: "Tickets".to_owned(),
                }],
            }
        );

        let activities = incoming(CoreEventPayload::PluginActivitiesUpdate(vec![
            core::PluginActivity {
                plugin_id: "paint".to_owned(),
                activity_key: "paint".to_owned(),
                name: "Paint together".to_owned(),
                description: "Draw with everyone in the call".to_owned(),
                icon: "assets/icon.png".to_owned(),
                plugin_name: "Shared Paint".to_owned(),
            },
        ]));
        assert_eq!(
            activities,
            MobileEvent::PluginActivitiesUpdated {
                server_key: "s".to_owned(),
                activities: vec![PluginActivityRecord {
                    plugin_id: "paint".to_owned(),
                    activity_key: "paint".to_owned(),
                    name: "Paint together".to_owned(),
                    description: "Draw with everyone in the call".to_owned(),
                    icon: "assets/icon.png".to_owned(),
                    plugin_name: "Shared Paint".to_owned(),
                }],
            }
        );

        let expected = ActivitySessionRecord {
            id: "900".to_owned(),
            channel_id: "20".to_owned(),
            plugin_id: "paint".to_owned(),
            activity_key: "paint".to_owned(),
            name: "Paint together".to_owned(),
            icon: "assets/icon.png".to_owned(),
            plugin_name: "Shared Paint".to_owned(),
            starter_id: "7".to_owned(),
            started_at_ms: 1_700_000_000_000,
            viewer_ids: vec!["7".to_owned(), "8".to_owned()],
        };
        assert_eq!(
            incoming(CoreEventPayload::ActivitySessionCreate(session(vec![7, 8]))),
            MobileEvent::ActivitySessionCreated {
                server_key: "s".to_owned(),
                session: expected.clone(),
            }
        );
        assert_eq!(
            incoming(CoreEventPayload::ActivitySessionUpdate(session(vec![7, 8]))),
            MobileEvent::ActivitySessionUpdated {
                server_key: "s".to_owned(),
                session: expected,
            }
        );
        for (core_reason, reason) in [
            (core::ActivityEndReason::Ended, ActivityEndReason::Ended),
            (core::ActivityEndReason::Plugin, ActivityEndReason::Plugin),
            (
                core::ActivityEndReason::CallEmpty,
                ActivityEndReason::CallEmpty,
            ),
            (
                core::ActivityEndReason::PluginStopped,
                ActivityEndReason::PluginStopped,
            ),
            (
                core::ActivityEndReason::Unavailable,
                ActivityEndReason::Unavailable,
            ),
        ] {
            assert_eq!(
                incoming(CoreEventPayload::ActivitySessionDelete {
                    session_id: 900,
                    channel_id: 20,
                    reason: core_reason,
                }),
                MobileEvent::ActivitySessionDeleted {
                    server_key: "s".to_owned(),
                    session_id: "900".to_owned(),
                    channel_id: "20".to_owned(),
                    reason,
                }
            );
        }
    }

    #[tokio::test]
    async fn a_batch_holds_what_is_ready_up_to_max() {
        let fed = fed();
        let generation = fed.pump.attach();
        for id in 1..=5 {
            fed.core.send(message(id)).unwrap();
        }
        let first = fed.pump.next(generation, 3).await;
        let second = fed.pump.next(generation, 3).await;
        let ids: Vec<_> = first.iter().chain(&second).map(message_id).collect();
        assert_eq!(first.len(), 3);
        assert_eq!(second.len(), 2);
        assert_eq!(ids, ["1", "2", "3", "4", "5"]);
    }

    #[tokio::test]
    async fn dropped_kinds_never_end_a_wait() {
        let fed = fed();
        let generation = fed.pump.attach();
        fed.core
            .send(CoreEvent {
                server_key: "s".to_owned(),
                payload: CoreEventPayload::PrivateSettingsUpdate(core::PrivateSettings::default()),
            })
            .unwrap();
        let pump = Arc::clone(&fed.pump);
        let waiting = tokio::spawn(async move { pump.next(generation, 10).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!waiting.is_finished(), "a dropped kind ended the wait");
        fed.core.send(message(9)).unwrap();
        let batch = waiting.await.unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(message_id(&batch[0]), "9");
    }

    #[tokio::test]
    async fn a_new_attach_releases_the_waiting_call() {
        let fed = fed();
        let old = fed.pump.attach();
        let pump = Arc::clone(&fed.pump);
        let waiting = tokio::spawn(async move { pump.next(old, 10).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        let new = fed.pump.attach();
        let released = tokio::time::timeout(Duration::from_millis(100), waiting)
            .await
            .expect("the old call was not released")
            .unwrap();
        assert!(released.is_empty());
        fed.core.send(message(3)).unwrap();
        let batch = tokio::time::timeout(Duration::from_millis(500), fed.pump.next(new, 10))
            .await
            .expect("the new generation got nothing");
        assert_eq!(message_id(&batch[0]), "3");
    }

    #[tokio::test]
    async fn an_old_generation_gets_nothing() {
        let fed = fed();
        let old = fed.pump.attach();
        fed.pump.attach();
        fed.core.send(message(1)).unwrap();
        let batch = tokio::time::timeout(Duration::from_millis(100), fed.pump.next(old, 10))
            .await
            .expect("a stale generation waited");
        assert!(batch.is_empty());
    }

    #[tokio::test]
    async fn media_and_server_events_both_arrive() {
        let fed = fed();
        let generation = fed.pump.attach();
        fed.media
            .send(MediaEvent::Speaking {
                server_key: "s".to_owned(),
                channel_id: 20,
                changes: vec![SpeakingChange {
                    user_id: 2,
                    speaking: true,
                }],
            })
            .unwrap();
        fed.core
            .send(CoreEvent {
                server_key: "s".to_owned(),
                payload: CoreEventPayload::TypingStart {
                    channel_id: 10,
                    user_id: 3,
                },
            })
            .unwrap();
        let batch = fed.pump.next(generation, 10).await;
        assert_eq!(batch.len(), 2);
        assert!(batch.iter().any(
            |event| matches!(event, MobileEvent::Speaking { channel_id, .. } if channel_id == "20")
        ));
        assert!(
            batch.iter().any(
                |event| matches!(event, MobileEvent::Typing { user_id, .. } if user_id == "3")
            )
        );
    }

    // Fails if the microphone's level is dropped again, or its value is
    // changed on the way to the level meter.
    #[tokio::test]
    async fn input_level_reaches_javascript() {
        let fed = fed();
        let generation = fed.pump.attach();
        fed.media
            .send(MediaEvent::InputLevel { dbfs: -32.5 })
            .unwrap();
        let batch = fed.pump.next(generation, 10).await;
        assert_eq!(batch, vec![MobileEvent::InputLevel { dbfs: -32.5 }]);
    }

    // Fails if the fallback from High noise suppression is dropped: the
    // page would keep showing High while the core runs Standard.
    #[test]
    fn a_fallback_reaches_javascript() {
        assert_eq!(
            media_event(MediaEvent::NoiseSuppressionFellBack),
            Some(MobileEvent::NoiseSuppressionFellBack)
        );
    }

    // Fails if RoleCreate, RoleDelete, or PresenceUpdate are dropped or fail to convert ids to strings.
    #[test]
    fn role_and_presence_events_reach_javascript() {
        use opencord_common::permissions::Permissions;

        let role = core::Role {
            id: 10,
            name: "Moderator".to_owned(),
            color: 0x5865f2,
            position: 1,
            permissions: i64::from_ne_bytes(Permissions::MANAGE_ROLES.bits().to_ne_bytes()),
            hoist: true,
            mentionable: true,
            max_upload_bytes: None,
            plugin_id: None,
        };
        let created = incoming(CoreEventPayload::RoleCreate(role.clone()));
        match created {
            MobileEvent::RoleUpserted {
                server_key,
                role: r,
            } => {
                assert_eq!(server_key, "s");
                assert_eq!(r.id, "10");
                assert_eq!(r.name, "Moderator");
                assert!(r.access.manage_roles);
            }
            other => panic!("expected RoleUpserted, got {other:?}"),
        }

        let deleted = incoming(CoreEventPayload::RoleDelete { role_id: 7 });
        match deleted {
            MobileEvent::RoleDeleted {
                server_key,
                role_id,
            } => {
                assert_eq!(server_key, "s");
                assert_eq!(role_id, "7");
            }
            other => panic!("expected RoleDeleted, got {other:?}"),
        }

        let presence = core::Presence {
            user_id: 5,
            status: core::PresenceStatus::Idle,
        };
        let presence_event = incoming(CoreEventPayload::PresenceUpdate(presence));
        match presence_event {
            MobileEvent::PresenceChanged {
                server_key,
                presence: p,
            } => {
                assert_eq!(server_key, "s");
                assert_eq!(p.user_id, "5");
                assert_eq!(p.status, PresenceStatus::Idle);
            }
            other => panic!("expected PresenceChanged, got {other:?}"),
        }
    }

    // Fails if PermissionsUpdate drops server_permissions or AccessChanged omits server_access.
    #[test]
    fn a_permissions_update_carries_server_access() {
        use opencord_common::permissions::Permissions;

        let event = incoming(CoreEventPayload::PermissionsUpdate {
            server_permissions: i64::from_ne_bytes(
                Permissions::MANAGE_CHANNELS.bits().to_ne_bytes(),
            ),
            channel_permissions: Vec::new(),
        });
        match event {
            MobileEvent::AccessChanged {
                server_key,
                access,
                server_access,
            } => {
                assert_eq!(server_key, "s");
                assert!(access.is_empty());
                assert!(server_access.manage_channels);
                assert!(!server_access.administrator);
            }
            other => panic!("expected AccessChanged, got {other:?}"),
        }
    }
}
