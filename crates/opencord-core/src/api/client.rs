//! Functions the Flutter app calls. Call `init` once, then `event_stream`
//! and `identity_load`; every other call needs those first.

use std::collections::VecDeque;
use std::future::Future;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use flutter_rust_bridge::frb;
use opencord_common::validation;
use opencord_media::audio::processing;
use tokio::runtime::Runtime;

use super::types::{
    AddServerOutcome, AudioDevices, AudioSettings, Ban, CameraDevice, CameraStarted, Channel,
    ChannelChanges, ChannelKind, ChannelPosition, CoreError, CoreEvent, GeneratedIdentity,
    HotkeyBinding, HotkeySupport, IdentityInfo, Invite, MediaEvent, Member, Message,
    NoiseSuppressionMode, OverwriteTargetKind, PermissionOverwrite, PresenceStatus, Role,
    RoleChanges, ScreenShareRequest, ScreenShareStarted, Server, ServerChanges, ServerInfo,
    TextureStats, TrustedFingerprint, User, VideoWant, VoiceSettings, VoiceSettingsChanges,
    VoiceState,
};
use crate::client::Client;
use crate::frb_generated::StreamSink;
use crate::identity::{self, Identity};
use crate::media::{self, MediaOptions};

/// Events kept for a stream that has not been opened yet.
const BACKLOG_LIMIT: usize = 10_000;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static CLIENT: OnceLock<Client> = OnceLock::new();
static EVENTS: Mutex<Events> = Mutex::new(Events {
    sink: None,
    backlog: VecDeque::new(),
});
/// Voice media's news; nothing is kept while no stream is open.
static MEDIA_EVENTS: Mutex<Option<StreamSink<MediaEvent>>> = Mutex::new(None);

struct Events {
    sink: Option<StreamSink<CoreEvent>>,
    backlog: VecDeque<CoreEvent>,
}

/// Opens local data in `app_data_dir`. Safe to call again (for example
/// after a hot restart); later calls do nothing.
#[frb(sync)]
pub fn init(app_data_dir: String) -> Result<(), CoreError> {
    if CLIENT.get().is_some() {
        return Ok(());
    }
    let runtime = match RUNTIME.get() {
        Some(runtime) => runtime,
        None => {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name("opencord-core")
                .build()
                .map_err(|error| CoreError::Connection {
                    message: error.to_string(),
                })?;
            RUNTIME.get_or_init(|| runtime)
        }
    };
    let (client, mut events) = Client::new(Path::new(&app_data_dir), runtime.handle().clone())?;
    let media_events = client.enable_media(MediaOptions { open_devices: true });
    if CLIENT.set(client).is_err() {
        return Ok(());
    }
    runtime.spawn(async move {
        while let Some(event) = events.recv().await {
            deliver(event);
        }
    });
    if let Some(mut media_events) = media_events {
        runtime.spawn(async move {
            while let Some(event) = media_events.recv().await {
                let mut sink = MEDIA_EVENTS.lock().unwrap_or_else(PoisonError::into_inner);
                if sink.as_ref().is_some_and(|sink| sink.add(event).is_err()) {
                    *sink = None;
                }
            }
        });
    }
    Ok(())
}

/// Voice media's news: how this device's voice connection is doing, and
/// audio devices. Opening it again replaces the previous stream.
pub fn media_event_stream(sink: StreamSink<MediaEvent>) -> Result<(), CoreError> {
    *MEDIA_EVENTS.lock().unwrap_or_else(PoisonError::into_inner) = Some(sink);
    Ok(())
}

/// Every event from every server, in order. Opening it again replaces the
/// previous stream.
pub fn event_stream(sink: StreamSink<CoreEvent>) -> Result<(), CoreError> {
    let mut events = lock_events();
    while let Some(event) = events.backlog.pop_front() {
        if sink.add(event).is_err() {
            return Ok(());
        }
    }
    events.sink = Some(sink);
    Ok(())
}

#[frb(sync)]
pub fn identity_generate() -> GeneratedIdentity {
    let identity = Identity::generate();
    GeneratedIdentity {
        secret: identity.secret().to_vec(),
        info: identity_info(&identity),
    }
}

/// Checks an identity and display name as `identity_load` does, without
/// using them: the app saves an identity before it is put to use.
#[frb(sync)]
pub fn identity_check(secret: Vec<u8>, display_name: String) -> Result<IdentityInfo, CoreError> {
    let (identity, _) = checked_identity(&secret, &display_name)?;
    Ok(identity_info(&identity))
}

/// Uses this identity and display name for every server, and connects to
/// the saved servers.
#[frb(sync)]
pub fn identity_load(secret: Vec<u8>, display_name: String) -> Result<IdentityInfo, CoreError> {
    let (identity, display_name) = checked_identity(&secret, &display_name)?;
    let info = identity_info(&identity);
    let client = client()?;
    client.set_identity(identity, display_name);
    client.connect_saved_servers();
    Ok(info)
}

/// Text for an identity backup file. It contains the secret key.
#[frb(sync)]
pub fn identity_backup_encode(secret: Vec<u8>) -> Result<String, CoreError> {
    identity::encode_backup(&secret).map_err(invalid_input)
}

/// The secret from an identity backup file.
#[frb(sync)]
pub fn identity_backup_decode(backup: String) -> Result<Vec<u8>, CoreError> {
    identity::decode_backup(&backup)
        .map(|secret| secret.to_vec())
        .map_err(invalid_input)
}

#[frb(sync)]
pub fn servers_list() -> Result<Vec<Server>, CoreError> {
    Ok(client()?.servers())
}

/// Adds a server from an invite link or `host:port`, optionally claiming
/// ownership with the token the server printed on first start.
pub async fn server_add(
    link_or_address: String,
    claim_token: Option<String>,
) -> Result<AddServerOutcome, CoreError> {
    on_runtime(move |client| async move { client.add_server(&link_or_address, claim_token).await })
        .await
}

/// Pins a certificate after the user confirmed it.
#[frb(sync)]
pub fn server_trust_fingerprint(address: String, fingerprint: String) -> Result<(), CoreError> {
    client()?.trust_fingerprint(&address, &fingerprint)
}

/// Tries a server again now instead of waiting (or after a failure).
pub async fn server_retry_now(server_key: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.retry_now(&server_key).await }).await
}

pub async fn server_remove(server_key: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.remove_server(&server_key).await }).await
}

#[frb(sync)]
pub fn trusted_fingerprints() -> Result<Vec<TrustedFingerprint>, CoreError> {
    Ok(client()?
        .trusted_fingerprints()
        .into_iter()
        .map(|(address, fingerprint)| TrustedFingerprint {
            address,
            fingerprint,
        })
        .collect())
}

#[frb(sync)]
pub fn settings_get(key: String) -> Result<Option<String>, CoreError> {
    Ok(client()?.setting(&key))
}

/// `None` removes the setting.
#[frb(sync)]
pub fn settings_set(key: String, value: Option<String>) -> Result<(), CoreError> {
    client()?.set_setting(&key, value)
}

pub async fn send_message(
    server_key: String,
    channel_id: i64,
    content: String,
    nonce: String,
) -> Result<Message, CoreError> {
    on_runtime(move |client| async move {
        client
            .send_message(&server_key, channel_id, content, nonce)
            .await
    })
    .await
}

pub async fn edit_message(
    server_key: String,
    message_id: i64,
    content: String,
) -> Result<Message, CoreError> {
    on_runtime(
        move |client| async move { client.edit_message(&server_key, message_id, content).await },
    )
    .await
}

pub async fn delete_message(server_key: String, message_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.delete_message(&server_key, message_id).await })
        .await
}

/// Newest first; `before` pages backwards.
pub async fn fetch_messages(
    server_key: String,
    channel_id: i64,
    before: Option<i64>,
    limit: u32,
) -> Result<Vec<Message>, CoreError> {
    on_runtime(move |client| async move {
        client
            .fetch_messages(&server_key, channel_id, before, limit)
            .await
    })
    .await
}

pub async fn start_typing(server_key: String, channel_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.start_typing(&server_key, channel_id).await })
        .await
}

pub async fn create_channel(
    server_key: String,
    kind: ChannelKind,
    name: String,
    topic: Option<String>,
    parent_id: Option<i64>,
) -> Result<Channel, CoreError> {
    on_runtime(move |client| async move {
        client
            .create_channel(&server_key, kind, name, topic, parent_id)
            .await
    })
    .await
}

pub async fn update_channel(
    server_key: String,
    channel_id: i64,
    changes: ChannelChanges,
) -> Result<Channel, CoreError> {
    on_runtime(move |client| async move {
        client
            .update_channel(&server_key, channel_id, changes)
            .await
    })
    .await
}

pub async fn delete_channel(server_key: String, channel_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.delete_channel(&server_key, channel_id).await })
        .await
}

pub async fn reorder_channels(
    server_key: String,
    positions: Vec<ChannelPosition>,
) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.reorder_channels(&server_key, positions).await })
        .await
}

pub async fn set_channel_overwrite(
    server_key: String,
    channel_id: i64,
    overwrite: PermissionOverwrite,
) -> Result<Channel, CoreError> {
    on_runtime(move |client| async move {
        client
            .set_channel_overwrite(&server_key, channel_id, overwrite)
            .await
    })
    .await
}

pub async fn delete_channel_overwrite(
    server_key: String,
    channel_id: i64,
    target_kind: OverwriteTargetKind,
    target_id: i64,
) -> Result<Channel, CoreError> {
    on_runtime(move |client| async move {
        client
            .delete_channel_overwrite(&server_key, channel_id, target_kind, target_id)
            .await
    })
    .await
}

pub async fn create_role(
    server_key: String,
    name: String,
    color: u32,
    permissions: i64,
    hoist: bool,
    mentionable: bool,
) -> Result<Role, CoreError> {
    on_runtime(move |client| async move {
        client
            .create_role(&server_key, name, color, permissions, hoist, mentionable)
            .await
    })
    .await
}

pub async fn update_role(
    server_key: String,
    role_id: i64,
    changes: RoleChanges,
) -> Result<Role, CoreError> {
    on_runtime(move |client| async move { client.update_role(&server_key, role_id, changes).await })
        .await
}

pub async fn delete_role(server_key: String, role_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.delete_role(&server_key, role_id).await }).await
}

/// The listed roles, lowest first, swap among the positions they hold.
pub async fn reorder_roles(server_key: String, role_ids: Vec<i64>) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.reorder_roles(&server_key, role_ids).await }).await
}

pub async fn add_member_role(
    server_key: String,
    user_id: i64,
    role_id: i64,
) -> Result<Member, CoreError> {
    on_runtime(
        move |client| async move { client.add_member_role(&server_key, user_id, role_id).await },
    )
    .await
}

pub async fn remove_member_role(
    server_key: String,
    user_id: i64,
    role_id: i64,
) -> Result<Member, CoreError> {
    on_runtime(move |client| async move {
        client
            .remove_member_role(&server_key, user_id, role_id)
            .await
    })
    .await
}

pub async fn kick_member(
    server_key: String,
    user_id: i64,
    reason: Option<String>,
) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.kick_member(&server_key, user_id, reason).await })
        .await
}

pub async fn ban_member(
    server_key: String,
    user_id: i64,
    reason: Option<String>,
) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.ban_member(&server_key, user_id, reason).await })
        .await
}

pub async fn unban_member(server_key: String, user_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.unban_member(&server_key, user_id).await }).await
}

pub async fn fetch_bans(server_key: String) -> Result<Vec<Ban>, CoreError> {
    on_runtime(move |client| async move { client.fetch_bans(&server_key).await }).await
}

/// `None` or empty clears the nickname.
pub async fn update_nickname(
    server_key: String,
    user_id: i64,
    nickname: Option<String>,
) -> Result<Member, CoreError> {
    on_runtime(
        move |client| async move { client.update_nickname(&server_key, user_id, nickname).await },
    )
    .await
}

pub async fn update_profile(server_key: String, display_name: String) -> Result<User, CoreError> {
    on_runtime(move |client| async move { client.update_profile(&server_key, display_name).await })
        .await
}

pub async fn update_presence(server_key: String, status: PresenceStatus) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.update_presence(&server_key, status).await }).await
}

pub async fn create_invite(
    server_key: String,
    max_uses: Option<u32>,
    expires_in_s: Option<u32>,
) -> Result<Invite, CoreError> {
    on_runtime(move |client| async move {
        client
            .create_invite(&server_key, max_uses, expires_in_s)
            .await
    })
    .await
}

pub async fn fetch_invites(server_key: String) -> Result<Vec<Invite>, CoreError> {
    on_runtime(move |client| async move { client.fetch_invites(&server_key).await }).await
}

pub async fn revoke_invite(server_key: String, code: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.revoke_invite(&server_key, code).await }).await
}

pub async fn update_server(
    server_key: String,
    changes: ServerChanges,
) -> Result<ServerInfo, CoreError> {
    on_runtime(move |client| async move { client.update_server(&server_key, changes).await }).await
}

pub async fn update_voice_settings(
    server_key: String,
    changes: VoiceSettingsChanges,
) -> Result<VoiceSettings, CoreError> {
    on_runtime(
        move |client| async move { client.update_voice_settings(&server_key, changes).await },
    )
    .await
}

/// Joins a voice channel. This device is in at most one voice channel
/// across all servers, so any other one is left first.
pub async fn voice_join(server_key: String, channel_id: i64) -> Result<VoiceState, CoreError> {
    on_runtime(move |client| async move { client.voice_join(&server_key, channel_id).await }).await
}

pub async fn voice_leave() -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.voice_leave().await }).await
}

/// Holds outside voice too, for the next join.
#[frb(sync)]
pub fn voice_set_self_mute(muted: bool) -> Result<(), CoreError> {
    client()?.set_voice_self(Some(muted), None);
    Ok(())
}

/// Holds outside voice too, for the next join.
#[frb(sync)]
pub fn voice_set_self_deaf(deafened: bool) -> Result<(), CoreError> {
    client()?.set_voice_self(None, Some(deafened));
    Ok(())
}

/// Video draws into this Flutter engine's textures (plan §7.11): call once
/// at start with `EngineContext.instance.getEngineHandle()`.
#[frb(sync)]
pub fn video_init(engine_handle: i64) -> Result<(), CoreError> {
    client()?.video_init(engine_handle);
    Ok(())
}

/// Whether this system has cameras and video yet (Linux only for now).
#[frb(sync)]
pub fn video_supported() -> bool {
    cfg!(target_os = "linux")
}

/// The cameras there are. On Linux this may ask the user for camera access
/// first (the Camera portal).
pub async fn camera_devices() -> Result<Vec<CameraDevice>, CoreError> {
    on_runtime(|_| async move { crate::video::cameras().await }).await
}

/// Turns this device's camera on in its voice channel and publishes it; its
/// preview is drawn into the texture it returns.
pub async fn camera_start(device_id: Option<String>) -> Result<CameraStarted, CoreError> {
    on_runtime(move |client| async move { client.camera_start(device_id).await }).await
}

#[frb(sync)]
pub fn camera_stop() -> Result<(), CoreError> {
    client()?.camera_stop();
    Ok(())
}

/// Goes live in this device's voice channel (Phase 2 plan §9): on Linux the
/// system picker chooses a screen or a window first. Sharing again while
/// live changes the source and keeps the viewers.
pub async fn screen_share_start(
    request: ScreenShareRequest,
) -> Result<ScreenShareStarted, CoreError> {
    on_runtime(move |client| async move { client.screen_share_start(request).await }).await
}

/// A new quality for the screen share, within the server's maximum.
pub async fn screen_share_update(request: ScreenShareRequest) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.screen_share_update(request).await }).await
}

pub async fn screen_share_stop() -> Result<(), CoreError> {
    on_runtime(|client| async move { client.screen_share_stop().await }).await
}

/// Starts watching a stream in this device's voice channel; its video then
/// comes on its screen track, like any other (`video_set_wants`).
pub async fn stream_watch(stream_key: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.stream_watch(&stream_key, true).await }).await
}

pub async fn stream_unwatch(stream_key: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.stream_watch(&stream_key, false).await }).await
}

/// The tiles showing video now, at their sizes in physical pixels; tracks
/// not named are neither received nor decoded (plan §6, §7.11).
#[frb(sync)]
pub fn video_set_wants(wants: Vec<VideoWant>) -> Result<(), CoreError> {
    client()?.video_set_wants(wants);
    Ok(())
}

/// How many pictures a video texture got and how often Flutter drew it;
/// `None` for a texture that is gone (or none at all).
#[frb(sync)]
pub fn video_texture_stats(texture_id: i64) -> Option<TextureStats> {
    #[cfg(target_os = "linux")]
    {
        crate::video::flutter::stats(texture_id)
            .map(|(presented, drawn)| TextureStats { presented, drawn })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = texture_id;
        None
    }
}

/// Microphones and speakers the system offers now.
pub async fn audio_devices() -> Result<AudioDevices, CoreError> {
    let runtime = RUNTIME.get().ok_or(CoreError::NotInitialized)?;
    runtime
        .spawn_blocking(media::audio_devices)
        .await
        .map_err(|error| CoreError::Connection {
            message: error.to_string(),
        })
}

/// Devices, input mode and volumes for voice; call it at start and on every
/// change.
#[frb(sync)]
pub fn audio_apply_settings(settings: AudioSettings) -> Result<(), CoreError> {
    client()?.apply_audio_settings(settings);
    Ok(())
}

/// The push-to-talk key went down or up.
#[frb(sync)]
pub fn voice_set_push_to_talk(held: bool) -> Result<(), CoreError> {
    client()?.set_push_to_talk(held);
    Ok(())
}

/// The priority speaker key went down or up: while held, everyone else in
/// the channel hears the others at 25 % (needs Priority speaker).
#[frb(sync)]
pub fn voice_set_priority_speaker(held: bool) -> Result<(), CoreError> {
    client()?.set_priority_speaker(held);
    Ok(())
}

/// Binds global hotkeys (plan §7.13), replacing the ones bound before; an
/// empty list unbinds them. Says whether they work while Opencord is in the
/// background; when not, the app handles them while focused.
pub async fn hotkeys_set(bindings: Vec<HotkeyBinding>) -> Result<HotkeySupport, CoreError> {
    on_runtime(move |client| async move { client.set_hotkeys(&bindings).await }).await
}

/// Report the microphone's level (`MediaEvent::InputLevel`) while a meter
/// shows it.
#[frb(sync)]
pub fn audio_set_level_meter(enabled: bool) -> Result<(), CoreError> {
    client()?.set_level_meter(enabled);
    Ok(())
}

/// Hear yourself through the whole chain, Opus included (plan §12); in
/// voice or not.
pub async fn audio_mic_test(enabled: bool) -> Result<(), CoreError> {
    let client = client()?;
    let runtime = RUNTIME.get().ok_or(CoreError::NotInitialized)?;
    runtime
        .spawn_blocking(move || client.set_mic_test(enabled))
        .await
        .map_err(|error| CoreError::Connection {
            message: error.to_string(),
        })
}

/// The noise suppression to start with on a first run (plan §7.3): High
/// when it needs under 20 % of each 10 ms tick on this computer, Standard
/// otherwise. Takes a moment.
pub async fn audio_recommended_noise_suppression() -> Result<NoiseSuppressionMode, CoreError> {
    let runtime = RUNTIME.get().ok_or(CoreError::NotInitialized)?;
    let mode = runtime
        .spawn_blocking(processing::recommended_noise_suppression)
        .await
        .map_err(|error| CoreError::Connection {
            message: error.to_string(),
        })?;
    Ok(match mode {
        processing::NoiseSuppression::Off => NoiseSuppressionMode::Off,
        processing::NoiseSuppression::Standard => NoiseSuppressionMode::Standard,
        processing::NoiseSuppression::High => NoiseSuppressionMode::High,
    })
}

/// How loud someone sounds on this device, 0–2 (200 %).
#[frb(sync)]
pub fn voice_set_user_volume(
    server_key: String,
    user_id: i64,
    volume: f32,
) -> Result<(), CoreError> {
    client()?.set_user_volume(&server_key, user_id, volume);
    Ok(())
}

/// Silences someone on this device only.
#[frb(sync)]
pub fn voice_set_user_local_mute(
    server_key: String,
    user_id: i64,
    muted: bool,
) -> Result<(), CoreError> {
    client()?.set_user_local_mute(&server_key, user_id, muted);
    Ok(())
}

/// Server mute: nobody hears them until it is lifted.
pub async fn voice_server_mute(
    server_key: String,
    user_id: i64,
    muted: bool,
) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.server_mute(&server_key, user_id, muted).await })
        .await
}

/// Server deafen: they hear nobody until it is lifted.
pub async fn voice_server_deafen(
    server_key: String,
    user_id: i64,
    deafened: bool,
) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.server_deafen(&server_key, user_id, deafened).await },
    )
    .await
}

pub async fn voice_move_member(
    server_key: String,
    user_id: i64,
    channel_id: i64,
) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.move_member(&server_key, user_id, channel_id).await },
    )
    .await
}

pub async fn voice_disconnect_member(server_key: String, user_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.disconnect_member(&server_key, user_id).await })
        .await
}

fn client() -> Result<Client, CoreError> {
    CLIENT.get().cloned().ok_or(CoreError::NotInitialized)
}

/// Runs `work` on the core's runtime, where connections live.
async fn on_runtime<T, F>(work: impl FnOnce(Client) -> F) -> Result<T, CoreError>
where
    F: Future<Output = Result<T, CoreError>> + Send + 'static,
    T: Send + 'static,
{
    let client = client()?;
    let runtime = RUNTIME.get().ok_or(CoreError::NotInitialized)?;
    runtime
        .spawn(work(client))
        .await
        .map_err(|error| CoreError::Connection {
            message: error.to_string(),
        })?
}

fn deliver(event: CoreEvent) {
    let mut events = lock_events();
    let undelivered = match &events.sink {
        Some(sink) => sink.add(event).err().map(|_| None),
        None => Some(Some(event)),
    };
    match undelivered {
        None => {}
        Some(None) => events.sink = None,
        Some(Some(event)) => {
            if events.backlog.len() >= BACKLOG_LIMIT {
                events.backlog.pop_front();
            }
            events.backlog.push_back(event);
        }
    }
}

fn lock_events() -> MutexGuard<'static, Events> {
    EVENTS.lock().unwrap_or_else(PoisonError::into_inner)
}

fn checked_identity(secret: &[u8], display_name: &str) -> Result<(Identity, String), CoreError> {
    let identity = Identity::from_secret(secret).map_err(invalid_input)?;
    let display_name = validation::display_name(display_name).map_err(invalid_input)?;
    Ok((identity, display_name))
}

fn identity_info(identity: &Identity) -> IdentityInfo {
    IdentityInfo {
        public_key_hex: hex::encode(identity.public_key()),
        fingerprint: identity.fingerprint(),
    }
}

fn invalid_input(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidInput {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_check_validates_without_needing_the_client() {
        let identity = Identity::generate();
        let secret = identity.secret().to_vec();

        let info = identity_check(secret.clone(), " Alex ".to_owned()).unwrap();

        assert_eq!(info.fingerprint, identity.fingerprint());
        assert!(matches!(
            identity_check(secret, "   ".to_owned()),
            Err(CoreError::InvalidInput { .. })
        ));
        assert!(matches!(
            identity_check(vec![1, 2, 3], "Alex".to_owned()),
            Err(CoreError::InvalidInput { .. })
        ));
    }
}
