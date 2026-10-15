//! State shared by every connection, and event fan-out.

use std::collections::HashSet;
use std::sync::{Mutex as StdMutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use ed25519_dalek::SigningKey;
use opencord_common::address::Fingerprint;
use opencord_common::snowflake::SnowflakeGenerator;
use opencord_proto::v1 as proto;
use sqlx::SqlitePool;
use tokio::sync::{Mutex, MutexGuard};
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::gateway::session::SessionRegistry;
use crate::guild::Guild;
use crate::presence::Presence;
use crate::rate_limit::RateLimits;
use crate::voice::nodes::VoiceNodes;
use crate::voice::states::VoiceStates;

/// Who receives an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    Everyone,
    /// Sessions whose user can view the channel.
    Channel(i64),
    /// Sessions whose user is connected to the voice channel. Never
    /// broadcast to it while holding the voice states.
    Voice(i64),
    User(i64),
}

#[derive(Debug)]
pub struct AppState {
    pub config: Config,
    pub db: SqlitePool,
    pub ids: SnowflakeGenerator,
    pub fingerprint: Fingerprint,
    pub sessions: SessionRegistry,
    pub presence: Presence,
    pub rate_limits: RateLimits,
    /// Cancelled when the server shuts down.
    pub shutdown: CancellationToken,
    /// Signs voice and media tokens.
    pub voice_key: SigningKey,
    pub voice_nodes: VoiceNodes,
    /// Domains & network (Phase 3 plan §5).
    pub network: crate::network::Network,
    /// The domains attached (Phase 3 plan §6).
    pub domains: crate::domains::Domains,
    /// Their certificates (Phase 3 plan §7).
    pub certificates: crate::certs::Certificates,
    /// Where Lavalink plays into, when music is on (Phase 3 plan §13).
    pub music_bridge: Option<opencord_voice::bridge::MusicBridge>,
    guild: RwLock<Guild>,
    /// Lock after the guild, never before it.
    voice: StdMutex<VoiceStates>,
    writes: Mutex<()>,
}

impl AppState {
    pub fn new(
        config: Config,
        db: SqlitePool,
        ids: SnowflakeGenerator,
        fingerprint: Fingerprint,
        guild: Guild,
        voice_key: SigningKey,
        voice_nodes: VoiceNodes,
    ) -> Self {
        let rate_limits = RateLimits::with_identify_per_minute(config.gateway.identify_per_minute);
        let network = crate::network::Network::new(
            crate::network::settings::NetworkSettings::default(),
            crate::network::Lookup {
                v4: config.network.ip_lookup_v4.clone(),
                v6: config.network.ip_lookup_v6.clone(),
            },
            crate::tls::http_client().expect("the HTTPS client builds with the ring provider"),
            config.network.dns_name_server_port,
        );
        Self {
            config,
            db,
            ids,
            fingerprint,
            sessions: SessionRegistry::default(),
            presence: Presence::default(),
            rate_limits,
            shutdown: CancellationToken::new(),
            voice_key,
            voice_nodes,
            network,
            domains: crate::domains::Domains::default(),
            certificates: crate::certs::Certificates::throwaway(),
            music_bridge: None,
            guild: RwLock::new(guild),
            voice: StdMutex::new(VoiceStates::default()),
            writes: Mutex::new(()),
        }
    }

    /// With the certificate store the TLS listener serves from.
    #[must_use]
    pub fn with_certificates(mut self, certificates: crate::certs::Certificates) -> Self {
        self.certificates = certificates;
        self
    }

    /// With the domains saved.
    #[must_use]
    pub fn with_domains(mut self, domains: crate::domains::Domains) -> Self {
        self.domains = domains;
        self
    }

    /// With the audio bridge music plays into.
    #[must_use]
    pub fn with_music_bridge(
        mut self,
        bridge: Option<opencord_voice::bridge::MusicBridge>,
    ) -> Self {
        self.music_bridge = bridge;
        self
    }

    pub fn guild(&self) -> RwLockReadGuard<'_, Guild> {
        self.guild.read().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn guild_mut(&self) -> RwLockWriteGuard<'_, Guild> {
        self.guild.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Voice states. Never lock the guild while holding this.
    pub fn voice(&self) -> std::sync::MutexGuard<'_, VoiceStates> {
        self.voice.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Serializes changes to roles, channels, members, voice and settings,
    /// so the database, the cache and the events sent stay in step.
    pub async fn write_lock(&self) -> MutexGuard<'_, ()> {
        self.writes.lock().await
    }

    pub fn broadcast(&self, kind: proto::event::Kind, audience: Audience) {
        let event = proto::Event { kind: Some(kind) };
        let now = Instant::now();
        let guild = self.guild();
        let in_voice: HashSet<i64> = match audience {
            Audience::Voice(channel_id) => self
                .voice()
                .in_channel(channel_id)
                .map(|state| state.user_id)
                .collect(),
            _ => HashSet::new(),
        };
        for session in self.sessions.all() {
            let receives = match audience {
                Audience::Everyone => true,
                Audience::Channel(channel_id) => guild.can_view(session.user_id, channel_id),
                Audience::Voice(_) => in_voice.contains(&session.user_id),
                Audience::User(user_id) => session.user_id == user_id,
            };
            if receives {
                session.push_event(&event, now);
            }
        }
    }

    /// What other members see for this user.
    pub fn presence_of(&self, user_id: i64) -> proto::PresenceStatus {
        if self.sessions.has_user(user_id) {
            self.presence.chosen(user_id)
        } else {
            proto::PresenceStatus::Offline
        }
    }

    /// Every member who does not appear offline.
    pub fn presences(&self) -> Vec<proto::Presence> {
        let member_ids: Vec<i64> = self.guild().members.keys().copied().collect();
        member_ids
            .into_iter()
            .map(|user_id| proto::Presence {
                user_id,
                status: self.presence_of(user_id) as i32,
            })
            .filter(|presence| presence.status != proto::PresenceStatus::Offline as i32)
            .collect()
    }

    pub fn broadcast_presence(&self, user_id: i64) {
        let presence = proto::Presence {
            user_id,
            status: self.presence_of(user_id) as i32,
        };
        self.broadcast(
            proto::event::Kind::PresenceUpdate(proto::PresenceUpdate {
                presence: Some(presence),
            }),
            Audience::Everyone,
        );
    }
}

pub fn now_ms() -> i64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX)
}
