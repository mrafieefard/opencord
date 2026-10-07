# Gateway protocol (v1)

This document is the source of truth for the wire protocol between Opencord clients and servers. The message definitions live in [`proto/opencord/v1/`](../proto/opencord/v1/). Shared rules (permission bits, the resolver, limits, the signed payload) are implemented once in `crates/opencord-common`.

Conventions:
- All ids are 64-bit Snowflakes (`int64`). The 41 bits of milliseconds count from 2026-01-01T00:00:00Z, followed by 10 bits of worker id and 12 bits of sequence. Ids sort by creation time.
- All timestamps are Unix milliseconds.
- String lengths count Unicode scalar values.

## Transport and framing

- One WebSocket connection per client per server, at `wss://host:port/gateway`. The default port is 7710.
- TLS is always on. A server without a configured certificate generates a self-signed one on first start. Clients pin the SHA-256 fingerprint of the server's DER certificate on first use (TOFU), or verify it against an invite link's `#fp=`. Certificates from a public CA validate normally.
- **Binary frames only.** Each frame holds exactly one protobuf `Envelope`. Frames over 1 MiB are rejected.
- `Envelope.seq` is set only on server-to-client `Event` envelopes. It starts at 1 and increases by one per event within a session.
- `Envelope.request_id` is chosen by the client for each `Request` and echoed on the matching `Response`.

Invite links have the form `opencord://host:port/invite/CODE#fp=SHA256HEX`. The port defaults to 7710; the fingerprint is optional. IPv6 hosts use brackets.

## Handshake

```
Client                                         Server
  |  <------------- Hello -------------------   |  server_id, server_name, nonce (32 B),
  |                                             |  heartbeat_interval_ms, protocol_version
  |  -------------- Identify --------------->   |  public_key, signature, timestamp_ms,
  |                                             |  display_name, invite_code?, claim_token?,
  |                                             |  protocol_version
  |  <------------- Ready -------------------   |  session_id, resume_token, self_user,
  |                                             |  server, channels, roles, members,
  |                                             |  presences, server_permissions,
  |                                             |  channel_permissions
```

1. The server sends `Hello` right after the upgrade, with a fresh random `nonce`.
2. The client must answer with `Identify` or `Resume` within **10 seconds**, or the server closes the connection (4004).
3. The `Identify` signature is an Ed25519 signature over:

   ```
   "opencord-auth-v1" (16 bytes) || server_id (16 bytes) || certificate (32 bytes) || nonce (32 bytes) || timestamp_ms (u64 big-endian)
   ```

   `certificate` is the SHA-256 fingerprint of the TLS certificate the client verified on this connection, and the server checks it against its own certificate. This binds the signature to the TLS endpoint: a malicious server that passes another server's `Hello` through to its users cannot use their answers there, because they cover its own certificate, not the other server's. One consequence: TLS must end at the Opencord server itself (a proxy can pass TLS through, but must not terminate it).

4. The server rejects the `Identify` with an `Error` and closes the connection (4003) when:
   - the protocol version differs from the server's;
   - the public key or signature is malformed or does not verify;
   - `timestamp_ms` is more than 60 seconds from the server clock;
   - the user is banned;
   - the user is not a member, and there is no valid invite, no valid claim token, and open joining is off.

   Because the nonce is fresh for every connection, only one `Identify` is accepted per connection, and the signature covers the server's certificate, a captured signature cannot be replayed on another connection or server.
5. The user's identity is the public key:
   - **Unknown key with a valid invite**, or with open joining on: the server creates the user (with `display_name`) and the membership, and uses up one invite use.
   - **Valid claim token:** the user becomes the server **owner**, and the token is invalidated.
   - **Existing member:** `display_name` is ignored; use `UpdateProfile` to change it.
6. `Ready` contains only the channels the user can view. `channel_permissions` maps each of those channels to the user's resolved permissions there.

Identify attempts are limited to 5 per minute per IP address. Over the limit, the server sends `RATE_LIMITED` and closes the connection (4008).

## Heartbeat

- The client sends `Heartbeat { last_seq }` every `heartbeat_interval_ms` (default 30 000), and the server answers `HeartbeatAck`.
- If two intervals pass without a heartbeat, the server closes the connection (4005). The session stays resumable.
- A client that gets no `HeartbeatAck` before its next heartbeat should treat the connection as dead and reconnect.

## Resume

- The server keeps each session's **last 1 000 events, up to 60 seconds old**. A session stays resumable for 60 seconds after its connection drops.
- To resume, the client opens a new connection and answers `Hello` with `Resume { session_id, resume_token, last_seq }`.
- If every event after `last_seq` is still buffered, the server replays them with their original `seq` values, then sends `Resumed`. If the old connection is still open, it is closed (4009).
- Otherwise the server sends `Error(INVALID_SESSION)` and keeps the connection open. The client then sends `Identify` on the same connection, signing the same `Hello` nonce.
- Responses are not replayed. A request whose response was lost must be retried.

## Requests

Each `Request` gets exactly one `Response`, which is either an `error` or the payload listed below. Channel permissions are resolved with overwrites (see [Permissions](#permissions)). "Base" means server-wide, without overwrites.

| Request | Response | Who may send it |
|---|---|---|
| `SendMessage { channel_id, content, nonce }` | `message` | `SEND_MESSAGES` in a text channel |
| `EditMessage { message_id, content }` | `message` | The author, if they can still view the channel |
| `DeleteMessage { message_id }` | `ack` | The author, or `MANAGE_MESSAGES` in the channel |
| `FetchMessages { channel_id, before?, limit }` | `messages`, newest first | `VIEW_CHANNEL` and `READ_HISTORY` in the channel |
| `StartTyping { channel_id }` | `ack` | `SEND_MESSAGES` in the channel |
| `CreateChannel { kind, name, topic?, parent_id? }` | `channel` | Base `MANAGE_CHANNELS` |
| `UpdateChannel { channel_id, name?, topic?, parent_id? }` | `channel` | `MANAGE_CHANNELS` in the channel |
| `DeleteChannel { channel_id }` | `ack` | `MANAGE_CHANNELS` in the channel |
| `ReorderChannels { positions }` | `ack` | Base `MANAGE_CHANNELS` |
| `SetChannelOverwrite { channel_id, overwrite }` | `channel` | `MANAGE_ROLES` in the channel; can only allow or deny permissions the caller has there |
| `DeleteChannelOverwrite { channel_id, target_kind, target_id }` | `channel` | `MANAGE_ROLES` in the channel |
| `CreateRole { name, color, permissions, hoist, mentionable }` | `role` | Base `MANAGE_ROLES`; can only grant permissions the caller has |
| `UpdateRole { role_id, … }` | `role` | Base `MANAGE_ROLES`; the role ranks below the caller; can only grant permissions the caller has |
| `DeleteRole { role_id }` | `ack` | Base `MANAGE_ROLES`; the role ranks below the caller; not @everyone |
| `ReorderRoles { role_ids }` | `ack` | Base `MANAGE_ROLES`; every listed role ranks below the caller |
| `AddMemberRole` / `RemoveMemberRole { user_id, role_id }` | `member` | Base `MANAGE_ROLES`; the role ranks below the caller; not @everyone; adding needs every permission the role carries (assigning a role grants them) |
| `KickMember { user_id, reason? }` | `ack` | Base `KICK_MEMBERS`; outranks the target |
| `BanMember { user_id, reason? }` | `ack` | Base `BAN_MEMBERS`; outranks the target if they are a member |
| `UnbanMember { user_id }` | `ack` | Base `BAN_MEMBERS` |
| `FetchBans {}` | `bans` | Base `BAN_MEMBERS` |
| `UpdateNickname { user_id, nickname? }` | `member` | Own nickname: `CHANGE_NICKNAME`. Someone else's: `MANAGE_NICKNAMES` and outranks them |
| `UpdateProfile { display_name }` | `user` | Anyone |
| `UpdatePresence { status }` | `ack` | Anyone; `OFFLINE` appears offline |
| `CreateInvite { max_uses?, expires_in_s? }` | `invite` | Base `CREATE_INVITE` |
| `FetchInvites {}` | `invites` | Base `MANAGE_SERVER` |
| `RevokeInvite { code }` | `ack` | Base `MANAGE_SERVER`, or the invite's creator |
| `UpdateServer { name?, description?, open_join? }` | `server` | Base `MANAGE_SERVER` |
| `UpdateVoiceSettings { … }` | `voice_settings` | Base `MANAGE_SERVER`; see [Voice](#voice) |
| `RefreshMediaToken {}` | `media_token` | Anyone |
| `UpdateVoiceState { channel_id?, self_mute, self_deaf, self_video, self_stream }` | `voice_state` | `CONNECT` to join; see [Voice](#voice) |
| `ServerMuteMember` / `ServerDeafenMember { user_id, value }` | `voice_state` | `MUTE_MEMBERS` / `DEAFEN_MEMBERS` in the target's channel; outranks the target unless it is the caller |
| `MoveMember { user_id, channel_id }` | `voice_state` | `MOVE_MEMBERS` in both channels; outranks the target unless it is the caller; the target can view and connect to the destination |
| `DisconnectMember { user_id }` | `ack` | `MOVE_MEMBERS` in the target's channel; outranks the target unless it is the caller |

Details:
- **Messages:** the content is trimmed and must be 1–4 000 characters. Only text channels accept messages.
- **Mentions and channel links** are written inside the content as `<@user_id>` and `<#channel_id>`. The server stores the content as plain text; clients render these tokens as names.
- **Message nonce:** echoed on the `message` response and on the `MessageCreate` event, so the sender can match its pending message whichever arrives first.
- **Partial updates:** absent optional fields are left unchanged. An empty topic clears it. `parent_id: 0` moves a channel out of its category.
- **Channel names:** text channel names are normalized to lowercase-kebab (`My Chat!` becomes `my-chat`).
- **Categories:** a channel's parent must be a category, and categories cannot have parents. Deleting a category moves its children to the top level.
- **New roles** are placed directly above @everyone.
- **`ReorderRoles`** rearranges the listed roles among the positions they already hold, so it can never move a role above the caller.
- **Kicks and bans** end all of the target's sessions; those sessions cannot be resumed.
- **Cleanup:** deleting a role removes it from every member and deletes its channel overwrites. Kicking or banning a member deletes their member overwrites.
- **Deleted messages** disappear from history. They are kept in the database, marked as deleted.
- **Rate limits** are per user: 50 requests per 10 s, plus 5 messages per 5 s per channel and 10 `UpdateVoiceState` per 10 s. Over the limit, the response is `RATE_LIMITED` with `retry_after_ms`.
- **Not yet supported:** the screen share requests (`CreateStream`, `UpdateStream`, `DeleteStream`, `WatchStream`, `UnwatchStream`) and the soundboard requests answer `INVALID_ARGUMENT` until those features arrive.

## Events

Every event goes to every session of the server, with these exceptions:

| Event | Audience |
|---|---|
| `MessageCreate`, `MessageUpdate`, `MessageDelete`, `TypingStart`, `ChannelUpdate` | Sessions that can view the channel |
| `ChannelCreate` | Sessions that can view the new channel, and sessions for whom a permission change made a channel visible |
| `ChannelDelete` | Sessions that could view the deleted channel, and sessions for whom a permission change hid a channel |
| `VoiceStateUpdate` | Sessions that can view the channel joined or left (both, for a move), and the user's own sessions |
| `VoiceServerUpdate` | The one session that joined or was moved |

Other events:
- `RoleCreate`, `RoleUpdate`, `RoleDelete`, `ServerUpdate` and `PresenceUpdate` behave as their names say.
- `MemberJoin` and `MemberLeave` are sent when a member joins or leaves. `MemberLeave` also covers kicks and bans.
- `MemberUpdate` is sent when a member's roles, nickname or display name change.
- Reordering sends one `ChannelUpdate` or `RoleUpdate` per moved item.

When roles, member roles or overwrites change, the server recomputes visibility for every session and sends `ChannelCreate` or `ChannelDelete` accordingly. Clients recompute their own permissions from the roles, member and overwrite events with the shared resolver.

## Errors

| Code | Meaning |
|---|---|
| `UNAUTHORIZED` | The handshake failed: bad signature, stale timestamp, banned, or not a member |
| `FORBIDDEN` | Missing permission or outranked |
| `NOT_FOUND` | The channel, message, role, member or invite does not exist or is not visible |
| `INVALID_ARGUMENT` | Malformed or out-of-range input |
| `RATE_LIMITED` | Too many requests; see `retry_after_ms` |
| `INVALID_SESSION` | A resume failed; send `Identify` |
| `CONFLICT` | The request conflicts with the current state (for example, a limit was reached) |
| `INTERNAL` | A server bug or storage failure |
| `VOICE_CHANNEL_FULL` | The voice channel is at its user limit, or at the server's per-channel cap |
| `VOICE_NOT_CONNECTED` | The target (or the caller) is not in a voice channel |
| `QUALITY_LIMIT` | A stream or track asks for more than the server allows |
| `CAMERA_LIMIT` | The channel's camera limit has been reached |
| `STREAM_VIEWER_LIMIT` | The stream already has as many viewers as the server allows |
| `SOUND_COOLDOWN` | Wait before playing another sound, or too many are playing |
| `SOUND_TOO_LONG`, `SOUND_INVALID` | An uploaded sound is over 5 seconds, or not a valid Ogg Opus file |
| `SOUNDBOARD_FULL` | The server holds as many sounds as it allows |

## Close codes

| Code | Meaning | Resumable |
|---|---|---|
| 1001 | Server shutting down | Yes |
| 4000 | Unknown error | Yes |
| 4001 | Invalid frame (not binary, undecodable, or too large) | Yes |
| 4002 | Request sent before the handshake finished | No |
| 4003 | Authentication failed | No |
| 4004 | Handshake timeout | No |
| 4005 | Heartbeat timeout | Yes |
| 4008 | Identify rate limit | No |
| 4009 | Session resumed elsewhere | No |
| 4010 | Kicked | No |
| 4011 | Banned | No |
| 4012 | Too slow to receive events | Yes |

## Permissions

The bits are listed below. `ATTACH_FILES` is reserved for a later phase.

| Bit | Name | Bit | Name |
|---|---|---|---|
| 0 | `VIEW_CHANNEL` | 16 | `CONNECT` |
| 1 | `SEND_MESSAGES` | 17 | `SPEAK` |
| 2 | `READ_HISTORY` | 18 | `VIDEO` (camera) |
| 3 | `MANAGE_MESSAGES` | 19 | `SCREENSHARE` |
| 4 | `MANAGE_CHANNELS` | 20 | `MUTE_MEMBERS` |
| 5 | `MANAGE_ROLES` | 21 | `DEAFEN_MEMBERS` |
| 6 | `KICK_MEMBERS` | 22 | `MOVE_MEMBERS` (also joins full channels) |
| 7 | `BAN_MEMBERS` | 23 | `PRIORITY_SPEAKER` |
| 8 | `CREATE_INVITE` | 24 | `USE_VOICE_ACTIVITY` (without it, push-to-talk is forced; only clients can enforce it) |
| 9 | `MANAGE_SERVER` | 25 | `USE_SOUNDBOARD` |
| 10 | `ATTACH_FILES` | 26 | `USE_EXTERNAL_SOUNDS` |
| 11 | `MENTION_EVERYONE` | 27 | `MANAGE_SOUNDBOARD` |
| 12 | `CHANGE_NICKNAME` | 63 | `ADMINISTRATOR` |
| 13 | `MANAGE_NICKNAMES` | | |

Resolution:
1. The owner has every permission.
2. `base = @everyone | every role the member has`.
3. If `base` contains `ADMINISTRATOR`, the member has every permission.
4. Channel overwrites are applied in order, each as `(perms & !deny) | allow`:
   1. the @everyone overwrite;
   2. the overwrites for the member's roles, with their allows and denies combined first;
   3. the member's own overwrite.
5. Without `VIEW_CHANNEL`, the member has no permissions in the channel.

Hierarchy:
- @everyone has position 0 and cannot be deleted. A member's rank is their highest role's position (0 if they have no roles).
- Members can only manage roles below their rank, and only kick, ban or rename members of lower rank. The owner outranks everyone; nobody outranks the owner.
- Nobody can grant or deny a permission they do not have, unless they are an administrator or the owner.

A new server's @everyone has `VIEW_CHANNEL | SEND_MESSAGES | READ_HISTORY | CREATE_INVITE | CHANGE_NICKNAME | CONNECT | SPEAK | VIDEO | SCREENSHARE | USE_VOICE_ACTIVITY | USE_SOUNDBOARD | USE_EXTERNAL_SOUNDS`. Servers created before voice existed get the last three added to @everyone when they upgrade. The server starts with a `#general` text channel and a `General` voice channel.

## Limits

| Item | Limit |
|---|---|
| Message | 1–4 000 characters, trimmed |
| Display name, nickname | 1–32 characters, no control characters |
| Channel name, role name, server name | 1–100 characters |
| Topic | Up to 1 024 characters |
| Server description | Up to 1 000 characters |
| Kick and ban reason | Up to 512 characters |
| Roles per server, including @everyone | 250 |
| Channels per server | 500 |
| `FetchMessages` limit | 1–100; 0 means 50, larger values are capped |

## Voice

Voice follows Discord's design (Phase 2 plan §3). Joining is a request on this gateway; audio and video go to a **voice node** with its own gateway (`proto/opencord/v1/voice.proto`, package `opencord.voice.v1`) and media over ICE + DTLS-SRTP on one UDP port (7711 by default). The voice node arrives in milestone V1; until then joining changes only the voice state.

### Voice states

- A voice state says who is in which voice channel, with their self mute, deafen, camera and stream flags, their server mute and deafen, and `suppress` (no `SPEAK` in the channel, or in the AFK channel). States live in server memory only.
- A user has at most one voice state per server, held by one gateway session. Joining from another session takes it over; the old session sees the new `session_id`.
- `UpdateVoiceState` with a `channel_id` joins, moves within the server, or changes the self flags. Without one, it leaves, from whichever session holds the state.
- Joining needs `CONNECT` in a voice channel the user can view, and room: the channel's user limit (skipped with `MOVE_MEMBERS`) and the server's per-channel cap from `opencord.toml` (never skipped). A camera needs `VIDEO`, cameras allowed on the server, and room under the per-channel camera limit; a stream needs `SCREENSHARE`.
- Losing `VIEW_CHANNEL` ends a voice state; losing `CONNECT` does not. Server mute and deafen are remembered per user while the server runs, across leaving and rejoining.
- **Grace period:** a voice state outlives its session's dropped connection by 30 seconds (checked every 5), so a resume keeps the call. A session that ends or expires ends its voice state.
- A voice channel that becomes visible to someone (`ChannelCreate`) is followed by a `VoiceStateUpdate` for each person in it.

### Joining a voice node

After a join or a move, the session gets `VoiceServerUpdate { channel_id, endpoint, certificate_fingerprint, token }`:

- `endpoint` is the voice gateway's `wss://` URL; empty means the server's own voice node, on the same host and port as this gateway, at `/voice`.
- `certificate_fingerprint` is the voice node's certificate, to pin for that connection. It arrives over the already pinned gateway.
- `token` is single use and valid for 60 seconds: the encoded `VoiceTokenClaims` (`proto/opencord/v1/internal.proto`) followed by the server's Ed25519 signature over `"opencord-voice-token-v1" || claims`. It names the user, channel and session, the resolved channel permissions, the channel's limits and the flags. Voice nodes hold the public key and check tokens without a database. Clients treat it as opaque bytes.

The server makes the signing key (`voice-signing.key`) in its data folder on first start.

### Settings

`VoiceSettings` (in `Ready` and `VoiceSettingsUpdate`, changed with `UpdateVoiceSettings`):

| Setting | Default | Range |
|---|---|---|
| `screen_share_max_resolution` | 720p | 480p, 720p, 1080p, 1440p, Source |
| `screen_share_max_fps` | 30 | 15, 30, 60 |
| `max_stream_viewers` | 50 | 1–200 |
| `camera_allowed` | on | |
| `max_camera_participants` | 25 | 1–50 |
| `max_voice_bitrate` | 96 000 | 32 000–256 000 bits per second |
| `afk_channel_id` | none | a voice channel; 0 clears it |
| `afk_timeout_s` | 300 | 60, 300, 900, 1 800, 3 600 |
| `soundboard_enabled`, `allow_default_sounds`, `allow_external_sounds` | on | |
| `sound_cooldown_s` | 3 | 0–30 |
| `max_sounds` | 48 | 0–200 |

Deleting the AFK channel clears the setting. Voice channels carry `bitrate` (default 64 000, from 8 000 to `max_voice_bitrate`), `user_limit` (0 is none, at most 99) and `text_in_voice` (default on), changed with `UpdateChannel` on voice channels only.

### Ready

`Ready` also carries `voice_enabled` (whether the server has voice at all), the voice states of every voice channel the user can view, active streams, the soundboard sounds (metadata), the voice settings and a `media_token`.

### Media endpoints

`GET /media/sounds/{sha256}` returns a soundboard sound (`data/sounds/<sha256>.opus`, lowercase hex). Requests need `Authorization: Bearer <media_token>`: the encoded `MediaTokenClaims` plus the signature over `"opencord-media-token-v1" || claims`, base64url without padding, valid for 24 hours. `RefreshMediaToken` gives a new one. Answers: 401 without a valid token, 403 for someone no longer a member, 400 for a malformed hash, 404 for an unknown sound. The upload endpoints (`POST /media/sounds`, `POST /media/external-sounds`) answer 501 until the soundboard arrives.

`GET /info` adds `voice_enabled` and `voice_udp_port`.

### Voice limits

| Item | Limit |
|---|---|
| `UpdateVoiceState` | 10 per 10 s per user |
| Voice token | 60 seconds, single use |
| Media token | 24 hours |
| Voice grace period | 30 seconds (up to 35) |

