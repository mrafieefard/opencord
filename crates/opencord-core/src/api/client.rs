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
    ChannelChanges, ChannelKind, ChannelPosition, CommandChoice, CommandList, CommandOptionValue,
    CommandOutcome, CommandSettingsChanges, CoreError, CoreEvent, DefaultSound, DomainCheck,
    DomainKind, DomainUpdate, GeneratedIdentity, HotkeyBinding, HotkeySupport, IdentityInfo,
    Invite, LavalinkNodeChanges, LavalinkNodeSettings, LavalinkNodes, MediaEvent, Member, Message,
    MusicAction, MusicBlock, MusicPlacement, MusicRepeat, MusicSearchResults, MusicSession,
    MusicSettings, MusicSettingsChanges, NetworkInfo, NetworkSettingsChanges, NoiseSuppressionMode,
    OverwriteTargetKind, PermissionOverwrite, PresenceStatus, Role, RoleChanges, ScreenAudioApp,
    ScreenAudioSupport, ScreenShareRequest, ScreenShareStarted, ScreenSource, ScreenThumbnail,
    Server, ServerChanges, ServerInfo, SoundDraft, SoundSelection, SoundboardSound,
    SoundboardSoundChanges, StreamSourceKind, TextureStats, TrustedFingerprint, TunnelKind, User,
    VideoSupport, VideoWant, VoiceCue, VoiceSettings, VoiceSettingsChanges, VoiceState,
};
use crate::client::Client;
use crate::frb_generated::StreamSink;
use crate::identity::{self, Identity};
use crate::media::{self, MediaOptions};
use crate::soundboard::Soundboard;

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
    return_large_buffers();
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

/// `max_upload_bytes`: `None` leaves uploads to the member's other roles.
#[allow(clippy::too_many_arguments)]
pub async fn create_role(
    server_key: String,
    name: String,
    color: u32,
    permissions: i64,
    hoist: bool,
    mentionable: bool,
    max_upload_bytes: Option<u64>,
) -> Result<Role, CoreError> {
    on_runtime(move |client| async move {
        client
            .create_role(
                &server_key,
                name,
                color,
                permissions,
                hoist,
                mentionable,
                max_upload_bytes,
            )
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

/// Makes this person's profile picture from the file at `source_path`, or
/// removes it (`None`); its hash. Kept on this device, as the display name
/// is: `upload_avatar` sends it to each server.
pub async fn set_own_avatar(source_path: Option<String>) -> Result<Option<String>, CoreError> {
    let client = client()?;
    on_blocking(move || client.set_own_avatar(source_path.as_deref())).await
}

/// This person's profile picture's file on this device; `None` without one.
#[frb(sync)]
pub fn own_avatar_file() -> Result<Option<String>, CoreError> {
    Ok(client()?.own_avatar_file())
}

/// This person's profile picture's hash; `None` without one.
#[frb(sync)]
pub fn own_avatar_hash() -> Result<Option<String>, CoreError> {
    Ok(client()?.own_avatar_hash())
}

/// Sends this person's profile picture, or that there is none, to the
/// server; a server whose `User.avatar_hash` differs from
/// `own_avatar_hash` missed a change.
pub async fn upload_avatar(server_key: String) -> Result<User, CoreError> {
    on_runtime(move |client| async move { client.upload_avatar(&server_key).await }).await
}

/// The server's icon, made from the file at `source_path`, or none.
/// Manage server.
pub async fn set_server_icon(
    server_key: String,
    source_path: Option<String>,
) -> Result<ServerInfo, CoreError> {
    on_runtime(move |client| async move { client.set_server_icon(&server_key, source_path).await })
        .await
}

/// The file of the profile picture or server icon named `hash`, fetched
/// from the server unless this device has it.
pub async fn avatar_path(server_key: String, hash: String) -> Result<String, CoreError> {
    on_runtime(move |client| async move { client.avatar_path(&server_key, &hash).await }).await
}

pub async fn update_presence(server_key: String, status: PresenceStatus) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.update_presence(&server_key, status).await }).await
}

/// An invite, through `domain_id`'s domain, or the server's primary
/// domain when it has one (Phase 3 plan §7.6).
pub async fn create_invite(
    server_key: String,
    max_uses: Option<u32>,
    expires_in_s: Option<u32>,
    domain_id: Option<i64>,
) -> Result<Invite, CoreError> {
    on_runtime(move |client| async move {
        client
            .create_invite(&server_key, max_uses, expires_in_s, domain_id)
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

/// Starts or stops `NetworkStatus` events for this session; returns the
/// network as it is. Owner only (Phase 3 plan §8).
pub async fn watch_network(server_key: String, watch: bool) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move { client.watch_network(&server_key, watch).await }).await
}

/// How the server finds its public address: detected, or overridden (an
/// empty override is none).
pub async fn set_public_address(
    server_key: String,
    auto_detect: bool,
    override_ipv4: String,
    override_ipv6: String,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move {
        client
            .set_public_address(&server_key, auto_detect, override_ipv4, override_ipv6)
            .await
    })
    .await
}

/// Looks the public address up now.
pub async fn detect_public_address(server_key: String) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move { client.detect_public_address(&server_key).await }).await
}

pub async fn update_network_settings(
    server_key: String,
    changes: NetworkSettingsChanges,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(
        move |client| async move { client.update_network_settings(&server_key, changes).await },
    )
    .await
}

/// Attaches a domain to the server, pending its check (Phase 3 plan §6).
pub async fn add_domain(
    server_key: String,
    hostname: String,
    kind: DomainKind,
    public_port: Option<u32>,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move {
        client
            .add_domain(&server_key, hostname, kind, public_port)
            .await
    })
    .await
}

pub async fn update_domain(
    server_key: String,
    domain_id: i64,
    update: DomainUpdate,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(
        move |client| async move { client.update_domain(&server_key, domain_id, update).await },
    )
    .await
}

pub async fn delete_domain(server_key: String, domain_id: i64) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move { client.delete_domain(&server_key, domain_id).await })
        .await
}

/// Starts a check of a domain; the result arrives as `NetworkStatus`.
pub async fn check_domain(server_key: String, domain_id: i64) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move { client.check_domain(&server_key, domain_id).await }).await
}

/// A domain's last checks, newest first.
pub async fn domain_checks(
    server_key: String,
    domain_id: i64,
) -> Result<Vec<DomainCheck>, CoreError> {
    on_runtime(move |client| async move { client.domain_checks(&server_key, domain_id).await })
        .await
}

/// Starts getting a Let's Encrypt certificate for a domain; progress
/// arrives as `NetworkStatus`. `agree_terms` is needed for the first one.
pub async fn request_certificate(
    server_key: String,
    domain_id: i64,
    email: Option<String>,
    agree_terms: bool,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move {
        client
            .request_certificate(&server_key, domain_id, email, agree_terms)
            .await
    })
    .await
}

/// The owner's own certificate chain and key, in PEM.
pub async fn upload_certificate(
    server_key: String,
    domain_id: i64,
    chain_pem: String,
    key_pem: String,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move {
        client
            .upload_certificate(&server_key, domain_id, chain_pem, key_pem)
            .await
    })
    .await
}

/// Connects a DNS provider (cloudflare, duckdns or desec); an empty token
/// disconnects it.
pub async fn set_dns_provider(
    server_key: String,
    kind: String,
    token: String,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move { client.set_dns_provider(&server_key, kind, token).await })
        .await
}

/// Gets a free subdomain from DuckDNS or deSEC (`provider`: duckdns or
/// desec); an empty token keeps the one connected.
pub async fn add_free_subdomain(
    server_key: String,
    provider: String,
    name: String,
    token: String,
    email: Option<String>,
    agree_terms: bool,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move {
        client
            .add_free_subdomain(&server_key, provider, name, token, email, agree_terms)
            .await
    })
    .await
}

/// Starts a Cloudflare Tunnel; a named one needs its token (empty keeps the
/// saved one) and hostname.
pub async fn start_tunnel(
    server_key: String,
    kind: TunnelKind,
    token: String,
    hostname: String,
) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move {
        client
            .start_tunnel(&server_key, kind, token, hostname)
            .await
    })
    .await
}

pub async fn stop_tunnel(server_key: String) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move { client.stop_tunnel(&server_key).await }).await
}

/// cloudflared's last lines of output.
pub async fn tunnel_log(server_key: String) -> Result<Vec<String>, CoreError> {
    on_runtime(move |client| async move { client.tunnel_log(&server_key).await }).await
}

/// Has the server download cloudflared and check its checksum.
pub async fn install_cloudflared(server_key: String) -> Result<NetworkInfo, CoreError> {
    on_runtime(move |client| async move { client.install_cloudflared(&server_key).await }).await
}

/// The server's Lavalink nodes; with `watch`, their changes arrive as
/// events until called again with `watch` false. Needs Manage server.
pub async fn lavalink_nodes(server_key: String, watch: bool) -> Result<LavalinkNodes, CoreError> {
    on_runtime(move |client| async move { client.lavalink_nodes(&server_key, watch).await }).await
}

pub async fn create_lavalink_node(
    server_key: String,
    settings: LavalinkNodeSettings,
    password: String,
) -> Result<LavalinkNodes, CoreError> {
    on_runtime(move |client| async move {
        client
            .create_lavalink_node(&server_key, settings, password)
            .await
    })
    .await
}

/// Changes a node; `None` fields stay as they are.
pub async fn update_lavalink_node(
    server_key: String,
    node_id: i64,
    changes: LavalinkNodeChanges,
) -> Result<LavalinkNodes, CoreError> {
    on_runtime(move |client| async move {
        client
            .update_lavalink_node(&server_key, node_id, changes)
            .await
    })
    .await
}

pub async fn delete_lavalink_node(
    server_key: String,
    node_id: i64,
) -> Result<LavalinkNodes, CoreError> {
    on_runtime(move |client| async move { client.delete_lavalink_node(&server_key, node_id).await })
        .await
}

/// Starts a node's connection test; its steps arrive as
/// `LavalinkTestProgress` events.
pub async fn test_lavalink_node(server_key: String, node_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.test_lavalink_node(&server_key, node_id).await })
        .await
}

/// Moves the channels on a node now: to `to_node_id`, or each to the best
/// other node. Needs Manage server.
pub async fn move_lavalink_players(
    server_key: String,
    from_node_id: i64,
    to_node_id: Option<i64>,
) -> Result<LavalinkNodes, CoreError> {
    on_runtime(move |client| async move {
        client
            .move_lavalink_players(&server_key, from_node_id, to_node_id)
            .await
    })
    .await
}

/// Runs a chat command typed in `channel_id` (Phase 3 plan §17). An
/// "only you" reply arrives as an `EphemeralMessage` with `nonce`.
pub async fn command_execute(
    server_key: String,
    channel_id: i64,
    name: String,
    options: Vec<CommandOptionValue>,
    nonce: String,
) -> Result<CommandOutcome, CoreError> {
    on_runtime(move |client| async move {
        client
            .command_execute(&server_key, channel_id, name, options, nonce)
            .await
    })
    .await
}

/// Up to 10 suggestions for a command's option as typed so far.
pub async fn command_autocomplete(
    server_key: String,
    channel_id: i64,
    name: String,
    option: String,
    value: String,
) -> Result<Vec<CommandChoice>, CoreError> {
    on_runtime(move |client| async move {
        client
            .command_autocomplete(&server_key, channel_id, name, option, value)
            .await
    })
    .await
}

/// Every command, on or off, for the Commands page. Needs Manage server.
pub async fn command_settings(server_key: String) -> Result<CommandList, CoreError> {
    on_runtime(move |client| async move { client.command_settings(&server_key).await }).await
}

/// Turns a command on or off, chooses where it works, or where results go.
/// Needs Manage server.
pub async fn update_command_settings(
    server_key: String,
    changes: CommandSettingsChanges,
) -> Result<CommandList, CoreError> {
    on_runtime(
        move |client| async move { client.update_command_settings(&server_key, changes).await },
    )
    .await
}

/// Searches, or loads a link, for a voice channel; `source` empty for the
/// server's default (Phase 3 plan §15.1). Needs Add music.
pub async fn music_search(
    server_key: String,
    channel_id: i64,
    query: String,
    source: String,
) -> Result<MusicSearchResults, CoreError> {
    on_runtime(move |client| async move {
        client
            .music_search(&server_key, channel_id, query, source)
            .await
    })
    .await
}

/// The channel's player; from now on its changes arrive as
/// `MusicSession` events even when not connected to it (Phase 3 plan
/// §14.3). Needs only to see the channel.
pub async fn music_subscribe(
    server_key: String,
    channel_id: i64,
) -> Result<MusicSession, CoreError> {
    on_runtime(
        move |client| async move { client.music_subscribe(&server_key, channel_id, true).await },
    )
    .await
}

/// Stops the changes `music_subscribe` started; those connected to the
/// channel keep getting them.
pub async fn music_unsubscribe(server_key: String, channel_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move {
        client
            .music_subscribe(&server_key, channel_id, false)
            .await
            .map(|_| ())
    })
    .await
}

/// Adds search results, or tracks from the channel's history by their item
/// ids ("Add again"): at the end, before `before_item_id`, next or now.
/// Needs Add music; a position, next and now need Manage the queue.
pub async fn music_enqueue(
    server_key: String,
    channel_id: i64,
    result_ids: Vec<String>,
    history_item_ids: Vec<String>,
    before_item_id: Option<String>,
    placement: MusicPlacement,
) -> Result<MusicSession, CoreError> {
    on_runtime(move |client| async move {
        client
            .music_enqueue(
                &server_key,
                channel_id,
                result_ids,
                history_item_ids,
                before_item_id,
                placement,
            )
            .await
    })
    .await
}

/// Pause, resume, next, previous, seek (to `position_ms`) or stop. With
/// `expected_item_id`, nothing happens when another item plays by then.
pub async fn music_playback(
    server_key: String,
    channel_id: i64,
    action: MusicAction,
    position_ms: Option<u64>,
    expected_item_id: Option<String>,
) -> Result<MusicSession, CoreError> {
    on_runtime(move |client| async move {
        client
            .music_playback(
                &server_key,
                channel_id,
                action,
                position_ms,
                expected_item_id,
            )
            .await
    })
    .await
}

/// Repeat and shuffle, shared by everyone in the channel.
pub async fn music_set_mode(
    server_key: String,
    channel_id: i64,
    repeat: Option<MusicRepeat>,
    shuffle: Option<bool>,
) -> Result<MusicSession, CoreError> {
    on_runtime(move |client| async move {
        client
            .music_set_mode(&server_key, channel_id, repeat, shuffle)
            .await
    })
    .await
}

/// Puts an item before another, or at the end. Needs Manage the queue.
pub async fn music_queue_move(
    server_key: String,
    channel_id: i64,
    item_id: String,
    before_item_id: Option<String>,
) -> Result<MusicSession, CoreError> {
    on_runtime(move |client| async move {
        client
            .music_queue_move(&server_key, channel_id, item_id, before_item_id)
            .await
    })
    .await
}

/// Removes items: one's own with Add music, others' with Manage the queue.
pub async fn music_queue_remove(
    server_key: String,
    channel_id: i64,
    item_ids: Vec<String>,
) -> Result<MusicSession, CoreError> {
    on_runtime(move |client| async move {
        client
            .music_queue_remove(&server_key, channel_id, item_ids)
            .await
    })
    .await
}

/// Needs Manage the queue.
pub async fn music_queue_clear(
    server_key: String,
    channel_id: i64,
) -> Result<MusicSession, CoreError> {
    on_runtime(move |client| async move { client.music_queue_clear(&server_key, channel_id).await })
        .await
}

/// Votes to skip the current track when vote skip is enabled (Phase 3 plan §18.1).
pub async fn music_vote_skip(
    server_key: String,
    channel_id: i64,
    expected_item_id: String,
) -> Result<MusicSession, CoreError> {
    on_runtime(move |client| async move {
        client
            .music_vote_skip(&server_key, channel_id, expected_item_id)
            .await
    })
    .await
}

/// This person's music volume (0–2, 200 %), mute, and "Lower music while
/// people talk", for every server; kept on this device and never sent.
/// Muted, the voice node stops sending the music (Phase 3 plan §19.8).
#[frb(sync)]
pub fn music_set_listening(
    volume: f32,
    muted: bool,
    duck_while_talking: bool,
) -> Result<(), CoreError> {
    client()?.music_set_listening(volume, muted, duck_while_talking);
    Ok(())
}

/// A track's artwork through the server, at 128 or 512 px: the path of a
/// file kept on this device (Phase 3 plan §15.4).
pub async fn music_artwork(
    server_key: String,
    artwork_id: String,
    size: u32,
) -> Result<String, CoreError> {
    on_runtime(
        move |client| async move { client.music_artwork(&server_key, &artwork_id, size).await },
    )
    .await
}

/// The music settings, with the search sources available now.
pub async fn music_settings(server_key: String) -> Result<MusicSettings, CoreError> {
    on_runtime(move |client| async move { client.music_settings(&server_key).await }).await
}

/// Changes the music settings; `None` fields stay. Needs Manage music.
pub async fn update_music_settings(
    server_key: String,
    changes: MusicSettingsChanges,
) -> Result<MusicSettings, CoreError> {
    on_runtime(
        move |client| async move { client.update_music_settings(&server_key, changes).await },
    )
    .await
}

/// Members blocked from music. Needs Manage music.
pub async fn music_blocks(server_key: String) -> Result<Vec<MusicBlock>, CoreError> {
    on_runtime(move |client| async move { client.music_blocks(&server_key).await }).await
}

/// Blocks a member from music, or unblocks them. Needs Manage music.
pub async fn block_from_music(
    server_key: String,
    user_id: i64,
    blocked: bool,
    reason: String,
) -> Result<Vec<MusicBlock>, CoreError> {
    on_runtime(move |client| async move {
        client
            .block_from_music(&server_key, user_id, blocked, reason)
            .await
    })
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

/// What this system can do with video: each system's backends say
/// (docs/plans/platforms.md). Sending needs the codec too.
#[frb(sync)]
pub fn video_support() -> VideoSupport {
    use opencord_media::video::{camera, codec, screen};

    let receive = codec::supported()
        && cfg!(any(
            target_os = "linux",
            target_os = "macos",
            target_os = "windows"
        ));
    let screens = screen::support();
    let screen = receive && screens.supported;
    VideoSupport {
        receive,
        camera: receive && camera::supported(),
        screen,
        screen_reason: (!screen).then(|| {
            screens
                .reason
                .unwrap_or_else(|| "Screen sharing is not available on this system yet".to_owned())
        }),
        own_screen_picker: screens.own_picker,
    }
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

/// Goes live in this device's voice channel (Phase 2 plan §9): where the
/// system has a picker (`VideoSupport::own_screen_picker` false) it chooses
/// a screen or a window first; otherwise `source_id` is one of
/// [`screen_sources`]. Sharing again while live changes the source and
/// keeps the viewers.
pub async fn screen_share_start(
    request: ScreenShareRequest,
    source_id: Option<String>,
) -> Result<ScreenShareStarted, CoreError> {
    on_runtime(move |client| async move { client.screen_share_start(request, source_id).await })
        .await
}

/// The screens and windows the share dialog offers where Opencord has its
/// own picker; empty where the system picks.
pub async fn screen_sources() -> Result<Vec<ScreenSource>, CoreError> {
    use opencord_media::video::screen::{self, SourceKind};

    let sources = screen::sources()
        .await
        .map_err(crate::video::screen::problem)?;
    Ok(sources
        .into_iter()
        .map(|source| ScreenSource {
            id: source.id,
            kind: match source.kind {
                SourceKind::Screen => StreamSourceKind::Screen,
                SourceKind::Window => StreamSourceKind::Window,
            },
            title: source.title,
            app: source.app,
            thumbnail: source.thumbnail.map(|thumbnail| ScreenThumbnail {
                width: thumbnail.width,
                height: thumbnail.height,
                rgba: thumbnail.rgba,
            }),
        })
        .collect())
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

/// Whether this system can share a screen's sound (plan §10.1), and why
/// not: each system's backend says.
pub async fn screen_audio_support() -> ScreenAudioSupport {
    let support = tokio::task::spawn_blocking(opencord_media::share_audio::support)
        .await
        .ok();
    match support {
        Some(support) => ScreenAudioSupport {
            supported: support.supported,
            reason: support.reason,
        },
        None => ScreenAudioSupport {
            supported: false,
            reason: Some("Sharing sound could not be checked".to_owned()),
        },
    }
}

/// The apps with sound playing now, Opencord left out: what a window
/// share's "Sound from" offers.
pub async fn screen_audio_apps() -> Result<Vec<ScreenAudioApp>, CoreError> {
    let apps = tokio::task::spawn_blocking(opencord_media::share_audio::apps_with_audio)
        .await
        .map_err(|error| CoreError::Connection {
            message: error.to_string(),
        })?
        .map_err(|error| CoreError::InvalidInput {
            message: error.to_string(),
        })?;
    Ok(apps
        .into_iter()
        .map(|app| ScreenAudioApp {
            name: app.name,
            icon: app.icon,
        })
        .collect())
}

/// Whose sound this device's window share sends: an app from
/// [`screen_audio_apps`] by name, or none.
#[frb(sync)]
pub fn screen_share_audio_from(app: Option<String>) -> Result<(), CoreError> {
    client()?.screen_share_audio_from(app)
}

/// How loud someone's screen share sound is here, 0–2 (200 %), apart from
/// their voice.
#[frb(sync)]
pub fn voice_set_stream_volume(
    server_key: String,
    user_id: i64,
    volume: f32,
) -> Result<(), CoreError> {
    client()?.set_stream_volume(&server_key, user_id, volume);
    Ok(())
}

#[frb(sync)]
pub fn voice_set_stream_mute(
    server_key: String,
    user_id: i64,
    muted: bool,
) -> Result<(), CoreError> {
    client()?.set_stream_mute(&server_key, user_id, muted);
    Ok(())
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
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        crate::video::flutter::stats(texture_id)
            .map(|(presented, drawn)| TextureStats { presented, drawn })
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
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

/// The sounds every client has (plan §11.8), played as `default:<id>`.
#[frb(sync)]
pub fn soundboard_default_sounds() -> Vec<DefaultSound> {
    Soundboard::default_sounds()
}

/// Plays a sound in a voice channel this device is in, for everyone there
/// (plan §11.5). `sound_ref` is `server:<sha256>`, `external:<sha256>` or
/// `default:<id>`. For a sound from another server, `external_server_key`
/// and `external_sound` name it: it is uploaded here first if needed.
pub async fn soundboard_play(
    server_key: String,
    channel_id: i64,
    sound_ref: String,
    external_server_key: Option<String>,
    external_sound: Option<SoundboardSound>,
) -> Result<(), CoreError> {
    let external = external_server_key.zip(external_sound);
    on_runtime(move |client| async move {
        client
            .soundboard_play(&server_key, channel_id, &sound_ref, external)
            .await
    })
    .await
}

/// Plays a sound on this device only; nobody else hears it.
pub async fn soundboard_preview(server_key: String, sound_ref: String) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.soundboard_preview(&server_key, &sound_ref).await },
    )
    .await
}

/// Whether this system can lower other apps while people talk (Phase 2
/// §12); the setting is hidden where it cannot.
#[frb(sync)]
pub fn audio_attenuation_supported() -> bool {
    opencord_media::attenuation::supported()
}

/// Plays a voice event's sound on this device only (Phase 2 §12), at
/// `volume` (0–1).
#[frb(sync)]
pub fn play_cue(cue: VoiceCue, volume: f32) -> Result<(), CoreError> {
    client()?.play_cue(cue, volume);
    Ok(())
}

/// The soundboard's volume (0–1) and "Mute all soundboards" (User settings
/// → Sounds).
#[frb(sync)]
pub fn soundboard_set_listening(volume: f32, muted: bool) -> Result<(), CoreError> {
    client()?.soundboard_set_listening(volume, muted);
    Ok(())
}

/// "Mute their soundboard": their sounds are not played on this device.
#[frb(sync)]
pub fn soundboard_mute_player(
    server_key: String,
    user_id: i64,
    muted: bool,
) -> Result<(), CoreError> {
    client()?.soundboard_mute_player(&server_key, user_id, muted);
    Ok(())
}

pub async fn soundboard_update_sound(
    server_key: String,
    sound_id: i64,
    changes: SoundboardSoundChanges,
) -> Result<SoundboardSound, CoreError> {
    on_runtime(move |client| async move {
        client
            .update_soundboard_sound(&server_key, sound_id, changes)
            .await
    })
    .await
}

pub async fn soundboard_delete_sound(server_key: String, sound_id: i64) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.delete_soundboard_sound(&server_key, sound_id).await },
    )
    .await
}

/// Opens an audio file (MP3, WAV, FLAC, OGG, M4A; at most 10 MB) for the
/// trim editor.
pub async fn soundboard_open_draft(path: String) -> Result<SoundDraft, CoreError> {
    on_runtime(move |client| async move { client.soundboard_open_draft(path).await }).await
}

/// Plays the selected window on this device, as it would be uploaded.
pub async fn soundboard_preview_draft(selection: SoundSelection) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.soundboard_preview_draft(selection).await }).await
}

/// Uploads the selected window as a new sound on that server.
pub async fn soundboard_upload(
    server_key: String,
    selection: SoundSelection,
    name: String,
    emoji: Option<String>,
) -> Result<SoundboardSound, CoreError> {
    on_runtime(move |client| async move {
        client
            .soundboard_upload(&server_key, selection, name, emoji)
            .await
    })
    .await
}

#[frb(sync)]
pub fn soundboard_close_draft(draft_id: u64) -> Result<(), CoreError> {
    client()?.soundboard_close_draft(draft_id);
    Ok(())
}

pub(crate) fn client() -> Result<Client, CoreError> {
    CLIENT.get().cloned().ok_or(CoreError::NotInitialized)
}

pub(crate) fn runtime() -> Result<&'static Runtime, CoreError> {
    RUNTIME.get().ok_or(CoreError::NotInitialized)
}

/// Runs `work` on the core's runtime, where connections live.
pub(crate) async fn on_runtime<T, F>(work: impl FnOnce(Client) -> F) -> Result<T, CoreError>
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

/// Runs blocking `work` (the clipboard) on the core's runtime.
pub(crate) async fn on_blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
) -> Result<T, CoreError> {
    let runtime = RUNTIME.get().ok_or(CoreError::NotInitialized)?;
    runtime
        .spawn_blocking(work)
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

/// Large allocations, video pictures above all, go straight back to the
/// system when freed. glibc otherwise raises its mmap threshold after such
/// frees, and later pictures come from heap arenas that stay resident: in a
/// call of ten with four cameras and a stream the app sat about 30 MB
/// higher (V10, docs/performance.md). Pictures are freed whole, so this
/// costs no measurable CPU.
fn return_large_buffers() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        /// glibc's `M_MMAP_THRESHOLD`.
        const M_MMAP_THRESHOLD: std::ffi::c_int = -3;
        unsafe extern "C" {
            fn mallopt(param: std::ffi::c_int, value: std::ffi::c_int) -> std::ffi::c_int;
        }
        // SAFETY: mallopt only changes glibc's allocator settings; a fixed
        // threshold also turns its dynamic adjustment off.
        unsafe {
            mallopt(M_MMAP_THRESHOLD, 128 * 1024);
        }
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
