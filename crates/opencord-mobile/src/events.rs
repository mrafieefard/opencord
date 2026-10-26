//! Events, pulled by JavaScript in batches (design §5.4).
//!
//! A callback from a Rust thread into JavaScript would block that thread
//! until JavaScript returns, so the app instead loops on `next_events`.
//! Each JavaScript start (a reload too) calls `events_attach` first: a
//! newer generation releases a call still waiting under an older one, so
//! the stream is never held by a JavaScript context that is gone.

use std::sync::atomic::{AtomicU64, Ordering};

use opencord_core::api::types::{CoreEvent, CoreEventPayload, MediaEvent};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{Mutex, Notify};

use crate::convert;
use crate::ids::id;
use crate::types::*;

/// The most one batch holds.
pub const BATCH_MAX: u32 = 1024;

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum MobileEvent {
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
    /// The user's own permissions changed.
    AccessChanged {
        server_key: String,
        access: Vec<ChannelAccess>,
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
}

enum Incoming {
    Core(Box<CoreEvent>),
    Media(MediaEvent),
}

struct Receivers {
    core: UnboundedReceiver<CoreEvent>,
    media: Option<UnboundedReceiver<MediaEvent>>,
}

impl Receivers {
    /// The next event from either stream; `None` once the core's has ended.
    async fn recv(&mut self) -> Option<Incoming> {
        loop {
            let Some(media) = self.media.as_mut() else {
                return self
                    .core
                    .recv()
                    .await
                    .map(|event| Incoming::Core(Box::new(event)));
            };
            tokio::select! {
                event = self.core.recv() => return event.map(|event| Incoming::Core(Box::new(event))),
                event = media.recv() => match event {
                    Some(event) => return Some(Incoming::Media(event)),
                    // Media has stopped for good; carry on with the core's.
                    None => self.media = None,
                },
            }
        }
    }

    fn try_recv(&mut self) -> Option<Incoming> {
        if let Ok(event) = self.core.try_recv() {
            return Some(Incoming::Core(Box::new(event)));
        }
        self.media
            .as_mut()
            .and_then(|media| media.try_recv().ok())
            .map(Incoming::Media)
    }
}

/// The core's two event streams, handed out in batches to one waiter at a
/// time.
pub struct EventPump {
    receivers: Mutex<Receivers>,
    generation: AtomicU64,
    attached: Notify,
}

impl EventPump {
    pub fn new(
        core: UnboundedReceiver<CoreEvent>,
        media: Option<UnboundedReceiver<MediaEvent>>,
    ) -> Self {
        Self {
            receivers: Mutex::new(Receivers { core, media }),
            generation: AtomicU64::new(0),
            attached: Notify::new(),
        }
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
                channel: convert::channel(channel),
            }
        }
        CoreEventPayload::ChannelDelete { channel_id } => MobileEvent::ChannelDeleted {
            server_key,
            channel_id: id(channel_id),
        },
        CoreEventPayload::PermissionsUpdate {
            channel_permissions,
            ..
        } => MobileEvent::AccessChanged {
            server_key,
            access: convert::access_list(channel_permissions),
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
        CoreEventPayload::RoleCreate(_)
        | CoreEventPayload::RoleUpdate(_)
        | CoreEventPayload::RoleDelete { .. }
        | CoreEventPayload::PresenceUpdate(_)
        | CoreEventPayload::VoiceSettingsUpdate(_)
        | CoreEventPayload::StreamCreate(_)
        | CoreEventPayload::StreamUpdate(_)
        | CoreEventPayload::StreamDelete { .. }
        | CoreEventPayload::StreamViewersUpdate { .. }
        | CoreEventPayload::SoundboardSoundCreate(_)
        | CoreEventPayload::SoundboardSoundUpdate(_)
        | CoreEventPayload::SoundboardSoundDelete { .. }
        | CoreEventPayload::VoiceChannelEffect(_)
        | CoreEventPayload::ServerAddressesUpdate(_)
        | CoreEventPayload::NetworkStatus(_)
        | CoreEventPayload::LavalinkNodeStatus { .. }
        | CoreEventPayload::LavalinkNodeRemoved { .. }
        | CoreEventPayload::LavalinkNodeEvent(_)
        | CoreEventPayload::LavalinkTestProgress(_)
        | CoreEventPayload::CommandsUpdate(_)
        | CoreEventPayload::EphemeralMessage(_)
        | CoreEventPayload::UploadSettingsUpdate(_)
        | CoreEventPayload::PrivateSettingsUpdate(_)
        | CoreEventPayload::StorageUpdate(_)
        | CoreEventPayload::StagedUploadProgress { .. }
        | CoreEventPayload::DownloadProgress { .. } => return None,
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
        MediaEvent::DevicesChanged(_)
        | MediaEvent::InputLevel { .. }
        | MediaEvent::NoiseSuppressionFellBack
        | MediaEvent::HotkeyPressed { .. }
        | MediaEvent::VideoTrackAdded { .. }
        | MediaEvent::VideoTrackRemoved { .. }
        | MediaEvent::CameraStopped { .. }
        | MediaEvent::ScreenShareStopped { .. }
        | MediaEvent::ConnectionStats { .. }
        | MediaEvent::VoiceRecordLevel { .. }
        | MediaEvent::VoiceRecordStopped { .. }
        | MediaEvent::PlaybackUpdate { .. }
        | MediaEvent::PlaybackEnded { .. }
        | MediaEvent::PlaybackFailed { .. } => return None,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use opencord_core::api::types::{Message, Role, SpeakingChange};
    use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

    use super::*;

    struct Fed {
        pump: Arc<EventPump>,
        core: UnboundedSender<CoreEvent>,
        media: UnboundedSender<MediaEvent>,
    }

    fn fed() -> Fed {
        let (core, core_rx) = unbounded_channel();
        let (media, media_rx) = unbounded_channel();
        Fed {
            pump: Arc::new(EventPump::new(core_rx, Some(media_rx))),
            core,
            media,
        }
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
                payload: CoreEventPayload::RoleCreate(Role {
                    id: 1,
                    name: "r".to_owned(),
                    color: 0,
                    position: 0,
                    permissions: 0,
                    hoist: false,
                    mentionable: false,
                    max_upload_bytes: None,
                }),
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
}
