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
| `RefreshVoiceServer {}` | `ack`, then a fresh `VoiceServerUpdate` | The session holding the caller's voice state; otherwise `VOICE_NOT_CONNECTED` |
| `CreateStream { channel_id, source_kind, resolution, fps, has_audio }` | `stream` | A voice state in that channel, `SCREENSHARE` there, a quality within the server's cap; 5 per minute. Going live again replaces the stream and keeps its viewers |
| `UpdateStream { stream_key, resolution?, fps?, has_audio? }` | `stream` | The streamer; within the server's cap |
| `DeleteStream { stream_key }` | `ack` | The streamer |
| `WatchStream` / `UnwatchStream { stream_key }` | `ack` | A voice state in the stream's channel, not one's own stream, room under `max_stream_viewers` (`STREAM_VIEWER_LIMIT`) |
| `PlaySoundboardSound { channel_id, sound_ref, name?, emoji?, volume? }` | `ack` | See [Soundboard](#soundboard) |
| `UpdateSoundboardSound { sound_id, name?, emoji?, volume? }` | `soundboard_sound` | Base `MANAGE_SOUNDBOARD`; an empty emoji clears it |
| `DeleteSoundboardSound { sound_id }` | `ack` | Base `MANAGE_SOUNDBOARD` |
| `WatchNetwork { watch }` | `network_info` | The owner; see [Domains and network](#domains-and-network) |
| `SetPublicAddress { auto_detect, override_ipv4, override_ipv6 }` | `network_info` | The owner |
| `DetectPublicAddress {}` | `network_info` | The owner |
| `UpdateNetworkSettings { main_port?, upnp_enabled?, local_listener_port?, trusted_proxies?, dns_resolvers?, acme_staging? }` | `network_info` | The owner |
| `AddDomain { hostname, kind, public_port? }` | `network_info` | The owner; at most 20 domains |
| `UpdateDomain { domain_id, active?, primary?, public_port?, confirm }` | `network_info` | The owner |
| `DeleteDomain { domain_id }` | `network_info` | The owner |
| `CheckDomain { domain_id }` | `network_info`; the result follows as `NetworkStatus` | The owner; once per 10 s per domain |
| `FetchDomainChecks { domain_id }` | `domain_checks`, newest first, up to 10 | The owner |
| `RequestCertificate { domain_id, email?, agree_terms }` | `network_info`; progress follows as `NetworkStatus` | The owner; a verified own domain or free subdomain; `agree_terms` until an account exists; 5 failures per hour per domain (`ACME_RATE_LIMITED` with `retry_after_ms`) |
| `UploadCertificate { domain_id, chain_pem, key_pem }` | `network_info` | The owner; 64 KB each; not for tunnel or proxy domains |
| `SetDnsProvider { kind, token }` | `network_info` | The owner; `cloudflare`, `duckdns` or `desec`; an empty token disconnects |
| `AddFreeSubdomain { provider, name, token, email?, agree_terms }` | `network_info`; the check and certificate follow as `NetworkStatus` | The owner; `duckdns` or `desec`; `agree_terms` until an ACME account exists |
| `StartTunnel { kind, token, hostname }` | `network_info`; the tunnel's state follows as `NetworkStatus` | The owner; a named tunnel needs its token (empty keeps the saved one) and hostname |
| `StopTunnel {}` | `network_info` | The owner |
| `FetchTunnelLog {}` | `tunnel_log`, cloudflared's last 200 lines, the token hidden | The owner |
| `InstallCloudflared {}` | `network_info`; `tunnel.installing` until done | The owner; Linux |

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
- **Rate limits** are per user: 50 requests per 10 s, plus 5 messages per 5 s per channel and 10 `UpdateVoiceState` or `RefreshVoiceServer` per 10 s. Over the limit, the response is `RATE_LIMITED` with `retry_after_ms`.

## Events

Every event goes to every session of the server, with these exceptions:

| Event | Audience |
|---|---|
| `MessageCreate`, `MessageUpdate`, `MessageDelete`, `TypingStart`, `ChannelUpdate` | Sessions that can view the channel |
| `ChannelCreate` | Sessions that can view the new channel, and sessions for whom a permission change made a channel visible |
| `ChannelDelete` | Sessions that could view the deleted channel, and sessions for whom a permission change hid a channel |
| `VoiceStateUpdate` | Sessions that can view the channel joined or left (both, for a move), and the user's own sessions |
| `VoiceServerUpdate` | The one session that joined or was moved |
| `StreamCreate`, `StreamUpdate`, `StreamDelete` | Sessions that can view the stream's channel |
| `StreamViewersUpdate` | The streamer's sessions |
| `VoiceChannelEffect` | Sessions whose user is connected to that voice channel, the player included |
| `NetworkStatus` | The owner's sessions that sent `WatchNetwork { watch: true }`, until they stop or end |
| `ServerAddressesUpdate` | Everyone, when the active domains, the primary or the public address change |
| `ServerNotice` | Whom it concerns: the owner for the network, domains and certificates |

Other events:
- `RoleCreate`, `RoleUpdate`, `RoleDelete`, `ServerUpdate` and `PresenceUpdate` behave as their names say.
- `MemberJoin` and `MemberLeave` are sent when a member joins or leaves. `MemberLeave` also covers kicks and bans.
- `MemberUpdate` is sent when a member's roles, nickname or display name change.
- Reordering sends one `ChannelUpdate` or `RoleUpdate` per moved item.
- `SoundboardSoundCreate`, `SoundboardSoundUpdate` and `SoundboardSoundDelete` go to every session: everyone sees the soundboard.

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

Voice follows Discord's design (Phase 2 plan §3). Joining is a request on this gateway; audio and video go to a **voice node** with its own gateway (`proto/opencord/v1/voice.proto`, package `opencord.voice.v1`) and media over ICE + DTLS-SRTP on one UDP port (7711 by default). The voice node runs inside the server (the embedded node, at `/voice`), or on other machines as `opencord-voice-node` (external nodes, see [Voice nodes](#voice-nodes)).

### Voice states

- A voice state says who is in which voice channel, with their self mute, deafen, camera and stream flags, their server mute and deafen, and `suppress` (no `SPEAK` in the channel, or in the AFK channel). States live in server memory only.
- A user has at most one voice state per server, held by one gateway session. Joining from another session takes it over; the old session sees the new `session_id`.
- `UpdateVoiceState` with a `channel_id` joins, moves within the server, or changes the self flags. Without one, it leaves, from whichever session holds the state.
- A client whose voice connection failed for good (a refused or expired token, a resume that did not work) sends `RefreshVoiceServer` for a fresh `VoiceServerUpdate`; changing only the self flags never sends one.
- Joining needs `CONNECT` in a voice channel the user can view, and room: the channel's user limit (skipped with `MOVE_MEMBERS`) and the server's per-channel cap from `opencord.toml` (never skipped). A camera needs `VIDEO`, cameras allowed on the server, and room under the per-channel camera limit; a stream needs `SCREENSHARE`.
- Losing `VIEW_CHANNEL` ends a voice state; losing `CONNECT` does not. Server mute and deafen are remembered per user while the server runs, across leaving and rejoining.
- **Grace period:** a voice state outlives its session's dropped connection by 30 seconds (checked every 5), so a resume keeps the call. A session that ends or expires ends its voice state.
- A voice channel that becomes visible to someone (`ChannelCreate`) is followed by a `VoiceStateUpdate` for each person in it.

### Joining a voice node

After a join or a move, the session gets `VoiceServerUpdate { channel_id, endpoint, certificate_fingerprint, token }`:

- `endpoint` is the voice gateway's `wss://host:port` URL (the path defaults to `/voice`); empty means the server's own voice node, on the same host and port as this gateway, at `/voice`.
- `certificate_fingerprint` is the voice node's certificate, to pin for that connection. It arrives over the already pinned gateway.
- `token` is single use and valid for 60 seconds: the encoded `VoiceTokenClaims` (`proto/opencord/v1/internal.proto`) followed by the server's Ed25519 signature over `"opencord-voice-token-v1" || claims`. It names the user, channel and session, the resolved channel permissions, the channel's limits and the flags. Voice nodes hold the public key and check tokens without a database. Clients treat it as opaque bytes.

The server makes the signing key (`voice-signing.key`) in its data folder on first start.

Everyone in a channel is on the same node. A channel's first participant picks the node with the fewest people (then the fewest channels; ties go to the embedded node), and the channel keeps it until it is empty. When no node is available, joining fails with `CONFLICT`. When a channel's node goes away, everyone in it gets a new `VoiceServerUpdate` for another node; while there is none, they keep their voice state and get one as soon as a node registers. A move also sends a new `VoiceServerUpdate`: the old voice connection is closed and the client connects again.

### Voice gateway

Every WebSocket binary frame carries one `Envelope { seq, payload }`.

1. The node sends `Hello { heartbeat_interval_ms }` (5 000).
2. The client sends `Identify { token, user_id, session_id, channel_id }` within 10 seconds, with the token and ids from `VoiceServerUpdate`, or `Resume` (below).
3. The node answers `Ready { voice_session_id, resume_token, audio_ssrc, ice_ufrag, ice_pwd, candidates, dtls_fingerprint, codecs, limits, participants }`. `participants` lists everyone already in the channel with their audio SSRC. A candidate with an empty `ip` means the host the client reached the voice gateway with.
4. The client sends `TransportInfo { ice_ufrag, ice_pwd, dtls_fingerprint }`, its half of ICE and DTLS, and connects media to the candidate.

After that the client sends `Heartbeat { nonce }` every interval and gets `HeartbeatAck { nonce }`; two intervals without one close the connection (4005). `Speaking { flags }` (microphone 1, soundshare 2, priority 4) is relayed to everyone else in the channel with the speaker's `user_id`; the priority flag is dropped without `PRIORITY_SPEAKER`. `ClientConnect { user_id, audio_ssrc }` and `ClientDisconnect { user_id }` say who joined and left.

**Media** uses str0m's RTP mode without SDP. The node is ICE-lite and the DTLS server; the client controls ICE and is the DTLS client. RTP timestamps follow the sender's capture clock, so pauses show, and a talk spurt's first packet carries the marker bit. After a network change the client sends from a new socket; when a connectivity check nominating the new address arrives signed with the client's ICE password, the node sends there from then on. Audio is Opus, payload type 111, at 48 kHz, with the RFC 6464 audio level header extension. A client sends on media id `a`, and receives each other person on media id `u` followed by their audio SSRC as eight hex digits. The node forwards packets without decoding them: nothing from the server-muted, suppressed or self-muted, nothing to the deafened, and in channels of more than 50 people only the 10 loudest.

**Music** (Phase 3). A channel playing music has a stream of its own, not a participant: `MusicStream { channel_id, ssrc, active }` says it started or ended, and `Ready.music` carries it for whoever joins while it plays. It arrives on media id `u` followed by its SSRC, like a person's audio, as stereo Opus. `MediaSinkWants.music_muted` stops it being sent to this client; the deafened get none either. `docs/music.md` explains where it comes from.

**Resume.** Messages with a `seq` (`ClientConnect`, `ClientDisconnect`, `Speaking`, `MusicStream`) are kept. A client whose connection dropped connects again within 30 seconds and sends `Resume { voice_session_id, resume_token, last_seq }` instead of `Identify`; the node replays what it missed and media goes on meanwhile. After 30 seconds the voice session ends, and the main server ends the voice state.

The node allows 50 messages per 10 seconds per connection. Close codes:

| Code | Meaning | Resumable |
|---|---|---|
| 4001 | Invalid frame or message | Yes |
| 4003 | Authentication failed (bad, used or expired token) | No |
| 4004 | Handshake timeout | No |
| 4005 | Heartbeat timeout | Yes |
| 4006 | Voice session unknown or expired | No |
| 4008 | Rate limited | Yes |
| 4009 | The same user connected again | No |
| 4014 | Disconnected by the main server (left, moved or kicked from voice) | No |
| 4015 | The node is shutting down, or lost the main server; wait for a new `VoiceServerUpdate` | No |

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

Requests need `Authorization: Bearer <media_token>`: the encoded `MediaTokenClaims` plus the signature over `"opencord-media-token-v1" || claims`, base64url without padding, valid for 24 hours. `RefreshMediaToken` gives a new one. Without a valid token the answer is 401; for someone no longer a member, 403.

- `GET /media/sounds/{sha256}` returns a sound's Ogg Opus file (`data/sounds/<sha256>.opus`, lowercase hex), cacheable forever: files are named by their hash and never change. 400 for a malformed hash, 404 for an unknown sound.
- `POST /media/sounds?name=…&emoji=…&volume=…`, with the Ogg Opus file as the body, adds a sound to this server's soundboard. It needs base `MANAGE_SOUNDBOARD` and room under `max_sounds`; the answer is 201 with the sound.
- `POST /media/external-sounds`, with the file as the body, keeps another server's sound here so this server's listeners can fetch it. It needs `allow_external_sounds`; 10 per minute per user; the answer is 200 with an ack.

- `GET /media/music/artwork/{artwork_id}?size=128|512` returns a track's artwork as JPEG (Phase 3 plan §15.4), cacheable for 7 days. `artwork_id` comes from a `MusicTrack` or `MusicNowPlaying`; it stands for an address Lavalink gave in a result, which the server fetches (https, public hosts, at most 5 MB in 5 s), resizes and encodes again without metadata. 404 for an id the server didn't hand out or another size; 502 when the picture can't be fetched (clients show their placeholder).

Both uploads answer with a protobuf `Response` (`application/x-protobuf`), the same envelope as gateway requests: the result, or an `error` with its code and a matching status (400 `SOUND_TOO_LONG`, `SOUND_INVALID` or `INVALID_ARGUMENT`; 403 `FORBIDDEN`; 409 `SOUNDBOARD_FULL`; 429 `RATE_LIMITED`). The body is read up to 256 KB and no further, then parsed: the server trusts nothing the client says about the file.

`GET /info` adds `voice_enabled` and `voice_udp_port` (the embedded node's media port; null without one).

### Soundboard

Sounds play on each listener's device; the server only checks and announces them (Phase 2 plan §11).

- **Files** are Ogg Opus, 48 kHz, one or two channels, at most 256 KB and 5 seconds (20 ms of tolerance). The server reads the whole file: every page checksum, one logical stream, the `OpusHead` header, and the length of every packet. A file whose header claims less time than its packets play is too long.
- **Sound references:** `server:<sha256>` (this server's sound), `external:<sha256>` (another server's, uploaded here first) and `default:<id>` (the sounds every client has: `chime`, `tada`, `air-horn`, `rimshot`, `applause`, `sad-trombone`, `boing`, `crickets`).
- **Names** are 2–32 characters. An **emoji** is one emoji sequence of at most 8 Unicode scalar values. **Volume** is 0–100 %.
- **`PlaySoundboardSound`** is checked in this order: the soundboard is enabled; the caller has a voice state in that channel (`VOICE_NOT_CONNECTED`); they are not deafened, server-muted or suppressed (`FORBIDDEN`; self-muted people may play); `USE_SOUNDBOARD` in the channel; the sound exists (`NOT_FOUND`). An external sound also needs `allow_external_sounds` and `USE_EXTERNAL_SOUNDS` in the channel, and a default sound needs `allow_default_sounds`. Then a per-user cooldown (`sound_cooldown_s`) and at most three sounds playing at once in a channel; both answer `SOUND_COOLDOWN` with `retry_after_ms`. An external sound the server does not have answers `NOT_FOUND`: the client uploads it to `/media/external-sounds` and asks again. Its `name`, `emoji` and `volume` come with the request, since this server cannot know them; they are checked like this server's own and ignored for other sounds.
- **`VoiceChannelEffect`** carries the sound reference, its name, emoji and volume, when it started and how long it plays (`duration_ms`).
- **External sounds** are kept by hash, at most 50 MB, the least recently played going first. A file this server's own sounds use is never removed.

### Voice nodes

A server with `[[voice.external_nodes]]` in `opencord.toml` serves a control channel at `/internal/voice` on its own port; nodes connect to it (`proto/opencord/v1/internal.proto`, package `opencord.internal.v1`, one `ControlEnvelope` per binary frame).

1. The server sends `Challenge { nonce }`, 32 random bytes.
2. Within 10 seconds the node sends `Register { proof, endpoint, certificate_fingerprint, public_address, udp_port }`. `proof` is HMAC-SHA256 of the nonce, keyed with the shared secret from the node's `secret_file` (surrounding whitespace removed, at least 16 bytes). `endpoint` must match a configured node exactly.
3. The server answers `Registered { node_id, voice_signing_public_key, settings }`, or `Error` and close 4003.

Then the server sends `ParticipantUpdate` (flags, moderation and permissions), `DisconnectParticipant` and `ChannelUpdate` (limits) for the node's channels. The node sends `LoadReport` every 5 seconds, with a WebSocket ping, and `ParticipantDisconnected` when someone's voice connection did not come back within the resume window; reports about channels the node does not serve are ignored. Either side treats 15 seconds without hearing from the other as a lost channel.

When the channel closes, the server moves the node's channels elsewhere (see [Joining a voice node](#joining-a-voice-node)). The node ends every call on it (4015) and accepts nobody until it has registered again. A node that registers again replaces its old registration and starts without channels.

### Domains and network

Phase 3 (`network.proto`). The owner's settings page asks for `NetworkInfo` with `WatchNetwork { watch: true }` and gets `NetworkStatus` events with the whole of it after each change, until it sends `watch: false` or the session ends. A new session starts unwatched.

- **Public address.** The server asks `[network] ip_lookup_v4` and `ip_lookup_v6` in `opencord.toml` (ipify by default; IPv6 only when the machine has a global IPv6 address) at start-up and every 6 hours while `auto_detect` is on. An override replaces what was detected; an empty one is none. Overrides must be addresses of their family that the internet can reach (not loopback, unspecified or multicast). When the address in use changes, the owner gets a `ServerNotice` with the key `network.public_address`.
- **Ports.** Each one's state: `LISTENING`, `OFF` (its feature is off), `FREE`, `IN_USE` by another program, or `NOT_PERMITTED` (below 1024 without the capability). With UPnP on, the server maps the main port and the voice and music UDP ports on the router for an hour and renews the mappings every half hour; it removes them when UPnP is turned off and on shutdown. Each port reports `MAPPED` or `FAILED` with the router's reason.
- **Main port.** `main_port` takes effect at the next start. A port that cannot be had then falls back to the one in `opencord.toml`, so the owner is never locked out.
- **Local listener.** Plain HTTP on `127.0.0.1` only, serving the same routes as the main port, for tunnels and reverse proxies on the machine. It runs while a tunnel or reverse-proxy domain exists. Behind it, and behind any address in `trusted_proxies`, the client's address is the last `X-Forwarded-For` entry that is not itself a trusted proxy.
- **Domains.** Names are trimmed, lowercased and turned into punycode; IP addresses, single labels and local names (`localhost`, `.local`, `.lan`, ...) are refused (`DOMAIN_INVALID`); a name added twice is `DOMAIN_EXISTS`, a 21st `DOMAIN_LIMIT`. Own domains and free subdomains are checked with DNS: the zone's name servers, found through `dns_resolvers`, are asked directly without recursion, CNAMEs followed (8 at most); one public resolver is asked too, and a different answer is noted as a stale cache. Tunnel and reverse-proxy domains are checked over HTTPS: `https://<domain>/.well-known/opencord/verify` must answer the server's own token. A domain passes when its records include the public address and nothing else. The first domain to pass becomes active and the primary. Only domains that passed once can be active (`DOMAIN_NOT_VERIFIED`); the primary moves before it is turned off or deleted (`CONFLICT`). A domain that fails after passing is marked `DNS_PROBLEM` and stays active; the owner gets a `ServerNotice` keyed `domain.<id>.dns`. Domains are checked at start-up, every 12 hours (plus up to 30 minutes), and when the public address changes; each keeps its last 10 checks.
- **Addresses.** `Ready.server_addresses` and `ServerAddressesUpdate` give the primary, every active domain (the primary first), and the public IPv4 address, each with `trusted_certificate` (checked by a certificate authority rather than the pinned fingerprint) and its `domain_id`. Clients keep them and connect to the domains first, then the address they added the server with, then the IP address. A server reached at another address is the same server when `Hello.server_id` matches.
- **Identify through a domain.** A client that verified a domain by a certificate authority signs the challenge twice: with the certificate it saw, and with `host_binding(host)` in its place (`auth.rs`), sending `host` and `host_signature`. Behind a tunnel or proxy the certificate is the proxy's, which the server never holds; it accepts the second signature for a name it answers to: its configured `public_host`, an active domain, or one turned off less than 7 days ago.
- **Certificates.** Each `Domain` carries a `CertificateStatus`: its state (`NONE`, `REQUESTING` with the `step`, `VALID`, `RENEWAL_FAILED`, `FAILED`, `PROVIDED` by a tunnel or proxy, `EXPIRED`), issuer, expiry, whether it is a test or uploaded one, the last `error` in plain words, and `retry_at_ms` while failures must wait. The server picks the challenge itself: DNS-01 through a connected provider that holds the name, else TLS-ALPN-01 when the main port is the `[acme] tls_alpn_port` (443), else HTTP-01 on `[acme] http_port` (80), opened only while Let's Encrypt checks. The certificate is served by SNI as soon as it is issued; connections by IP or other names keep the self-signed one. A domain's certificate counts as `trusted_certificate` while it is valid, not a test one, and the domain is active or was turned off less than 7 days ago. Renewal is checked hourly and runs when a third of the lifetime is left, or earlier when the CA's renewal information says so; the old certificate is served until the new one is in. The owner gets a `ServerNotice` keyed `certificate.<id>` 7 days before a Let's Encrypt certificate expires and 14 days before an uploaded one does. `NetworkInfo.acme` says whether an account exists with the CA in use (Let's Encrypt, or its staging environment with `acme_staging`), with its email and the agreement's address; `dns_providers` lists the connected providers with only the token's last four characters. Private keys, the ACME account and provider tokens are sealed with `secrets.key` in the data directory and never sent or logged.
- **Free subdomains.** `AddFreeSubdomain` takes a DuckDNS or deSEC name (`name`, or the full `name.duckdns.org` / `name.dedyn.io`); a token given with it connects the provider once the provider accepted the update with it. The server points the name at the public addresses in use, then checks it every 5 seconds for a minute and every 30 seconds after, for 10 minutes at most, until it passes; once verified, it requests the certificate by itself (DNS-01 through the provider). When the public address changes, and before each scheduled check, every free subdomain is pointed at it again.
- **Tunnels.** `NetworkInfo.tunnel` (`TunnelStatus`) gives the state (`OFF`, `STARTING`, `RUNNING` once cloudflared registered a connection, `RESTARTING` after it stopped, `FAILED` when it cannot run), the kind, the hostname and its `domain_id`, the last error, the restarts, the cloudflared version found (empty: none) and `installing`. A quick tunnel's `*.trycloudflare.com` address becomes a `TUNNEL` domain, replacing the previous quick tunnel's; a named tunnel adds its hostname as one when it is not there. Both are checked like other tunnel domains, repeatedly while Cloudflare's DNS catches up. cloudflared runs with `HOME` in the data directory and the named tunnel's token in its environment (not its arguments); it is started again after 1, 2, 4… up to 60 seconds, and the tunnel starts again with the server. The token is stored sealed with `secrets.key`.
- **Invites.** `CreateInvite.domain_id` makes the invite for that active domain; without it, for the primary when there is one. `Invite.address` is its domain. An invite whose domain is turned off or deleted stops working. `GET /invite/<code>` is a small page (no JavaScript) with the server's name, member count, an `opencord://` link for the address the page was opened on (with the fingerprint unless an authority vouches for that domain) and a download link; a dead invite answers 404.

- **Lavalink nodes.** `FetchLavalinkNodes`, `CreateLavalinkNode`, `UpdateLavalinkNode` (settings, password and `draining`, each optional), `DeleteLavalinkNode` and `TestLavalinkNode` need Manage server and answer `LavalinkNodes`: the nodes, the bridge checklist (`LavalinkBridge`), the last 50 node events and the channels playing. Passwords are never returned; at most 20 nodes. With `FetchLavalinkNodes.watch` the session gets `LavalinkNodeStatus` (a node changed, or `removed`) and `LavalinkNodeEvent` until it asks with `watch` false. `TestLavalinkNode` answers `Ack`; its three steps (`REST`, `WEBSOCKET`, `BRIDGE`, each `WAITING`, `RUNNING`, `PASSED`, `FAILED` or `SKIPPED` with a detail in plain words) arrive as `LavalinkTestProgress` to that session only, the last with `done`. When every enabled node is unavailable, members with Manage server get a `ServerNotice` keyed `music.nodes`. `MoveLavalinkPlayers { from_node_id, to_node_id? }` (Manage server) moves a node's channels now and answers `LavalinkNodes`. `LavalinkBridge` also carries the bridge's counters (`LavalinkBridgeStats`: connections, packets, decrypt failures, dropped, jitter) and `node_bridges`, the external voice nodes running a bridge.
- **Music.** Every music request names a voice channel the member can see. `MusicSubscribe { channel_id, subscribe }` answers `MusicSession` and starts (or stops) `MusicSnapshot` events for that session; members connected to the channel get them anyway. A `MusicSession` has a `version` that grows with every change, the state (`IDLE`, `LOADING`, `PLAYING`, `PAUSED`), what plays, the queue, the last 50 played, the position at a server time, repeat and shuffle, and whether it waits for a node or for listeners. `MusicSearch { channel_id, query, source }` (Add music) answers `MusicSearchResults`: up to 15 results, or a playlist's, each with an opaque `result_id` good for 15 minutes and only for the member who searched. `MusicEnqueue { channel_id, result_ids, before_item_id?, placement }` (Add music; a position, `NEXT` or `NOW` need Manage the queue), `MusicPlayback { channel_id, action, position_ms?, expected_item_id? }` (`PAUSE`, `RESUME`, `NEXT`, `PREVIOUS`, `SEEK` need Control playback, `NEXT` on one's own track only Add music, `STOP` Manage music), `MusicSetMode`, `MusicQueueMove { item_id, before_item_id? }` and `MusicQueueClear` (Manage the queue) and `MusicQueueRemove` (one's own with Add music, others' with Manage the queue) answer the `MusicSession` after the change. With `expected_item_id`, `NEXT`, `PREVIOUS` and `SEEK` fail with `MUSIC_STALE` when another item plays. Controlling music needs being connected to the channel unless the member holds Manage music. Rate limits: 10 searches, 10 enqueues and 20 other changes per 10 seconds. `MusicNotice` lines go to a channel's subscribers, `MusicNowPlaying` to everyone who can see the channel. `FetchMusicSettings` (anyone) and `UpdateMusicSettings` (Manage music; fields left out stay) answer `MusicSettings`, which also carries the sources a healthy node offers now; changes reach everyone as `MusicSettingsUpdate`. `FetchMusicBlocks` and `MusicBlockMember { user_id, blocked, reason }` (Manage music; blocking needs a higher role) answer `MusicBlocks`. Errors: `MUSIC_NOT_CONFIGURED`, `MUSIC_NODE_UNAVAILABLE`, `MUSIC_NOT_IN_CHANNEL`, `MUSIC_BLOCKED`, `MUSIC_QUEUE_FULL`, `MUSIC_MEMBER_LIMIT`, `MUSIC_TRACK_TOO_LONG`, `MUSIC_STREAMS_NOT_ALLOWED`, `MUSIC_SOURCE_NOT_ALLOWED`, `MUSIC_LINK_NOT_ALLOWED`, `MUSIC_STALE`. Encoded tracks and Lavalink identifiers never reach clients.

### Voice limits

| Item | Limit |
|---|---|
| `UpdateVoiceState` and `RefreshVoiceServer` | 10 per 10 s per user |
| Voice token | 60 seconds, single use |
| Media token | 24 hours |
| Voice grace period | 30 seconds (up to 35) |
| Voice gateway messages | 50 per 10 s per connection |
| Voice gateway resume window | 30 seconds |
| Voice node registrations | 10 per minute per IP address |
| Voice node shared secret | at least 16 bytes |
| `CreateStream` | 5 per minute per user |
| Sound file | 256 KB, 5 seconds |
| Soundboard sounds | `max_sounds`, 0–200 (48 by default) |
| `/media/external-sounds` | 10 per minute per user |
| Sounds playing at once | 3 per voice channel |

**Music on external voice nodes** (Phase 3). A node whose `Register` has `music` runs an audio bridge at its endpoint. For a channel it serves, the main server sends `ExpectMusicBridge { channel_id, user_id, token, session_id, strict, lavalink_addresses }` before pointing Lavalink at it, and `CancelMusicBridge` when the music stops. The node answers with `MusicBridgeStatus { channel_id, kind, connection, code, address }` (`CONNECTED`, `CLOSED`, `DISCOVERED`, `REFUSED`, `AUDIO`, `SILENT`), which the main server takes only for channels that node serves.
