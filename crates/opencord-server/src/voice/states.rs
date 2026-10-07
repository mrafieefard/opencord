//! Who is in which voice channel. Voice states live in memory only (Phase 2
//! plan §3.5): one per user, held by one gateway session.

use std::collections::HashMap;

use opencord_common::channel::ChannelKind;
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use opencord_proto::voice::v1 as voice_proto;

use crate::error::ApiError;
use crate::guild::{Channel, Guild};
use crate::permissions::require_channel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceState {
    pub user_id: i64,
    pub channel_id: i64,
    pub session_id: String,
    pub self_mute: bool,
    pub self_deaf: bool,
    pub self_video: bool,
    pub self_stream: bool,
    /// As last announced; see [`suppressed`].
    pub suppress: bool,
}

/// What the user asks for in `UpdateVoiceState`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelfFlags {
    pub mute: bool,
    pub deaf: bool,
    pub video: bool,
    pub stream: bool,
}

/// Server mute and deafen. Kept while the server runs, even after the user
/// leaves, as Discord does; not saved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Moderation {
    pub mute: bool,
    pub deaf: bool,
}

/// Limits from the configuration file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceConfig {
    pub enabled: bool,
    /// Nobody gets past this, whatever their permissions.
    pub max_participants_per_channel: u32,
}

#[derive(Debug, Default)]
pub struct VoiceStates {
    by_user: HashMap<i64, VoiceState>,
    moderation: HashMap<i64, Moderation>,
}

impl VoiceStates {
    pub fn get(&self, user_id: i64) -> Option<&VoiceState> {
        self.by_user.get(&user_id)
    }

    pub fn all(&self) -> impl Iterator<Item = &VoiceState> {
        self.by_user.values()
    }

    pub fn in_channel(&self, channel_id: i64) -> impl Iterator<Item = &VoiceState> {
        self.by_user
            .values()
            .filter(move |state| state.channel_id == channel_id)
    }

    /// Stores the state, replacing the user's previous one.
    pub fn put(&mut self, state: VoiceState) {
        self.by_user.insert(state.user_id, state);
    }

    pub fn remove(&mut self, user_id: i64) -> Option<VoiceState> {
        self.by_user.remove(&user_id)
    }

    pub fn moderation(&self, user_id: i64) -> Moderation {
        self.moderation.get(&user_id).copied().unwrap_or_default()
    }

    pub fn set_moderation(&mut self, user_id: i64, moderation: Moderation) {
        if moderation == Moderation::default() {
            self.moderation.remove(&user_id);
        } else {
            self.moderation.insert(user_id, moderation);
        }
    }

    pub fn to_proto(&self, state: &VoiceState) -> proto::VoiceState {
        let moderation = self.moderation(state.user_id);
        proto::VoiceState {
            user_id: state.user_id,
            channel_id: Some(state.channel_id),
            session_id: state.session_id.clone(),
            self_mute: state.self_mute,
            self_deaf: state.self_deaf,
            server_mute: moderation.mute,
            server_deaf: moderation.deaf,
            suppress: state.suppress,
            self_video: state.self_video,
            self_stream: state.self_stream,
        }
    }

    /// Everyone in the voice channels `user_id` can view.
    pub fn visible_to(&self, guild: &Guild, user_id: i64) -> Vec<proto::VoiceState> {
        self.by_user
            .values()
            .filter(|state| guild.can_view(user_id, state.channel_id))
            .map(|state| self.to_proto(state))
            .collect()
    }
}

/// The state announced when a user leaves voice.
pub fn left(user_id: i64, session_id: &str) -> proto::VoiceState {
    proto::VoiceState {
        user_id,
        session_id: session_id.to_owned(),
        ..Default::default()
    }
}

/// Whether the user may not speak in the channel: no SPEAK there, or it is
/// the AFK channel.
pub fn suppressed(guild: &Guild, user_id: i64, channel_id: i64) -> bool {
    guild.voice_settings.afk_channel_id == Some(channel_id)
        || !guild
            .channel_permissions(user_id, channel_id)
            .contains(Permissions::SPEAK)
}

/// The voice channel `user_id` may enter with `flags`, or why not. Users
/// already in the channel are not counted against its limits.
pub fn check_join<'a>(
    guild: &'a Guild,
    states: &VoiceStates,
    config: VoiceConfig,
    user_id: i64,
    channel_id: i64,
    flags: SelfFlags,
) -> Result<&'a Channel, ApiError> {
    if !config.enabled {
        return Err(ApiError::forbidden("voice is turned off on this server"));
    }
    let channel = require_channel(guild, user_id, channel_id, Permissions::VIEW_CHANNEL)?;
    if channel.kind != ChannelKind::Voice {
        return Err(ApiError::invalid_argument("that is not a voice channel"));
    }
    require_channel(guild, user_id, channel_id, Permissions::CONNECT)?;
    let already_in = states
        .get(user_id)
        .is_some_and(|state| state.channel_id == channel_id);
    if !already_in {
        let present = states.in_channel(channel_id).count();
        let over_cap = present >= to_usize(config.max_participants_per_channel);
        let over_limit = channel.user_limit > 0
            && present >= to_usize(channel.user_limit)
            && !guild
                .channel_permissions(user_id, channel_id)
                .contains(Permissions::MOVE_MEMBERS);
        if over_cap || over_limit {
            return Err(ApiError::voice_channel_full());
        }
    }
    check_flags(guild, states, user_id, channel, flags)?;
    Ok(channel)
}

/// Whether `user_id` may turn on their camera or stream in `channel`.
pub fn check_flags(
    guild: &Guild,
    states: &VoiceStates,
    user_id: i64,
    channel: &Channel,
    flags: SelfFlags,
) -> Result<(), ApiError> {
    if flags.video {
        require_channel(guild, user_id, channel.id, Permissions::VIDEO)?;
        if !guild.voice_settings.camera_allowed {
            return Err(ApiError::forbidden("cameras are turned off on this server"));
        }
        let cameras_on = states
            .in_channel(channel.id)
            .filter(|state| state.user_id != user_id && state.self_video)
            .count();
        if cameras_on >= to_usize(guild.voice_settings.max_camera_participants) {
            return Err(ApiError::camera_limit());
        }
    }
    if flags.stream {
        require_channel(guild, user_id, channel.id, Permissions::SCREENSHARE)?;
    }
    Ok(())
}

/// What the voice node enforces for this channel.
pub fn limits(guild: &Guild, channel: &Channel) -> voice_proto::Limits {
    let settings = &guild.voice_settings;
    voice_proto::Limits {
        screen_share_max_resolution: settings.screen_share_max_resolution.to_proto() as i32,
        screen_share_max_fps: settings.screen_share_max_fps,
        voice_bitrate: channel.bitrate.min(settings.max_voice_bitrate),
        camera_allowed: settings.camera_allowed,
        max_camera_participants: settings.max_camera_participants,
        max_stream_viewers: settings.max_stream_viewers,
    }
}

fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use opencord_common::permissions::{Overwrite, OverwriteTarget};

    use super::*;
    use crate::guild::tests::sample_guild;

    const OWNER: i64 = 1;
    const ALICE: i64 = 2;
    const BOB: i64 = 4;
    const CONFIG: VoiceConfig = VoiceConfig {
        enabled: true,
        max_participants_per_channel: 99,
    };

    fn voice_channel(guild: &Guild) -> i64 {
        guild
            .channels
            .values()
            .find(|channel| channel.kind == ChannelKind::Voice)
            .unwrap()
            .id
    }

    fn text_channel(guild: &Guild) -> i64 {
        guild
            .channels
            .values()
            .find(|channel| channel.kind == ChannelKind::Text)
            .unwrap()
            .id
    }

    fn deny_everyone(guild: &mut Guild, channel_id: i64, deny: Permissions) {
        let everyone = guild.meta.everyone_role_id;
        guild.channels.get_mut(&channel_id).unwrap().overwrites = vec![Overwrite {
            target: OverwriteTarget::Role(everyone),
            allow: Permissions::empty(),
            deny,
        }];
    }

    fn in_voice(user_id: i64, channel_id: i64) -> VoiceState {
        VoiceState {
            user_id,
            channel_id,
            session_id: format!("s{user_id}"),
            self_mute: false,
            self_deaf: false,
            self_video: false,
            self_stream: false,
            suppress: false,
        }
    }

    fn add_member(guild: &mut Guild, user_id: i64) {
        let mut member = guild.members[&ALICE].clone();
        member.user.id = user_id;
        guild.members.insert(user_id, member);
    }

    fn code(result: Result<&Channel, ApiError>) -> proto::ErrorCode {
        result.unwrap_err().code
    }

    #[tokio::test]
    async fn members_join_voice_channels_by_default() {
        let guild = sample_guild().await;
        let channel = voice_channel(&guild);

        let joined = check_join(
            &guild,
            &VoiceStates::default(),
            CONFIG,
            ALICE,
            channel,
            SelfFlags::default(),
        );

        assert_eq!(joined.unwrap().id, channel);
    }

    #[tokio::test]
    async fn joining_needs_connect_and_a_voice_channel() {
        let mut guild = sample_guild().await;
        let voice = voice_channel(&guild);
        let text = text_channel(&guild);
        let none = VoiceStates::default();
        let join = |guild: &Guild, channel_id| {
            code(check_join(
                guild,
                &none,
                CONFIG,
                ALICE,
                channel_id,
                SelfFlags::default(),
            ))
        };

        assert_eq!(join(&guild, text), proto::ErrorCode::InvalidArgument);
        assert_eq!(join(&guild, 999), proto::ErrorCode::NotFound);
        deny_everyone(&mut guild, voice, Permissions::CONNECT);
        assert_eq!(join(&guild, voice), proto::ErrorCode::Forbidden);
        deny_everyone(&mut guild, voice, Permissions::VIEW_CHANNEL);
        assert_eq!(join(&guild, voice), proto::ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn voice_turned_off_in_the_config_refuses_everyone() {
        let guild = sample_guild().await;
        let off = VoiceConfig {
            enabled: false,
            ..CONFIG
        };

        let result = check_join(
            &guild,
            &VoiceStates::default(),
            off,
            OWNER,
            voice_channel(&guild),
            SelfFlags::default(),
        );

        assert_eq!(code(result), proto::ErrorCode::Forbidden);
    }

    #[tokio::test]
    async fn the_user_limit_holds_unless_you_can_move_members() {
        let mut guild = sample_guild().await;
        let channel = voice_channel(&guild);
        guild.channels.get_mut(&channel).unwrap().user_limit = 1;
        add_member(&mut guild, BOB);
        let mut states = VoiceStates::default();
        states.put(in_voice(BOB, channel));

        let alice = check_join(
            &guild,
            &states,
            CONFIG,
            ALICE,
            channel,
            SelfFlags::default(),
        );
        let owner = check_join(
            &guild,
            &states,
            CONFIG,
            OWNER,
            channel,
            SelfFlags::default(),
        );
        let bob_again = check_join(&guild, &states, CONFIG, BOB, channel, SelfFlags::default());

        assert_eq!(code(alice), proto::ErrorCode::VoiceChannelFull);
        assert!(owner.is_ok(), "MOVE_MEMBERS bypasses the user limit");
        assert!(bob_again.is_ok(), "people already in are not counted twice");
    }

    #[tokio::test]
    async fn the_configured_cap_holds_for_everyone() {
        let mut guild = sample_guild().await;
        let channel = voice_channel(&guild);
        add_member(&mut guild, BOB);
        let mut states = VoiceStates::default();
        states.put(in_voice(BOB, channel));
        let cap_of_one = VoiceConfig {
            max_participants_per_channel: 1,
            ..CONFIG
        };

        let owner = check_join(
            &guild,
            &states,
            cap_of_one,
            OWNER,
            channel,
            SelfFlags::default(),
        );

        assert_eq!(code(owner), proto::ErrorCode::VoiceChannelFull);
    }

    #[tokio::test]
    async fn cameras_need_video_the_server_setting_and_room() {
        let mut guild = sample_guild().await;
        let channel_id = voice_channel(&guild);
        add_member(&mut guild, BOB);
        let camera = SelfFlags {
            video: true,
            ..SelfFlags::default()
        };
        let mut states = VoiceStates::default();
        let check = |guild: &Guild, states: &VoiceStates| {
            check_flags(guild, states, ALICE, &guild.channels[&channel_id], camera)
                .map_err(|error| error.code)
        };

        assert_eq!(check(&guild, &states), Ok(()));
        guild.voice_settings.max_camera_participants = 1;
        states.put(VoiceState {
            self_video: true,
            ..in_voice(BOB, channel_id)
        });
        assert_eq!(check(&guild, &states), Err(proto::ErrorCode::CameraLimit));
        guild.voice_settings.camera_allowed = false;
        assert_eq!(check(&guild, &states), Err(proto::ErrorCode::Forbidden));
        guild.voice_settings.camera_allowed = true;
        guild.voice_settings.max_camera_participants = 25;
        deny_everyone(&mut guild, channel_id, Permissions::VIDEO);
        assert_eq!(check(&guild, &states), Err(proto::ErrorCode::Forbidden));
    }

    #[tokio::test]
    async fn streaming_needs_screenshare() {
        let mut guild = sample_guild().await;
        let channel_id = voice_channel(&guild);
        deny_everyone(&mut guild, channel_id, Permissions::SCREENSHARE);
        let stream = SelfFlags {
            stream: true,
            ..SelfFlags::default()
        };

        let result = check_flags(
            &guild,
            &VoiceStates::default(),
            ALICE,
            &guild.channels[&channel_id],
            stream,
        );

        assert_eq!(result.unwrap_err().code, proto::ErrorCode::Forbidden);
    }

    #[tokio::test]
    async fn people_without_speak_or_in_the_afk_channel_are_suppressed() {
        let mut guild = sample_guild().await;
        let channel = voice_channel(&guild);

        assert!(!suppressed(&guild, ALICE, channel));
        guild.voice_settings.afk_channel_id = Some(channel);
        assert!(suppressed(&guild, ALICE, channel));
        guild.voice_settings.afk_channel_id = None;
        deny_everyone(&mut guild, channel, Permissions::SPEAK);
        assert!(suppressed(&guild, ALICE, channel));
        assert!(!suppressed(&guild, OWNER, channel));
    }

    #[tokio::test]
    async fn limits_cap_the_channel_bitrate_at_the_server_maximum() {
        let mut guild = sample_guild().await;
        let channel_id = voice_channel(&guild);
        guild.voice_settings.max_voice_bitrate = 48_000;

        let limits = limits(&guild, &guild.channels[&channel_id]);

        assert_eq!(limits.voice_bitrate, 48_000);
        assert_eq!(limits.max_stream_viewers, 50);
        assert!(limits.camera_allowed);
    }

    #[test]
    fn moderation_is_remembered_after_leaving() {
        let mut states = VoiceStates::default();
        states.put(in_voice(ALICE, 10));
        states.set_moderation(
            ALICE,
            Moderation {
                mute: true,
                deaf: false,
            },
        );

        states.remove(ALICE);
        states.put(in_voice(ALICE, 11));

        let shown = states.to_proto(states.get(ALICE).unwrap());
        assert!(shown.server_mute);
        assert_eq!(shown.channel_id, Some(11));
    }
}
