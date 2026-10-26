# Mobile client: design (early version)

The phone apps for Android and iOS, written in React Native on the Rust client core. This is the first of two plans: it covers the app's shape, chat, voice and music. Video, camera and screen sharing come in the second plan.

## 1. What the owner asked for

In the owner's words (2026-10-26):

- "The UI will be basically like WhatsApp or Telegram, where in the main list the user sees the servers they joined. When they tap a server it navigates into the server and they see the list of text and voice channels. Then they can open text channels, where the channels list shifts into the left sidebar and the text messages show."
- "Be careful: the left sidebar shouldn't show the servers the user joined. Only channels go on the left side."
- "Implement the music player too, and a section for uploading custom audio and changing the volume."
- "The app is written in React Native. The app should use native components, since we are using React Native."
- "Use the Rust client core with slight changes for mobile."
- "Don't write the UI as slop": UI work follows Software Mansion's React Native skills (`react-native-best-practices`, installed under `~/.claude/plugins/marketplaces/swmansion/skills`).
- The QEMU-based Android emulator on the dev machine can be used.

## 2. Scope

### Goals: this plan

- **Two apps, one codebase:** Android and iOS from `mobile/`. Android is built and tested on the dev machine's emulator. iOS is built in CI and checked on the Mac.
- **Identity:**
  - create one with a display name, or import a desktop backup file;
  - kept in the system keychain (iOS) or keystore (Android);
  - backed up from the You tab.
- **Servers:**
  - add one from an invite link or `host:port`, with the trust prompt for self-signed certificates and an optional claim token;
  - list them with the last activity, unread counts and connection state;
  - leave one; retry a connection.
- **Inside a server:**
  - the channel list (categories, text and voice channels, who is in voice, what music plays);
  - text chat, where the channel list moves into a left sidebar;
  - history going back;
  - send, edit and delete; typing;
  - unread and mention counts;
  - images and files: view them, and send them.
- **Voice:**
  - join and leave; mute and deafen; who is speaking;
  - each person's volume;
  - keeps going with the screen off or the app in the background.
- **Music:**
  - what plays, with play, pause, skip, seek, shuffle and repeat;
  - the queue: Up next, reorder and remove;
  - search and add; Recently played;
  - **an Upload section** for audio files from the phone, with progress, cancel and retry;
  - **your music volume** (0–200 %), mute, and "Lower music while people talk".
- **The You tab:** display name, identity backup, theme, music volume, version.

### Later: not in this plan

| What | Where it goes |
|---|---|
| Camera, screen sharing, watching streams and video tiles | The second mobile plan. It needs phone codecs (VideoToolbox, MediaCodec) and native video views. |
| Private messages and friends | A later plan. The core has them; the app needs their screens. |
| Push notifications | A later plan. They need a server-side push relay (APNs, FCM). |
| Server administration: channels, roles, invites, settings | Later. The desktop does it. |
| Soundboard, recording voice messages, chat commands (`/play`) | Later. Voice messages already sent still show as files. |
| A "Your songs" list in the Upload section | Later. Members can't list their own uploads today (see §13). |

### Anti-goals: failure even if it works

- **It looks like a web page or a template:**
  - generic cards, default fonts, uneven spacing;
  - custom-drawn controls where the system has native ones;
  - navigation that doesn't behave like the platform's.
- **Joined servers appear in the left sidebar.** The owner ruled this out.
- **Chat that jumps:**
  - the list moves when older messages load;
  - the keyboard covers the composer;
  - frames drop while typing.
- **Voice that stops when the screen locks.**
- **The same data shows differently on desktop and phone:** markdown rendered differently, or unread counts that disagree.

## 3. Constraints

- **Stack:**
  - Expo SDK 58: React Native 0.88, the New Architecture, Hermes;
  - Expo Router with development builds (Expo Go can't load our native module);
  - TypeScript, strict.
- **The Rust core:**
  - reached through UniFFI and uniffi-bindgen-react-native (`ubrn`), version **0.31.0-6** (UniFFI 0.31);
  - changes to `opencord-core` stay additive and small; the mobile surface lives in a new crate.
- **Platforms:**
  - **Android:** 8.0 (API 26) and newer, 64-bit only (`arm64-v8a`, `x86_64`). API 26 is AAudio's, which cpal needs for sound (Expo's own minimum is 24). Google Play requires 64-bit, and 32-bit phones are rare.
  - **iOS:** at the deployment target Expo SDK 58 sets.
- **Package manager:** Yarn 1 (classic) workspaces, the Yarn on the dev machine. Node 26 has no corepack, so Yarn 4 would need a separate install.
- **Licences:**
  - the app is MPL-2.0, like `app/`;
  - new dependencies (npm and crates) must be MIT, Apache-2.0, BSD, ISC, Zlib or 0BSD;
  - MPL-2.0 is allowed only for UniFFI and uniffi-bindgen-react-native and their runtimes, which carry the core's own licence.
- **Design:** monochrome, with `voice` (blue) and `recording` (red) the only hues, in Inter and JetBrains Mono, as on desktop (§9).
- **Process:**
  - every change goes through a pull request (`CLAUDE.md`, `GEMINI.md`);
  - each milestone ends in one, into `canary`.
- **The dev machine:**
  - Arch Linux with the Android SDK at `/home/amin/sdk/android` (NDK 27, 28 and 30);
  - the `Pixel_7` emulator (API 33, x86_64);
  - about 29 GB free, so local builds target `x86_64` only;
  - no Mac: iOS work runs in CI and in a Claude Code session on the Mac.

## 4. How it fits together

```
 ┌───────────────────────────── mobile/ (Expo app, TypeScript) ─────────────────────────────┐
 │  src/app/**          routes (Expo Router): tabs, server screen, sheets                     │
 │  src/features/**     screens and their parts                                               │
 │  src/ui/**           design system: tokens, text, icons, rows, native control wrappers     │
 │  src/state/**        one Zustand store: snapshots, unread, messages, voice, music, prefs   │
 │  src/core/**         CoreApi ─┬─ NativeCore (real)      eventLoop: nextEvents → applyEvents│
 │                               └─ FakeCore (tests, screenshots, EXPO_PUBLIC_FAKE_CORE=1)    │
 └───────────────────────────────────────────┬──────────────────────────────────────────────┘
                                             │ generated TypeScript + C++ (JSI turbo-module)
 ┌──────────────────── mobile/packages/core-native (@opencord/core-native) ─────────────────┐
 │  built by ubrn from crates/opencord-mobile: libopencord_mobile.a per ABI / xcframework     │
 └───────────────────────────────────────────┬──────────────────────────────────────────────┘
                                             │ Rust calls
 ┌───────── crates/opencord-mobile (UniFFI facade) ─────────┐   ┌──── crates/opencord-core ────┐
 │  JS-friendly records: string ids, number times, flags    │──▶│  Client: servers, chat,      │
 │  next_events: batched, pulled; one waiter at a time      │   │  voice (cpal, AEC3, Opus),   │
 │  attachment file cache                                   │   │  music, uploads, settings KV │
 └──────────────────────────────────────────────────────────┘   └──────────────────────────────┘
```

- **The core is reused as is.** `opencord-core` already type-checks for `x86_64-linux-android`: the C libraries (Opus, SQLite, aws-lc) build with the NDK, and its TLS path already has an Android branch (`tofu.rs`, bundled roots). Its media code has "unsupported" back ends for video, hotkeys, idle and screen sound on other systems, so phones get audio and no video, as planned.
- **A facade crate, not a second API inside the core.** `crates/opencord-mobile` wraps the same `opencord_core::client::Client` that the Flutter API (`opencord-core/src/api/`) wraps. That API is shaped by flutter_rust_bridge, so it uses `StreamSink`, 64-bit ids and permission bits. The facade gives JavaScript what it handles well:
  - ids as decimal strings: an `i64` becomes a `bigint` in TypeScript, which can't be a React key or a route parameter;
  - times and sizes as numbers;
  - permissions as named booleans.
- **Events are pulled, not pushed.** A callback from a Rust thread into JavaScript blocks that thread until JavaScript returns (ubrn's threading guide). The app instead loops on `await nextEvents(...)`. Each call returns a batch, so one JSI crossing carries many events and the store updates once per batch.
- **State lives in TypeScript.** The store mirrors the desktop's Dart repository and providers: it applies events to server snapshots and works out unread counts the same way (§6). The core stays the same for both apps.

## 5. The facade: `crates/opencord-mobile`

Library name `opencord_mobile`, crate types `lib` and `staticlib`, MPL-2.0. It exports with UniFFI proc macros (`uniffi::setup_scaffolding!()`). TypeScript sees each function in camelCase (`next_events` → `nextEvents`).

### 5.1 Conventions

- **Ids** (users, channels, messages, attachments, roles) are decimal strings. Parsing one that isn't a valid `i64` fails with `InvalidInput { message: "not an id: …" }`.
- **Numbers:**
  - times are milliseconds since the Unix epoch, as `f64`;
  - durations, positions and byte sizes are `f64`;
  - small counts are `u32`;
  - volumes are `f32`, from 0.0 to 2.0.
- **Errors:** every fallible function returns `Result<_, MobileError>` (§5.5).
- **Async calls** run on the core's tokio runtime (spawn there and await the handle, as `api::client::on_runtime` does). Quick local reads (settings, the server list, identity helpers) are synchronous.
- **The facade owns its process state:** the runtime, the client and the event receivers, in `OnceLock`s, as `api/client.rs` does for Flutter. Only one of the two layers runs in a given app. The logic sits in a plain `Mobile` struct so tests can build one directly.

### 5.2 Functions

| Area | Function | Returns | Notes |
|---|---|---|---|
| Setup | `core_init(data_dir, cache_dir)` | `()` | Sync. Opens the core in `data_dir` (`Client::new`), enables media (`open_devices: true`), keeps `cache_dir` for attachments. A second call does nothing. |
| | `core_version()` | `String` | The crate version. |
| | `events_attach()` | `u64` | Sync. Starts a new event generation. A `next_events` still waiting under an older generation returns `[]` at once, releasing the receivers. JavaScript calls it once per start, including after a reload. |
| | `next_events(generation, max)` | `Vec<MobileEvent>` | Async. Waits until at least one event is ready, then returns up to `max` (1–1024) in order. Returns `[]` only when a newer generation has attached. Kinds the app doesn't use are dropped here and never end a wait early. |
| | `settings_get(key)` / `settings_set(key, value?)` | `Option<String>` / `()` | The core's local key-value store. |
| Identity | `identity_generate()` | `NewIdentity` | |
| | `identity_check(secret, display_name)` | `IdentityInfo` | |
| | `identity_load(secret, display_name)` | `IdentityInfo` | Uses it for every server, and connects the saved servers. |
| | `identity_backup_encode(secret)` / `identity_backup_decode(text)` | `String` / `Vec<u8>` | The desktop's backup file format. |
| Servers | `servers_list()` | `Vec<ServerEntry>` | |
| | `server_add(link_or_address, claim_token?)` | `AddServerResult` | Invite links and `host:port`, as on desktop. |
| | `server_trust(address, fingerprint)` | `()` | Then add again. |
| | `server_retry_now(server_key)` / `server_remove(server_key)` | `()` | |
| | `profile_update(server_key, display_name)` | `UserRecord` | |
| | `avatar_file(server_key, hash)` | `String` | A local file path; the core caches it. Also used for server icons. |
| Chat | `messages_fetch(server_key, channel_id, before?, limit)` | `Vec<MessageRecord>` | Newest first, as the core returns them. `limit` is 1–100. |
| | `message_send(server_key, channel_id, content, nonce)` | `MessageRecord` | |
| | `message_send_files(server_key, channel_id, content, nonce, files)` | `MessageRecord` | `files: Vec<OutgoingFileRecord { path, filename }>`. `SendProgress` events follow. |
| | `send_cancel(nonce)` | `()` | |
| | `message_edit(server_key, message_id, content)` | `MessageRecord` | |
| | `message_delete(server_key, message_id)` | `()` | |
| | `typing_start(server_key, channel_id)` | `()` | |
| | `attachment_file(server_key, attachment_id, variant)` | `String` | A path in the attachment cache (§5.6). `variant` is `Original` or `Thumbnail`. |
| Voice | `voice_join(server_key, channel_id)` | `VoiceStateRecord` | |
| | `voice_leave()` | `()` | |
| | `voice_set_mute(muted)` / `voice_set_deaf(deafened)` | `()` | |
| | `voice_set_user_volume(server_key, user_id, volume)` / `voice_set_user_local_mute(server_key, user_id, muted)` | `()` | Sync. This device only, as on desktop. |
| Music | `music_settings(server_key)` | `MusicSettingsRecord` | |
| | `music_subscribe(server_key, channel_id)` / `music_unsubscribe(…)` | `MusicSessionRecord` / `()` | |
| | `music_search(server_key, channel_id, query, source)` | `MusicSearchResultsRecord` | |
| | `music_enqueue(server_key, channel_id, result_ids, history_item_ids, before_item_id?, placement)` | `MusicSessionRecord` | |
| | `music_playback(server_key, channel_id, action, position_ms?, expected_item_id?)` | `MusicSessionRecord` | |
| | `music_set_mode(server_key, channel_id, repeat?, shuffle?)` | `MusicSessionRecord` | |
| | `music_queue_move(…, item_id, before_item_id?)`, `music_queue_remove(…, item_ids)`, `music_queue_clear(…)`, `music_vote_skip(…, expected_item_id)` | `MusicSessionRecord` | |
| | `music_set_listening(volume, muted, duck)` | `()` | Sync. Applies to every server and stays on this device. |
| | `music_artwork(server_key, artwork_id, size)` | `String` | A local path, at 128 or 512 px. |
| | `music_upload_song(server_key, channel_id, upload_id, path, play_now)` | `()` | Resolves once the song is queued. `MusicUpload` events follow under `upload_id`. |
| | `music_cancel_song(server_key, upload_id)` | `()` | The upload then fails with `Cancelled`. |

### 5.3 Records and enums

Every record and enum the functions and events use. Field types follow §5.1.

- `IdentityInfo { public_key_hex, fingerprint }`, `NewIdentity { secret: Vec<u8>, info: IdentityInfo }`
- `ServerEntry { key, name, host, port: u16, user_id?, fingerprint? }`
- `AddServerResult`: `Added { server: ServerEntry }` | `NeedsTrust { address, fingerprint }`
- `ConnectionRecord`: `Connecting` | `Connected` | `Reconnecting { attempt: u32, retry_in_ms: u32 }` | `Failed { reason: FailureKind, message, expected_fingerprint?, presented_fingerprint? }`
- `FailureKind`: `Rejected`, `Kicked`, `Banned`, `FingerprintChanged`, `Incompatible`, the core's `FailureReason`.
- `UserRecord { id, display_name, avatar_hash?, fingerprint }`
- `MemberRecord { user: UserRecord, nickname?, role_ids: Vec<String> }`
- `ChannelKind`: `Text`, `Voice`, `Category`
- `ChannelRecord { id, kind, name, topic?, parent_id?, position: i32, user_limit: u32, text_in_voice: bool }`
- `ChannelAccess { channel_id, view, read_history, send, attach, connect, speak, manage_messages, music_add, music_control, music_manage_queue, music_manage }`: booleans from the channel's permission bits (`opencord_common::permissions::Permissions`); `ADMINISTRATOR` makes all of them true.
- `ServerSnapshot { server_key, self_user: UserRecord, name, description, owner_id?, icon_hash?, channels, access: Vec<ChannelAccess>, members, voice_states, voice_enabled: bool }`
- `MessageRecord { id, channel_id, author_id, content, created_at_ms, edited_at_ms?, nonce?, command?, attachments: Vec<AttachmentRecord> }`
- `AttachmentKind`: `File`, `Image`, `Video`, `Audio`, `VoiceMessage`
- `AttachmentRecord { id, filename, content_type, size_bytes, kind, width?, height?, duration_ms?, has_thumbnail, thumbnail_width?, thumbnail_height?, animated }`
- `AttachmentVariant`: `Original`, `Thumbnail`
- `VoiceStateRecord { user_id, channel_id?, this_device, self_mute, self_deaf, server_mute, server_deaf }`
- `VoiceLinkState`: one variant per `VoiceConnectionState` variant, with the same names and fields (`AwaitingEndpoint`, `Authenticating`, `RtcConnecting`, `Connected`, `Reconnecting`, `NoRoute`, `Disconnected { … }`).
- `SpeakingChangeRecord { user_id, speaking }`
- `UploadPhaseRecord`: `Preparing` | `Uploading` | `Processing` | `Sending` | `Done` | `Failed { message }` | `Cancelled`
- Music:
  - `MusicTrackRecord { title, artist, duration_ms, is_stream, is_seekable, source, source_display_name, uri, artwork_id, local_song_id?, removable }`: a `local_song_id` of 0 becomes `None`.
  - `MusicQueueItemRecord { item_id, track, requester_id, added_at_ms }`
  - `MusicPlayState`: `Idle`, `Loading`, `Playing`, `Paused`
  - `MusicRepeat`: `Off`, `Queue`, `Track`
  - `MusicPlacement`: `End`, `Next`, `Now`
  - `MusicAction`: `Pause`, `Resume`, `Next`, `Previous`, `Seek`, `Stop`
  - `MusicSessionRecord { channel_id, version, state, now_playing?, queue, history, position_ms, anchor_ms, repeat, shuffle, waiting_for_node, paused_empty, skip_votes: Vec<String>, skip_votes_needed: u32 }`: `anchor_ms` is already on this device's clock, as the core makes it.
  - `MusicNowPlayingRecord { channel_id, title, artist, artwork_id, playing, ended, source_display_name }`
  - `MusicSettingsRecord { supported, enabled, available_sources: Vec<String>, default_source, allow_links, vote_skip }`
  - `MusicSearchResultsRecord { results: Vec<MusicSearchResultRecord { result_id, track }>, playlist?: MusicPlaylistRecord { name, result_ids } }`
- `OutgoingFileRecord { path, filename }`

### 5.4 Events

`MobileEvent`, built from the core's `CoreEvent { server_key, payload }` and its `MediaEvent`:

| Event | From |
|---|---|
| `Connection { server_key, state: ConnectionRecord }` | `ConnectionState` |
| `Ready { snapshot: ServerSnapshot }` | `Ready`. It replaces everything the app knows of that server. |
| `MessageCreated { server_key, message }` / `MessageUpdated { … }` | `MessageCreate` / `MessageUpdate` |
| `MessageDeleted { server_key, channel_id, message_id }` | `MessageDelete` |
| `ChannelUpserted { server_key, channel }` | `ChannelCreate`, `ChannelUpdate` |
| `ChannelDeleted { server_key, channel_id }` | `ChannelDelete` |
| `AccessChanged { server_key, access: Vec<ChannelAccess> }` | `PermissionsUpdate` |
| `MemberUpserted { server_key, member }` / `MemberLeft { server_key, user_id }` | `MemberJoin`, `MemberUpdate` / `MemberLeave` |
| `Typing { server_key, channel_id, user_id }` | `TypingStart` |
| `ServerInfoChanged { server_key, name, icon_hash? }` | `ServerUpdate` |
| `VoiceStateChanged { server_key, state }` | `VoiceStateUpdate` |
| `VoiceLink { server_key, channel_id, state: VoiceLinkState }` | media `ConnectionState` |
| `Speaking { server_key, channel_id, changes }` | media `Speaking` |
| `AudioProblem { output: bool, fell_back: bool, message }` | media `DeviceFellBack` (`fell_back: true`, the device name as the message) and `DeviceFailed` |
| `SpeakingWhileMuted` | media `SpeakingWhileMuted` |
| `MusicSession { server_key, session }` / `MusicNowPlaying { server_key, now_playing }` | the same |
| `MusicNotice { server_key, channel_id, text }` | `MusicNotice` |
| `MusicSettingsChanged { server_key, settings }` | `MusicSettingsUpdate` |
| `MusicUpload { server_key, channel_id, upload_id, filename, sent_bytes, total_bytes, phase }` | `MusicUploadProgress` |
| `SendProgress { server_key, nonce, index: u32, sent_bytes, total_bytes, phase }` | `UploadProgress` |
| `Notice { server_key, key, title, body }` | `ServerNotice` |

**Dropped** (the app doesn't use them yet):
- server events: roles, presence, voice settings, streams, soundboard, voice channel effects, server addresses, network status, Lavalink, commands, ephemeral messages, upload settings, private settings, storage, staged uploads, downloads;
- media events: input level, the noise suppression fallback, hotkeys, devices changed.

### 5.5 Errors

`MobileError` mirrors `CoreError` variant for variant:
- `NotInitialized`, `NoIdentity`, `UnknownServer`, `NotConnected`, `Timeout`;
- `InvalidInput { message }`;
- `Server { code, message, retry_after_ms? }`: `code` is the `ErrorCode` variant's name in snake case, so the facade doesn't mirror 60 codes;
- `FingerprintMismatch { expected, presented }`;
- `Rejected { reason: FailureKind, message }`;
- `Connection { message }`, `Storage { message }`, `Cancelled`;
- plus `Other { message }` for anything new.

The app turns each into a sentence (`src/core/errors.ts`). It shows a server's `message` only where the desktop does, for upload refusals and music errors.

### 5.6 The attachment cache

- **Where:** `attachment_file` keeps files at `<cache_dir>/attachments/<first 16 hex of sha256(server_key)>/<attachment_id>.<o|t>`.
- **When the file is there:** it returns the path at once.
- **When it isn't:**
  - a thumbnail comes from the core's `attachment_bytes`;
  - an original streams to disk through `download_attachment`.

  Either way the file is written under a unique `.part` name and renamed into place, so a reader never sees half a file, and two callers can race safely.
- **Encryption:** the files are plain. The phone's app sandbox and disk encryption protect them, so the desktop's encrypted media cache (`media_cache_init`) isn't turned on.
- **Cleanup:** the system may clear the folder when space runs short. There is no size limit in this plan.

## 6. App state

One Zustand store (`src/state/store.ts`). A pure reducer applies event batches: `applyEvents(state, batch) → state`. The actions in `src/state/actions/` call `CoreApi` and update the store. Components read through selector hooks (`src/state/selectors.ts`), so a row re-renders only when its own data changes.

### 6.1 Per server

- **`entry`** from `serversList()`.
- **`connection`**, from the latest `Connection` event; `Connecting` until one arrives.
- **`snapshot`**, from `Ready`, then patched by the channel, member, access, voice and info events.
- **`activity`** per readable text or voice channel: `{ last?: MessageRecord, lastReadId: string, unread: number, mentions: number }`.
- **`typing`:** channel → user → when it expires. A `Typing` shows for **10 s**, or until that user's next message.
- **`voice`:** user → `VoiceStateRecord`, for everyone in a voice channel.
- **`nowPlaying`** per voice channel, and **`musicSettings`** (fetched the first time a voice channel opens; `MusicSettingsChanged` keeps it current).

### 6.2 Unread, as the desktop counts it

`rust_repository.dart` `_recentActivity` and `read_positions.dart`, ported:

1. **On `Ready`:**
   - mark the server `seeding`;
   - for every text or voice channel the user may read (`view` and `read_history`), fetch the newest **50** messages;
   - at most **4** fetches at once, each with a **10 s** time-out.
2. **For each channel:**
   - **No messages:** start it at 0 if this device has no position for it yet ("start"), so what arrives from now on counts.
   - **Messages:**
     - `last` is the newest;
     - with no position saved, the position becomes that newest id (channels never read before start out read);
     - `unread` counts the fetched messages newer than the position that others wrote;
     - `mentions` counts those containing `<@selfId>`.
   - **A failed or timed-out fetch** leaves the channel without activity. It is logged; nothing is shown.
3. **Events while seeding:** they are held, then applied in order once seeding ends (the Dart `_held`). If a newer `Ready` arrives meanwhile, the older seeding's result is thrown away.
4. **Live messages:**
   - `MessageCreated` sets `last`;
   - it adds 1 to `unread` (and to `mentions` if it mentions the user) unless the user wrote it or it lands in the open channel while the app is active and the chat is at the bottom.
5. **Read positions:**
   - kept in the core's settings under `mobile.read.<serverKey>`, a JSON object of channel id → message id;
   - they only move forward;
   - writes are batched to at most one per **2 s** per server.
6. **Marking read:** when the open chat shows its newest message with the app active, the position moves to that message and `unread` and `mentions` drop to 0. "Mark as read" on a server does that for all its channels.

### 6.3 Messages

Per channel, keyed `serverKey/channelId`:
- **`items`:** loaded messages, ascending by id, never duplicated (merged by id).
- **`hasOlder`**, **`loadingOlder`** and **`loaded`**.
- **`pending`:** sends that haven't been confirmed, keyed by nonce: `{ nonce, content, files, state: 'sending' | 'failed', error? }`.

Rules:
- **Loading never replaces what arrived meanwhile.** A first page or an older page is merged into `items` by id, never assigned over them.
- **A confirmed send shows once, whichever comes first:** the `MessageCreated` echo carrying the nonce, or `messageSend` resolving. Whichever comes first moves the message into `items` and drops the pending entry; the second finds it there.
- **A closed channel keeps only its newest 100 messages.** `hasOlder` is set again so scrolling up fetches the rest.
- **Deleting a message** removes it. If it was `last`, `last` falls back to the newest loaded message or none.

### 6.4 Voice, music and preferences

- **`myVoice`:** `{ serverKey, channelId, link: VoiceLinkState, muted, deafened } | null`, set by `voiceJoin` and `VoiceLink`, cleared by `voiceLeave` or a `VoiceStateChanged` for `this_device` with no channel.
- **`speaking`:** per `serverKey/channelId`, the ids speaking now, from `Speaking`.
- **`music`:** per `serverKey/channelId`, the session from `musicSubscribe` and `MusicSession`. A session whose `version` is lower than the one held is ignored.
- **`uploads`:** per upload id, `{ serverKey, channelId, filename, sentBytes, totalBytes, phase, path }`, from the upload calls and `MusicUpload`.
- **`prefs`:**
  - `theme` (`system` | `light` | `dark`), in setting `mobile.theme`;
  - `music: { volume, muted, duck }`, in setting `mobile.musicListening` as JSON. The defaults are the core's own: **0.5, false, false**. They are applied with `musicSetListening` at start and on every change.
- **Toasts:** a short queue of `{ id, text, tone: 'neutral' | 'error' }`, each shown for **3 s**.

### 6.5 The display name

The name lives on this phone: SecureStore holds the secret, and the setting `mobile.displayName` holds the name. As on desktop (`_catchUpName`), after each `Ready` the app sends `profileUpdate` if that server knows the user by another name. Changing the name in the You tab saves it and sends it to every connected server.

## 7. Navigation: the owner's scheme

### 7.1 Routes

| Route (`mobile/src/app/`) | What | Presentation |
|---|---|---|
| `_layout.tsx` | Providers, fonts, theme, the boot gate | Root native stack |
| `(onboarding)/index.tsx`, `create.tsx`, `import.tsx` | Welcome, create an identity, import a backup | Stack, no header |
| `(tabs)/_layout.tsx` | `NativeTabs` with **Servers** and **You** | Native tab bar |
| `(tabs)/servers/index.tsx` | The server list | Native header, large title, search bar |
| `(tabs)/you/index.tsx`, `(tabs)/you/name.tsx` | The You tab, and changing the name | Native header, large title |
| `add-server.tsx` | Add a server | Form sheet |
| `server/[serverKey]/index.tsx` | Inside a server: channels, chat, voice | Pushed full screen; the tab bar hides |
| `server/[serverKey]/music/[channelId].tsx` | The music player | Form sheet, detents 0.6 and 1.0, grabber |
| `server/[serverKey]/member/[userId].tsx` | A person in voice: volume, mute for me | Form sheet, fitted to its content |
| `viewer.tsx` | An image, full screen (params `serverKey`, `attachmentId`) | Transparent full-screen modal |
| `dev/gallery.tsx` | Every UI primitive, for screenshots | Development builds only |

- **Schemes:**
  - the app's own deep links use `opencord-app://` (routes and screenshots);
  - `opencord://` and `opencords://` invite links are rewritten in `+native-intent.tsx` to `/add-server?link=<the link>`.
- **Opening a channel directly:** `server/[serverKey]` takes an optional `channel` parameter. The connected-voice bar uses it.

### 7.2 Inside a server: channels, then chat with the channels on the left

**On a phone** (window narrower than **768 pt**), the server screen is two panes in one drawer layout: Gesture Handler's `ReanimatedDrawerLayout` with `drawerType="slide"`, so the channel list and the content move together.

```
 state: channels                      state: content (sidebar closed)      sidebar open over content
┌──────────────────────────┐        ┌──────────────────────────┐        ┌───────────────────┬──────┐
│ ‹ Servers  Night Owls  … │        │ ☰  # general             │        │ ‹  Night Owls   … │ # ge │
├──────────────────────────┤        │    The usual chatter     │        ├───────────────────┤ ▒▒▒▒ │
│ # general            ●   │        ├──────────────────────────┤        │ # general       ● │ ▒▒▒▒ │
│ # music-requests    @2   │  tap   │  Kai                     │ swipe  │ # music-requests  │ ▒▒▒▒ │
│ TEXT                     │ ─────▶ │ ┌──────────────┐         │ ─────▶ │ VOICE             │ ▒▒▒▒ │
│ # off-topic              │ shift  │ │ evening all  │ 21:04   │  right │ 🔊 Lounge         │ ▒▒▒▒ │
│ VOICE                    │        │ └──────────────┘         │        │   ◉ Kai           │ ▒▒▒▒ │
│ 🔊 Lounge            2   │        │      ┌───────────────┐   │        │   ○ Mira          │ ▒▒▒▒ │
│    ◉ Kai                 │        │      │ hey! 🎶 queue │   │        │   ♪ Song — Artist │ ▒▒▒▒ │
│    ○ Mira                │        │      └───────────────┘   │        │                   │ ▒▒▒▒ │
│    ♪ Song — Artist       │        ├──────────────────────────┤        │                   │ ▒▒▒▒ │
│                          │        │ ＋ [ Message #general ] ⬆│        │                   │ ▒▒▒▒ │
└──────────────────────────┘        └──────────────────────────┘        └───────────────────┴──────┘
```

The server screen is in one of two states:

- **`channels`:**
  - the channel list fills the screen: the drawer is open at the full window width and locked open;
  - its header has the native-looking back button to Servers;
  - the stack's swipe-back gesture is on;
  - entering a server always starts here.
- **`content`:** a text or voice channel is open.
  - **Getting there:** tapping a channel puts it in the content pane and closes the drawer, so the list slides out to the left as the chat slides in from the right. This is the owner's "shift".
  - **Once closed:** the drawer becomes a sidebar `min(86 % of the width, 360 pt)` wide and unlocked.
  - **Opening the sidebar:** a swipe right from anywhere in the left half, or the sidebar button in the pane header. The content stays visible on the right under the `scrim` color at 40 %; tapping it closes the sidebar.
  - **What the sidebar shows:** the same channel list, with the open channel highlighted. **It never lists servers**; its header names this server and goes back to the full list.
  - **Gestures:** the stack's swipe-back gesture is off here, so a swipe from the left edge opens the sidebar instead of leaving the server.

Back, on either system, moves one step out:

| Where | Android back, iOS header back | Result |
|---|---|---|
| `content`, sidebar open | back | Close the sidebar |
| `content`, sidebar closed | back | `channels` (the drawer widens and opens: the reverse shift) |
| `channels` | back | Leave the server screen (to Servers) |

**On a tablet** (**768 pt** or wider), there is no drawer:
- the channel list is a fixed **320 pt** sidebar and the content fills the rest;
- with no channel open, the content says "Choose a channel";
- back always leaves the server screen.

The panes draw their own headers (`PaneHeader`), so a header moves with its pane during the shift. Inside the panes, everything native stays native: lists, text input, menus and sheets.

## 8. Screens

The visual tokens they use are in §9. Every screen works in light and dark, at the system's text size, and at phone and tablet widths.

### 8.1 Starting and onboarding

- **Boot:**
  1. The screen is the `chat` color with the wordmark **Opencord** (Inter Bold 34) centered, while the core opens.
  2. It reads the secret from SecureStore and the name from `mobile.displayName`.
  3. With both, it loads the identity and goes to Servers. Otherwise it goes to Welcome.
  4. A failure shows its sentence with a **Try again** button.
- **Welcome:**
  - the wordmark;
  - under it, "Chat and voice on servers you trust." (callout, `textSecondary`);
  - at the bottom, a primary button **Create an identity** and a secondary **Import a backup**;
  - a footnote: "Your identity is a key that stays on this phone."
- **Create:**
  - "What should people call you?" (title);
  - a text field (1–32 characters after trimming, no control characters: the core's display-name rule);
  - **Continue** generates the identity, saves the secret and the name, and loads them.
- **Import:**
  1. The document picker opens.
  2. The file's text goes through `identityBackupDecode`.
  3. The name step follows, empty.
  4. A wrong file says "That isn't an Opencord backup."

### 8.2 Servers (tab)

A native header with the large title **Servers**, a native search bar (it filters by name), and a **+** button that opens Add server.

Each row (**72 pt** tall), like a WhatsApp chat:

| Part | Content |
|---|---|
| Left | The server icon, 52 pt with a 14 pt corner radius: its picture (`avatarFile` of `icon_hash`), else the first two letters of its name on an `avatarShades` color picked by the name's hash. |
| Title line | Name (headline). Time of the last activity on the right (footnote, `textMuted`): today `21:04`, yesterday "Yesterday", within 6 days the weekday ("Mon"), else the short date, all in the device's locale. |
| Second line | The preview (callout, `textMuted`, one line): `#general · Kai: evening all`, with "You" for the user's own messages. Markdown is stripped and mentions are resolved to names. An attachment-only message reads "Photo" or "File". Then, by state: "No messages yet"; "Connecting…"; "Reconnecting…"; a failure's sentence in `recording`. |
| Right of the preview | An unread badge with the count, or with "@" and the mentions count when there are any. A voice glyph in `voice` when the user is in voice here. |

- **Order:** the latest activity first; servers with none after them, by name.
- **Swipes:**
  - leading **Read** marks the whole server read;
  - trailing **Leave** asks "Leave Night Owls? You can come back with an invite." with **Leave** in red.
- **Long press:** a native menu with Mark as read, Retry now (when not connected), Copy address and Leave server.
- **Pull to refresh:** retries every server that isn't connected.
- **Empty:** "No servers yet" and "Add one with an invite link or its address." with an **Add a server** button.
- **When in voice:** the connected bar (§8.6) sits above the tab bar.

### 8.3 Add server (sheet)

- **The fields:**
  - "Invite link or address" (`opencord://…` or `host:port`), with a paste button;
  - a collapsed "I own this server" row that shows "Claim token".
- **Add:**
  - shows progress, then closes the sheet on success; the new row appears as it connects;
  - errors appear under the field in `recording`.
- **The trust prompt:**
  - "Trust this server?";
  - body: "Its certificate isn't from a public authority. Check this fingerprint with the server's owner.";
  - the fingerprint in JetBrains Mono, in groups of four;
  - **Trust and add** or **Cancel**.

### 8.4 The channel list

Used both full screen (`channels`) and as the sidebar. Pane header: back, the server's name, the connection state as a subtitle when not connected, and a menu (Mark all as read, Copy address, Leave server).

- **Order:**
  - channels without a category first;
  - then each category in position order, with its name as an uppercase caption header (`textMuted`).
- **Visibility:** channels the user can't view are left out.
- **Text channel row** (44 pt):
  - a `#` icon and the name;
  - unread: the name in weight 600 in `text`, with a 4 × 8 pt pill at the left edge;
  - mentions: a count badge;
  - read: `textSecondary`, weight 500;
  - the open channel (in the sidebar): a `selected` background with an 8 pt corner radius.
- **Voice channel row** (44 pt): a speaker icon, the name, and on the right the number connected (and "/limit" when it has one). Under it:
  - **People in it** (32 pt rows, indented): a 22 pt avatar with a 2 pt `voice` ring while speaking, the name (`text` while speaking, `textSecondary` otherwise), and mute or deafen glyphs.
  - **What plays:** "Song — Artist" with a music note.
- **Tapping:**
  - a text channel opens the chat;
  - a voice channel opens the voice pane;
  - neither joins anything.

### 8.5 Chat

**Pane header:**
- the sidebar button;
- `# name`, with the topic as a one-line subtitle;
- a "Not connected" subtitle while the server isn't connected.

**Messages**, as bubbles, like Telegram:
- **Incoming:** `bubbleIn`, on the left.
  - The author's 32 pt avatar sits beside the last bubble of their group.
  - Their name is on top of the first bubble (subhead, weight 600).
- **The user's own:** `bubbleOut`, on the right, with no avatar or name.
- **Groups:** consecutive messages by the same author within **5 minutes**.
- **Shape:**
  - bubble corners are 18 pt;
  - between bubbles of one group, the corner on the author's side is 6 pt (the top corner for all but the first, the bottom corner for all but the last);
  - a bubble is at most **78 %** of the pane wide.
- **The time** (and "edited") sits at the bottom right, inside the bubble's last line where it fits (the desktop's "trailing" trick), in footnote `textMuted`.
- **Separators:**
  - day separators as small centered pills ("Today", "Yesterday", "Monday 26 October");
  - a "New messages" divider above the first unread message when the chat opens.
- **Text** is the desktop's own markdown subset, parsed by a port of `app/lib/features/chat/markdown.dart`:
  - paragraphs with their line breaks, `> quotes` and fenced code;
  - bold, italic, strike and inline code;
  - links, `<@user>` mentions and `<#channel>` links;
  - nothing else becomes formatting: `# heading` stays text, as on desktop.

  It renders as nested native `Text`, so it is selectable. Mentions are weight 600 on `mentionBg`. Code is JetBrains Mono on `mentionBg`. Links are underlined, the same color as the text. Tapping a channel link opens that channel.
- **Attachments:**
  - **Images and animated images:** the thumbnail, fitted within **260 × 320 pt** with a 12 pt corner radius (a `surface` placeholder at the thumbnail's size while it loads). A tap opens the viewer.
  - **Other files, video and voice messages:** a card with an icon, the filename, the size and the kind ("Video", "Voice message, 0:12"). A tap downloads the file (progress on the card) and opens the system share sheet.
  - Playing video and voice messages in the app comes in the second plan.
- **Pending sends** show at once with a clock glyph. A failed one turns red with "Not sent. Tap to retry", and its long-press menu has Retry and Delete.
- **History:**
  - scrolling near the top loads **50** older messages, with a small spinner, without moving what's on screen;
  - **When scrolled up more than one screen:** a round "jump to latest" button appears at the bottom right, with the count of new messages since.
- **Typing:** "Kai is typing…", "Kai and Mira are typing…" or "Several people are typing…", on a line above the composer.
- **Long press on a bubble:** a native menu with Copy text, Edit (own messages), Delete (own, or anyone's with Manage messages; it asks first).
  - **Editing** puts the text in the composer, with an "Editing message" bar and a cancel button.

**Composer:**
- a rounded multiline field (up to 6 lines, then it scrolls), "Message #general";
- a **+** button with a native menu: Photo library, Files;
- a send button (an up arrow in an `accent` circle) when there is text or a file;
- picked files show as removable chips above the field, with upload progress once sent;
- from 3,900 characters a counter shows how many are left, and above **4,000** (the core's limit) the send button is off;
- no `send` permission: the composer is replaced by "You can't send messages here.";
- the keyboard pushes the composer up and the list keeps its place (`react-native-keyboard-controller`);
- **Typing** is sent at most every **8 s** while the user types.

**The image viewer:**
- black background;
- pinch and double-tap to zoom, pan when zoomed;
- swipe down to close;
- a share button and a close button at the top, fading in and out with a tap.

### 8.6 Voice

**Pane header:** a speaker icon and the name. On the right, **Music**, shown when music is supported and enabled on the server.

**Not connected:**
- tiles of who is in the channel (or "No one's here yet");
- at the bottom, a **Join voice** button: full width, 52 pt tall, `voice` background, white text;
- it is disabled with a reason when the user lacks `connect` ("You can't join this channel") or the channel is full ("This channel is full").

**Connected:**
- **Tiles:**
  - two columns on a phone, more on a tablet;
  - each a `surface` square with a 16 pt corner radius, a 64 pt avatar, the name and status glyphs;
  - a 3 pt `voice` ring while speaking;
  - the user's own tile first.
- **Control bar:** four 56 pt round buttons: **Mute**, **Deafen**, **Music** and **Leave**.
  - Toggles are `surface` with `text` when off, and `accent` with `onAccent` when on.
  - Leave is `recording`.
- **A link banner** at the top while the link isn't connected:
  - "Connecting…" (`AwaitingEndpoint`, `Authenticating`, `RtcConnecting`);
  - "Reconnecting…";
  - `NoRoute` as "Can't reach the voice server. Your network may block it.";
  - `Disconnected` as "Disconnected from voice." with **Rejoin**.
- **A tap on a person** opens their sheet: avatar, name, a **Volume** slider (0–200 %, 100 % marked) and a **Mute for me** switch. The user's own sheet says "This is you."

**The connected bar:**
- **Where:** while in voice anywhere, a 56 pt bar sits above the tab bar on Servers and at the bottom of the server screen's panes.
- **Left:** a `voice` speaker glyph, the channel's name, and under it the server's name, or "♪ Song — Artist" when music plays.
- **Right:** mute and leave buttons.
- **A tap** opens that voice channel.

**Microphone permission:**
- the first Join asks for it;
- refused, it says "Opencord needs the microphone for voice. You can allow it in Settings." with **Open Settings**.

### 8.7 The music player (sheet)

```
┌────────────────────────────────┐
│              ───               │  grabber
│        ┌──────────────┐        │
│        │   artwork    │        │  min(280, width − 64), corner 16
│        └──────────────┘        │
│   Song title that may wrap     │  title2
│   Artist                       │  callout, textSecondary
│   Added by Kai · YouTube       │  footnote, textMuted
│   ──────────●───────────────   │  native slider
│   1:12                  -2:41  │
│   ⤮      ⏮     ( ▶ )     ⏭   ↻ │  play/pause 64 pt accent circle
│   🔈 ───────●─────── 120 %     │  volume 0–200 %, step 5
│   Lower music while people talk ◯ │
│ [ Up next | Search | Upload | History ]
│   …section…                     │
└────────────────────────────────┘
```

- **The player:**
  - it subscribes (`musicSubscribe`) when it opens and unsubscribes when it closes;
  - the position moves on every **500 ms** while playing, by the desktop's `positionAt`: the session's position, plus (now − anchor) while playing and after the anchor, capped at the track's length;
  - seeking needs a seekable track and `music_control`.
- **Controls:**
  - **Previous, play or pause, next:** need `music_control`. Without it, next becomes **Vote to skip (2/3)** when vote skip is on.
  - **Shuffle and repeat** (repeat cycles off → queue → track; track shows "1"): also need `music_control`.
  - A control the user may not use stays visible, dimmed, and says why when tapped ("You need Control music for this.").
- **Volume:**
  - the speaker button toggles mute;
  - the slider sets the volume, showing a percentage;
  - the switch "Lower music while people talk";
  - the same settings appear in the You tab.
- **Not in this channel's voice:**
  - "Listen by joining Lounge" with **Join**;
  - the controls still show and work for those allowed. Music plays to the channel whether or not this user listens.
- **Up next:**
  - **Rows:** 56 pt, with 40 pt artwork, the title, "artist · 3:41" and the requester's avatar.
  - **Swipe to remove:** the user's own items with `music_add`, any item with `music_manage_queue`.
  - **Long-press menu:** Play next, Move up, Move down, Remove, Copy link.
  - **Clear:** in the section's header, with `music_manage_queue`; it asks first.
  - **Empty:** "Nothing queued. Search for a song or upload one."
- **Search:**
  - a search field ("Search songs or paste a link") with a native source menu from `available_sources`;
  - it searches when submitted, not while typing;
  - result rows look like the queue's, with a **Local** pill on uploaded songs;
  - a tap adds the song at the end (a light haptic and "Added to the queue");
  - a long press offers Play next and Play now (with `music_manage_queue`);
  - needs `music_add`.
- **Upload** (the owner's "section for upload custom audio"):
  - **Choose audio files:** a large button with a dashed `border` outline (56 pt tall, 14 pt corners), opening the system document picker for audio, several files at once.
  - **The help line:** "MP3, FLAC, M4A, AAC, OGG or WAV. Songs you upload stay on this server: find them in Search, marked Local."
  - **Each file** gets a row: the filename, a status line and a thin progress bar. The status reads:
    - "Checking…" (`Preparing`);
    - "Uploading · 42 % of 8.4 MB";
    - "The server is checking it…" (`Processing`);
    - "Queued" (`Done`), after which the row fades out within **3 s**;
    - "Failed: <the server's reason>" in `recording`;
    - "Cancelled".
  - **Row buttons:** a cancel button while active; retry and dismiss after a failure.
  - Files go to the end of the queue. Every file uploads at once, as on desktop.
  - The picker copies each file into the cache. The copy is deleted once the song is queued, cancelled or dismissed.
  - **Who can upload:** `music_add` and `attach`, while connected to this channel's voice, or with `music_manage`. Otherwise the section says what's missing ("Join Lounge to upload songs.").
- **History:** the last 50 tracks, each with an **Add again** button.
- **Notices** (`MusicNotice`) show as toasts.

### 8.8 You (tab)

An inset grouped list under the large title **You**:
- **The header:**
  - the user's avatar, 72 pt, with initials;
  - their name (title2);
  - the identity fingerprint in mono footnote (a tap copies it).
- **Profile:** Display name → its own screen, with a field and **Save**.
- **Identity:** **Back up your identity**.
  1. It writes the backup text to `opencord-identity.txt` in the cache.
  2. It opens the share sheet, under the warning "This file is your identity. Anyone who has it can be you. Keep it somewhere safe."
- **Appearance:** Theme, as a native segmented control: System, Light, Dark.
- **Music:** volume slider, Mute music, Lower music while people talk.
- **About:** the app version and the core's version.

## 9. Visual design

### 9.1 Color tokens

The desktop's `OcColors` (`app/lib/ui/theme/oc_colors.dart`), value for value, in `src/ui/theme/tokens.ts`:

| Token | Dark | Light | Used for |
|---|---|---|---|
| `rail` | `#0A0A0B` | `#E9E9EC` | Tab bar background where the system lets us set it |
| `sidebar` | `#121214` | `#FFFFFF` | Servers list, channel list, headers, composer bar |
| `chat` | `#0E0E10` | `#F3F3F5` | Chat background, boot screen |
| `surface` | `#18181B` | `#FFFFFF` | Tiles, cards, file cards, placeholders |
| `elevated` | `#1F1F23` | `#FFFFFF` | Sheets, toasts |
| `hover` | `#1C1C20` | `#F2F2F4` | Input fills |
| `selected` | `#27272C` | `#E7E7EA` | Pressed and selected rows |
| `border` | `#222226` | `#E3E3E7` | Hairlines, outlines |
| `text` | `#EDEDEF` | `#111114` | Primary text and icons |
| `textSecondary` | `#A1A1A8` | `#55555C` | Secondary text, inactive icons |
| `textMuted` | `#6E6E76` | `#8A8A92` | Times, hints, previews, section labels |
| `accent` | `#EDEDEF` | `#111114` | Badges, the send button, toggled controls |
| `onAccent` | `#0E0E10` | `#FFFFFF` | Foreground on `accent` |
| `bubbleIn` | `#1A1A1E` | `#FFFFFF` | Others' bubbles |
| `bubbleOut` | `#2A2A30` | `#E4E4E8` | Own bubbles |
| `mentionBg` | `#26262B` | `#E2E2E6` | Mentions, inline code |
| `scrim` | `#000000` at 70 % | `#000000` at 40 % | Behind the open sidebar, sheets |
| `avatarShades` | `#2B2B30`, `#34343A`, `#3E3E45`, `#494951`, `#55555D` | `#D9D9DE`, `#CDCDD3`, `#C1C1C8`, `#E3E3E7`, `#B6B6BE` | Initials backgrounds |
| `voice` | `#4C8DFF` | `#2F6FEB` | Speaking rings, Join, voice glyphs |
| `recording` | `#F2555A` | `#DC3E42` | Leave, failures, destructive actions |

No other color appears in the app; a test fails on any color literal outside `tokens.ts`. Images and the viewer draw on the dark set whatever the theme.

### 9.2 Type

The fonts are copied from `app/assets/fonts` and loaded with the `expo-font` config plugin:
- Inter: Regular 400, Medium 500, SemiBold 600, Bold 700;
- Inter Italic and SemiBold Italic, for markdown's italics: italic uses Italic, and bold italic uses SemiBold Italic, as on desktop;
- JetBrains Mono Regular.

| Style | Size / line height (pt) | Weight | Used for |
|---|---|---|---|
| `display` | 34 / 41 | 700 | The wordmark |
| `title1` | 28 / 34 | 700 | Onboarding titles |
| `title2` | 22 / 28 | 600 | The track title, profile name |
| `headline` | 17 / 22 | 600 | Row titles, pane header titles |
| `body` | 16 / 22 | 400 | Messages, inputs |
| `callout` | 15 / 20 | 400 | Previews, secondary lines |
| `subhead` | 14 / 19 | 600 | Author names, buttons in rows |
| `footnote` | 13 / 18 | 400 | Times, meta, status lines |
| `caption` | 12 / 16 | 600, letter spacing 0.4, uppercase | Section and category headers |
| `mono` | 14 / 20 | 400 | Code, fingerprints |

Text follows the system's size setting, up to **1.6×**. Numbers that change in place (times, percentages) use tabular figures.

### 9.3 Space, shape and motion

- **Spacing** steps: 4, 8, 12, 16, 20, 24, 32. Screen edges are 16.
- **Corner radii:**
  - 6 for pills;
  - 8 for selected rows;
  - 12 for images;
  - 14 for icons and big buttons;
  - 16 for tiles and artwork;
  - 18 for bubbles;
  - 20 for sheets;
  - full circles for avatars and round buttons.
- **Hairlines:** `StyleSheet.hairlineWidth` in `border`. In lists they start after the leading icon, as on iOS.
- **Shadows:** none. Depth comes from the surface colors and the system's own sheets and headers.
- **Motion** (Reanimated 4, per the Software Mansion animations skill):
  - **Press feedback:** a CSS transition to `selected` over **80 ms**.
  - **Badges:** scale in over **120 ms** (`ease-out`).
  - **Live messages:** a message arriving while the chat is at the bottom fades in over **120 ms**; history never animates.
  - **Speaking rings:** fade over **120 ms**.
  - **Upload rows:** fade out over **300 ms**.
  - **The drawer** uses Gesture Handler's own spring.
  - **Reduced motion:** with Reduce Motion on, durations drop to 1 ms.
- **Haptics** (`expo-haptics`):
  - light: sending a message, adding a song;
  - medium: joining or leaving voice;
  - none on scrolling or ordinary taps.

### 9.4 Icons

`expo-symbols`: SF Symbols on iOS and Material Symbols on Android. Only the names in this table are used, through one `Icon` component that maps a name to both.

| Name | iOS (SF) | Android (Material) |
|---|---|---|
| `servers` | `bubble.left.and.bubble.right` | `forum` |
| `you` | `person.crop.circle` | `account_circle` |
| `add` | `plus` | `add` |
| `textChannel` | `number` | `tag` |
| `voiceChannel` | `speaker.wave.2` | `volume_up` |
| `sidebar` | `sidebar.left` | `menu` |
| `back` | `chevron.left` | `arrow_back` |
| `more` | `ellipsis` | `more_horiz` |
| `search` | `magnifyingglass` | `search` |
| `send` | `arrow.up` | `arrow_upward` |
| `photo` | `photo` | `image` |
| `file` | `doc` | `description` |
| `copy` | `doc.on.doc` | `content_copy` |
| `edit` | `pencil` | `edit` |
| `delete` | `trash` | `delete` |
| `close` | `xmark` | `close` |
| `check` | `checkmark` | `check` |
| `clock` | `clock` | `schedule` |
| `warning` | `exclamationmark.triangle.fill` | `warning` |
| `lock` | `lock.fill` | `lock` |
| `mic` | `mic.fill` | `mic` |
| `micOff` | `mic.slash.fill` | `mic_off` |
| `deafen` | `speaker.slash.fill` | `headset_off` |
| `leave` | `phone.down.fill` | `call_end` |
| `music` | `music.note` | `music_note` |
| `play` | `play.fill` | `play_arrow` |
| `pause` | `pause.fill` | `pause` |
| `next` | `forward.fill` | `skip_next` |
| `previous` | `backward.fill` | `skip_previous` |
| `shuffle` | `shuffle` | `shuffle` |
| `repeat` | `repeat` | `repeat` |
| `repeatOne` | `repeat.1` | `repeat_one` |
| `upload` | `square.and.arrow.up` | `upload` |
| `retry` | `arrow.clockwise` | `refresh` |
| `volume` | `speaker.wave.2.fill` | `volume_up` |
| `volumeOff` | `speaker.slash.fill` | `volume_off` |
| `share` | `square.and.arrow.up` | `share` |
| `jumpDown` | `chevron.down` | `keyboard_arrow_down` |

### 9.5 Words

- **Case and punctuation:** sentence case everywhere, no exclamation marks, and no emoji in the app's own text.
- **Names and buttons:** a person is called by their nickname, else their display name. Buttons say what they do ("Leave", not "OK").
- **Errors:** they say what happened and what to do, in the sentences of §8 and `src/core/errors.ts`.

## 10. Native components

Expo's and React Native's own native pieces are used wherever they exist:

| Need | Component |
|---|---|
| Screens, headers, large titles, search bar, swipe back | `expo-router` `Stack` (react-native-screens native stack) |
| Tabs | `expo-router/native-tabs` (`NativeTabs`: UITabBarController, BottomNavigationView) |
| Sheets | Native stack `formSheet` presentation with detents |
| Menus (long press and buttons) | `@react-native-menu/menu` (`UIMenu`, `PopupMenu`) |
| Sliders and switches | `@expo/ui`: `swift-ui` `Slider` and `Toggle` on iOS, `jetpack-compose` `Slider` and `Switch` on Android, each in a `Host` |
| Segmented controls | `@react-native-segmented-control/segmented-control` |
| Confirmations | `Alert.alert` (system dialogs) |
| Lists | `@shopify/flash-list` v2 (chat: `maintainVisibleContentPosition` with `startRenderingFromBottom`, no `inverted`) |
| Text input, keyboard | `TextInput`; `react-native-keyboard-controller` |
| Gestures, drawer, swipe actions | `react-native-gesture-handler` (`ReanimatedDrawerLayout`, `ReanimatedSwipeable`) |
| Images | `expo-image` |
| Pickers, sharing, clipboard | `expo-document-picker`, `expo-image-picker`, `expo-sharing`, `expo-clipboard` |
| Secrets | `expo-secure-store` |
| Audio session, microphone permission | `expo-audio` (`setAudioModeAsync`, `requestRecordingPermissionsAsync`) |

Rows, bubbles, avatars, badges, tiles and player buttons are React Native views drawn with the tokens. They have no system equivalent.

## 11. Platform details

- **App ids:** `dev.opencord.mobile` on both systems; the display name is "Opencord". The Flutter app keeps `dev.opencord.opencord`, so a desktop build and a phone build never collide.
- **Icon:**
  - "There is no logo yet" (the owner, for the updater), so the icon is the letters "oc" in Inter Bold, `#EDEDEF` on `#0E0E10`, centered;
  - it is drawn from `assets/icon.svg` by a script, with the Android adaptive icon from the same picture;
  - the splash screen is the `chat` color with no image, and the boot screen draws the wordmark.
- **Permissions:**
  - **iOS:** `NSMicrophoneUsageDescription` ("Opencord uses the microphone in voice channels."), `NSPhotoLibraryUsageDescription` ("Opencord lets you send photos from your library."), and `UIBackgroundModes: [audio]`.
  - **Android:** `INTERNET`, `RECORD_AUDIO`, `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_MICROPHONE`, `FOREGROUND_SERVICE_MEDIA_PLAYBACK`, `POST_NOTIFICATIONS`, `WAKE_LOCK`. The release manifest gets `INTERNET` too: the Flutter scaffold has it only in debug.
- **The audio session:** before joining voice, `setAudioModeAsync({ allowsRecording: true, playsInSilentMode: true, shouldPlayInBackground: true, interruptionMode: 'doNotMix' })`. The core then opens the microphone and speaker through cpal (AAudio on Android, CoreAudio on iOS).
- **The background:**
  - **iOS:** the `audio` background mode keeps a call alive while the screen is off.
  - **Android:** a foreground service runs while connected to voice. It is a small local Expo module, `@opencord/voice-service` in Kotlin, of types `microphone|mediaPlayback`. Its ongoing notification ("In voice: Lounge · Night Owls") opens the app.
  - **On return:** when the app comes back to the foreground, every server that isn't connected is retried at once.
- **Links:** `opencord://` and `opencords://` invites open Add server with the link filled in.

## 12. Building, CI and testing

- **Building the core for a phone:**
  - **The library:** `mobile/packages/core-native` is a turbo-module library made with `create-react-native-library` (Turbo module, C++ for Android and iOS) and configured by `ubrn.config.yaml`, which points at `crates/opencord-mobile`.
  - **Its build:** `ubrn build android|ios --and-generate` builds the Rust and regenerates the bindings.
  - **The generated files** are committed, as the Flutter bindings are, so screen work doesn't need a Rust toolchain. A check fails if they are out of date.
- **Running locally:**
  - `yarn core:android` builds `x86_64` only;
  - `yarn android` builds and starts the development build on the `Pixel_7` emulator;
  - **against a real server:** a local `opencord-server`, reached from the emulator at `10.0.2.2`;
  - **with no server:** `EXPO_PUBLIC_FAKE_CORE=1` runs the app on `FakeCore`'s fixed world.
- **Tests:**
  - **Rust:** the facade's conversions and id parsing, and its events against a real `opencord-server` in the same process, as `opencord-core`'s tests do.
  - **Jest:**
    - `jest-expo` with React Native Testing Library;
    - the reducer, the unread rules, the selectors and the markdown port (with the desktop's test cases, case for case);
    - the screens' logic against `FakeCore`.
  - **Screenshots:**
    - `mobile/scripts/screens.sh` opens each screen of the fake world by deep link on the emulator, in dark and light, at phone width (`Pixel_7`) and tablet width (a `Pixel_Tablet` emulator made from the same system image);
    - it saves PNGs under `.superpowers/sdd/mobile/screens/`;
    - every UI task is checked by looking at them.
- **CI** (`.github/workflows/cicd.yml`), on pull requests and pushes like every other job:
  - **`mobile-android`** (Ubuntu):
    - Jest, `tsc` and ESLint;
    - the binding freshness check;
    - the Rust library in release for `arm64-v8a` and `x86_64`;
    - `expo prebuild`;
    - a release APK, signed with the debug key unless the repository has a release key;
    - uploaded as an artifact.
  - **`mobile-ios`** (macOS): the Rust library for the device and the simulator, `expo prebuild`, `pod install`, and an `xcodebuild` for the simulator without signing. The `.app` is uploaded as an artifact.
- **`.github/check.sh mobile`:** the facade's clippy and tests, the binding check, and Jest, `tsc` and ESLint.

## 13. Calls made without the owner

These are the implementer's calls. Each can be changed before the plan starts.

1. **Expo with Expo Router, not bare React Native CLI.** Expo is the framework React Native's own docs recommend. It brings native tabs (stable in SDK 58), native sheets, `@expo/ui` controls and config plugins for the permissions. Development builds load the Rust module.
2. **A facade crate, not UniFFI attributes on the core's types.** The core stays as it is for Flutter. JavaScript gets string ids, number times and named permission flags instead of `bigint`s and bit masks.
3. **Events pulled in batches** instead of callbacks, to avoid blocking Rust threads on JavaScript, with a generation number so a JavaScript reload can take over the stream.
4. **The desktop's markdown subset, ported, instead of `react-native-enriched-markdown`.** CommonMark would make headings and lists the desktop shows as text, and would fold single line breaks into spaces. The phone would then show other people's messages differently from the desktop.
5. **The sidebar is a sliding drawer on phones and a fixed column on tablets.** This is my reading of "shift the channels list on the left sidebar". The channel list slides away as the chat comes in, and slides back with a swipe right.
6. **Bubbles, like WhatsApp and Telegram**, with the desktop's bubble colors.
7. **The Upload section has no "Your songs" list.** A member can't list their own uploads today: search finds them, and the full list is for administrators only. Showing one needs a small server change (a "my songs" request and queueing by song id), left for later. Re-uploading a file the server has queues it at once, without uploading.
8. **Android ships 64-bit only.**
9. **The app id is `dev.opencord.mobile`**, separate from the Flutter app's.
10. **Attachments are cached in plain files** in the app's private cache.
11. **Voice keeps running in the background**, with a foreground service on Android. Without it, locking the screen would end every call.
12. **Zustand for the store**, the smallest common choice, with selector hooks.
13. **Yarn 1 workspaces:** the Yarn already installed.

## 14. Later

- **The second plan:**
  - the camera, screen sharing and watching streams;
  - video tiles drawn by native views fed from the core;
  - VideoToolbox (iOS) and MediaCodec (Android) codec back ends in `opencord-media`.
- **After that:**
  - private messages and friends;
  - push notifications (a relay on the server);
  - voice messages and in-app video playback;
  - server administration;
  - the soundboard;
  - chat commands;
  - a "Your songs" list (§13, item 7);
  - store releases with signing and review.
