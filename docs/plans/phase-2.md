# Opencord — Phase 2 Plan: Voice, Video, Screen Share & Soundboard

> Builds on `opencord-phase1-plan.md` (server, protocol, permissions) and `opencord-desktop-ui-plan.md` (§4.10 voice view, §17 voice controls). End-to-end encryption is **not** built in this phase, but its structure is (§13); background in `opencord-encryption-and-devices-notes.md`.
>
> **Goal:** Discord-quality voice, camera and screen share, using Discord's voice architecture, a Rust media engine in the client for performance, and Discord's soundboard.

---

## 0. Instructions for Claude Code

- Start after Phase 1 is complete (server, Rust core, desktop UI running on real data).
- Work through milestones **V0 → V10** (§16) in order. Each has acceptance criteria; keep the build green after each one.
- After every milestone run the Phase 1 checks (`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `flutter analyze`, `flutter test`) plus that milestone's manual platform checklist (§15).
- **Read the official platform documentation (§17) before writing each capture, audio or encoder backend.** Don't write these from memory: exact API behavior and minimum OS versions matter here.
- Use the latest stable crate versions. Several crates named in this plan are young; check maintenance and quality before depending on them, and record each choice in `docs/decisions.md`.
- Performance rules for all media code:
  - Real-time audio callbacks never allocate, lock, log or block. They only move samples through lock-free ring buffers.
  - Keep video on the GPU from capture to encoder wherever the platform allows (zero-copy). Use CPU copies only as a fallback, and measure them.
  - Encode with hardware encoders; software encoding is the fallback.
  - Decode and render only what is visible, at the size it is shown.
- Terminal commands in docs must not contain inline comments.
- **Do not implement end-to-end encryption.** Build the hooks described in §13.

---

## 1. Scope

**In this phase**
- Voice channels: join/leave, speaking indicators, self mute/deafen, server mute/deafen, move, disconnect, priority speaker, user limit, bitrate.
- Client audio: device selection, push-to-talk and voice activity, noise suppression (in Rust), echo cancellation, gain control, per-user volume and local mute, mic test, global hotkeys.
- Camera video with automatic quality.
- Screen share: entire screen or a single window, quality chosen by the user and capped by the server, audio sharing on by default, opt-in watching.
- Soundboard.
- Basic Discord voice features that were missing (§12): AFK channel, text chat in voice channels, stream viewer list, connection diagnostics, join/leave sounds, auto-reconnect.
- Every control in the desktop UI plan §17 wired to the real engine.

**Later (keep extension points)**
End-to-end encryption, stage channels, stream preview thumbnails, pop-out windows, camera background blur, AV1, TCP/TURN fallback for networks that block UDP, recording, mobile, browser client.

---

## 2. Changes to earlier plans

| Area | Before | Now | Why |
|---|---|---|---|
| Client media | `flutter_webrtc` (Phase 1 plan §2, §13) | Rust media engine `opencord-media` (§7): `str0m` + native capture APIs + FFmpeg codecs | Noise suppression must run in Rust inside the capture path; screen share needs native APIs (window audio, excluding Opencord's own sound); one pipeline we control on every platform |
| Camera quality | Resolution and frame-rate options (UI plan §17.5) | Automatic, up to 720p 30 fps (§8) | Same as other call apps; no setting needed |
| Voice channel "video quality" (UI plan §17.4) | Auto / 720p / 1080p | Removed | Camera is automatic; screen share is capped server-wide |
| Screen share quality (UI plan §4.10, §17.5) | Fixed preset list | User picks resolution and frame rate, capped by the server's maximum, default **720p 30 fps** (§9.2) | Server owners control their bandwidth |
| "Push-to-talk required" (UI plan §17.4) | Channel switch | Shortcut that denies `USE_VOICE_ACTIVITY` for `@everyone` in that channel (§5.1) | Same model as Discord, works per role |
| Soundboard (UI plan §17.3) | Hidden placeholder | Real feature (§11) | |
| Permissions (Phase 1 plan §6.1) | Bits 16–23 reserved | Bits 16–23 active, new bits 24–27 (§5.1) | |

The desktop UI plan has been updated to match these changes.

---

## 3. Architecture (Discord-style)

### 3.1 Overview

```
Opencord client
  Flutter UI
     │ flutter_rust_bridge
  opencord-core  ──────── main gateway   wss://host:7710/gateway   (text, voice state, soundboard events)
     │
  opencord-media ──────── voice gateway  wss://<voice node>/voice  (voice signalling)
     │           ════════ media          udp://<voice node>:7711   (ICE + DTLS-SRTP, RTP/RTCP, one port)
     │
  OS APIs: audio devices, camera, screen and audio capture, hardware encoders and decoders

opencord-server
  ├─ main gateway, database, permissions, soundboard storage, media HTTP endpoints
  └─ voice node (embedded by default) ⇄ internal control channel
        ├─ voice gateway (WebSocket)
        └─ SFU (str0m, RTP mode): forwards packets, never decodes media

opencord-voice-node (optional): the same voice node as a separate binary on another machine,
registered with the main server over the internal control channel
```

### 3.2 Mapping to Discord's voice design

| Discord | Opencord |
|---|---|
| Gateway op 4 *Voice State Update* | `UpdateVoiceState` request on the main gateway |
| `VOICE_STATE_UPDATE` dispatch | `VoiceStateUpdate` event to everyone who can see the channel |
| `VOICE_SERVER_UPDATE` (endpoint + token) | `VoiceServerUpdate` event to the joining session |
| Voice gateway Hello, Identify, Ready, Heartbeat/ACK, Resume/Resumed | Same messages in `voice.proto` |
| Select Protocol / Session Description, UDP IP discovery, transport encryption | Transport exchange with ICE + DTLS-SRTP handled by `str0m` |
| Speaking (microphone / soundshare / priority flags) | `Speaking` with the same three flags |
| Clients Connect / Client Disconnect | `ClientConnect` / `ClientDisconnect` |
| Video op, Media Sink Wants | `PublishTrack` / `MediaSinkWants` |
| Go Live: stream create / watch / delete with stream keys | `StreamCreate` / `StreamWatch` / `StreamDelete`, stream key `stream:<channel_id>:<user_id>` |
| `VOICE_CHANNEL_EFFECT_SEND` (soundboard) | `VoiceChannelEffect` |
| DAVE end-to-end encryption opcodes | Reserved message range and `max_e2ee_version = 0` (§13) |

**One deliberate difference:** Discord opens a separate media connection for every stream. Opencord carries voice, camera and screen share over **one** media connection per user, so a single congestion controller can always keep voice first. Stream keys still exist, so a stream can be moved to its own node later without protocol changes.

### 3.3 Join flow

```
Client                                Main gateway                          Voice node
  |-- UpdateVoiceState{ch, mute, deaf} ->|                                      |
  |                                      | check CONNECT, user limit, bans      |
  |                                      | pick node, sign voice token -------->| expect user
  |<- VoiceStateUpdate (to all who can see ch)                                  |
  |<- VoiceServerUpdate{endpoint, cert fingerprint, token}                      |
  |------------------------------------- VoiceIdentify{token, caps} ---------->|
  |<------------------------------------ VoiceReady{ssrc, ICE/DTLS, participants, limits}
  |==================== ICE + DTLS-SRTP on UDP 7711 ==========================|
  |------------------------------------- Speaking / PublishTrack / MediaSinkWants
  |-- UpdateVoiceState{ch: null} ------->|---------- control: disconnect ------>|
```

### 3.4 Voice nodes
- **Embedded (default):** the voice node runs inside `opencord-server`. The voice gateway shares the main TLS listener at `/voice`; media uses UDP 7711. No extra setup for small communities.
- **External nodes (optional):** the `opencord-voice-node` binary is configured with the main server's address and a shared secret. It connects to the main server over an authenticated WebSocket (internal control channel, `proto/opencord/v1/internal.proto`), registers its public endpoint, reports load every 5 s and receives commands.
- **Node selection:** all participants of a voice channel are on the same node. When a channel's first participant joins, pick the least-loaded healthy node and keep it until the channel is empty. If a node dies, the main server reassigns its channels and sends new `VoiceServerUpdate`s; clients reconnect automatically.
- **TLS trust:** `VoiceServerUpdate` carries the voice node's TLS certificate fingerprint. It arrives over the already-pinned main gateway connection, so the client pins it for the voice connection (TOFU stays intact for external nodes with self-signed certificates).
- **Voice token:** signed by the main server with an Ed25519 voice-signing key (generated on first start; nodes get the public key). It contains user ID, channel ID, session ID, resolved channel permissions, limits and a 60 s expiry, and is single-use. Nodes validate it without a database.
- **Live updates:** permission changes, server mute/deafen, moves, disconnects and setting changes are pushed from the main server to the node over the control channel and applied immediately.
- **Public address:** for the embedded node, `public_address = "auto"` (the default) tells clients to send media to the same host they used to reach the main server, on the UDP port. No third-party STUN server is contacted. External nodes register their own public address. An explicit address can be configured for unusual setups.

### 3.5 Session rules
- A client is in **at most one voice channel at a time across all servers** (like Discord). Joining another channel leaves the current one; if the user is streaming, ask first.
- One voice state per user per server. Joining from a second session moves the voice state to the new session.
- If the main gateway connection drops, the voice state is kept for a **30 s grace period**, so a quick resume doesn't drop the call.
- **Move:** the main server sends a new `VoiceServerUpdate` and the client switches channels without user action.
- Voice states and streams live in server memory, not in the database. After a server restart, clients rejoin their last channel automatically if they're still allowed to.

---

## 4. Protocol additions

### 4.1 Main gateway requests

| Request | Notes |
|---|---|
| `UpdateVoiceState { channel_id?, self_mute, self_deaf, self_video, self_stream }` | `channel_id = null` leaves. Rate limit 10 / 10 s |
| `ServerMuteMember` / `ServerDeafenMember { user_id, value }` | `MUTE_MEMBERS` / `DEAFEN_MEMBERS` |
| `MoveMember { user_id, channel_id }`, `DisconnectMember { user_id }` | `MOVE_MEMBERS` |
| `StreamCreate { channel_id, source_kind, resolution, fps, has_audio }` | Checks `SCREENSHARE` and the server maximum |
| `StreamUpdate` / `StreamDelete { stream_key }` | Change quality or audio, stop |
| `StreamWatch` / `StreamUnwatch { stream_key }` | Viewer opt-in, viewer limit |
| `PlaySoundboardSound { channel_id, sound_ref }` | §11.5 |
| `UpdateSoundboardSound`, `DeleteSoundboardSound` | `MANAGE_SOUNDBOARD`. New sounds are uploaded over HTTP (§4.4) |
| `UpdateVoiceSettings { ... }` | Server-wide voice and video settings (§5.2), `MANAGE_SERVER` |
| `RefreshMediaToken` | New token for the media HTTP endpoints |

### 4.2 Main gateway events
`VoiceStateUpdate`, `VoiceServerUpdate`, `StreamCreate`, `StreamUpdate`, `StreamDelete`, `StreamViewersUpdate` (to the streamer), `VoiceChannelEffect` (soundboard), `SoundboardSoundCreate` / `Update` / `Delete`, `VoiceSettingsUpdate`.

`Ready` gains: the voice states of every voice channel the user can see, active streams, the soundboard sound list (metadata only), the voice and video settings, and a `media_token`.

```proto
message VoiceState {
  uint64 user_id = 1;
  optional uint64 channel_id = 2;
  string session_id = 3;
  bool self_mute = 4;
  bool self_deaf = 5;
  bool server_mute = 6;
  bool server_deaf = 7;
  bool suppress = 8;
  bool self_video = 9;
  bool self_stream = 10;
}
```
`suppress` is set when the user lacks `SPEAK` in the channel or sits in the AFK channel.

### 4.3 Voice gateway (`proto/opencord/v1/voice.proto`)
Binary protobuf frames, same envelope style as the main gateway.

| Direction | Message | Purpose |
|---|---|---|
| S→C | `Hello { heartbeat_interval_ms }` | |
| C→S | `VoiceIdentify { token, user_id, session_id, channel_id, client_caps, max_e2ee_version = 0 }` | `client_caps`: codecs with hardware encode/decode flags, max decode resolution, simulcast support |
| S→C | `VoiceReady { voice_session_id, resume_token, audio_ssrc, ice_ufrag, ice_pwd, candidates, dtls_fingerprint, codecs, limits, participants }` | `limits`: screen share maximum, bitrate ceilings, camera participant cap |
| C→S | `TransportInfo { ice_ufrag, ice_pwd, dtls_fingerprint }` | Client side of str0m's direct API (no SDP) |
| both | `Heartbeat { nonce, client_ts }` / `HeartbeatAck { nonce }` | Also measures round-trip time |
| C→S, S→C | `Speaking { flags }` | `MICROPHONE = 1`, `SOUNDSHARE = 2`, `PRIORITY = 4`. Server validates `PRIORITY_SPEAKER` and relays |
| S→C | `ClientConnect { user_id, audio_ssrc, tracks }` / `ClientDisconnect { user_id }` | |
| C→S | `PublishTrack { track_id, kind: camera \| screen \| screen_audio, codec, layers }` | `layers`: rid, width, height, fps, max bitrate |
| S→C | `TrackPublished { track_id, ssrcs }` / `TrackRejected { track_id, reason }` | |
| C→S | `UnpublishTrack { track_id }` | |
| S→C | `TrackUpdate { user_id, track_id, state, available_layers }` | Track added, removed, paused |
| C→S | `MediaSinkWants { wants: [{ track_id, max_height, max_fps, paused }] }` | Which remote video the client wants and at what size (Discord's message of the same name) |
| S→C | `SenderLayerWants { track_id, active_layers }` | Tells a sender which of its layers anyone needs, so unused layers aren't encoded |
| C→S | `Resume { voice_session_id, resume_token, last_seq }` → S→C `Resumed` | The voice token is single-use, so resuming uses the `resume_token` from `VoiceReady` |
| — | Field numbers **100–149 reserved** for end-to-end encryption | §13 |

### 4.4 Media HTTP endpoints
Authenticated with `Authorization: Bearer <media_token>` (signed by the server, 24 h lifetime). The Rust core uses the same pinned-certificate TLS configuration as the gateway.
- `GET /media/sounds/{sha256}`: a sound file (content-addressed, immutable, cacheable).
- `POST /media/sounds`: upload a new server sound with name, emoji and volume (`MANAGE_SOUNDBOARD`).
- `POST /media/external-sounds`: upload a sound from another server so it can be played here (§11.5).

These endpoints are the start of the media service that file attachments will use later.

### 4.5 New error codes
`VOICE_CHANNEL_FULL`, `VOICE_NOT_CONNECTED`, `QUALITY_LIMIT`, `CAMERA_LIMIT`, `STREAM_VIEWER_LIMIT`, `SOUND_COOLDOWN`, `SOUND_TOO_LONG`, `SOUND_INVALID`, `SOUNDBOARD_FULL`.

---

## 5. Permissions, settings and data

### 5.1 Permissions

| Bit | Name | Default for `@everyone` |
|---|---|---|
| 16 | `CONNECT` | on |
| 17 | `SPEAK` | on |
| 18 | `VIDEO` (camera) | on |
| 19 | `SCREENSHARE` | on |
| 20 | `MUTE_MEMBERS` | off |
| 21 | `DEAFEN_MEMBERS` | off |
| 22 | `MOVE_MEMBERS` (also bypasses user limits) | off |
| 23 | `PRIORITY_SPEAKER` | off |
| 24 | `USE_VOICE_ACTIVITY` (without it, push-to-talk is forced) | on |
| 25 | `USE_SOUNDBOARD` | on |
| 26 | `USE_EXTERNAL_SOUNDS` (sounds from other servers) | on |
| 27 | `MANAGE_SOUNDBOARD` | off |

- Everything is enforced by the server or voice node, except `USE_VOICE_ACTIVITY`, which only the client can enforce (the server can't tell push-to-talk audio from voice-activity audio). Discord works the same way.
- The client also checks permissions so it can hide or disable controls with a reason.
- The roles editor (UI plan §8.2) shows the new bits in its "Voice & video" group and a new "Soundboard" group.

### 5.2 Server settings → Voice & video (new page)

| Setting | Default | Range |
|---|---|---|
| Screen share max resolution | **720p** | 480p · 720p · 1080p · 1440p · Source |
| Screen share max frame rate | **30 fps** | 15 · 30 · 60 |
| Max viewers per stream | 50 | 1–200 |
| Camera allowed | on | |
| Max participants with camera on, per channel | 25 | 1–50 |
| Max voice bitrate | 96 kbps | 32–256 kbps |
| AFK channel | none | any voice channel |
| AFK timeout | 5 min | 1 · 5 · 15 · 30 · 60 min |

Soundboard settings have their own page (§11.7).

### 5.3 Voice channel settings

| Setting | Default |
|---|---|
| Bitrate | 64 kbps (8 kbps up to the server maximum) |
| User limit | 0 = unlimited (max 99) |
| Text chat in this channel | on |
| Push-to-talk required | off (shortcut for the `USE_VOICE_ACTIVITY` overwrite) |

### 5.4 Database (sqlx migrations)
- `server_meta`: keys for every setting in §5.2 and §11.7.
- `channels`: `bitrate INTEGER`, `user_limit INTEGER`, `text_in_voice INTEGER`.
- `soundboard_sounds (id, name, emoji, volume, sha256, size_bytes, duration_ms, channel_count, uploader_id, created_at)`.
- `external_sounds (sha256 PRIMARY KEY, size_bytes, duration_ms, uploaded_by, last_used_at)`.
- Sound files on disk: `data/sounds/<sha256>.opus` and `data/external-sounds/<sha256>.opus`.
- The voice-signing key lives in the data directory, not the database.

### 5.5 Server configuration (`opencord.toml`)

```toml
[voice]
enabled = true
mode = "embedded"
udp_port = 7711
public_address = "auto"
max_participants_per_channel = 99
max_egress_mbps = 0

[[voice.external_nodes]]
endpoint = "wss://voice1.example.com:7712"
secret_file = "voice1.secret"
```

| Key | Meaning |
|---|---|
| `mode` | `embedded`, or `external` to use only `voice.external_nodes` |
| `public_address` | Host or IP clients send media to for the embedded node. `auto` = the host clients used to reach this server (§3.4) |
| `max_egress_mbps` | Optional total upload cap for self-hosters on home connections; 0 = unlimited. When reached, the SFU lowers video layers first |

- Docker and compose: publish `7711/udp`. The README explains UDP port forwarding on home routers.
- `GET /info` adds voice availability and the media port.

---

## 6. Voice node and SFU (server)

- Built on **str0m in RTP mode**: packets are forwarded, never decoded. One UDP socket on port 7711 serves all clients, demultiplexed by ICE credentials and then by remote address. The server side uses ICE-lite if str0m supports it, otherwise full ICE. DTLS-SRTP for all media.
- **Audio forwarding**
  - Forward each participant's audio to every other participant.
  - Drop audio from users who are server-muted, suppressed (no `SPEAK`, AFK channel) or self-muted.
  - Send no audio to deafened users (saves their bandwidth).
  - Opus DTX keeps silent users near zero bandwidth.
  - Channels above 50 participants: forward only the 10 loudest speakers, using the RFC 6464 audio-level header extension.
- **Video forwarding**
  - Each published track has up to three simulcast layers.
  - For each receiver and track, pick the layer from what the receiver asked for (`MediaSinkWants`), what its downlink bandwidth estimate allows, and which layers the sender is producing.
  - Switch layers only on keyframes, and rewrite SSRC, sequence numbers and timestamps so the receiver sees one continuous stream.
  - Request a keyframe (PLI) from the sender when a receiver subscribes or switches layers; at most one request per second per track.
  - NACK/RTX retransmission for video; TWCC feedback to senders.
  - Send `SenderLayerWants` so senders stop encoding layers nobody needs.
- **Layer and keyframe information comes from RTP header extensions, never from codec payloads.** Use a small Opencord frame-marking extension (keyframe flag, layer, frame size) or the dependency descriptor. This keeps the SFU working when payloads are end-to-end encrypted later (§13).
- **Per-receiver priority:** voice → stream audio → watched screen share → focused or large camera tiles → other cameras.
- **Streams:** screen share video and audio are only sent to users who chose to watch (`StreamWatch`), up to the viewer limit. The streamer receives the viewer list.
- **Quality enforcement:** reject `PublishTrack` requests above the server's limits. Monitor each track's bitrate; above 1.25× its ceiling for more than 5 s, stop forwarding it and send `TrackRejected { QUALITY_LIMIT }`.
- **Moderation and permission changes** arrive over the control channel and take effect within 100 ms (for example, server mute drops packets immediately).
- **AFK tracking:** the node reports each user's last speaking time to the main server, which handles AFK moves (§12).
- **Metrics:** CPU, packets per second, bitrate in and out, participants per channel; reported over the control channel and logged.

---

## 7. Client media engine (`crates/opencord-media`, Rust)

### 7.1 Layout and threads

```
opencord-media/src/
  audio/        devices (per OS), resampler, processing chain, noise suppression, voice activity,
                Opus, jitter buffer, mixer, limiter, soundboard player
  video/        camera (per OS), screen capture (per OS), convert and scale, encoder, decoder, simulcast
  share_audio/  per-OS system and application audio capture for screen share
  transport/    str0m client, UDP socket, bandwidth estimation, voice gateway client
  render/       video frames to Flutter textures
  transform/    FrameTransform trait (identity now, end-to-end encryption later)
  hotkeys/      global push-to-talk and voice hotkeys
```

Threads:
- **Device callbacks** (OS real-time threads): only copy samples into or out of lock-free SPSC ring buffers (`rtrb`).
- **Audio processing thread:** 10 ms tick at raised priority (`audio_thread_priority`; MMCSS "Pro Audio" on Windows). Runs capture processing, encoding, decoding and mixing.
- **Network thread** (tokio): UDP socket, str0m, voice gateway WebSocket.
- **Video threads:** capture callback → one encoder thread per track; one decoder thread per visible remote track.
- Dart never touches audio or video data. It receives state events and texture IDs.

### 7.2 Audio send pipeline

```
microphone (any rate/format) → resample to 48 kHz mono f32 (rubato)
 → high-pass filter → echo cancellation (reference: everything Opencord plays, plus shared audio §10.6)
 → noise suppression (Off · Standard · High)
 → automatic gain control → input volume (0–200 %)
 → voice activity / push-to-talk gate (release delay, hangover, 20 ms pre-roll)
 → Opus encode (20 ms) → FrameTransform (identity) → str0m → UDP
```
Echo cancellation runs before noise suppression; gain control runs after it, so noise isn't amplified.

### 7.3 Noise suppression (Rust)

| Mode | Library | Notes |
|---|---|---|
| **Standard** | `nnnoiseless`: a pure-Rust port of RNNoise (BSD-3-Clause) | Very low CPU. 48 kHz, 480-sample (10 ms) frames. Expects `f32` samples scaled to the i16 range (±32768), not ±1.0. Also returns a voice probability, used for automatic sensitivity |
| **High** | DeepFilterNet's Rust library (`deep_filter` / libDF, MIT or Apache-2.0), the same code behind DeepFilterNet's real-time LADSPA/PipeWire plugin | Full-band 48 kHz neural suppression; much better on keyboards, dogs, fans and background talk. Uses more CPU and adds roughly 20–40 ms of delay depending on the model; measure both. Bundle the model with the app. Check the current crate features (inference backend, default model) and the model's licence |
| **Off** | — | |

- **Default:** on first run, benchmark High for 2 s. If it needs less than 20 % of the 10 ms frame budget, default to High; otherwise Standard.
- **Automatic fallback:** if High needs more than 60 % of the frame budget for 2 s (for example on battery), switch to Standard and show a toast. It switches back only when the user picks High again.
- Noise suppression only processes the user's own microphone, not incoming audio.
- When Standard or High is on, disable the echo-cancellation library's own noise suppressor so audio isn't processed twice.

### 7.4 Echo cancellation, gain control, voice activity
- **Echo cancellation and gain control:** WebRTC's AudioProcessing module (AEC3, AGC2, high-pass filter) through the `webrtc-audio-processing` crate (BSD-3-Clause, `bundled` feature; needs meson, ninja and clang). Its docs don't mention Windows, so verify the Windows (MSVC) build at the very start of V3. Evaluate the young pure-Rust `aec3` crate as an alternative with the same test recordings. Pick by measured quality and record the decision.
- The echo canceller's reference signal is the final mix sent to the output device (voices, stream audio, soundboard, notification sounds), plus shared screen audio (§10.6).
- Setting "Use the operating system's processing when available" (UI plan §17.5): macOS Voice Processing I/O audio unit, Windows communications audio effects where the device offers them. Off by default.
- **Voice activity:** automatic sensitivity uses the RNNoise voice probability with a 250 ms hangover. Manual sensitivity is a dBFS threshold drawn over the live level meter. Keep a 20 ms pre-roll so the start of words isn't clipped.
- **Push-to-talk:** handled entirely in Rust (§7.13), so the mic gate opens without a round trip through Dart. Release delay 0–2000 ms, default 200 ms.

### 7.5 Audio receive pipeline

```
UDP → str0m → per-user jitter buffer → FrameTransform (identity) → Opus decode (FEC, then PLC on loss)
 → per-user volume (0–200 %) and local mute → priority-speaker ducking
 + stream audio (own volume and mute) + soundboard sounds + notification sounds
 → master output volume → soft limiter → resample to the device rate → output device
                                                  └→ echo-canceller reference
```
- **Jitter buffer** (our own; str0m has no adaptive jitter buffer): target delay = 95th-percentile jitter + 10 ms, clamped to 20–200 ms. Adjust the delay during silence (DTX gaps) by dropping or repeating 10 ms frames. Time-stretching is a later improvement.
- **Loss handling:** Opus in-band FEC first, then Opus packet-loss concealment.
- **Deafen:** output is silenced and the voice node stops sending audio. Undeafening restores the previous mute state (UI plan §17.1).
- **Priority speaker:** while a priority speaker talks, everyone else is lowered to 25 % (−12 dB).
- **Speaking indicators:** from the decoded audio level, on immediately and off after 250 ms of silence. Sent to the UI at most every 50 ms, as a diff of changed users.

### 7.6 Audio devices

| Platform | API | Device changes |
|---|---|---|
| Windows | WASAPI shared mode, event-driven (`cpal` or the `windows` crate) | `IMMNotificationClient` |
| macOS | Core Audio (`cpal` or `objc2-core-audio`) | `AudioObjectAddPropertyListener` |
| Linux | PipeWire native (`pipewire` crate); ALSA through `cpal` as fallback | PipeWire registry events; names from node properties |

- `cpal` has no device-change notifications; use the native mechanism, or poll every 2 s as a fallback.
- If the selected device disappears, switch to the default device and show a toast (UI plan §17.2).

### 7.7 Opus settings

| Stream | Settings |
|---|---|
| Voice | 48 kHz mono, 20 ms frames, VoIP mode, complexity 10, VBR, in-band FEC on (loss estimate from receiver reports), DTX on, bitrate from the channel setting (default 64 kbps) |
| Screen share audio | 48 kHz stereo, 20 ms, music mode, 128 kbps, FEC on |
| Soundboard files | Ogg Opus, 48 kHz, mono or stereo, 96 kbps |

Use libopus through a Rust binding (`opus` or `audiopus`), built statically.

### 7.8 Video pipeline

```
send:    capture (camera / screen, GPU frames) → GPU convert and scale to NV12, per layer
         → hardware encoder per layer → FrameTransform (identity) → str0m (packetize, RTX, TWCC) → UDP
receive: UDP → str0m (depacketize) → FrameTransform (identity)
         → decoder (hardware for large layers, software for small) → Flutter texture at tile size
```

### 7.9 Codecs and hardware acceleration
- **H.264 for all video in this phase.** Every desktop GPU encodes and decodes it in hardware. Codec negotiation fields exist so AV1 can be added later.
- **Codec layer:** FFmpeg's libavcodec, built as LGPL (no GPL components), through `rsmpeg` or `ffmpeg-next` (pick the better-maintained one). One API for every hardware encoder and decoder, with GPU frame import.

| Platform | Hardware encode (zero-copy input) | Software fallback |
|---|---|---|
| Windows | NVENC, AMF or Quick Sync through FFmpeg with D3D11 textures | Media Foundation's built-in H.264 encoder, then OpenH264 |
| macOS | VideoToolbox (CVPixelBuffer/IOSurface input) | VideoToolbox software encoder |
| Linux | VA-API (Intel, AMD) with DMA-BUF import; NVENC (NVIDIA) | OpenH264 |

- **OpenH264:** download Cisco's prebuilt binary on first use (as Firefox does), so Cisco's H.264 patent licence covers it. Check codec licensing before distributing binaries and record it in `docs/decisions.md`.
- **Real-time encoder settings:** low-latency presets, no B-frames, rate control with about 1 s of buffer, infinite GOP with keyframes on request (at most one per second).
- **Encoder session limits:** consumer NVIDIA GPUs limit concurrent encoder sessions. Encode the top layer in hardware; if another hardware session can't be opened, encode small layers (≤ 360p) in software.
- **Decoding:** hardware decoders for layers above 360p; software decoding (FFmpeg) for small layers, so many small tiles don't exhaust hardware decoder sessions.

### 7.10 Bandwidth and priorities (sender)
- str0m's TWCC-based bandwidth estimator sets the total budget.
- Protection order: voice → stream audio → screen video → camera.
- When the budget drops: turn off the camera's top layer, then its middle layer, then the screen share's low layer, then lower the screen frame rate (down to 5 fps for "detail" content), then the screen resolution. Voice is never starved.
- Recover gradually when bandwidth returns, probing before re-enabling a layer.

### 7.11 Rendering in Flutter
- Video frames go from Rust straight into Flutter external textures. Dart only places `Texture(textureId: ...)` widgets.
- **First:** pixel-buffer textures on all platforms (`irondash_texture` supports pixel buffers everywhere), with YUV→RGBA conversion on the GPU where possible or with SIMD on the CPU (`yuvutils-rs` or libyuv).
- **Later (V10):** zero-copy GPU textures per platform: DXGI shared handles (Windows), CVPixelBuffer/IOSurface (macOS), OpenGL/EGL with DMA-BUF (Linux).
- Render at tile size. Hidden tiles pause their decoders and unsubscribe through `MediaSinkWants`.

### 7.12 Rust ↔ Dart API (sketch)

```rust
pub async fn voice_join(server_key: String, channel_id: i64) -> Result<()>;
pub async fn voice_leave() -> Result<()>;
pub fn voice_set_self_mute(muted: bool);
pub fn voice_set_self_deaf(deafened: bool);
pub fn voice_set_user_volume(user_key: String, volume: f32);
pub fn voice_set_user_local_mute(user_key: String, muted: bool);
pub fn voice_set_user_soundboard_mute(user_key: String, muted: bool);
pub fn voice_set_stream_volume(stream_key: String, volume: f32, muted: bool);
pub fn audio_devices() -> AudioDevices;
pub fn audio_apply_settings(settings: AudioSettings);
pub fn audio_mic_test(enabled: bool);
pub fn hotkeys_set(bindings: Vec<HotkeyBinding>) -> Result<HotkeySupport>;
pub async fn camera_start(device_id: Option<String>) -> Result<i64>;
pub async fn camera_stop() -> Result<()>;
pub async fn screen_sources() -> Result<ScreenSourceList>;
pub async fn screen_share_start(request: ScreenShareRequest) -> Result<ScreenShareInfo>;
pub async fn screen_share_stop() -> Result<()>;
pub async fn stream_watch(stream_key: String) -> Result<i64>;
pub async fn stream_unwatch(stream_key: String) -> Result<()>;
pub fn video_set_wants(wants: Vec<VideoWant>);
pub async fn soundboard_play(server_key: String, channel_id: i64, sound_ref: SoundRef) -> Result<()>;
pub fn soundboard_preview(sound_ref: SoundRef);
pub async fn soundboard_prepare_upload(path: String) -> Result<SoundDraft>;
pub async fn soundboard_upload(server_key: String, edit: SoundDraftEdit) -> Result<()>;
pub fn media_event_stream(sink: StreamSink<MediaEvent>);
```

`MediaEvent` covers: connection state, participants and voice states, speaking diffs, audio levels (20 Hz, only while a meter is visible), video track added/removed with its texture ID, streams and viewers, soundboard effects, device list changes, noise-suppression fallback, capture errors, and connection stats (1 Hz, only while diagnostics are open).

### 7.13 Global hotkeys
Implemented in Rust so push-to-talk opens the mic gate directly:
- **Windows:** `RegisterHotKey` for toggles; a low-level keyboard hook for push-to-talk press and release.
- **macOS:** `CGEventTap`; needs Input Monitoring or Accessibility permission, so guide the user to grant it.
- **Linux Wayland:** `org.freedesktop.portal.GlobalShortcuts` through `ashpd` (activated/deactivated signals give press and release). Check which portal backends implement it (Hyprland's and KDE's do).
- **Linux X11:** `XGrabKey` or XInput2 raw events.
- If no global method is available, `hotkeys_set` reports it and the settings page says hotkeys only work while Opencord is focused (UI plan §17.5).

### 7.14 Connection states and recovery
States shown in the voice panel (UI plan §4.2): *Awaiting endpoint → Authenticating → RTC connecting → Connected*, plus *Reconnecting*, *No route* ("UDP port 7711 may be blocked by your network or the server's firewall") and *Disconnected (reason)*.
- Voice gateway drop → `Resume`; media keeps flowing while ICE is alive.
- Network change (Wi-Fi switch, VPN, wake from sleep) → ICE restart, or a full re-identify if that fails. Target: back in under 3 s.
- Voice node failure → the main server sends a new `VoiceServerUpdate` and the client reconnects without user action.

---

## 8. Camera

**Quality is automatic**, the way Discord, Google Meet and Zoom handle cameras: capture up to **1280×720 at 30 fps** and send three simulcast layers. There are no camera quality settings.

| Layer | Resolution | Frame rate | Bitrate (H.264) |
|---|---|---|---|
| High | 1280×720 | 30 | ≈ 1.5–2.5 Mbps |
| Medium | 640×360 | 30 | ≈ 500 kbps |
| Low | 320×180 | 15 | ≈ 150 kbps |

- Under congestion the camera keeps its frame rate and lowers resolution (the opposite of screen share).
- Receivers get the layer that fits their tile: focused or large → High, grid → Medium, small strip → Low.
- The camera participant cap is a server setting (default 25). The next person sees "Camera limit reached in this channel".
- The local preview is mirrored; the sent video is not.
- "Always preview video before turning on camera" (UI plan §17.5) shows the local preview first.

| Platform | Capture API | Notes |
|---|---|---|
| Windows | Media Foundation source reader (`IMFSourceReader`), NV12 or MJPEG | Hardware MJPEG decoding where available |
| macOS | AVFoundation `AVCaptureSession` + `AVCaptureVideoDataOutput` (NV12 `CVPixelBuffer`) | `NSCameraUsageDescription`; Continuity Camera shows up as a device |
| Linux | Camera portal (`org.freedesktop.portal.Camera`) + PipeWire; fallback V4L2 (`v4l` crate) | MJPEG decoded with a SIMD JPEG decoder; YUYV converted to NV12 |

The `nokhwa` crate can be evaluated as a shortcut, as long as the zero-copy path stays possible.

---

## 9. Screen share

### 9.1 User flow
1. The user clicks **Screen** in the voice panel or the voice view's control bar.
2. **Share dialog** (Opencord's own, monochrome):
   - **Source** (Windows, and macOS 13): tabs **Screens** and **Windows** with thumbnails refreshed every 2 s (max 30 windows). Opencord's own windows aren't listed.
   - **Quality:** resolution (480p · 720p · 1080p · 1440p · Source) and frame rate (15 · 30 · 60) as segmented controls. Options above the server's maximum are disabled with "Server limit: 720p · 30 fps". The default is the server maximum; the last choice per server is remembered.
   - **Share audio:** switch, **on by default**. Its label follows the source: "Share computer sound (except Opencord)" for screens, "Share sound from <app>" for windows. When the platform can't do it, the switch is disabled and says why (§10.1).
   - **Go live** button.
3. On **Linux** and **macOS 14+**, after the user confirms quality and audio, the **system picker** opens to choose the screen or window (xdg-desktop-portal on Linux, `SCContentSharingPicker` on macOS). On macOS 15, apps that capture through their own picker trigger a recurring "bypass the system private window picker" permission prompt; the system picker avoids it.
4. While live: "LIVE" pill on the user's tile and sidebar row, viewer count with a viewer list popover, "Change source", "Change quality" and "Stop sharing".
5. Optional "Hide Opencord from my screen share": Windows uses `SetWindowDisplayAffinity` with `WDA_EXCLUDEFROMCAPTURE` while live; macOS excludes Opencord's windows from the content filter. Not possible on Linux.
6. On Linux, restore tokens (`persist_mode`) let "Share the same screen again" skip the system picker.

### 9.2 Quality presets and the server cap
- A resolution preset is a **maximum pixel count**, not a fixed size, so ultrawide and portrait screens are treated fairly: 480p = 854×480, 720p = 1280×720, 1080p = 1920×1080, 1440p = 2560×1440, Source = native.
- Output size = source size × `min(1, sqrt(budget / source_pixels))`, rounded down to even numbers. Never upscale. Windows that change size are rescaled on the fly.
- The frame rate is a maximum. Capture APIs deliver frames only when content changes; at least 1 fps is sent so the stream stays alive.
- **Starting bitrate ceilings (H.264; tune during V6):**

| Max resolution | 15 fps | 30 fps | 60 fps |
|---|---|---|---|
| 480p | 0.6 Mbps | 1.0 Mbps | 1.6 Mbps |
| 720p | 1.2 Mbps | 2.0 Mbps | 3.5 Mbps |
| 1080p | 2.5 Mbps | 4.0 Mbps | 6.5 Mbps |
| 1440p | 4.0 Mbps | 6.5 Mbps | 10 Mbps |
| Source (≤ 4K) | 6 Mbps | 10 Mbps | 16 Mbps |

- **Content mode:** 15 and 30 fps → "detail" (keep resolution, lower frame rate under congestion; good for text and code). 60 fps → "motion" (keep frame rate, lower resolution; good for games and video).
- **Low layer:** when the main layer is above 480p and the uplink allows it, also send a layer of at most 640×360 at 15 fps (≈ 300 kbps) for viewers with weak connections or small tiles.
- The server cap is enforced three times: the dialog doesn't offer higher options, `StreamCreate` and `PublishTrack` are rejected above it, and the voice node monitors bitrate (§6).

### 9.3 Video capture per platform

**Windows**
- **Windows.Graphics.Capture (WGC)** for screens and windows: `IGraphicsCaptureItemInterop::CreateForMonitor` / `CreateForWindow`, free-threaded `Direct3D11CaptureFramePool`. Frames arrive as D3D11 textures; convert and scale them with the D3D11 video processor and hand them to the encoder without copying to the CPU.
- Cursor via `IsCursorCaptureEnabled`. The yellow capture border can be turned off on Windows 11 (`IsBorderRequired`) when borderless access is granted; check the documented requirements. Keeping the border is acceptable.
- **DXGI Desktop Duplication** as the fallback for whole screens when WGC isn't available or fails.
- Window list: `EnumWindows`, skipping invisible, cloaked (`DWMWA_CLOAKED`), tool and untitled windows. Thumbnails with `PrintWindow(PW_RENDERFULLCONTENT)` for windows and a single duplicated frame for screens, so the picker doesn't start many WGC sessions (each one would flash the capture border).
- Bindings: the `windows` crate.

**macOS** (deployment target macOS 13)
- **ScreenCaptureKit:** `SCStream` with `SCStreamConfiguration`: output width/height (scaled on the GPU), `minimumFrameInterval` from the frame rate, NV12 `pixelFormat` for VideoToolbox, `showsCursor`, `queueDepth` 5.
- Content comes from `SCContentSharingPicker` on macOS 14+ (§9.1), or from `SCShareableContent` plus Opencord's own picker on macOS 13 (thumbnails with `CGWindowListCreateImage`).
- Frames are IOSurface-backed `CVPixelBuffer`s passed to VideoToolbox without copying.
- Permission: Screen Recording (`CGPreflightScreenCaptureAccess` / `CGRequestScreenCaptureAccess`). Check whether the system-picker path still needs it.
- Bindings: the `objc2` framework crates (`objc2-screen-capture-kit`, `objc2-core-media`, `objc2-core-video`, `objc2-video-toolbox`). Where blocks or delegates are awkward from Rust, write a small Swift or Objective-C shim in the macOS runner and call it over a C ABI.

**Linux**
- **xdg-desktop-portal ScreenCast** through `ashpd`: `CreateSession` → `SelectSources` (monitor + window types, embedded cursor, `persist_mode` 2 with a restore token) → `Start` (system picker; returns PipeWire node IDs) → `OpenPipeWireRemote` (file descriptor).
- Consume the stream with the `pipewire` crate. Negotiate **DMA-BUF** with modifiers first and import it into VA-API for zero-copy encoding (FFmpeg DRM PRIME → VA-API mapping). Fall back to shared-memory BGRx/RGBx frames.
- NVIDIA on Wayland: importing DMA-BUF into NVENC is complex. Copy to the CPU and upload (fine at 720p30) unless a direct path works.
- Window sharing depends on the portal backend: `xdg-desktop-portal-hyprland` (screens, windows and regions), GNOME and KDE (screens and windows), `xdg-desktop-portal-wlr` (screens; window support depends on the version). If the backend can't share windows, say so in the dialog.
- **X11 without a portal:** XShm for screens, XComposite + XShm for windows, window list from `_NET_CLIENT_LIST` (`x11rb`). Lower priority than the portal path.

### 9.4 Edge cases
- Minimized windows stop producing frames on Windows and macOS: keep sending the last frame at 1 fps and show "<app> is minimized" on the streamer's own preview.
- The shared window closes → stop the stream and show "The shared window was closed".
- Protected (DRM) video shows up black. That's expected.
- HDR displays: capture as 8-bit SDR and test on an HDR monitor for washed-out colours.
- Display changes (resolution change, monitor unplugged): restart capture on the same display, or stop if it's gone.
- Mirror effect when streamers look at their own stream: their own tile shows a small, low-frame-rate preview by default ("Show my stream preview" toggle).

### 9.5 Watching
- Streams are **opt-in**. A stream tile shows the streamer's name, "LIVE" and a **Watch** button; nothing is downloaded until the user clicks it. Clicking a "LIVE" pill in the sidebar joins the channel if needed and starts watching.
- Watching opens the stream in focus mode (UI plan §4.10) with fullscreen, stream volume and "Stop watching".
- When the viewer limit is reached: "This stream is full (50 viewers)".

---

## 10. Screen share audio

This is the hardest part of the phase. Each platform needs a different API, and some combinations need fallbacks.

### 10.1 Requirements and platform matrix
- Audio sharing is **on by default** where supported.
- **Entire screen:** all computer audio **except Opencord's own output** (other people's voices, soundboard, notification sounds), so viewers don't hear the call twice.
- **Window:** only the audio of the app that owns the window, including its helper processes.

| Platform | Entire screen (all except Opencord) | Single window (that app only) | Fallback |
|---|---|---|---|
| **Windows 11** (the API needs build 20348 or newer) | WASAPI process loopback, `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE` with Opencord's PID | Process loopback, `PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE` with the window's PID | Windows 10 (consumer builds are older than 20348): audio switch disabled with "Requires Windows 11". Windows 10 support ended in October 2025 |
| **macOS 14.2+** | Core Audio process tap: global tap excluding Opencord's process | Core Audio process tap of the window's app and its helper processes | — |
| **macOS 13 – 14.1** | ScreenCaptureKit audio with `excludesCurrentProcessAudio = true` | ScreenCaptureKit audio filtered to the window's app | — |
| **Linux (PipeWire)** | Link every app's playback stream except Opencord's into Opencord's capture stream | X11: only streams whose PID matches the window's `_NET_WM_PID` or its child processes. Wayland: the portal doesn't say which app owns the window, so Opencord asks (§10.4) | PulseAudio-only systems: audio switch disabled |

### 10.2 Windows
- Activate with `ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, ...)` and `AUDIOCLIENT_ACTIVATION_PARAMS { ActivationType = PROCESS_LOOPBACK, ProcessLoopbackParams { TargetProcessId, ProcessLoopbackMode } }`. Minimum supported client: Windows 10 build 20348.
- Initialize with `AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK` and an explicit 48 kHz stereo format, as Microsoft's *ApplicationLoopback* sample does. Follow that sample.
- Window PID: `GetWindowThreadProcessId`. For UWP apps hosted by `ApplicationFrameHost.exe`, use the PID of the hosted `CoreWindow` child.
- Include mode covers the target's child processes, which handles browsers whose audio runs in a child process.

### 10.3 macOS
- **14.2+ (preferred for both modes):** `CATapDescription` (a global tap that excludes given processes, or a mixdown of given processes) → `AudioHardwareCreateProcessTap` → a private aggregate device containing the tap → an IOProc reads the audio. Translate PIDs to process objects with `kAudioHardwarePropertyTranslatePIDToProcessObject`.
- **Helper processes:** browsers and Electron apps play audio from helper processes. Include every audio process object whose bundle ID starts with the app's bundle ID.
- **Window → app:** known directly when the window was chosen in Opencord's picker (macOS 13). With the system picker, read the included window/app from the returned content filter where the OS exposes it; if the app can't be determined, ask the user like on Linux Wayland (§10.4).
- **Permission:** `NSAudioCaptureUsageDescription` in Info.plist; macOS asks the first time audio is captured.
- **13 – 14.1:** ScreenCaptureKit audio (`capturesAudio`, `excludesCurrentProcessAudio`, 48 kHz, 2 channels). For windows, use a second `SCStream` with an application filter and a tiny video size, only for its audio.

### 10.4 Linux (PipeWire)
- Create a capture stream with `media.class = Stream/Input/Audio`, `node.autoconnect = false`, `node.name = opencord-screen-audio`, 48 kHz stereo float.
- Watch the PipeWire registry. For each node with `media.class = Stream/Output/Audio` that should be shared, create links from its output ports to our input ports (mono → both channels, multichannel → front left/right). Streams that appear during sharing are linked automatically; removed ones simply disappear.
- **Exclude Opencord:** skip nodes whose `application.process.id` is Opencord's PID or a child of it, and nodes whose name starts with `opencord-`.
- **Never link devices** (microphones, speaker monitors); only application playback streams.
- **Window share on X11:** include only nodes whose PID is the window's `_NET_WM_PID` or a descendant of it (parent chain from `/proc/<pid>/stat`).
- **Window share on Wayland:** after the system picker closes, show a compact "Sound from" selector listing apps that are currently playing audio (name and icon from node properties). If exactly one app is playing, preselect it; otherwise select "No sound" until the user picks one, so another app's audio is never shared by surprise. Remember the choice per app.
- Prior art: Vencord's **venmic** (MPL-2.0, C++) does this linking for Vesktop. Read it for the approach, but write our own in Rust.
- Flatpak packaging later needs access to the PipeWire socket.

### 10.5 Encoding and sync
- Separate `screen_audio` track: Opus stereo, 128 kbps, music mode (§7.7).
- Audio and video timestamps come from the same capture clock; receivers sync them with RTCP sender reports (target within ±80 ms).
- Viewers set the stream's volume and mute separately from the streamer's voice (UI plan §17.3).

### 10.6 Echo when the streamer uses speakers
The streamer's microphone hears the shared audio from their speakers and would send it a second time through voice. For entire-screen sharing, feed the captured share audio into the echo canceller's reference together with Opencord's own output, so it is removed from the microphone too.

### 10.7 "No double audio" test
On each platform: user A shares the entire screen with audio while in a call with B and C, and plays music. B talks. C must hear B once (directly) and the music once (from the stream), and A's stream must not contain B's voice. Repeat with a window share of a browser playing a video: the stream contains only the browser's audio.

---

## 11. Soundboard

### 11.1 Rules
- Admins with `MANAGE_SOUNDBOARD` upload sounds of **up to 5 seconds**.
- Users need `USE_SOUNDBOARD` in the voice channel to play sounds, and also `USE_EXTERNAL_SOUNDS` to play sounds from another server they're in.
- **Playback is client-side.** Playing a sound sends a small event; every listener's client plays the file it already downloaded. No audio goes through the voice stream, so the player can keep talking and everyone hears both.
- The client checks permissions before playing (to show or disable sounds), **and the server checks again before broadcasting**, so a modified client can't get around permissions, cooldowns or limits.
- Sounds can't be played while self-deafened, server-muted or suppressed. Self-muted users can play sounds.
- While a sound plays, the player gets a different ring than the speaking ring (§11.6).

### 11.2 Limits

| Item | Limit |
|---|---|
| Duration | ≤ 5.0 s (the server allows a 20 ms tolerance) |
| Stored file | Ogg Opus, 48 kHz, ≤ 2 channels, ≤ 256 KB |
| Input files the

> **Not yet received:** the plan was cut off here when it was pasted (message length limit). The rest of §11.2, §11.3 onward (including §12–§17 and the V0–V10 milestones in §16) is still to come from the user.
