# Media uploads, S3 storage and voice messages: design

Date: 2026-10-18. Status: agreed in conversation with the owner. The plan is
written from this document and handed to a cheaper implementer model, so the
plan, not this file, carries file-by-file instructions.

## 1. Intent

Opencord has no file sharing yet: the attach button and the drop zone say
"coming later", and the mic button in the composer shows a toast. This
project adds the following:

- **File attachments.** Every server stores files on its own disk by default.
  The owner can connect S3-compatible buckets instead.
- **Encryption at rest.** Every stored byte is encrypted. A leaked bucket, a
  copied attachments folder, or a backup made without `secrets.key` gives
  nothing away. Protecting against a stolen server disk also needs
  `secrets.key` kept off that disk (§5.2 makes its location configurable).
- **In-app only.** Files are only ever served to members through the app.
  There are no public links. Members view everything inside the app (images,
  video, audio, voice) and can save a copy with "Save as".
- **Role-based upload limits.** @everyone defaults to 50 MB. A member's limit
  is the highest of their roles' limits.
- **Blocked extensions.** A server-wide list, empty by default.
- **Voice messages.** Hold the mic button, watch a blue live waveform replace
  the text field, and release to send. Recipients see a waveform bubble they
  can play, seek and speed up.

Why: chat without file sharing isn't a complete Discord replacement, and
self-hosters want their files to stay unreadable outside their own server.

## 2. Goals, non-goals, anti-goals

**Goals**

- Works fully on **Linux, Windows and macOS** desktops (owner's choice).
- Uploads are resumable and survive dropped connections and server restarts.
  They do **not** survive the app being closed: there is no saved outbox, and
  the server's 24 h cleanup removes what was left.
- The server enforces every rule; the client checks the same rules early, for
  a good experience.
- Video, audio and voice play inside the app.

**Non-goals (this phase)**

- End-to-end encryption. The server can read files; that is what makes
  thumbnails and checks possible. The planned MLS work may revisit this.
- Mobile UI.
- Direct messages.
- Spoiler tags.
- Image descriptions (alt text).
- Inline HEIC/AVIF images.
- Adding or removing files when editing a message.
- Per-channel upload limits.
- Storage quotas.
- Hardware video decoding in the player.
- Server-side video transcoding.

**Anti-goals (a failure even if everything "works")**

- Any URL that reaches a file without the app's media token: presigned S3
  URLs given to clients, public buckets, "copy link" or "open in browser".
- Plaintext file bytes, filenames or thumbnails in the bucket or in the
  attachments folder. The database keeps filenames in plain text, as it
  already keeps message text.
- The S3 secret key ever leaving the server after it is entered, or appearing
  in logs.
- A server that buffers a whole upload in memory, or trusts what the client
  says about a file's type, size, duration or waveform.
- Chat scrolling that jumps while attachments load.

## 3. Constraints and existing pieces to build on

- **Server:** axum, sqlx (SQLite), protobuf over the gateway WebSocket, media
  HTTP endpoints in `crates/opencord-server/src/media.rs`. These are
  authenticated with **media tokens** (Ed25519-signed, 24 h, bearer) and
  already serve soundboard sounds, uploads included.
- **Secrets at rest:** `crates/opencord-server/src/secrets.rs` seals values
  with XChaCha20-Poly1305 under `data/secrets.key`. It is reused for file keys
  and S3 secrets.
- **Permissions:** `ATTACH_FILES = 1 << 10` exists but is reserved, and is not
  in `DEFAULT_EVERYONE`. Bits 14 and 15 are free. The Dart app mirrors the
  permission model in `app/lib/core/model/permissions.dart`.
- **Client core:** `crates/opencord-core` talks to servers through pinned TLS
  (TOFU, `tofu.rs`). Its media HTTP (`http.rs`) is a hand-written HTTP/1.1
  client that reads whole bodies with a 30 s timeout. That is fine for sounds,
  but not for large transfers.
- **Audio in the core:** cpal capture and playback, libopus
  (`opus` → `opusic-sys`, built with cmake), and RNNoise and DeepFilterNet
  noise suppression, all in `crates/opencord-media`. The soundboard already
  has an Ogg Opus reader and writer (`opencord-common/src/sound/ogg.rs`) and a
  waveform helper.
- **Video in the core:** frames reach Flutter through irondash textures
  (`crates/opencord-core/src/video*.rs`). FFmpeg is linked through
  `ffmpeg-next` 9 on Linux (an FFmpeg built by `app/tool/build_ffmpeg.sh`
  with only H.264 and MJPEG decoders, D32) and on Windows (full LGPL DLLs).
  **macOS has no FFmpeg** (D34). `ffmpeg-next` 9 has safe custom I/O
  (`format::input_from_stream` with `StreamIo::from_read_seek`), so no unsafe
  FFI is needed.
- **Platform rules:** `docs/plans/platforms.md`. The Linux session owns the
  shared code and the seams. The Mac and Windows sessions fill in only their
  own files and packaging.
- **Design system:** `docs/plans/desktop-ui.md`. No hard-coded colors outside
  the token file. The palette is monochrome today; there is no blue.
- **Dev machine:** the disk is nearly full. Build with `CARGO_INCREMENTAL=0`
  and `CARGO_PROFILE_DEV_DEBUG=0`.
- **Decisions** continue at **D52** in `docs/decisions.md`.

## 4. The owner's decisions

1. **Encryption protects against a leaked disk or bucket.** The key stays on
   the server, and the server can read files.
2. **Video plays in our own player** built on the app's FFmpeg. Linux's build
   gains decoders, Windows already has them, and **macOS gains a decode-only
   FFmpeg** (this reverses part of D34). The player is the last milestone.
3. **All three desktops** must fully work.
4. **Switching storage:** old files stay readable where they are. "Move
   existing files" copies them to the active storage in the background.
5. **Voice waveform:** the app sends audio and its waveform. The server
   **recomputes the waveform from the audio** and stores its own. The app's
   copy is only for the local "sending…" preview.
6. **Red appears only while recording.** A new `recording` token is used
   only for the recording dot and the "Release to cancel" state. Files that
   can't be sent stay in the design system's monochrome error style. Blue
   (`voice`) and this red are the only exceptions to the monochrome rule
   (desktop-ui.md §1). The monochrome test gets an exception for exactly
   these two tokens.
7. Everything in sections 5–12 below was played back in three parts and
   approved.

## 5. Data and storage

### 5.1 Attachment lifecycle

One table row per file. Its id is a Snowflake and doubles as the upload id.

```
uploading ──(finish)──► processing ──(OK)──► ready ──(SendMessage)──► sent
    │                       │                  │
    │                       └──(fails)──► failed ──(1 h)──► deleted
    └──(idle 24 h, cancel)──► deleted           └──(unsent 24 h)──► deleted
sent ──(message deleted, channel deleted)──► deleted
```

- **uploading:** pieces are being received into the local temp directory,
  already encrypted.
- **processing:** every byte has arrived. A background job sniffs the type,
  makes the thumbnail, checks voice messages and moves the file into the
  active backend. Two jobs run at a time across the server.
  - The temp file is kept until the row turns `ready`.
  - On startup, `processing` rows are queued again. If the temp file is gone,
    the row turns `failed`.
- **ready:** the encrypted file and its thumbnail are in a storage backend,
  and metadata is filled in. Nobody but the uploader can see it.
- **failed:** processing refused the file or storage failed. The row keeps
  the error code and message so the uploader can see why, and is deleted
  after 1 h.
- **sent:** linked to a message, and visible to whoever can read the channel.

Deleting a row, for any reason, also deletes its stored objects. A SQLite
`AFTER DELETE` trigger queues the object keys in `blob_deletions`, and a
background worker deletes them with retries. This covers channel deletion
through `ON DELETE CASCADE` without special code.

### 5.2 Encryption format

- **Keys.** Each attachment has its own random 32-byte data key. It is sealed
  with `Secrets::seal("attachment-key:<id>", key)` and stored in the row.
  Reading a file needs the database, `secrets.key` and the object, all three.
  **Losing `secrets.key` loses every file.** The docs say to back it up.
- **Where the key lives.** A new `[server] secrets_file` setting (env
  `OPENCORD_SECRETS_FILE`), default `<data_dir>/secrets.key`. Operators who
  need protection against a stolen disk keep it elsewhere: a Docker secret, a
  systemd credential, or another mount. It covers every sealed value, the
  Phase 3 certificates and tokens included.
- **Guard.** If the key file is missing, the server checks the database first.
  If it already holds sealed data (certificates, DNS tokens, Lavalink
  passwords, storage secrets, attachments), it **refuses to start** and names
  the path. Making a new key would silently make all of that unreadable.
- **Format** (shared code in `opencord-common`, also used by the client
  cache):
  - **Header:** 8 bytes: `OCA1` then four zero bytes.
  - **Segments:** the plaintext is cut into **64 KiB** segments (the last one
    may be shorter). Each is stored as
    `nonce (24 random bytes) || XChaCha20-Poly1305 ciphertext || tag (16)`,
    which is 65,576 bytes for a full segment.
  - **AAD** for each segment: `OCA1 || kind byte (0 file, 1 thumbnail) ||
    attachment id (i64 BE) || segment index (u64 BE) || last flag (u8)`.
    This stops reordering and swapping segments between files, and detects
    truncation.
- **Range reads:** plaintext byte `p` lives in segment `p / 65536`, which
  starts at ciphertext offset `8 + index × 65576`. A video seek therefore
  fetches and decrypts only the segments it touches.
- **Thumbnails** use the same key with kind byte 1.

### 5.3 Storage backends

- **One trait, two backends:**
  - `put`: from a local encrypted file.
  - `get_range`: a byte range of an object.
  - `delete`: idempotent.
  - `size`.
  - `local`: files under `<local_dir>/{a|t}/{shard}/{id}`, where the shard
    is `(id >> 22) & 0xff` in hex. `[uploads] local_dir` defaults to
    `<data_dir>/attachments`, so it can live on another disk.
  - `s3`: keys `{prefix}a/{id}` and `{prefix}t/{id}`. **Object names are
    numbers only.**
- **The local backend** (id 1) always exists and cannot be removed. It is
  active on a fresh server.
- **S3** is signed with **`rusty-s3` 0.10** (sans-IO: it builds presigned
  requests) and sent with the server's existing **reqwest 0.13**.
  - Presigned requests live 15 minutes and are **used only by the server,
    never given out**.
  - Files over 64 MiB go up as multipart uploads in 16 MiB parts, and are
    aborted on failure.
  - Reads use a presigned GET with a `Range` header.
- **Provider presets** fill in the endpoint pattern, region and URL style:
  - AWS S3;
  - Cloudflare R2 (region `auto`, path style);
  - Backblaze B2;
  - Wasabi;
  - MinIO / custom (path style).

  Plain `http://` endpoints need an explicit "Allow plain HTTP" tick, with a
  warning.
- **Test** runs before a bucket can be saved, and again on demand:
  1. put a 1 KiB object;
  2. read 100 bytes back with a Range;
  3. run a two-part multipart upload (5 MiB + 1 KiB);
  4. delete both.

  Each step reports OK or the S3 error code and message.
- **Exactly one backend is active** and receives new uploads.
  - Activating another one keeps the old one readable.
  - A backend can be removed only when it is inactive and holds no files.
- **Move existing files** copies objects from inactive backends to the active
  one.
  - It copies ciphertext as is, so nothing is re-encrypted. The row switches
    in a transaction, then the old copy is queued for deletion.
  - Two files run at a time, with three retries each.
  - It can be stopped and restarted, picking up from rows not yet moved.
  - Progress (files and bytes done and total, failures) goes to the owner
    every 2 s.
- **Secrets.** The S3 secret key is sealed (`storage-secret:<id>`) and never
  sent back. The app sees `has_secret` and the access key id masked to its
  last 4 characters.
- **Usage.** The Storage page shows file count and bytes per backend.

## 6. Upload protocol

1. **CreateUpload** (gateway request).
   - **Fields:** `channel_id`, `filename`, `size_bytes`, `kind` (`FILE` or
     `VOICE_MESSAGE`), and optional video hints (width, height, duration)
     from the sender's probe.
   - **The server checks**, in order:
     1. the sender can view the channel and send messages there;
     2. `ATTACH_FILES` for files, or `SEND_VOICE_MESSAGES` for voice;
     3. the sanitized filename and its extension against the blocked list
        (voice messages skip this);
     4. `size_bytes` against the member's effective limit (voice messages use
        the duration-based cap instead);
     5. the rate limit (20 per minute) and at most 10 unfinished uploads per
        user;
     6. free space in the temp directory ≥ size + 512 MiB.
   - **It answers** with `upload_id` and `piece_size` (8 MiB).
2. **`PUT /media/uploads/{id}`** with `Upload-Offset: n` and one piece as the
   body.
   - The body is at most `piece_size`, and exactly `piece_size` unless it is
     the last piece.
   - The server accepts it only if `n == received_bytes`. It encrypts the
     piece into segments, appends them, fsyncs, then records the new
     `received_bytes`.
   - **A wrong offset gets 409** with the current offset, so the client
     resumes from there.
   - A failed request commits nothing: the temp file is truncated back to
     the committed length.
   - **Why 8 MiB:** pieces stay under Cloudflare Tunnel's 100 MB request cap.
     nginx needs `client_max_body_size 16m` (see the docs).
3. **`GET /media/uploads/{id}`** returns the state, `received_bytes`, and
   either the `Attachment` (when ready) or the error (when failed). It is used
   to resume after a dropped connection or a server restart, and to poll.
4. **`POST /media/uploads/{id}/finish`** once every byte has arrived. The body
   is optional: for videos, a poster image (≤ 2 MiB) from the sender.
   - **It answers at once** with `202` and state `processing`. A large S3
     move can take minutes, and Cloudflare cuts requests off at 100 s.
   - **The background job:**
     - **sniffs the type** from the first bytes (section 7);
     - **processes it** (thumbnail, dimensions, voice checks and waveform);
     - **stores it:** moves the encrypted file into the active backend
       (rename for local, upload for S3) and uploads the encrypted thumbnail;
     - sets the row to `ready` and sends **UploadReady { attachment }** to
       the uploader's sessions;
     - **or**, on failure, sets the row to `failed` and sends
       **UploadFailed { upload_id, error }**.
   - **Calling finish again** while processing, ready or failed just returns
     the current state.
5. **SendMessage** gains `repeated int64 attachment_ids` (at most 10).
   - Each attachment must be `ready`, uploaded by the sender, created for
     this channel and not yet sent.
   - Content may then be empty.
   - A voice message is exactly one `VOICE_MESSAGE` attachment with empty
     content.
   - Linking and inserting the message happen in one transaction.
   - Afterwards, `MessageCreate` and fetched history carry `attachments`.
6. **CancelUpload** deletes an `uploading` or `ready` row of one's own.
7. **Garbage collection** runs every 10 minutes:
   - unfinished uploads idle for 24 h go, with their temp files;
   - `ready` rows that were never sent go after 24 h;
   - `failed` rows go after 1 h;
   - temp files without a row go at startup;
   - the `blob_deletions` queue drains with backoff.

Every HTTP media request uses the existing media token. Only the uploader may
touch their upload.

## 7. What the server learns at finish

**Kinds** come from magic bytes only, never from the extension or the client:

| Kind | Recognized by | Then |
|---|---|---|
| IMAGE | PNG, JPEG, GIF, WebP | Read the header with `image` limits (sides ≤ 16,384, ≤ 50 MP, max alloc 512 MiB). Decode in `spawn_blocking`, honoring EXIF orientation, and make a thumbnail ≤ 800 px on the long side: JPEG q82, or PNG when the image has alpha. `animated` is set for multi-frame GIF/WebP. If decoding fails, the kind becomes FILE. |
| VIDEO | MP4/MOV (`ftyp`, not an audio brand), WebM/MKV (EBML) | Width, height and duration are the sender's hints, clamped (sides 1–16,384, ≤ 24 h). They are used only for layout. The poster, if sent, is decoded with limits and re-encoded as the thumbnail. |
| AUDIO | Ogg (not voice), MP3 (ID3 or frame sync), FLAC, WAV, M4A brands | Nothing more. |
| VOICE_MESSAGE | Declared at CreateUpload; must parse | Strict Ogg Opus check with the soundboard reader (`OpusFile::read`): 48 kHz, 1–2 channels, real duration from the packets. Must be ≤ the server's voice limit. Decoded with libopus to compute **128 waveform values** (RMS per equal slice, dBFS mapped −60…0 dB → 0…255). Size cap: `max_seconds × 8,000 + 64 KiB`. |
| FILE | Anything else | Nothing more. |

**Served `Content-Type`** is chosen from the kind (`image/png`, `video/mp4`,
`audio/ogg`, …), or `application/octet-stream` otherwise. Never `text/html`
or `image/svg+xml`.

**Filenames** are sanitized:
- path parts, control characters and trailing dots or spaces are stripped;
- Windows reserved names (CON, PRN, AUX, NUL, COM1–9, LPT1–9) get a `_`
  prefix;
- names are at most 200 characters, keeping the extension;
- an empty name becomes `file`.

**Extension blocking** compares, ignoring case, the end of the sanitized name
against each blocked entry. An entry may contain inner dots (`tar.gz`).

## 8. Downloads

- **Endpoints:** `GET /media/attachments/{id}` and
  `GET /media/attachments/{id}/thumbnail`, both with the media token.
- **Access:** the member must hold VIEW_CHANNEL and READ_HISTORY in the
  attachment's channel, checked at request time. The message must not be
  deleted. Anything else gets 404 or 403 with no detail.
- **Ranges:** single ranges only (`a-b`, `a-`, `-n`); anything else gets 416.
  Responses are streamed: read the ciphertext range, decrypt segment by
  segment, trim to the range.
- **Headers:**
  - `Accept-Ranges: bytes`
  - `Cache-Control: private, no-store`
  - `X-Content-Type-Options: nosniff`
  - `Content-Disposition: attachment; filename*=UTF-8''…`
- **A backend that cannot be reached** gives 502, and the app shows "Couldn't
  load · Retry".

## 9. Permissions, limits and settings

**Permission bits**
- `ATTACH_FILES` (bit 10) becomes real and joins `DEFAULT_EVERYONE`.
- New `SEND_VOICE_MESSAGES` (bit 14), also in `DEFAULT_EVERYONE`.
- The migration adds both bits to @everyone on existing servers once. Bit 10
  did nothing until now, so no deliberate choice is overridden.

**Role upload limit**
- New `roles.max_upload_bytes`, nullable.
- @everyone always has a value: 50 MiB (52,428,800) by default, and it can't
  be cleared.
- Any other role can be "not set".

**Effective limit**
- A member's limit is the highest of @everyone's value and each of their
  roles' set values, capped at the **ceiling**.
- The **owner's limit is the ceiling**.
- ADMINISTRATOR does not change the size limit.

**Ceiling**
- Set in `opencord.toml`: `[uploads] max_file_mb = 4096`, range 1–102,400.
  "MB" means MiB everywhere, as the soundboard's KB already does.

**Who may set a role's limit**
- Anyone who can edit that role today (MANAGE_ROLES and outranking it).
- **They may not set it above their own effective limit**, unless they are
  the owner. This mirrors `can_grant`.

**Upload settings** (new Uploads page, MANAGE_SERVER)
- Blocked extensions:
  - up to 100 entries, each 1–20 characters of `[a-z0-9]` with inner dots
    allowed, stored lowercase without a leading dot;
  - empty by default;
  - a "Block programs" button adds a preset list (exe, msi, msix, appx, bat,
    cmd, com, scr, pif, cpl, ps1, psm1, vbs, vbe, js, jse, wsf, wsh, hta,
    lnk, reg, jar, apk, app, dmg, pkg, deb, rpm, appimage, sh, run).
- Voice message maximum length: 30–1,800 s, default 300 s.
- Fixed values: 10 attachments per message, 8 MiB pieces.

**Where clients get all this**
- **Ready** carries `UploadSettings`: blocked list, voice maximum, ceiling,
  attachment maximum.
- **Ready** carries each role's `max_upload_bytes`.
- **UploadSettingsUpdate** goes to everyone; the existing role update event
  carries the new field.

**The client computes the same effective limit**, in Dart, from the same
rule. Rust and Dart unit tests share one table of cases.

**Storage settings are owner only:** GetStorage, AddStorage, UpdateStorage,
TestStorage, ActivateStorage, RemoveStorage, StartStorageMove and
StopStorageMove. **StorageUpdate** events go to the owner only.

**UploadReady** and **UploadFailed** go only to the uploader's own sessions.

**New error codes:** FILE_TOO_LARGE, FILE_TYPE_BLOCKED,
UPLOAD_OFFSET_MISMATCH, STORAGE_UNAVAILABLE, STORAGE_FULL,
VOICE_MESSAGE_INVALID and VOICE_MESSAGE_TOO_LONG. Existing codes cover the
rest (FORBIDDEN, NOT_FOUND, RATE_LIMITED, INVALID_ARGUMENT).

## 10. Client core (Rust)

### Transfers
- New media transfers use **reqwest 0.13**:
  - one client per server route, built with the route's TOFU rustls config
    (`use_preconfigured_tls`);
  - keep-alive.
- A certificate mismatch maps to the same "certificate changed" error as
  today.
- The small existing requests (sounds, artwork) may stay on `http.rs`.

### Upload manager
- **Sending:** `send_message_with_files(server, channel, content, nonce,
  files)`.
  - Sources are a file path or in-memory bytes (pasted images, voice drafts).
  - It runs uploads two at a time per server and retries network errors with
    backoff, resuming from the server's offset.
  - Progress events (nonce, file index, sent, total, state) are throttled to
    10 per second per file.
  - After finish, it waits for UploadReady or UploadFailed. If no event comes
    within 5 s, or after a reconnect, it polls `GET /media/uploads/{id}`
    every 5 s.
  - When every file is `ready`, it sends the message with their ids.
  - Sends in progress live in memory only; closing the app abandons them.
- **Cancel:** `cancel_send(nonce)` stops the uploads and cancels them on the
  server.
- **Images up to 100 MB lose their metadata before upload.**
  - JPEG: APP1 Exif and XMP and APP13 are removed. JFIF and the ICC profile
    are kept, and a minimal Exif segment carrying only Orientation goes back
    when it wasn't 1.
  - PNG: `eXIf` and XMP `iTXt` are removed.
  - WebP: `EXIF` and `XMP ` chunks are removed, with the VP8X flags and RIFF
    size fixed.
  - Larger or unknown files are sent unchanged.
- **Videos** are probed with FFmpeg where it exists: size, rotation-aware
  dimensions, duration, and a poster frame (≤ 800 px JPEG). Without FFmpeg
  there are no hints and no poster.

### Downloads
- **Save as:** `download_attachment(server, id, path)` streams to the chosen
  file with progress events and can be cancelled.
- **Viewing:** `attachment_bytes(server, id, thumbnail)` returns cached bytes
  for display.

### Encrypted cache
- **Key:** a random 32-byte key made on first run and kept in the system
  keychain next to the identity, namespaced by profile. It is passed to the
  core at startup.
- **Files:** stored in the app's data folder using the same segment format as
  the server, named by a hash of (server, attachment, variant).
- **Size:** least-recently-used beyond 1 GiB is evicted. "Clear media cache"
  goes in user settings.
- **What's cached:** thumbnails, images ≤ 20 MiB, voice messages, and audio
  files ≤ 20 MiB. Video is streamed, not cached.

### Clipboard
- `clipboard_image()` uses **arboard 3.7** (with its Wayland data-control
  feature). It returns PNG bytes when the clipboard holds an image and no
  text.

### Voice recorder
- **Input:** cpal on the user's chosen input device, converted to 48 kHz
  mono, with the user's noise suppression setting.
- **Encoding:** libopus VOIP mode, 32 kbps VBR, 20 ms frames, no DTX, written
  to Ogg in memory with `ogg::write`.
- **Levels:** RMS per frame, mapped the same way as the server's waveform,
  sent to the app about every 33 ms.
- **Stopping:** it stops on its own at the server's voice maximum.
- **Result:** a draft (id, duration, 128-value waveform). Drafts can be
  previewed, sent or discarded.

### Voice player
- Reads Ogg Opus as a stream and decodes with libopus.
- Seeks by packet index with 80 ms of pre-roll.
- **Pitch-preserving speed** of 1×, 1.5× and 2× through the pure-Rust **`wsola`
  0.2** crate (MIT, about 500 lines).
- Plays through cpal on the user's chosen output device and volume, and sends
  position events.
- Works on all three desktops without FFmpeg.

### Media player (FFmpeg)
- **Source:** a `Read + Seek` range reader over the attachment (1 MiB blocks
  over the transfer client, two blocks read ahead, small memory cache),
  opened with `format::input_from_stream_with_interrupt`.
- **Decode:** one demux and decode thread, with software decoding using frame
  threads.
- **Output:** audio to cpal; video frames to the existing texture path.
  Rotation metadata is honored.
- **Clock and seeking:** audio is the master clock, or a monotonic clock when
  there is no audio. Seeks are accurate: seek to the keyframe, then drop
  frames before the target.
- **Events:** position, duration, buffering, ended and error.

**Only one thing plays at a time** across voice messages, audio files and
videos.

### During a voice call
- **Recording** takes the call engine's processed microphone (echo-cancelled,
  noise-suppressed) instead of opening its own.
  - While recording, the user's microphone sends silence to the call. The
    mute state doesn't change.
  - The voice panel shows "Recording a voice message".
- **Playback** of voice messages, audio and video goes through the call
  engine's output mixer, as the soundboard does (D31). The echo canceller
  then removes it from the microphone, and other people don't hear it.
- **Outside a call**, the recorder and the players open their own capture and
  output streams.

## 11. App (Flutter)

### Composer
- **The attach button** opens a multi-file picker. The tooltip shows the
  limit ("Attach files · up to 50 MB each"). The button is disabled without
  ATTACH_FILES.
- **Drag and drop:** the overlay says "Drop to send to #channel · up to 50 MB
  each".
- **Paste:** Ctrl+V with an image on the clipboard stages
  `pasted-image-YYYYMMDD-HHMMSS.png`.
- **Staged tiles** sit above the field: thumbnail or icon, name, size, and ×
  to remove. A tile that is over the limit or blocked takes the monochrome
  error style (outlined, an error icon, the reason in `textSecondary`), and
  sending is disabled until it is removed. At most 10 files.
- **Send:** the message appears at once, with per-file progress bars and a
  cancel ×. When an upload fails, the message shows Retry and Remove.

### Voice recording (approved as played back)
- **Start:** when the field is empty, pressing and holding the mic starts
  recording. A press shorter than 500 ms shows a "Hold to record" tooltip
  instead. With the mic button focused, holding Space does the same.
- **The recording bar** replaces the field:
  - a pulsing dot in the `recording` red, and a timer;
  - a **blue live waveform**: one bar per 100 ms (the loudest level in that
    window), new bars entering from the right, scrolling left;
  - the hint "Release to send · Esc to cancel".
- **Release sends.**
- **Cancel:** Esc, or dragging more than 120 px left. The bar takes the
  `recording` red with "Release to cancel", and releasing discards the
  recording.
- **Lock:** dragging more than 60 px up, onto the lock icon. The bar then
  shows Delete, Stop and Send.
  - Stop opens a preview with play and pause.
  - Enter sends; Esc deletes.
- **At the maximum length**, a held recording sends and a locked recording
  stops.
- **Permissions:** the mic button is hidden without SEND_VOICE_MESSAGES.

### Messages
- **Images:**
  - One image displays at up to 400×300, in its own proportions.
  - 2–4 images form a grid; more than 4 show "+N" on the fourth.
  - Animated images up to 15 MiB load the original so they animate.
  - **The space each image takes is reserved from its stored dimensions**, so
    nothing shifts.
- **Media viewer** (in the app, over a scrim): fit to window, wheel to zoom,
  drag to pan, ←/→ to move between the message's images and videos, a
  Download button, Esc to close. Videos play here with play/pause, a seek
  bar, time, volume and mute.
- **Video tiles** show the poster, a play button and the duration. Without a
  poster, a generic tile.
- **Audio files** get an inline card: play/pause, seek bar, time.
- **Other files** get a card with an icon by type, the name (middle-ellipsized),
  the size and Download. Text-like extensions under 256 KiB also get
  **Preview**: a monospace dialog with the text decoded as UTF-8, never
  rendered.
- **Voice bubble:**
  - play/pause, the waveform (3 px bars with 2 px gaps), the played part in
    the blue token and the rest in `textMuted`;
  - "0:12 / 0:45" while playing, the total when idle;
  - a 1× → 1.5× → 2× chip;
  - click or drag on the bars to seek;
  - width from 200 px (≤ 10 s) to 320 px (≥ 60 s).
- **Right-click** on an attachment: Download, Copy image (images), Delete
  message (if allowed). **There is no link-copying anywhere.**
- **One-line summaries** (reply previews, pins, notifications) for a message
  with no text read "Photo", "Video", "Voice message (0:12)" or the first
  file name.
- **Download** uses the system "Save as" dialog (`file_selector`), with a
  progress bar on the card.

### Settings
- **Storage page** (owner only):
  - the backends with usage;
  - "Connect a bucket": presets, fields, Test, then Save;
  - Activate and Remove;
  - "Move existing files here" with a progress bar and Stop.
- **Uploads page** (MANAGE_SERVER): blocked extensions as chips with "Block
  programs", and the voice maximum length.
- **Roles page:** a "Max upload size" field (number plus MB/GB). @everyone
  must have a value; other roles can be "Not set". It is validated against
  the editor's own limit.
- **User settings:** "Clear media cache" with the current cache size.

### Theme
- Two new color tokens in `OcColors`, the only hues in the app:
  - **`voice`** (blue), for the live waveform and the played part of voice
    bubbles: dark `#4C8DFF`, light `#2F6FEB`;
  - **`recording`** (red), for the recording dot and the cancel state only:
    dark `#F2555A`, light `#DC3E42`.
- `app/test/widget/monochrome_test.dart` allows exactly these two colors and
  still fails on any other hue.
- desktop-ui.md §1 gets a note naming both exceptions.

## 12. FFmpeg and packaging

- **Linux** (`build_ffmpeg.sh`):
  - add decoders: hevc, vp8, vp9, aac, mp3, opus, vorbis, flac and common PCM;
  - add demuxers: mov, matroska, ogg, mp3, flac, wav and aac, with their
    parsers;
  - add **libdav1d** (pinned and checksummed) for AV1;
  - keep it LGPL.
- **Windows:** the shipped DLLs are checked for the same set, dav1d
  included.
- **macOS** (Mac session):
  - a decode-only universal FFmpeg (the same decoders and demuxers, dav1d,
    and the VideoToolbox hwaccel for future use), bundled in the app's
    Frameworks;
  - `ffmpeg-next` added to opencord-media's macOS section;
  - the player's cfg turned on;
  - a new decision record supersedes the "no FFmpeg" part of D34.
- **Server Docker build:** gains `cmake`, because libopus is now linked for
  the voice waveform.

## 13. Security checklist

- Uploads are read with hard limits, and pieces are buffered at most 8 MiB.
- Image decoding has limits.
- The poster is re-encoded.
- Range parsing is strict.
- No plaintext touches disk on the server. Temp files are encrypted as they
  are received.
- Object keys are numeric.
- Filenames are sanitized on the server and again before saving on the
  client.
- Permission checks happen at download time.
- The S3 secret is write-only and never logged.
- Errors don't leak paths or S3 responses to non-owners.
- Rate limits apply to CreateUpload.
- Owner-only storage endpoints have tests that non-owners are refused.

## 14. Docs to write or update

- **New `docs/attachments.md`:**
  - storage choices and setting up each provider, with a minimal IAM policy:
    `s3:PutObject`, `s3:GetObject`, `s3:DeleteObject`, `s3:ListBucket`,
    `s3:AbortMultipartUpload`, `s3:ListMultipartUploadParts`;
  - **backing up `secrets.key`**;
  - limits and blocked extensions;
  - reverse-proxy body size;
  - what "only in the app" does and doesn't mean (a member can still save a
    copy).
- **Updates:** `docs/protocol.md`, `docs/domains.md` (nginx
  `client_max_body_size`), `docs/packaging.md` (FFmpeg on all three systems),
  and `docs/decisions.md` (D52 onward).

## 15. Testing approach

- **Shared Rust (`opencord-common`):**
  - encryption: round trip, tampering, truncation, reordering, range maths;
  - sniffing;
  - filename sanitizing;
  - extension matching;
  - effective limits;
  - waveform mapping.
- **Server integration tests** (existing harness under `tests/`): every
  numbered rule in sections 6–9, including resume, offset conflicts, limits,
  blocking, permissions at download, deletes queueing blobs (channel cascade
  included), GC, voice validation and owner-only storage.
- **S3 against MinIO in Docker:** `#[ignore]` tests, run manually, as the
  Pebble ACME tests are.
- **Core:**
  - uploads and downloads against a real server, as `tests/client.rs` does;
  - the recorder from a fixture;
  - the voice player's seek and speed;
  - EXIF stripping on fixture photos;
  - the player opening fixture media where FFmpeg exists.
- **App:** widget tests for staging, every recording state, the waveform
  painter, image grid layout, viewer navigation and the settings validation.
  The mock repository gains all new calls.
- **Live checks:** a `check_*.sh` script for Linux, as earlier phases had;
  each platform session runs the same flows by hand.

## 16. Milestones (order)

| # | Milestone | Machine |
|---|---|---|
| A0 | Shared foundations: proto, permission bits, encryption, sniffing, filenames, extensions, limits, migration | Linux |
| A1 | Server: local storage, uploads, finish processing (images, voice, posters), downloads, GC | Linux |
| A2 | Server: messages with attachments, settings, role limits, Ready and events | Linux |
| A3 | Server: S3 backend, storage management, move job, MinIO tests | Linux |
| A4 | Core: transfers, upload manager, downloads, encrypted cache, EXIF, clipboard, Dart API | Linux |
| A5 | App: sending, viewing (images, viewer, files, preview, download), settings pages | Linux |
| A6 | Voice messages: recorder, player, recording bar, voice bubble | Linux |
| A7 | FFmpeg build (Linux), media player, video and audio UI, video probe and poster | Linux |
| A8 | Hardening, docs, security review, performance | Linux |
| P1 | Windows: check the FFmpeg set, run every flow, fix Windows-only issues | Windows |
| P2 | macOS: decode-only FFmpeg bundle, player on, run every flow | Mac |

Each milestone ends with fmt, clippy `-D warnings`, the workspace tests,
`flutter analyze` and `flutter test`, then stops for review.

## 17. Calls made without asking (for the owner to check)

1. The S3 library is `rusty-s3` with reqwest, not the AWS SDK (much lighter)
   and not hand-written signing.
2. 64 KiB encryption segments, each with a random 24-byte XChaCha nonce.
3. Pieces are 8 MiB; uploads are resumable; free temp space must be ≥ size
   + 512 MiB.
4. Unsent uploads and unfinished uploads are deleted after 24 h.
5. The owner's limit is the config ceiling (default 4 GB). ADMINISTRATOR gets
   no automatic increase.
6. "Attach files" and "Send voice messages" are turned on for @everyone on
   existing servers.
7. Blocked extensions and the voice maximum need MANAGE_SERVER; role limits
   follow the role-editing rules; storage is owner only.
8. Voice: 32 kbps Opus, 300 s default maximum (30–1,800), exempt from the
   size limit, 128 waveform values.
9. Video width, height and duration come from the sender, used only for
   layout. The server derives everything else itself.
10. The client cache is encrypted with a key in the system keychain, LRU,
    1 GiB.
11. Photo metadata is stripped by the sender's app (up to 100 MB), keeping
    orientation.
12. Ten attachments per message. Animated images up to 15 MiB animate in the
    chat.
13. Software video decoding only. 4K may be heavy on older machines.
14. AV1 through libdav1d on all three systems.
15. Pitch-preserving voice speed through the small `wsola` crate.
16. The blue and red tokens' values.
17. Closing the app abandons sends in progress (no saved outbox).
18. Finish answers at once. The move to storage runs in the background, and
    the uploader gets UploadReady or UploadFailed. This avoids Cloudflare's
    100 s cut-off on big S3 moves.
19. `secrets.key`'s location becomes configurable. The server refuses to make
    a new key when the database already holds sealed data. The local
    attachments folder is configurable too.
20. In a call:
    - recording uses the call's echo-cancelled microphone and silences the
      user to the call while recording;
    - playback goes through the call's mixer, so others don't hear it.

## 18. Left to the builder

Internal module layout beyond what the plan fixes, widget composition within
the design tokens, fixture files, log wording, exact retry and backoff
timings beyond those given, and the order of tasks inside a milestone.
