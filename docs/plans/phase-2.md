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
| Input files the client accepts for trimming | ≤ 10 MB: MP3, WAV, FLAC, OGG (Vorbis or Opus), M4A/AAC |
| Name | 2–32 characters, optional emoji |
| Sound volume | 0–100 %, default 100 % |
| Sounds per server | 48 by default (server setting, 0–200) |
| Cooldown | 3 s per user by default (server setting, 0–30 s) |
| Sounds playing at once in a channel | 3; more are rejected with `SOUND_COOLDOWN` |
| External sound cache on a server | 50 MB, least recently used evicted; 10 uploads per minute per user |
| Client cache | 200 MB across all servers, deduplicated by SHA-256 |

### 11.3 Upload (client prepares, server validates)
1. The admin picks a file. The Rust core decodes it (`symphonia`, plus libopus for Opus input) and returns a waveform.
2. **Trim editor** (like Discord's): waveform with a draggable 5-second window, preview, volume slider, name and emoji fields, and "Normalize loudness" (on by default; EBU R128 target −18 LUFS with `ebur128`).
3. The client encodes the selection to Ogg Opus at 96 kbps and uploads it with `POST /media/sounds`.
4. The server **parses the Ogg Opus file itself** (headers, sample rate, channel count, final granule position for the duration) and rejects anything over the limits, even from a modified client. It stores `data/sounds/<sha256>.opus`, inserts the row and broadcasts `SoundboardSoundCreate`.

### 11.4 Background sync
- On `Ready` and on every `SoundboardSoundCreate`, the client downloads missing sounds in the background (`GET /media/sounds/{sha256}`, at most 2 at a time per server, low priority), verifies the SHA-256 and stores them in the shared cache.
- A sound used on several servers is stored once.
- Sounds are decoded when played (a 5-second Opus file decodes in a few milliseconds).

### 11.5 Playing

```
Player's client checks:
  connected to voice channel C, not deafened, not server-muted, not suppressed
  USE_SOUNDBOARD in C
  sound from another server → USE_EXTERNAL_SOUNDS in C, and the server allows external sounds
  cooldown has passed
  → external sound this server doesn't have yet → POST /media/external-sounds (addressed by SHA-256)
  → PlaySoundboardSound { channel C, sound_ref }
Server re-checks all of the above, applies rate limits, confirms the sound exists
  → VoiceChannelEffect { channel C, user, sound_ref, name, emoji, volume, started_at }
    to everyone connected to C, including the player
Each client in C:
  skip if deafened, if it locally muted the player or their soundboard, or if soundboard volume is 0
  → fetch the file if missing → decode → mix into the output at sound volume × local soundboard volume
  → show the soundboard indicator on the player for the sound's duration
```
- `sound_ref` is `server:<sha256>`, `external:<sha256>` or `default:<id>`.
- Previews in the picker play locally only and are never sent.
- Sounds play through Opencord's mixer, so every listener's echo canceller removes them from their microphone.

### 11.6 Indicator (monochrome)

| State | Avatar (sidebar, member list) | Video tile |
|---|---|---|
| Speaking | Solid 2 px ring | Solid 2 px border |
| Soundboard playing | **Dashed** 2 px ring that slowly rotates, plus the sound's emoji next to the name | Dashed 2 px border, plus the sound's emoji in the top-left corner |
| Both | Solid ring inside a dashed outer ring | Solid border with a dashed outer border |

- The difference is shape, not colour, because the app is monochrome. The ring uses its own token, `soundboardRing` (equal to `text` by default), so it can be given a colour later with a one-line change.
- "Reduce motion" stops the rotation.
- The emoji is the only colour, since emoji are user content (UI plan §2.1).

### 11.7 UI
- **Soundboard button** in the voice view's control bar and in the "Voice connected" panel, plus a keybind to open it.
- **Picker** (360 px popover): search, **Favorites**, **This server**, **Other servers** (one group per server), **Default**. Each sound is a tile with its emoji and name; hovering shows a preview button. Sounds the user can't play are dimmed with a tooltip giving the reason ("You need Use External Sounds here"). A thin bar shows the cooldown.
- **Server settings → Soundboard:** list (emoji, name, duration, volume, uploader, preview, edit, delete), "Upload sound" (trim editor), a "12 / 48" counter, and settings: soundboard enabled, allow default sounds, allow sounds from other servers, cooldown, max sounds.
- **User settings → Sounds:** soundboard volume (0–100 %) and "Mute all soundboards".
- The per-user menu's "Mute their soundboard" (UI plan §17.3) is now active.

### 11.8 Default sounds
Ship 6–8 short CC0-licensed sounds inside the client, so every client has them without downloading. Servers can turn them off ("Allow default sounds").

---

## 12. Basic Discord voice features added

| Feature | Spec |
|---|---|
| **Opt-in stream watching + viewer list** | §9.5. Nobody downloads a stream they aren't watching |
| **AFK channel and timeout** | A user who hasn't spoken and whose computer reports no keyboard/mouse input for the timeout is moved to the AFK channel and suppressed. Never move someone who is streaming |
| **User limit and bitrate per channel** | §5.3; `MOVE_MEMBERS` bypasses the user limit |
| **Text chat in voice channels** | Voice channels can hold messages (same message system and permissions). Chat panel toggle in the voice view, unread badge on the voice channel row |
| **Priority speaker** | Hold-to-talk hotkey; everyone else is lowered to 25 % while the priority speaker talks |
| **Server mute / deafen / move / disconnect** | §4.1, enforced at the voice node |
| **Join, leave, mute, deafen, stream start/stop sounds** | Bundled sounds with per-event switches (UI plan §17.5 Sounds) |
| **Hide non-video participants** | Toggle in the voice view |
| **Connection quality and diagnostics** | Ping, jitter, packet loss, bitrates, codec, hardware or software encoder, voice node; signal-bars indicator in the voice panel (UI plan §17.5) |
| **Auto-reconnect** | §7.14, including after sleep and network changes |
| **"You're muted" reminder** | Speaking while muted shows a toast with Unmute (UI plan §17.1) |
| **Lower other apps' volume (attenuation)** | Windows: lower other audio sessions (`IAudioSessionManager2`). Linux: lower other PipeWire playback streams. macOS: hide the setting unless a public API allows it |
| **Mic test** | Hear yourself through the full processing chain (UI plan §17.5) |

---

## 13. End-to-end encryption readiness (structure only)

- Media is always encrypted in transit (DTLS-SRTP), but the voice node could access it, as with Discord before its DAVE protocol. The UI shows **no** lock for voice in this phase. When E2EE arrives, the client-side indicator rule from the encryption notes (§2) applies.
- **FrameTransform** trait in both pipelines: between encoder and packetizer when sending, between depacketizer and decoder when receiving. The implementation is the identity for now.
- The transform must keep codec framing intact so str0m's packetizers still work. Follow Discord's DAVE approach: leave codec headers (for example H.264 NAL unit headers) in the clear and transform the rest; Opus frames can be transformed whole.
- **Proof test:** a test-only XOR transform enabled on two voicebots. Audio and video, including simulcast layer switching, must still flow through the voice node. This proves the SFU never needs payloads.
- The SFU takes layer and keyframe information only from RTP header extensions (§6).
- `VoiceIdentify.max_e2ee_version = 0`. Voice gateway field numbers 100–149 are reserved for MLS messages (key packages, proposals, commits, welcomes, epoch transitions).
- `ClientConnect` / `ClientDisconnect` are the points where MLS will re-key later; keep them explicit and ordered.
- Soundboard sounds are server files and stay outside end-to-end encryption.

---

## 14. Security and abuse limits
- Voice tokens: 60 s expiry, single use, bound to session and channel.
- Rate limits: `UpdateVoiceState` 10 / 10 s, `StreamCreate` 5 / min, voice gateway messages 50 / 10 s, `MediaSinkWants` 20 / s, soundboard as in §11.2.
- The voice node ignores media from unauthenticated transports and caps each participant's inbound bitrate (sum of their tracks' ceilings + 20 %).
- Media HTTP: enforce size limits while receiving uploads (never buffer without a limit) and trust parsing, not the declared content type.
- Validate every protobuf count and length (participants, layers, wants).
- No server-side recording or decoding of voice or video.

---

## 15. Testing
- **Unit:** jitter buffer (reordering, loss, bursts), mixer and limiter, preset-to-size math, bitrate ceilings, permission checks including external sounds, Ogg Opus validation (truncated, too long, wrong sample rate), token signing/expiry, SFU layer selection.
- **Voicebot** (`crates/opencord-voicebot`): a headless client that joins a channel, sends a WAV file or a tone, publishes a synthetic test-pattern video with simulcast, watches streams and plays soundboard sounds. It reports received audio levels, latency (click detection), layers and bitrates. Used by integration tests, the load test and self-hosters testing their setup.
- **Network impairment:** a feature-gated packet shim in `opencord-media` (loss %, jitter, reordering, bandwidth cap), so tests don't need root or `tc`.
- **Integration:** two users talk; server mute stops audio within 100 ms; deafened users receive no audio packets; layer switching under a 500 kbps cap; hidden tiles receive zero video bytes; quality limit rejection; soundboard permission, cooldown and duration enforced against a modified client; AFK move; voice node failover.
- **Manual platform checklist** for each capture milestone: Windows 11 (NVIDIA, AMD or Intel GPU), macOS 14 and 15 (Apple Silicon) plus macOS 13 if available, Arch + Hyprland, GNOME (Wayland), KDE (Wayland), one X11 session. Include multiple monitors, an HDR monitor, mixed DPI, headset hot-plugging and sleep/wake.
- **Performance measurements** recorded in `docs/performance.md` against §16.1.

---

## 16. Milestones

### 16.1 Performance targets
Measured on a mid-range laptop (4–8 cores, integrated or mid-range GPU) unless stated.

| Area | Target |
|---|---|
| Voice end-to-end latency on a LAN, including devices | ≤ 150 ms |
| 5 % random packet loss + 40 ms jitter | Speech stays intelligible without gaps |
| Silent participant upload | < 3 kbps |
| Audio processing, Standard / High noise suppression | ≤ 2 % / ≤ 10 % of one core |
| Camera on (720p30 + layers, hardware encoding) | ≤ 6 % CPU |
| Screen share 720p30 / 1080p60, hardware encoding | ≤ 8 % / ≤ 15 % CPU |
| Screen share capture-to-display latency on a LAN | ≤ 250 ms |
| Watching a 1080p60 stream, hardware decoding | ≤ 8 % CPU |
| Client memory in a 10-person call with 4 cameras and 1 stream | ≤ 350 MB |
| Voice node on 4 vCPUs: 10 channels × 10 people, 2 cameras + 1 stream each | < 50 % CPU |

### V0 — Protocol, permissions and data
- `voice.proto`, `internal.proto`, additions to `requests.proto`, `events.proto` and `models.proto` (§4).
- Permission bits 24–27 and defaults (§5.1); new groups in the roles editor.
- Migrations and settings (§5.2–5.4); server settings "Voice & video" page and voice channel settings in the UI.
- Voice-signing key, voice tokens, media tokens, media HTTP endpoint skeleton.
- Main-gateway voice states without media: join, leave, user limit, server mute/deafen/move/disconnect state changes, grace period, `Ready` additions. The sidebar shows participants live.
- **Accept:** unit tests for permissions, tokens (valid, expired, wrong channel) and settings validation. Integration test: two clients join and leave a voice channel, and every member who can see it receives the changes; a user without `CONNECT` is rejected; the user limit holds, and `MOVE_MEMBERS` bypasses it.

### V1 — Voice node and audio forwarding
- `crates/opencord-voice` (library, embedded in `opencord-server`), the `opencord-voice-node` binary and the internal control channel.
- Voice gateway handshake, heartbeat and resume; str0m SFU in RTP mode on UDP 7711; audio forwarding rules (§6); public address handling; node selection and failover.
- `opencord-voicebot` with audio send and receive.
- **Accept:** two voicebots exchange audio through the embedded node and through an external node; server mute stops forwarding within 100 ms; a deafened bot receives no audio packets; a voice gateway resume doesn't interrupt media; killing an external node moves its channels to another node and the bots reconnect within 5 s.

### V2 — Client audio engine
- `crates/opencord-media`: devices (§7.6), resampling, Opus (§7.7), jitter buffer, loss handling, mixer, per-user volume and local mute, master volume, limiter, deafen (§7.5), a basic voice-activity gate and in-app push-to-talk, str0m client transport, connection states and recovery (§7.14), Rust ↔ Dart API (§7.12).
- UI on real data: voice panel, user panel, quick audio menu, per-user menus, voice view participants (UI plan §4.2, §4.10, §17.1–17.3).
- **Accept:** two app instances on different machines talk within the §16.1 latency target; 5 % loss + 40 ms jitter stays intelligible; a silent participant uploads < 3 kbps; unplugging the headset falls back to the default device; switching Wi-Fi recovers in under 3 s.

### V3 — Voice processing and hotkeys
- Echo cancellation, gain control and high-pass filter (§7.4); Standard and High noise suppression with the first-run benchmark and automatic fallback (§7.3); automatic and manual sensitivity; push-to-talk release delay; global hotkeys (§7.13); mic test; level meters; priority speaker; speaking indicators; "You're muted" toast.
- **Accept:** with laptop speakers and built-in mic, the other side hears no echo; recorded noise samples (keyboard, fan, street, dog) are clearly reduced in both modes; CPU within §16.1; the fallback from High to Standard triggers under artificial load; global push-to-talk works on Windows 11, macOS, Hyprland and GNOME on Wayland.

### V4 — Video transport
- Track publish/unpublish, `MediaSinkWants`, `SenderLayerWants`, simulcast forwarding with layer switching and rewriting, keyframe requests, NACK/RTX, bandwidth estimation in both directions, sender priorities (§7.10), the frame-marking header extension, quality enforcement (§6), the camera participant cap.
- The voicebot publishes a synthetic three-layer test pattern.
- **Accept:** with a 500 kbps receiver cap, the SFU switches to a lower layer within 2 s and back up when the cap is lifted; a receiver that hides a tile gets zero video bytes for it; unused layers stop being encoded; a modified client exceeding its bitrate ceiling has its track stopped; the XOR-transform test (§13) passes.

### V5 — Camera
- Capture backends (§8), hardware encoding with fallbacks (§7.9), decoding, pixel-buffer Flutter textures (§7.11), preview and mirroring, device hot-plug, permission prompts, camera tiles in the voice view.
- **Accept:** a 9-person call with every camera on stays smooth on the reference laptop; camera on/off takes under 1 s; CPU within §16.1; Windows, macOS and Linux (Wayland and X11) all work.

### V6 — Screen share video
- Share dialog with quality options and the server cap (§9.1–9.2), system pickers on Linux and macOS 14+, Opencord's picker on Windows and macOS 13, capture backends (§9.3), GPU scaling and conversion, screen-content encoding and the low layer, stream events, opt-in watching and the viewer list (§9.5), edge cases (§9.4).
- **Accept:** the platform checklist (§15) passes for screens and windows; 720p30 and 1080p60 meet §16.1; options above the server cap can't be selected and are rejected by the server; window resize, minimize, close and monitor unplug behave as specified.

### V7 — Screen share audio
- Platform implementations and fallbacks (§10.1–10.4), echo-canceller reference (§10.6), audio/video sync (§10.5), stream volume for viewers.
- **Accept:** the "no double audio" test (§10.7) passes on Windows 11, macOS 14.2+, macOS 13 (if available), Hyprland, GNOME and X11; window audio contains only that app's sound (browser test); unsupported systems show the disabled switch with the reason.

### V8 — Soundboard
- Server storage, validation, endpoints and events; trim editor and upload; background sync and cache; picker; play flow with client and server checks; external sounds; default sounds; indicators; settings pages (§11).
- **Accept:** a 6-second file is rejected by the server even when uploaded by a modified client; playing without `USE_SOUNDBOARD`, during the cooldown, while server-muted, or from another server without `USE_EXTERNAL_SOUNDS` is rejected by the server; all listeners hear a sound within 150 ms of each other on a LAN; talking during a sound is heard together with it; the indicators show a dashed ring for a sound, a solid ring for speaking, and both when both happen.

### V9 — Basic Discord features
- Everything in §12.
- **Accept:** AFK move after the timeout (shortened in tests); text chat in voice channels; hide non-video participants; diagnostics match voicebot measurements; after sleep/wake the client rejoins its channel.

### V10 — Hardening, performance and packaging
- Zero-copy texture paths (§7.11); load test and `docs/performance.md`; packaging: macOS Info.plist keys (`NSMicrophoneUsageDescription`, `NSCameraUsageDescription`, `NSAudioCaptureUsageDescription`) and hardened-runtime entitlements for audio input and camera, Windows notes, Linux dependencies and Flatpak PipeWire access notes; `docs/voice.md` (architecture, ports, self-hosting behind NAT); deploy files with UDP 7711.
- **Accept:** every §16.1 target is met and recorded; `docker compose up` gives working voice from another machine; a fresh macOS install asks for exactly the needed permissions with clear explanations.

**After Phase 2 (optional):** AV1 when every participant can decode it, stream preview thumbnails, pop-out stream windows, camera background blur, TCP/TURN fallback.

---

## 17. Official documentation to read first

**Windows (Microsoft Learn)**
- Screen capture with `Windows.Graphics.Capture`; Desktop Duplication API.
- `AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS`, `PROCESS_LOOPBACK_MODE`, `ActivateAudioInterfaceAsync`, and the *ApplicationLoopback* sample in Windows-classic-samples.
- WASAPI: loopback recording, event-driven shared-mode streams, `IMMNotificationClient`.
- Media Foundation: Source Reader, hardware MFTs.
- Multimedia Class Scheduler Service; `SetWindowDisplayAffinity`.
- NVIDIA Video Codec SDK, AMD AMF and Intel oneVPL notes (used through FFmpeg).

**macOS (Apple Developer)**
- ScreenCaptureKit documentation; WWDC22 "Meet ScreenCaptureKit" and "Take ScreenCaptureKit to the next level"; WWDC23 "What's new in ScreenCaptureKit" (sharing picker).
- Core Audio taps: `CATapDescription`, `AudioHardwareCreateProcessTap`, and Apple's sample on capturing system audio with Core Audio taps.
- AVFoundation capture sessions and media-capture authorization.
- VideoToolbox `VTCompressionSession`; hardened-runtime entitlements.

**Linux**
- XDG Desktop Portal: ScreenCast, Camera and GlobalShortcuts interfaces.
- PipeWire documentation: streams, DMA-BUF sharing, ports and links; WirePlumber's linking policy (to understand how our links coexist with it).
- VA-API and FFmpeg VA-API encoding.
- X11 MIT-SHM and Composite extensions.

**Standards and references**
- RFC 3550 (RTP), RFC 8445 (ICE), RFC 5764 (DTLS-SRTP), RFC 7587 (Opus RTP), RFC 6184 (H.264 RTP), RFC 6464 (audio level), RFC 4588 (RTX), the transport-wide congestion control draft.
- str0m documentation and its SFU example.
- Discord developer documentation on voice connections (gateway voice flow), and Discord's DAVE protocol whitepaper (for §13, later).
- RNNoise / `nnnoiseless`, DeepFilterNet papers and repository.
- venmic source (Linux application audio linking).

---

## 18. Development environment additions

**Arch Linux (Hyprland)**
```bash
sudo pacman -S --needed pipewire wireplumber xdg-desktop-portal xdg-desktop-portal-hyprland ffmpeg opus libva libva-utils meson ninja clang pkgconf libx11 libxext libxfixes libxcomposite libxrandr
vainfo
```
`vainfo` should list an H.264 encode entry point for your GPU. NVIDIA users need the proprietary driver for NVENC.

**Windows:** Visual Studio 2022 Build Tools with the C++ workload and the Windows 11 SDK; Python with meson and ninja (for the bundled AudioProcessing build); an LGPL FFmpeg shared build whose DLLs ship next to the app.

**macOS:** Xcode, and Homebrew for development tools:
```bash
brew install meson ninja pkg-config opus ffmpeg
```
The app bundles its own LGPL FFmpeg libraries. Deployment target: macOS 13.

**CI:** build and unit tests on all three platforms. Capture and hardware tests are manual (§15).
