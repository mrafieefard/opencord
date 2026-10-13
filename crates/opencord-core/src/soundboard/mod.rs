//! The soundboard on this device (Phase 2 plan §11.3–11.5): every server's
//! sounds kept on disk ahead of time, sounds played in this device's voice
//! channel through the mixer, previews heard only here, and new sounds
//! made from a file in the trim editor (`drafts`).

mod cache;
mod drafts;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use opencord_common::sound::{
    CLIENT_CACHE_BYTES, DOWNLOADS_PER_SERVER, MAX_FILE_BYTES, SoundRef, defaults,
};
use opencord_media::soundboard::{self as audio, Pcm};
use opencord_proto::v1 as proto;
use prost::Message as _;
use tokio::runtime::Handle;
use tokio::sync::Semaphore;

pub(crate) use self::cache::SoundCache;
use crate::api::types::{
    CoreError, CoreEvent, CoreEventPayload, DefaultSound, ErrorCode, SoundEffect, SoundboardSound,
};
use crate::client::Client;
use crate::connection::MediaTokens;
use crate::convert;
use crate::http::{self, HttpError, HttpResponse};
use crate::store::Store;

/// What this person chose for sounds (plan §11.7, User settings → Sounds).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Listening {
    /// 0–1.
    volume: f32,
    muted: bool,
}

impl Default for Listening {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}

#[derive(Default)]
struct State {
    /// Each server's sounds by id.
    sounds: HashMap<String, HashMap<i64, SoundboardSound>>,
    /// Files being fetched now.
    fetching: HashSet<[u8; 32]>,
    downloads: HashMap<String, Arc<Semaphore>>,
    listening: Listening,
    /// Whose soundboard this person muted, per server.
    muted_players: HashSet<(String, i64)>,
    drafts: HashMap<u64, Pcm>,
    next_draft: u64,
}

pub(crate) struct Soundboard {
    runtime: Handle,
    store: Arc<Mutex<Store>>,
    tokens: MediaTokens,
    cache: SoundCache,
    state: Mutex<State>,
}

impl Soundboard {
    pub fn new(
        runtime: Handle,
        data_dir: &Path,
        store: Arc<Mutex<Store>>,
        tokens: MediaTokens,
    ) -> Self {
        Self {
            runtime,
            store,
            tokens,
            cache: SoundCache::new(data_dir, CLIENT_CACHE_BYTES),
            state: Mutex::new(State::default()),
        }
    }

    pub fn default_sounds() -> Vec<DefaultSound> {
        defaults::DEFAULT_SOUNDS
            .iter()
            .map(|sound| DefaultSound {
                id: sound.id.to_owned(),
                name: sound.name.to_owned(),
                emoji: sound.emoji.to_owned(),
                duration_ms: sound.duration_ms,
            })
            .collect()
    }

    /// Follows each server's sounds, fetching new ones in the background
    /// (plan §11.4), and plays sounds played in this device's channel.
    pub fn observe(&self, client: &Client, event: &CoreEvent) {
        let key = &event.server_key;
        match &event.payload {
            CoreEventPayload::Ready(snapshot) => {
                let sounds = snapshot
                    .soundboard_sounds
                    .iter()
                    .map(|sound| (sound.id, sound.clone()))
                    .collect();
                self.lock().sounds.insert(key.clone(), sounds);
                for sound in &snapshot.soundboard_sounds {
                    self.fetch_later(client, key, &sound.sha256);
                }
            }
            CoreEventPayload::SoundboardSoundCreate(sound)
            | CoreEventPayload::SoundboardSoundUpdate(sound) => {
                self.lock()
                    .sounds
                    .entry(key.clone())
                    .or_default()
                    .insert(sound.id, sound.clone());
                self.fetch_later(client, key, &sound.sha256);
            }
            CoreEventPayload::SoundboardSoundDelete { sound_id } => {
                if let Some(sounds) = self.lock().sounds.get_mut(key) {
                    sounds.remove(sound_id);
                }
            }
            CoreEventPayload::VoiceChannelEffect(effect) => self.on_effect(client, key, effect),
            _ => {}
        }
    }

    pub fn set_listening(&self, volume: f32, muted: bool) {
        self.lock().listening = Listening {
            volume: volume.clamp(0.0, 1.0),
            muted,
        };
    }

    /// "Mute their soundboard" (UI plan §17.3).
    pub fn set_player_muted(&self, key: &str, user_id: i64, muted: bool) {
        let mut state = self.lock();
        let player = (key.to_owned(), user_id);
        if muted {
            state.muted_players.insert(player);
        } else {
            state.muted_players.remove(&player);
        }
    }

    /// Plays a sound for everyone in the channel. An external sound (from
    /// `external`, another server's sound) is uploaded to this server first
    /// when it does not have it yet (plan §11.5).
    pub async fn play(
        &self,
        client: &Client,
        key: &str,
        channel_id: i64,
        sound_ref: &str,
        external: Option<(String, SoundboardSound)>,
    ) -> Result<(), CoreError> {
        let mut request = proto::PlaySoundboardSound {
            channel_id,
            sound_ref: sound_ref.to_owned(),
            ..Default::default()
        };
        if let Some((_, sound)) = &external {
            request.name = Some(sound.name.clone());
            request.emoji = sound.emoji.clone();
            request.volume = Some(sound.volume);
        }
        let play = || proto::request::Kind::PlaySoundboardSound(request.clone());
        let played = client.ack(key, play()).await;
        let not_here = matches!(
            played,
            Err(CoreError::Server {
                code: ErrorCode::NotFound,
                ..
            })
        );
        let Some((source, sound)) = external.filter(|_| not_here) else {
            return played;
        };
        let bytes = self.file(client, &source, &sound.sha256).await?;
        let response = self
            .http(client, key, "POST", "/media/external-sounds", &bytes)
            .await?;
        expect(response, |result| {
            matches!(result, proto::response::Result::Ack(_)).then_some(())
        })?;
        client.ack(key, play()).await
    }

    /// Plays a sound on this device only (plan §11.5).
    pub async fn preview(
        &self,
        client: &Client,
        key: &str,
        sound_ref: &str,
    ) -> Result<(), CoreError> {
        let (samples, volume) = self.samples(client, key, sound_ref).await?;
        let gain = f32_percent(volume) * self.lock().listening.volume;
        if let Some(media) = client.media() {
            media.preview(samples, gain);
        }
        Ok(())
    }

    fn on_effect(&self, client: &Client, key: &str, effect: &SoundEffect) {
        let listening = {
            let state = self.lock();
            if state
                .muted_players
                .contains(&(key.to_owned(), effect.user_id))
            {
                return;
            }
            state.listening
        };
        if listening.muted || listening.volume == 0.0 || client.media().is_none() {
            return;
        }
        let (soundboard, client) = (client.soundboard(), client.clone());
        let (key, effect) = (key.to_owned(), effect.clone());
        self.runtime.spawn(async move {
            match soundboard.samples(&client, &key, &effect.sound_ref).await {
                Ok((samples, _)) => {
                    let gain = f32_percent(effect.volume) * listening.volume;
                    if let Some(media) = client.media() {
                        media.play_effect(&key, effect.channel_id, effect.user_id, samples, gain);
                    }
                }
                Err(error) => tracing::debug!(%error, "a sound could not be played"),
            }
        });
    }

    /// A sound's samples, mono for the mixer, and its volume in percent.
    pub async fn samples(
        &self,
        client: &Client,
        key: &str,
        sound_ref: &str,
    ) -> Result<(Arc<[f32]>, u32), CoreError> {
        let sound = SoundRef::parse(sound_ref)
            .ok_or_else(|| invalid(format!("{sound_ref} is not a sound")))?;
        let (bytes, volume) = match sound {
            SoundRef::Default(sound) => (
                opencord_media::soundboard::defaults::file(sound.id)
                    .ok_or_else(|| invalid(format!("{sound_ref} is not a sound")))?
                    .to_vec(),
                opencord_common::sound::VOLUME_DEFAULT,
            ),
            SoundRef::Server(hash) | SoundRef::External(hash) => {
                let hex = hex::encode(hash);
                let volume = self
                    .lock()
                    .sounds
                    .get(key)
                    .and_then(|sounds| sounds.values().find(|sound| sound.sha256 == hex))
                    .map_or(opencord_common::sound::VOLUME_DEFAULT, |sound| sound.volume);
                (self.file(client, key, &hex).await?, volume)
            }
        };
        let samples = self
            .runtime
            .spawn_blocking(move || audio::decode_mono(&bytes))
            .await
            .map_err(|error| invalid(error.to_string()))?
            .map_err(|error| invalid(error.to_string()))?;
        Ok((Arc::from(samples), volume))
    }

    /// The file with this hash: from the cache, else fetched from the
    /// server at `key`.
    async fn file(
        &self,
        client: &Client,
        key: &str,
        sha256_hex: &str,
    ) -> Result<Vec<u8>, CoreError> {
        let hash = opencord_common::sound::sha256(sha256_hex)
            .ok_or_else(|| invalid(format!("{sha256_hex} is not a SHA-256")))?;
        if let Some(bytes) = self.cache.get(&hash) {
            return Ok(bytes);
        }
        let response = self
            .http(
                client,
                key,
                "GET",
                &format!("/media/sounds/{sha256_hex}"),
                &[],
            )
            .await?;
        if response.status != 200 {
            return Err(CoreError::Server {
                code: ErrorCode::NotFound,
                message: format!("the server has no sound {sha256_hex}"),
                retry_after_ms: None,
            });
        }
        let cache = self.cache.clone();
        let bytes = response.body;
        let stored = {
            let bytes = bytes.clone();
            self.runtime
                .spawn_blocking(move || cache.put(&hash, &bytes))
                .await
                .map_err(|error| invalid(error.to_string()))?
        };
        match stored {
            Ok(true) => Ok(bytes),
            Ok(false) => Err(invalid("the server sent a different file".to_owned())),
            Err(error) => Err(CoreError::Storage {
                message: error.to_string(),
            }),
        }
    }

    /// Fetches a sound in the background unless it is here already, at
    /// most two at a time per server.
    fn fetch_later(&self, client: &Client, key: &str, sha256_hex: &str) {
        let Some(hash) = opencord_common::sound::sha256(sha256_hex) else {
            return;
        };
        let downloads = {
            let mut state = self.lock();
            if self.cache.contains(&hash) || !state.fetching.insert(hash) {
                return;
            }
            Arc::clone(
                state
                    .downloads
                    .entry(key.to_owned())
                    .or_insert_with(|| Arc::new(Semaphore::new(DOWNLOADS_PER_SERVER))),
            )
        };
        let (soundboard, client) = (client.soundboard(), client.clone());
        let (key, hex) = (key.to_owned(), sha256_hex.to_owned());
        self.runtime.spawn(async move {
            if let Ok(_turn) = downloads.acquire().await
                && let Err(error) = soundboard.file(&client, &key, &hex).await
            {
                tracing::debug!(%error, sound = %hex, "a sound could not be fetched");
            }
            soundboard.lock().fetching.remove(&hash);
        });
    }

    /// A request to the server's media endpoints with this device's media
    /// token; an expired token is renewed once.
    async fn http(
        &self,
        client: &Client,
        key: &str,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> Result<HttpResponse, CoreError> {
        let (address, pin) = {
            let store = self.store.lock().unwrap_or_else(PoisonError::into_inner);
            let server = store.server(key).ok_or(CoreError::UnknownServer)?;
            let address = opencord_common::address::ServerAddress {
                host: server.host.clone(),
                port: server.port,
            };
            (address, store.pin(key).ok_or(CoreError::NotConnected)?)
        };
        let mut renewed = false;
        loop {
            let token = self.token(key).ok_or(CoreError::NotConnected)?;
            let response = http::request(
                &address,
                pin,
                method,
                path,
                &token,
                body,
                MAX_FILE_BYTES * 2,
            )
            .await
            .map_err(http_error)?;
            if response.status != 401 || renewed {
                return Ok(response);
            }
            let renewal = client
                .request(
                    key,
                    proto::request::Kind::RefreshMediaToken(proto::RefreshMediaToken {}),
                )
                .await?;
            if let proto::response::Result::MediaToken(token) = renewal {
                self.tokens
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(key.to_owned(), token.token);
            }
            renewed = true;
        }
    }

    fn token(&self, key: &str) -> Option<String> {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The result an upload endpoint answered with, or its error.
fn expect<T>(
    response: HttpResponse,
    pick: impl FnOnce(proto::response::Result) -> Option<T>,
) -> Result<T, CoreError> {
    let decoded = proto::Response::decode(response.body.as_slice())
        .ok()
        .and_then(|response| response.result);
    match decoded {
        Some(proto::response::Result::Error(error)) => Err(convert::error(error)),
        Some(result) => pick(result).ok_or_else(|| invalid("an unexpected answer".to_owned())),
        None => Err(invalid(format!("the server answered {}", response.status))),
    }
}

fn http_error(error: HttpError) -> CoreError {
    CoreError::Connection {
        message: error.to_string(),
    }
}

fn invalid(message: String) -> CoreError {
    CoreError::InvalidInput { message }
}

fn f32_percent(volume: u32) -> f32 {
    volume.min(100) as f32 / 100.0
}
