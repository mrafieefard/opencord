# Media Uploads, S3 Storage and Voice Messages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. If your harness has neither skill, work through the tasks in order yourself and tick the boxes as you go.

**Goal:** Members can attach files (stored encrypted on the server's disk or in S3-compatible buckets) and record voice messages, and view, play and save everything inside the Opencord app on Linux, Windows and macOS.

**Architecture:**
- **Server.** The server receives resumable 8 MiB pieces over HTTPS and encrypts them as they arrive (XChaCha20-Poly1305 in 64 KiB segments, one random key per file, sealed by `secrets.key`). A background job processes each finished upload (type sniffing, thumbnails, voice checks with a server-computed waveform) and moves it into the active storage backend (local folder or S3 through `rusty-s3` + `reqwest`).
- **Messages.** A message carries up to 10 attachment ids. Downloads decrypt on the fly, with Range support, and only for members who can read the channel.
- **Client core.** The Rust core uploads, downloads, keeps an encrypted cache, records Opus voice notes, and plays voice notes (libopus + `wsola`) and media (FFmpeg custom I/O → existing video textures).
- **App.** The Flutter app renders all of it.

**Tech Stack:**
- Rust 2024: axum 0.8, sqlx 0.9 (SQLite, compile-time-checked queries), prost/protox, chacha20poly1305 0.11, image 0.25, opus 0.4, rusty-s3 0.10.2, reqwest 0.13, fs4 1.1, arboard 3.7, wsola 0.2, ffmpeg-next 9.
- Flutter: Riverpod 3, flutter_rust_bridge 2.13, file_selector, desktop_drop, flutter_secure_storage.

**Spec:** `docs/superpowers/specs/2026-10-18-media-uploads-design.md`.
- Read it fully before Task 1.
- The spec says *what* and *why*; this plan says *where* and *how*.
- If the two disagree, or a step is impossible as written, stop and ask. Do not improvise.

## Global Constraints

- **Platforms:** Linux, Windows and macOS must fully work. Linux is built and tested here; Windows and macOS have their own tasks (P1, P2), run on those machines.
- **Branch:** create `phase4-media` from `pre` (`git switch -c phase4-media pre`). Commit there after every task. Never commit to `main` or `pre`. Push only when the user asks.
- **Decisions:** one entry per milestone in `docs/decisions.md`, numbered from **D52**, in the existing style (short "Why / What" bullets).
- **Encryption format:**
  - header `OCA1` + 4 zero bytes;
  - plaintext segments of **65,536** bytes;
  - each stored as `nonce(24) || ciphertext || tag(16)`, i.e. 65,576 bytes per full segment;
  - AAD `OCA1 || kind(u8) || blob id (i64 BE) || segment index (u64 BE) || last (u8)`;
  - kinds: 0 file, 1 thumbnail, 2 client cache.
- **Pieces:** **8 MiB = 8,388,608** bytes. Every piece except the last is exactly this size.
- **Limits:**
  - @everyone default **52,428,800** bytes (50 MiB);
  - ceiling `[uploads] max_file_mb`, default **4096**, range **1–102,400** (1 MB = 1,048,576 bytes everywhere);
  - **10** attachments per message;
  - **10** unfinished uploads per user;
  - CreateUpload **20 per 60 s** per user;
  - free temp space ≥ size + **512 MiB**.
- **Voice:**
  - Ogg Opus, 48 kHz mono, **32 kbps** VBR, 20 ms frames, VOIP mode, no DTX;
  - max length default **300 s**, range **30–1,800 s**;
  - size cap `max_seconds × 8,000 + 65,536` bytes;
  - waveform **128** bytes, each `round(clamp((20·log10(rms) + 60) / 60, 0, 1) · 255)`.
- **Thumbnails:** ≤ **800 px** on the long side, JPEG quality **82**, or PNG when the image has alpha.
- **Image limits:** sides ≤ **16,384**, ≤ **50,000,000** pixels, max alloc **512 MiB**. Only files ≤ **100 MiB** are decoded.
- **Expiry:**
  - `uploading` idle **24 h**, `ready` unsent **24 h**, `failed` **1 h**;
  - GC every **10 min**;
  - blob-deletion worker every **30 s**.
- **Processing:** at most **2** processing jobs at a time server-wide.
- **Client cache:** **1 GiB**, LRU. Caches thumbnails, images ≤ **20 MiB**, voice, and audio ≤ **20 MiB**.
- **Media player reads:** 1 MiB blocks, 2 blocks read ahead.
- **Colors:**
  - `voice` dark `#4C8DFF`, light `#2F6FEB`;
  - `recording` dark `#F2555A`, light `#DC3E42`;
  - no other hue anywhere. Resting screens stay monochrome, and the existing `monochrome_test.dart` must keep passing unchanged.
- **No public file URLs, ever.**
  - No presigned URL leaves the server.
  - No "Copy link" or "Open in browser" for attachments.
  - Media endpoints require the bearer media token.
- **Licences:** server code AGPL-3.0-or-later; `crates/opencord-common`, `opencord-core`, `opencord-media`, `proto/` and `app/` MPL-2.0. New dependencies must be MIT, Apache-2.0, BSD, ISC, Zlib or LGPL (FFmpeg, dynamically linked) only.

## Working Rules (read before Task 1)

### Environment (Linux dev machine; the disk is nearly full)

Each shell command runs in a fresh shell, so exports don't carry over. Start every `cargo`, `flutter`, `dart`, `sqlx` and codegen command with `source .superpowers/sdd/media/env.sh &&`. `GEMINI.md` gives the file's contents in case it is missing. It sets:

```bash
export PATH="$HOME/.cargo/bin:$HOME/.local/share/flutter/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
```

- Check `df -h /` before long builds.
- If space runs short, delete `target/debug/incremental` and stale test binaries in `target/debug/deps` (older than 45 min). Never run `cargo clean` without asking.

### Commands

| What | Command |
|---|---|
| Format | `cargo fmt --all` and `cd app && dart format lib test tool` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust tests | `cargo test --workspace --lib --bins --tests` (skips the heavy examples) |
| One server test | `cargo test -p opencord-server --test attachments <name>` |
| Off-Linux type check | `app/tool/check_platforms.sh` |
| Flutter | `cd app && flutter analyze && flutter test` |
| sqlx data (after **any** query or migration change) | see "sqlx" below |
| Dart bindings (after **any** change under `crates/opencord-core/src/api/`) | `cd app && flutter_rust_bridge_codegen generate` |
| Icon font (after editing `app/tool/build_icons.dart`) | `cd app && dart run tool/build_icons.dart` |

**sqlx** (from the README):

```bash
export DATABASE_URL="sqlite://$PWD/target/sqlx-dev.db"
rm -f target/sqlx-dev.db
sqlx database create
sqlx migrate run --source crates/opencord-server/migrations
cargo sqlx prepare --workspace -- --all-targets
```

Commit the changed `.sqlx/*.json` files with the task. CI builds with `SQLX_OFFLINE=true`; a missing query file fails CI.

### Generated files: never edit by hand

- `app/lib/src/rust/**`
- `crates/opencord-core/src/frb_generated.rs`
- `*.freezed.dart`
- `.sqlx/*.json`
- `app/lib/ui/theme/oc_icons.dart`
- the icon font

Regenerate them with the commands above.

### Rules reviews will enforce (each one has caught real bugs before)

1. **No `unwrap()`/`expect()` in non-test code**, except where the codebase already does the same thing (for example sealing with a fixed-size key).
2. **Never hold `state.guild()`, `state.guild_mut()` or any `std::sync` lock across `.await`.** Copy what you need out of the guard in a block, then await. A guard held across an await does not compile in handlers (not `Send`), and in the core it can deadlock.
3. **Writes:**
   - Take `let _writes = state.write_lock().await;` before changing the database and the in-memory guild together, as the existing handlers do.
   - Write the database **first**, then update memory, then broadcast.
4. **Proto field numbers are permanent.** Add fields only with the numbers this plan gives. Never renumber or reuse.
5. **Every new or changed `sqlx::query!`/`query_as!` needs the sqlx data regenerated** in the same commit.
6. **Every new public function under `crates/opencord-core/src/api/` needs the bindings regenerated** in the same commit. Dart code calls only the generated API.
7. **`DEFAULT_CONFIG_TOML` in `crates/opencord-server/src/config.rs` must equal `Config::default()`.** The test `default_file_matches_default_config` checks it. The root `opencord.toml` is git-ignored local runtime config: don't edit it.
8. **Dart:**
   - Every new `OpencordRepository` method needs implementations in **both** `RustRepository` (`app/lib/core/rust/rust_repository.dart`) and `MockRepository` (`app/lib/core/mock/mock_repository.dart`). Otherwise the mock app and the widget tests break.
   - **No hard-coded colors** outside `app/lib/ui/theme/oc_colors.dart`; `test/unit/hardcoded_colors_test.dart` checks this.
   - Mock data must stay **greyscale**. Any mock image you add must be a grey PNG, because `monochrome_test.dart` scans the mock main screen.
9. **Errors to users:**
   - Human sentences in the codebase's tone ("files can be at most 50 MB here").
   - Never leak file paths, SQL, S3 URLs (they contain signatures) or S3 response bodies to non-owners.
   - Log details server-side with `tracing`.
10. **Tests:**
    - Write the failing test first, run it, and see it fail for the right reason.
    - Do not weaken or delete existing tests to make them pass. If an existing assertion must change because the spec changed a behavior (for example `DEFAULT_EVERYONE` gaining bits), change only that assertion and say so in the progress ledger.
11. **Platform seams** (`docs/plans/platforms.md`):
    - Shared code compiles on every desktop system.
    - Platform-only code lives in `linux.rs` / `windows.rs` / `macos.rs` / `unsupported.rs` files picked by `cfg`.
    - On Linux you may not edit macOS- or Windows-only files.
12. **cpal streams are not `Send`.** Open `Microphone`/`Speaker` (`opencord_media::audio::device`) on the thread that will own them, as `AudioEngine` does.

Rules 13–21 came from the A1, A2 and A3 reviews. They now live in `GEMINI.md` → "How to write code here", which grows after every review. Earlier tasks that cite them by number mean those rules: 13 check cheapest first, 14 maps say when entries leave, 15 never turn an error into "nothing", 16 conditional state changes, 17 std locks and HTTP replies, 18 `spawn_blocking`, 19 reuse what exists, 20 a transaction holds only writes, 21 a check holds only under the lock.

### Progress ledger (makes the final review cheap)

Keep `.superpowers/sdd/media/progress.md`. After each task, append:

```
## Task N — <title>
- Commit: <hash>
- Tests added: <test names>
- Commands run: <command> → <result summary>
- Deviations from the plan: <none | what and why>
- Left undone / doubts: <none | list>
```

What counts as a deviation, and how to report it, is in `GEMINI.md` → "Reports and the ledger".

### When a step keeps failing

If the same step fails three times in a row, follow `GEMINI.md` → "When you're stuck: ask Claude".

### Milestone stops

- At the end of each milestone (A0 … A8, P1, P2):
  1. run the full checks (format, clippy, Rust tests, platform check, flutter analyze/test). Run the Rust tests with `--no-fail-fast`: a failure in one crate otherwise hides every crate after it;
  2. write the decisions entry:
     - A0–A4 give its text: copy it exactly, replace `<commits>` with the milestone's commit hashes, and check every sentence against the code. If one is wrong, fix the code or the sentence, and say which in your report;
     - from A5 on, the stop names the entry and you write it from `git diff`, as `GEMINI.md` → "Write docs from the code" says;
     - add a `- **Deviations.**` bullet only if you deviated from the plan;
  3. **stop and report** to the user.
- Continue only when the user releases the next milestone.

## Review Focus

These are the inputs most likely to break for real people while no ordinary test would catch them. Each has a pinned test in the task named.

1. **A piece sent twice after a client timeout** (the server committed it, the client never heard back). Expected: 409 with the current offset, and the upload continues with no corrupted bytes. Pinned in Task 8 (`a_repeated_piece_gets_a_conflict_and_the_upload_continues`) and Task 18 (`uploads_resume_after_a_lost_answer`).
2. **Permissions change between upload, send and download** (Attach Files removed, channel hidden, member kicked). Expected: send refused; download 403/404 at request time. Pinned in Task 12 (`sending_checks_permissions_again_at_send_time`) and Task 10 (`downloads_check_the_channel_at_request_time`).
3. **Odd or hostile images** (header claiming 20,000×20,000, truncated PNG, 16-bit PNG, animated GIF, EXIF-rotated JPEG). Expected: no crash or hang; huge or broken ones become FILE; rotation honored. Pinned in Task 9 (`odd_images_never_crash_processing`).
4. **Range edges** (zero-byte file, size exactly 65,536, `bytes=-n` with n > size, start beyond end, two ranges). Expected: correct bytes or 416. Pinned in Task 10 (`range_edges`).
5. **Storage failing during processing, and a restart mid-processing.** Expected: row `failed` with STORAGE_UNAVAILABLE, temp file gone and an UploadFailed event; after a restart, `processing` rows finish. Pinned in Task 9 (`a_storage_failure_fails_the_upload_cleanly`, `processing_resumes_after_a_restart`).

## File Map

### Shared (`crates/opencord-common/src/`)

| File | Status | Contents |
|---|---|---|
| `permissions.rs` | modify | `SEND_VOICE_MESSAGES`; `DEFAULT_EVERYONE` |
| `limits.rs` | modify | `UPLOAD_CREATE_RATE` |
| `upload.rs` | new | constants, effective limit, filename sanitizing, extension rules |
| `blob.rs` | new | encryption format |
| `sniff.rs` | new | magic-byte sniffing |
| `waveform.rs` | new | level mapping and 128-value waveform |
| `crates/opencord-common/tests/upload_rules.json` + `upload_rules.rs` | new | shared Rust/Dart fixture and its Rust test |

### Protocol (`proto/opencord/v1/`)

`models.proto`, `requests.proto`, `events.proto`, `gateway.proto`.

### Server (`crates/opencord-server/`)

| File | Status | Contents |
|---|---|---|
| `migrations/0008_attachments.sql` | new | schema |
| `src/attachments/mod.rs` | new | `Attachments` service, startup |
| `src/attachments/storage/{mod,local,s3}.rs` | new | backends |
| `src/attachments/temp.rs` | new | encrypted temp files for uploads in progress |
| `src/attachments/process.rs` | new | finish processing |
| `src/attachments/images.rs` | new | thumbnails |
| `src/attachments/voice.rs` | new | voice checks and waveform |
| `src/attachments/http.rs` | new | upload and download endpoints |
| `src/attachments/range.rs` | new | Range parsing |
| `src/attachments/gc.rs` | new | expiry sweeps, blob deletions |
| `src/attachments/settings.rs` | new | `UploadSettings` in server_meta |
| `src/attachments/manage.rs` | new | owner storage requests |
| `src/attachments/move_job.rs` | new | moving files between backends |
| `src/db/attachments.rs`, `src/db/storage.rs` | new | queries |
| `src/handlers/uploads.rs`, `src/handlers/storage.rs` | new | gateway handlers |
| existing | modify | `config.rs`, `secrets.rs`, `server.rs`, `state.rs`, `guild.rs`, `db/mod.rs`, `db/roles.rs`, `db/messages.rs`, `handlers/{mod,messages,roles}.rs`, `gateway/identify.rs`, `media.rs`, `http.rs`, `error.rs`, `rate_limit.rs`, `bootstrap.rs`, `Cargo.toml`, `tests/common/mod.rs` |
| tests | new | `tests/attachments.rs`, `tests/storage.rs`, `tests/s3.rs` |

### Media engine (`crates/opencord-media/src/`)

| File | Status | Contents |
|---|---|---|
| `audio/processor.rs`, `audio/engine.rs` | modify | capture tap, local media mix |
| `player/{mod,decode,sync,linux,windows,macos,unsupported}.rs` | new | FFmpeg media player |

### Client core (`crates/opencord-core/src/`)

| File | Status | Contents |
|---|---|---|
| `attachments/{mod,transfer,upload,download,cache,exif,clipboard}.rs` | new | uploads, downloads, cache, photo stripping, clipboard |
| `voice_message/{mod,recorder,player}.rs` | new | recording and voice playback |
| `media_player.rs` | new | glue between the FFmpeg player, textures and Dart |
| `api/attachments.rs` | new | Dart-facing API |
| existing | modify | `api/types.rs`, `api/mod.rs`, `convert.rs`, `connection.rs`, `client.rs`, `media.rs`, `Cargo.toml` |

### App (`app/lib/`)

| File | Status | Contents |
|---|---|---|
| `core/model/attachment.dart`, `core/model/storage.dart` | new | models |
| `core/upload_rules.dart` | new | Dart mirror of the shared rules |
| `core/rust/media_cache_key_store.dart` | new | the media cache's key in the keychain |
| existing | modify | `core/model/message.dart`, `core/model/user.dart` (Role), `core/model/snapshot.dart`, `core/providers/server_state.dart`, `core/providers/providers.dart`, `core/providers/messages_state.dart`, `core/repository/{repository,events}.dart`, `core/rust/{rust_repository,core_mapping}.dart`, `core/mock/{mock_repository,mock_world}.dart`, `core/format.dart` |
| `features/chat/attachments/*.dart` | new | staging, views, viewer, preview, downloads |
| `features/chat/voice/*.dart` | new | mic button, recording bar, waveform, voice bubble |
| `features/settings/server/{storage_page,storage_dialogs,uploads_page}.dart` | new | settings pages |
| existing | modify | `features/settings/server/roles_page.dart`, `features/settings/server_settings.dart`, `features/settings/user/app_pages.dart`, `features/chat/{composer,attachment_drop,bubble,compact_message,chat_area}.dart`, `ui/theme/oc_colors.dart`, `tool/build_icons.dart` |

### Build and docs

| File | Status |
|---|---|
| `app/tool/build_ffmpeg.sh`, `.github/workflows/ci.yml`, `deploy/Dockerfile` | modify |
| `docs/attachments.md` | new |
| `docs/{protocol,domains,packaging,decisions}.md`, `docs/plans/desktop-ui.md` §1 note, `README.md` | modify |

---

# Milestone A0: Shared foundations

### Task 1: Protocol messages

**Files:** Modify `proto/opencord/v1/models.proto`, `requests.proto`, `events.proto`, `gateway.proto`. Then fix every Rust struct literal the compiler reports in `crates/opencord-server`, `crates/opencord-core`, `crates/opencord-voicebot`.

**Interfaces:**
- Produces: the proto types below, used by every later task as `opencord_proto::v1::*`.

- [x] **Step 1: Add to `models.proto`** (after `MediaToken`):

```proto
// Media uploads (design 2026-10-18).
enum AttachmentKind {
  ATTACHMENT_KIND_UNSPECIFIED = 0;
  ATTACHMENT_KIND_FILE = 1;
  ATTACHMENT_KIND_IMAGE = 2;
  ATTACHMENT_KIND_VIDEO = 3;
  ATTACHMENT_KIND_AUDIO = 4;
  ATTACHMENT_KIND_VOICE_MESSAGE = 5;
}

message Attachment {
  int64 id = 1;
  string filename = 2;
  // Chosen by the server from the file's first bytes.
  string content_type = 3;
  uint64 size_bytes = 4;
  AttachmentKind kind = 5;
  // Images: stored after EXIF orientation. Videos: the sender's hints.
  optional uint32 width = 6;
  optional uint32 height = 7;
  // Videos (hint) and voice messages (measured).
  optional uint32 duration_ms = 8;
  // Voice messages: 128 values, 0-255, computed by the server.
  bytes waveform = 9;
  bool has_thumbnail = 10;
  optional uint32 thumbnail_width = 11;
  optional uint32 thumbnail_height = 12;
  // Multi-frame GIF or WebP.
  bool animated = 13;
}

message UploadSettings {
  // Lowercase, without a leading dot; may hold inner dots ("tar.gz").
  repeated string blocked_extensions = 1;
  uint32 voice_message_max_seconds = 2;
  // [uploads] max_file_mb in bytes: the owner's limit and every role
  // limit's ceiling.
  uint64 max_file_bytes = 3;
  uint32 max_attachments = 4;
  uint32 piece_bytes = 5;
}

enum UploadState {
  UPLOAD_STATE_UNSPECIFIED = 0;
  UPLOAD_STATE_UPLOADING = 1;
  UPLOAD_STATE_PROCESSING = 2;
  UPLOAD_STATE_READY = 3;
  UPLOAD_STATE_FAILED = 4;
  UPLOAD_STATE_SENT = 5;
}

// The answer to CreateUpload.
message Upload {
  int64 upload_id = 1;
  uint32 piece_bytes = 2;
}

// The HTTP upload endpoints' answer.
message UploadStatus {
  int64 upload_id = 1;
  UploadState state = 2;
  uint64 received_bytes = 3;
  // When ready (or sent).
  optional Attachment attachment = 4;
  // When failed.
  optional Error error = 5;
}

enum StorageKind {
  STORAGE_KIND_UNSPECIFIED = 0;
  STORAGE_KIND_LOCAL = 1;
  STORAGE_KIND_S3 = 2;
}

message S3Config {
  // https://… (http:// only with allow_http).
  string endpoint = 1;
  string region = 2;
  string bucket = 3;
  string key_prefix = 4;
  bool path_style = 5;
  bool allow_http = 6;
  // In answers: masked to its last 4 characters ("••••WXYZ").
  string access_key_id = 7;
  // Write-only: always empty in answers. Empty in UpdateStorage keeps the
  // stored one.
  string secret_access_key = 8;
}

message StorageBackend {
  int64 id = 1;
  StorageKind kind = 2;
  string name = 3;
  bool active = 4;
  optional S3Config s3 = 5;
  bool has_secret = 6;
  uint64 file_count = 7;
  uint64 bytes = 8;
  optional string last_error = 9;
}

message StorageMove {
  int64 to_storage_id = 1;
  bool running = 2;
  uint64 files_done = 3;
  uint64 files_total = 4;
  uint64 bytes_done = 5;
  uint64 bytes_total = 6;
  uint32 failed = 7;
}

message Storage {
  repeated StorageBackend backends = 1;
  optional StorageMove move = 2;
  // Free space where uploads are received.
  uint64 temp_free_bytes = 3;
}

message StorageTestStep {
  string name = 1;
  bool ok = 2;
  string detail = 3;
}

message StorageTest {
  repeated StorageTestStep steps = 1;
  bool ok = 2;
}
```

Also in `models.proto`:
- `Role`: add `optional uint64 max_upload_bytes = 8;` (absent = not set; @everyone always has it).
- `Message`: add `repeated Attachment attachments = 9;`
- `ErrorCode`: add after 40:

```proto
  // Media uploads (design 2026-10-18).
  ERROR_CODE_FILE_TOO_LARGE = 41;
  ERROR_CODE_FILE_TYPE_BLOCKED = 42;
  ERROR_CODE_UPLOAD_OFFSET_MISMATCH = 43;
  ERROR_CODE_STORAGE_UNAVAILABLE = 44;
  ERROR_CODE_STORAGE_FULL = 45;
  ERROR_CODE_VOICE_MESSAGE_INVALID = 46;
  ERROR_CODE_VOICE_MESSAGE_TOO_LONG = 47;
```

- [x] **Step 2: Add to `requests.proto`.**

Existing messages:
- `SendMessage`: `repeated int64 attachment_ids = 4;`
- `CreateRole`: `optional uint64 max_upload_bytes = 6;`
- `UpdateRole`: `optional uint64 max_upload_bytes = 7;` (0 clears it, refused for @everyone)

New messages:

```proto
// -> upload. kind is FILE or VOICE_MESSAGE; the server decides IMAGE,
// VIDEO and AUDIO itself.
message CreateUpload {
  int64 channel_id = 1;
  string filename = 2;
  uint64 size_bytes = 3;
  AttachmentKind kind = 4;
  // Videos: what the sender's probe found; used only for layout.
  optional uint32 width = 5;
  optional uint32 height = 6;
  optional uint32 duration_ms = 7;
}

// -> ack. One's own upload that is not sent yet.
message CancelUpload {
  int64 upload_id = 1;
}

message BlockedExtensions {
  repeated string extensions = 1;
}

// -> upload_settings. MANAGE_SERVER. Absent fields stay.
message UpdateUploadSettings {
  optional BlockedExtensions blocked_extensions = 1;
  optional uint32 voice_message_max_seconds = 2;
}

// Storage, owner only. Each answers -> storage, except TestStorage.
message GetStorage {}
// Tests the bucket first; refused if the test fails.
message AddStorage {
  string name = 1;
  S3Config s3 = 2;
}
message UpdateStorage {
  int64 storage_id = 1;
  optional string name = 2;
  optional S3Config s3 = 3;
}
// -> storage_test. Saves nothing.
message TestStorage {
  oneof target {
    int64 storage_id = 1;
    S3Config s3 = 2;
  }
}
message ActivateStorage {
  int64 storage_id = 1;
}
message RemoveStorage {
  int64 storage_id = 1;
}
// Moves every file to the active storage.
message StartStorageMove {}
message StopStorageMove {}
```

`Request.kind` additions:

```proto
    // Media uploads (design 2026-10-18).
    CreateUpload create_upload = 160;
    CancelUpload cancel_upload = 161;
    UpdateUploadSettings update_upload_settings = 162;
    GetStorage get_storage = 163;
    AddStorage add_storage = 164;
    UpdateStorage update_storage = 165;
    TestStorage test_storage = 166;
    ActivateStorage activate_storage = 167;
    RemoveStorage remove_storage = 168;
    StartStorageMove start_storage_move = 169;
    StopStorageMove stop_storage_move = 170;
```

`Response.result` additions:

```proto
    Upload upload = 29;
    UploadSettings upload_settings = 30;
    Storage storage = 31;
    StorageTest storage_test = 32;
    UploadStatus upload_status = 33;
```

- [x] **Step 3: Add to `events.proto`.**

`Event.kind` additions:

```proto
    // Media uploads (design 2026-10-18).
    // To everyone.
    UploadSettingsUpdate upload_settings_update = 110;
    // To the uploader's own sessions.
    UploadReady upload_ready = 111;
    UploadFailed upload_failed = 112;
    // To the owner only.
    StorageUpdate storage_update = 113;
```

Messages:

```proto
message UploadSettingsUpdate { UploadSettings settings = 1; }
message UploadReady { Attachment attachment = 1; }
message UploadFailed { int64 upload_id = 1; Error error = 2; }
message StorageUpdate { Storage storage = 1; }
```

- [x] **Step 4: Add to `gateway.proto`.** In `Ready`: `UploadSettings upload_settings = 20;`

- [x] **Step 5: Build and fix literals.**
  - Run `cargo check --workspace --all-targets`. It fails wherever a struct literal lacks a new field.
  - Add the empty value at each place: `attachments: Vec::new()`, `max_upload_bytes: None`, `attachment_ids: Vec::new()`, `upload_settings: None`. Behavior must not change in this task.
  - Run `app/tool/check_platforms.sh`.
  - Expected: both pass.
- [x] **Step 6:** `cargo test --workspace --lib --bins --tests`. Expected: PASS (nothing behaves differently yet).
- [x] **Step 7: Commit:** `feat: media upload protocol messages (A0)`.

### Task 2: Shared upload rules

**Files:**
- Modify: `crates/opencord-common/src/{permissions.rs,limits.rs,lib.rs}`.
- Create: `crates/opencord-common/src/upload.rs`, `crates/opencord-common/tests/upload_rules.json`, `crates/opencord-common/tests/upload_rules.rs`.
- Modify: `crates/opencord-common/Cargo.toml` (add `serde_json` as a dev-dependency, workspace).

**Interfaces:**
- Produces (module `opencord_common::upload`):
  - `pub const MIB: u64 = 1_048_576;`
  - `pub const PIECE_BYTES: u32 = 8_388_608;`
  - `pub const MAX_ATTACHMENTS: usize = 10;`
  - `pub const MAX_UNFINISHED_PER_USER: u32 = 10;`
  - `pub const EVERYONE_DEFAULT_BYTES: u64 = 52_428_800;`
  - `pub const MIN_ROLE_LIMIT_BYTES: u64 = MIB;`
  - `pub const MAX_FILE_MB_DEFAULT: u64 = 4096;`
  - `pub const MAX_FILE_MB_RANGE: RangeInclusive<u64> = 1..=102_400;`
  - `pub const VOICE_MAX_SECONDS_DEFAULT: u32 = 300;`
  - `pub const VOICE_MAX_SECONDS_RANGE: RangeInclusive<u32> = 30..=1_800;`
  - `pub const TEMP_HEADROOM_BYTES: u64 = 512 * MIB;`
  - `pub const POSTER_MAX_BYTES: usize = 2 * 1_048_576;`
  - `pub const IMAGE_PROCESS_MAX_BYTES: u64 = 100 * MIB;`
  - `pub const BLOCKED_MAX_ENTRIES: usize = 100;`
  - `pub const BLOCKED_ENTRY_MAX_CHARS: usize = 20;`
  - `pub const FILENAME_MAX_CHARS: usize = 200;`
  - `pub const VOICE_FILENAME: &str = "voice-message.ogg";`
  - `pub fn effective_upload_limit(is_owner: bool, everyone: u64, roles: impl IntoIterator<Item = Option<u64>>, ceiling: u64) -> u64`
  - `pub fn voice_max_bytes(max_seconds: u32) -> u64` returns `max_seconds as u64 * 8_000 + 65_536`.
  - `pub fn sanitize_filename(input: &str) -> String`
  - `pub fn normalize_extension(input: &str) -> Result<String, ValidationError>`
  - `pub fn normalize_blocked_list(entries: &[String]) -> Result<Vec<String>, ValidationError>`
  - `pub fn is_blocked(sanitized_filename: &str, blocked: &[String]) -> bool`
  - `pub const BLOCK_PROGRAMS: [&str; 31]` (the spec §9 list, in that order).
- Produces (`limits.rs`): `pub const UPLOAD_CREATE_RATE: RateLimit = RateLimit { burst: 20, period: Duration::from_secs(60) };`
- Produces (`permissions.rs`): `const SEND_VOICE_MESSAGES = 1 << 14;`. `DEFAULT_EVERYONE` gains `ATTACH_FILES` and `SEND_VOICE_MESSAGES`. Remove the "Reserved for attachments." comment on `ATTACH_FILES`.

Rules the functions implement:
- **`effective_upload_limit`:**
  - `is_owner` → `ceiling`.
  - Otherwise `max(everyone, every Some(role))`, then `min(…, ceiling)`.
  - `None` roles are skipped.
- **`sanitize_filename`:**
  1. Keep only the part after the last `/` or `\`.
  2. Drop control characters (`char::is_control`).
  3. Trim trailing `.` and spaces, and leading spaces.
  4. If the stem before the first `.` equals (ignoring case) `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9` or `LPT1`–`LPT9`, prefix `_`.
  5. If longer than 200 chars, cut the stem and keep the extension (the text after the last `.` if it is ≤ 20 chars) so the total is 200.
  6. Empty → `file`.
- **`normalize_extension`:**
  - Trim, strip one leading `.`, lowercase.
  - Must be 1–20 chars of `[a-z0-9.]`, not starting or ending with `.`, and without `..`.
  - Otherwise `ValidationError` (use the existing `ValidationError` type in `validation.rs`; read it first).
- **`normalize_blocked_list`:** normalize each entry, drop duplicates, sort, at most 100 (else error).
- **`is_blocked`:** true when the lowercase filename ends with `"." + entry` for any entry.

- [x] **Step 1: Write the shared fixture `tests/upload_rules.json`.**

```json
{
  "limits": [
    {"owner": false, "everyone": 52428800, "roles": [], "ceiling": 4294967296, "expected": 52428800},
    {"owner": false, "everyone": 52428800, "roles": [null, 104857600], "ceiling": 4294967296, "expected": 104857600},
    {"owner": false, "everyone": 52428800, "roles": [10485760], "ceiling": 4294967296, "expected": 52428800},
    {"owner": false, "everyone": 52428800, "roles": [9999999999999], "ceiling": 4294967296, "expected": 4294967296},
    {"owner": true, "everyone": 52428800, "roles": [], "ceiling": 4294967296, "expected": 4294967296}
  ],
  "filenames": [
    {"input": "photo.jpg", "expected": "photo.jpg"},
    {"input": "../../etc/passwd", "expected": "passwd"},
    {"input": "C:\\Users\\a\\x.png", "expected": "x.png"},
    {"input": "evil.exe. . ", "expected": "evil.exe"},
    {"input": "con.txt", "expected": "_con.txt"},
    {"input": "Lpt9", "expected": "_Lpt9"},
    {"input": "a\u0000b\u0007.txt", "expected": "ab.txt"},
    {"input": "   ", "expected": "file"},
    {"input": "", "expected": "file"}
  ],
  "extensions": [
    {"input": ".EXE", "expected": "exe"},
    {"input": "tar.gz", "expected": "tar.gz"},
    {"input": "a..b", "expected": null},
    {"input": "", "expected": null},
    {"input": "toolongextension12345", "expected": null},
    {"input": "ex e", "expected": null}
  ],
  "blocked": [
    {"filename": "setup.EXE", "list": ["exe"], "expected": true},
    {"filename": "archive.tar.gz", "list": ["tar.gz"], "expected": true},
    {"filename": "archive.gz", "list": ["tar.gz"], "expected": false},
    {"filename": "exe", "list": ["exe"], "expected": false},
    {"filename": "notes.txt", "list": [], "expected": false}
  ]
}
```

Add one generated case to the Rust test: a 300-char name `"a".repeat(296) + ".png"` sanitizes to 200 chars ending in `.png`.

- [x] **Step 2: Write `tests/upload_rules.rs`.**
  - It reads the JSON with `include_str!("upload_rules.json")` and `serde_json`, and asserts every case: `limits`, `filenames`, `extensions` (`null` → `Err`), `blocked`.
  - Also add unit tests in `upload.rs`: `voice_max_bytes(300) == 2_465_536`; `normalize_blocked_list` sorts and dedupes `["EXE", ".exe", "bat"]` → `["bat", "exe"]`; 101 entries → `Err`.
- [x] **Step 3: Update `permissions.rs` tests.**
  - `bits_match_the_protocol` asserts `SEND_VOICE_MESSAGES.bits() == 1 << 14` and `ATTACH_FILES.bits() == 1 << 10`.
  - New `everyone_may_attach_files_and_send_voice_messages`.
- [x] **Step 4:** Run `cargo test -p opencord-common`. Expected: FAIL (module missing).
- [x] **Step 5:** Implement. Add `pub mod upload;` to `lib.rs`.
- [x] **Step 6:** Run `cargo test -p opencord-common`. Expected: PASS. Then run the whole workspace tests. Server tests asserting the old `DEFAULT_EVERYONE` may fail: update only those assertions and note them in the ledger.
- [x] **Step 7: Commit:** `feat: upload limits, filename and extension rules (A0)`.

### Task 3: Encrypted blob format

**Files:**
- Create: `crates/opencord-common/src/blob.rs`.
- Modify: `crates/opencord-common/Cargo.toml` (add `chacha20poly1305.workspace = true`, `getrandom.workspace = true`), `lib.rs`.

**Interfaces:**
- Produces (module `opencord_common::blob`):
  - `pub const MAGIC: [u8; 4] = *b"OCA1";`
  - `pub const HEADER_LEN: u64 = 8;`
  - `pub const SEGMENT_PLAIN: u64 = 65_536;`
  - `pub const SEGMENT_OVERHEAD: u64 = 40;`
  - `pub const SEGMENT_SEALED: u64 = 65_576;`
  - `#[repr(u8)] #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum BlobKind { File = 0, Thumbnail = 1, Cache = 2 }`
  - `#[derive(Clone)] pub struct BlobKey([u8; 32])` with `pub fn random() -> Self`, `pub fn from_bytes(bytes: [u8; 32]) -> Self`, `pub fn as_bytes(&self) -> &[u8; 32]`, and a `Debug` that prints `BlobKey(..)`.
  - `pub fn header() -> [u8; 8]`
  - `pub fn segment_count(plain_len: u64) -> u64` = `max(1, ceil(plain_len / 65536))`
  - `pub fn sealed_len(plain_len: u64) -> u64` = `8 + segment_count × 40 + plain_len`
  - `pub fn seal_segment(key: &BlobKey, kind: BlobKind, blob_id: i64, index: u64, last: bool, plain: &[u8]) -> Vec<u8>`
  - `pub fn open_segment(key: &BlobKey, kind: BlobKind, blob_id: i64, index: u64, last: bool, sealed: &[u8]) -> Result<Vec<u8>, BlobError>`
  - `pub fn seal_all(key: &BlobKey, kind: BlobKind, blob_id: i64, plain: &[u8]) -> Vec<u8>`: header plus all segments; empty input gives one empty last segment.
  - `pub fn open_all(key: &BlobKey, kind: BlobKind, blob_id: i64, sealed: &[u8]) -> Result<Vec<u8>, BlobError>`
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct SealedSpan { pub first_segment: u64, pub last_segment: u64, pub sealed_start: u64, pub sealed_end: u64, pub skip: u64, pub take: u64 }`
  - `pub fn locate(plain_len: u64, start: u64, end_inclusive: u64) -> SealedSpan`
    - `sealed_start = 8 + first_segment × 65576`;
    - `sealed_end` = the end (exclusive) of `last_segment`, which for the final segment is `sealed_len(plain_len)`;
    - `skip = start - first_segment × 65536`;
    - `take = end_inclusive - start + 1`.
  - `pub fn open_span(key: &BlobKey, kind: BlobKind, blob_id: i64, plain_len: u64, span: &SealedSpan, sealed: &[u8]) -> Result<Vec<u8>, BlobError>`: decrypts the consecutive segments in `sealed` (which starts at `span.sealed_start`), then returns exactly `take` bytes after `skip`.
  - `#[derive(Debug, thiserror::Error, PartialEq, Eq)] pub enum BlobError { #[error("the file is not an Opencord blob")] Header, #[error("a part of the file could not be opened")] Segment, #[error("the file is shorter than its size")] Truncated }`

The AAD is 22 bytes: `MAGIC || kind as u8 || blob_id.to_be_bytes() || index.to_be_bytes() || last as u8`. Use `XChaCha20Poly1305` with `Payload { msg, aad }`, as `crates/opencord-server/src/secrets.rs` does. Nonces: 24 bytes from `getrandom`; look at `crates/opencord-server/src/random.rs` for the call the codebase uses with `getrandom` 0.4.

- [x] **Step 1: Write the tests** (in `blob.rs`, `#[cfg(test)]`):
  - `round_trips_every_size`: sizes `[0, 1, 65_535, 65_536, 65_537, 3 × 65_536 + 5]`.
    - `open_all(seal_all(x)) == x`;
    - `seal_all(x).len() as u64 == sealed_len(len)`;
    - `sealed_len(0) == 48`, `sealed_len(65_536) == 65_584`.
  - `tampering_is_detected`: flip one ciphertext byte → `Err(Segment)`.
  - `truncation_at_a_segment_boundary_is_detected`: seal 2 × 65,536 bytes, drop the last segment → `open_all` errors.
  - `segments_cannot_be_swapped_between_files_or_kinds`: open a segment sealed for id 1 with id 2, or as `Thumbnail` → error.
  - `locate_and_open_span_return_exact_ranges`: for a 200,000-byte random file, every `(start, end)` in `[(0,0), (65_535, 65_536), (199_999, 199_999), (10, 150_000)]` gives `&plain[start..=end]`.
- [x] **Step 2:** `cargo test -p opencord-common blob`. Expected: FAIL.
- [x] **Step 3:** Implement.
- [x] **Step 4:** `cargo test -p opencord-common blob`. Expected: PASS.
- [x] **Step 5: Commit:** `feat: the encrypted blob format (A0)`.

### Task 4: Type sniffing and waveform values

**Files:** Create `crates/opencord-common/src/sniff.rs` and `crates/opencord-common/src/waveform.rs`; modify `lib.rs`.

**Interfaces:**
- **`opencord_common::sniff`:**
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Sniffed { Png, Jpeg, Gif, WebP, Mp4, QuickTime, WebM, Matroska, M4a, Ogg, Mp3, Flac, Wav, Unknown }`
  - `pub fn sniff(head: &[u8]) -> Sniffed` (looks at up to the first 64 bytes)
  - `impl Sniffed { pub fn content_type(self) -> &'static str; pub fn kind(self) -> SniffKind }`
  - `pub enum SniffKind { Image, Video, Audio, File }`
- **`opencord_common::waveform`:**
  - `pub const WAVEFORM_LEN: usize = 128;`
  - `pub fn level_byte(rms: f32) -> u8`
  - `pub fn level_unit(rms: f32) -> f32` (the same mapping in 0..=1)
  - `pub fn waveform(samples: &[f32], len: usize) -> Vec<u8>`

**Sniff table** (check in this order):

| Test on `head` | Result | content_type | kind |
|---|---|---|---|
| starts `89 50 4E 47 0D 0A 1A 0A` | Png | image/png | Image |
| starts `FF D8 FF` | Jpeg | image/jpeg | Image |
| starts `GIF87a` or `GIF89a` | Gif | image/gif | Image |
| `[0..4]=RIFF` and `[8..12]=WEBP` | WebP | image/webp | Image |
| `[0..4]=RIFF` and `[8..12]=WAVE` | Wav | audio/wav | Audio |
| `[4..8]=ftyp`, brand `[8..12]` ∈ {`M4A `, `M4B `, `M4P `, `F4A `} | M4a | audio/mp4 | Audio |
| `[4..8]=ftyp`, brand `qt  ` | QuickTime | video/quicktime | Video |
| `[4..8]=ftyp` (any other brand) | Mp4 | video/mp4 | Video |
| starts `1A 45 DF A3` and the bytes `webm` appear in the first 64 | WebM | video/webm | Video |
| starts `1A 45 DF A3` | Matroska | video/x-matroska | Video |
| starts `OggS` | Ogg | audio/ogg | Audio |
| starts `fLaC` | Flac | audio/flac | Audio |
| starts `ID3`, or `b[0]==0xFF && b[1]&0xE0==0xE0 && b[1]&0x06!=0` | Mp3 | audio/mpeg | Audio |
| anything else | Unknown | application/octet-stream | File |

**Waveform:**
- `level_unit(rms) = clamp((20·log10(max(rms, 1e-9)) + 60) / 60, 0, 1)`; `level_byte = (level_unit × 255).round() as u8`.
- `waveform(samples, len)`: slice `i` covers `samples[i·n/len .. (i+1)·n/len]` (integer maths, `n = samples.len()`). Value = `level_byte(rms of the slice)`; an empty slice is 0. Always returns exactly `len` values.

- [x] **Step 1: Write the tests.**
  - One sniff test per table row, using literal byte arrays.
  - A 3-byte input is Unknown, and nothing panics on an empty slice.
  - `level_byte(1.0) == 255`; `level_byte(0.001) == 0`; `level_byte(0.0316) == 128 ± 1` (−30 dB).
  - `waveform(&[], 128)` is 128 zeros.
  - Half silence then half full-scale sine gives `w[0] == 0` and `w[127] > 240`.
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [x] **Step 5: Commit:** `feat: file sniffing and voice waveform values (A0)`.

### Task 5: Migration, config, secrets location and guard

**Files:**
- Create: `crates/opencord-server/migrations/0008_attachments.sql`.
- Modify: `crates/opencord-server/src/{config.rs,secrets.rs,server.rs,db/mod.rs,bootstrap.rs,db/roles.rs,guild.rs}`, `opencord.toml`, `.sqlx/`.

**Interfaces:**
- **Config:**
  - `pub struct UploadsSection { pub max_file_mb: u64, pub local_dir: Option<PathBuf> }`, with `Default` = `{ 4096, None }`, `#[serde(default, deny_unknown_fields)]`.
  - `Config` gets `pub uploads: UploadsSection`; `ServerSection` gets `pub secrets_file: Option<PathBuf>`.
  - `impl Config { pub fn secrets_file(&self) -> PathBuf; pub fn attachments_dir(&self) -> PathBuf; pub fn uploads_temp_dir(&self) -> PathBuf; pub fn max_file_bytes(&self) -> u64 }`, giving:
    - `secrets_file`: the setting or `data_dir/secrets.key`;
    - `attachments_dir`: `local_dir` or `data_dir/attachments`;
    - `uploads_temp_dir`: `data_dir/uploads`;
    - `max_file_bytes`: `max_file_mb × 1,048,576`.
- **Env:** `OPENCORD_SECRETS_FILE`, `OPENCORD_UPLOADS_MAX_FILE_MB`, `OPENCORD_UPLOADS_LOCAL_DIR`.
- `resolve_paths` makes relative `secrets_file` and `local_dir` relative to the config file's folder (like `data_dir`).
- New `ConfigError::Invalid { key: &'static str, message: String }` returned by load when `max_file_mb` is outside 1–102,400.
- **Secrets:** change to `Secrets::load_or_generate(path: &Path, may_generate: bool) -> Result<Self, SecretsError>`.
  - New variant: `#[error("the secrets key {path} is missing, but the database holds data sealed with it: restore the file or point [server] secrets_file at it")] Missing { path: String }`.
  - `may_generate == false` and file missing → `Missing`.
- `pub async fn holds_sealed_data(pool: &SqlitePool) -> Result<bool, sqlx::Error>` in `db/mod.rs`. It is true if any row exists in `domain_certificates`, `acme_accounts`, `dns_providers`, `lavalink_nodes` or `attachments`, or in `storage_backends` with `secret_enc IS NOT NULL`, or in `server_meta` with `key = 'tunnel.token'`.
- **Roles:** `RoleRow` gains `pub max_upload_bytes: Option<i64>` (list, insert and update queries include it). `guild::Role` gains `pub max_upload_bytes: Option<u64>`, and `to_proto` fills it. `bootstrap.rs::create_server` gives @everyone `Some(EVERYONE_DEFAULT_BYTES)`.

- [x] **Step 1: Write the migration** exactly:

```sql
-- Media uploads (design 2026-10-18): storage backends, attachments, and the
-- queue of stored objects to delete. Files are encrypted; their keys are
-- sealed with secrets.key (key_enc), as are S3 secrets (secret_enc).

CREATE TABLE storage_backends (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('local', 's3')),
    name TEXT NOT NULL,
    endpoint TEXT,
    region TEXT,
    bucket TEXT,
    key_prefix TEXT NOT NULL DEFAULT '',
    path_style BOOLEAN NOT NULL DEFAULT FALSE,
    allow_http BOOLEAN NOT NULL DEFAULT FALSE,
    access_key_id TEXT,
    secret_enc BLOB,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    last_error TEXT,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX storage_backends_one_active ON storage_backends (active) WHERE active;

INSERT INTO storage_backends (id, kind, name, active, created_at)
VALUES (1, 'local', 'This server', TRUE, 0);

CREATE TABLE attachments (
    id INTEGER PRIMARY KEY,
    uploader_id INTEGER NOT NULL REFERENCES users (id),
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    message_id INTEGER REFERENCES messages (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL
        CHECK (state IN ('uploading', 'processing', 'ready', 'failed', 'sent')),
    kind TEXT NOT NULL CHECK (kind IN ('file', 'image', 'video', 'audio', 'voice')),
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
    size_bytes INTEGER NOT NULL,
    received_bytes INTEGER NOT NULL DEFAULT 0,
    storage_id INTEGER NOT NULL REFERENCES storage_backends (id),
    key_enc BLOB NOT NULL,
    width INTEGER,
    height INTEGER,
    duration_ms INTEGER,
    animated BOOLEAN NOT NULL DEFAULT FALSE,
    waveform BLOB,
    has_thumbnail BOOLEAN NOT NULL DEFAULT FALSE,
    thumbnail_width INTEGER,
    thumbnail_height INTEGER,
    error_code INTEGER,
    error_message TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX attachments_by_message ON attachments (message_id);
CREATE INDEX attachments_by_state ON attachments (state, updated_at);
CREATE INDEX attachments_by_storage ON attachments (storage_id);
CREATE INDEX attachments_by_uploader ON attachments (uploader_id, state);

CREATE TABLE blob_deletions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    storage_id INTEGER NOT NULL,
    object_key TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL DEFAULT 0
);

-- Every way a row goes (cancel, expiry, message delete, channel cascade)
-- queues its objects for deletion.
CREATE TRIGGER attachments_queue_blobs AFTER DELETE ON attachments
BEGIN
    INSERT INTO blob_deletions (storage_id, object_key)
    VALUES (OLD.storage_id, 'a/' || OLD.id);
    INSERT INTO blob_deletions (storage_id, object_key)
    SELECT OLD.storage_id, 't/' || OLD.id WHERE OLD.has_thumbnail;
END;

ALTER TABLE roles ADD COLUMN max_upload_bytes INTEGER;

-- Attach files (bit 10) was reserved and did nothing until now; Send voice
-- messages (bit 14) is new. Existing servers' @everyone gets both, and 50 MB.
UPDATE roles
SET max_upload_bytes = 52428800,
    permissions = permissions | 1024 | 16384
WHERE id = (SELECT CAST(value AS INTEGER) FROM server_meta WHERE key = 'everyone_role_id');
```

- [x] **Step 2: Write the failing tests.**
  - **`config.rs` unit tests:**
    - `uploads_defaults_and_env`: defaults give `max_file_bytes() == 4096 × MIB` and `attachments_dir() == data_dir/attachments`; `OPENCORD_UPLOADS_MAX_FILE_MB=10` gives `10 × MIB`; `OPENCORD_SECRETS_FILE=keys/s.key` resolves against the config folder.
    - `max_file_mb_out_of_range_is_refused`: 0 and 102,401 → `ConfigError::Invalid`.
  - **`secrets.rs`:**
    - `a_missing_key_is_made_only_when_allowed`: `load_or_generate(path, true)` creates the file; deleting it and calling `load_or_generate(path, false)` → `Missing`.
  - **`db` test using `test_pool()`:**
    - `holds_sealed_data_sees_lavalink_and_storage_secrets`: false on a fresh DB; true after inserting one `lavalink_nodes` row (read migration 0005 for its columns).
  - **Migration test** in `db/mod.rs`:
    - `migration_gives_everyone_attach_files_voice_and_50_mb`:
      1. Build a `sqlx::migrate::Migrator` holding only the migrations with `version <= 7` (clone `sqlx::migrate!("./migrations")` and filter its public `migrations` field into a new `Migrator`), and run it on a fresh pool.
      2. Insert `server_meta('everyone_role_id', '1')` and a role row `(1, '@everyone', position 0, permissions 3)`.
      3. Run the full `sqlx::migrate!("./migrations")`, which applies 0008.
      4. Assert the role's permissions are `3 | 1024 | 16384` and `max_upload_bytes = 52428800`.
    - Also: deleting an attachments row inserts the `a/` key (and the `t/` key when `has_thumbnail`) into `blob_deletions`. Deleting the **channel** (cascade) does the same.
- [x] **Step 3:** Run → FAIL.
- [x] **Step 4: Implement.**
  - **`DEFAULT_CONFIG_TOML`:** add under `[server]`:

```toml
# The key that seals certificates, tokens, passwords and every file's key.
# Back it up: without it, uploaded files cannot be read. To protect files
# against a stolen disk, keep it on another disk. Default: secrets.key in
# the data directory. OPENCORD_SECRETS_FILE
# secrets_file = "/run/secrets/opencord-secrets.key"
```

    and a new section after `[music]`:

```toml
[uploads]
# The largest file anyone may upload, in MB (1 MB = 1,048,576 bytes): the
# owner's own limit and the ceiling for every role's limit. 1 to 102400.
# OPENCORD_UPLOADS_MAX_FILE_MB
max_file_mb = 4096
# Where files on this server's own storage are kept. Default: "attachments"
# in the data directory. OPENCORD_UPLOADS_LOCAL_DIR
# local_dir = "/mnt/storage/opencord-attachments"
```

  - **`server.rs`:** replace `Secrets::load_or_generate(&data_dir)` with `Secrets::load_or_generate(&config.secrets_file(), !db::holds_sealed_data(&pool).await?)`.
  - Regenerate sqlx data.
- [x] **Step 5:** `cargo test -p opencord-server --lib` and `cargo test -p opencord-server --tests`. Expected: PASS.
- [x] **Step 6: Commit:** `feat: attachments schema, uploads config and the secrets key location (A0)`.

**A0 stop.** Run the full checks, write **D52** ("Media uploads: protocol, rules, encryption format, schema"), update the ledger, and report.

---

# Milestone A1: Server storage, uploads, processing, downloads

### Task 6: Storage backends (local)

**Files:**
- Create: `crates/opencord-server/src/attachments/mod.rs` (module declarations only for now), `src/attachments/storage/mod.rs`, `src/attachments/storage/local.rs`.
- Modify: `src/lib.rs` (`pub mod attachments;`), `Cargo.toml` (`fs4 = "1.1.0"`).

**Interfaces:**
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ObjectKind { File, Thumbnail }`
- `pub fn object_key(kind: ObjectKind, id: i64) -> String`: `"a/{id}"` or `"t/{id}"`.
- `#[derive(Debug, thiserror::Error)] pub enum StorageError { #[error("not found")] NotFound, #[error("storage is unavailable: {0}")] Unavailable(String), #[error("storage is full")] Full }`. Map `ENOSPC` (`std::io::ErrorKind::StorageFull`) to `Full`.
- `pub enum Backend { Local(LocalStorage), S3(s3::S3Storage) }`. Add the S3 variant in Task 14; until then it is only `Local`. Methods, each dispatching with `match`, with no `async_trait`:
  - `pub async fn put_file(&self, key: &str, path: &Path) -> Result<(), StorageError>`: local does `rename`, falling back to copy + `sync_all` + remove when the rename crosses devices.
  - `pub async fn put_bytes(&self, key: &str, bytes: Vec<u8>) -> Result<(), StorageError>`
  - `pub async fn get_range(&self, key: &str, start: u64, end: u64) -> Result<Bytes, StorageError>` (`end` exclusive; local seeks and `read_exact`s)
  - `pub async fn size(&self, key: &str) -> Result<Option<u64>, StorageError>`
  - `pub async fn delete(&self, key: &str) -> Result<(), StorageError>`: idempotent; NotFound is `Ok`.
- `pub struct LocalStorage { root: PathBuf }`, `pub fn new(root: PathBuf) -> Self`, and `fn path(&self, key: &str) -> Result<PathBuf, StorageError>`:
  - `"a/123"` maps to `root/a/{shard}/123`, with `shard = format!("{:02x}", (id >> 22) & 0xff)`;
  - parse the prefix and id from the key, and **refuse anything else** (`Unavailable("bad key")`), so no path ever comes from input;
  - create parent folders on write.

- [x] **Step 1: Write tests** (`#[tokio::test]` in `local.rs` with `tempfile`):
  - `local_round_trip_and_ranges`: `put_bytes` then `get_range(…, 2, 5)`, `size`, and `delete` twice (the second is Ok); `size` is then `None`.
  - `put_file_moves_the_file`: the source path is gone and the content is readable.
  - `keys_cannot_escape_the_root`: `path("../x")`, `path("a/../../x")` and `path("z/1")` are `Err`.
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [x] **Step 5: Commit:** `feat: the local storage backend (A1)`.

### Task 7: Attachment and storage queries; upload settings; the Attachments service

**Files:**
- Create: `src/db/attachments.rs`, `src/db/storage.rs`, `src/attachments/settings.rs`.
- Modify: `src/db/mod.rs`, `src/guild.rs` (`upload_settings` field, loaded in `Guild::load`), `src/attachments/mod.rs`, `src/state.rs`, `src/server.rs`, `.sqlx/`.

**Interfaces:**
- **`db::attachments`:**
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct AttachmentRow`: every column of the table, SQL types mapped as the existing modules do (`bool` with `AS "x: bool"`, nullable → `Option`).
  - Functions (all `conn: &mut SqliteConnection`):
    - `insert_upload(conn, row: &AttachmentRow)`
    - `find(conn, id) -> Option<AttachmentRow>`
    - `set_received(conn, id, received: i64, now: i64)`
    - `set_state(conn, id, state: &str, now: i64)`
    - `set_ready(conn, id, ready: &ReadyFields, now: i64)`, with `pub struct ReadyFields { pub kind: String, pub content_type: String, pub filename: String, pub storage_id: i64, pub width: Option<i64>, pub height: Option<i64>, pub duration_ms: Option<i64>, pub animated: bool, pub waveform: Option<Vec<u8>>, pub has_thumbnail: bool, pub thumbnail_width: Option<i64>, pub thumbnail_height: Option<i64> }`
    - `set_failed(conn, id, code: i32, message: &str, now: i64)`
    - `mark_sent(conn, ids: &[i64], message_id: i64)`: sets `position` = index in `ids` and `state='sent'`.
    - `for_messages(conn, message_ids: &[i64]) -> Vec<AttachmentRow>`: sent rows, ordered by `message_id, position`. For the `IN` list use one query per batch of ≤ 100 ids built with `sqlx::QueryBuilder`; this one query may be unchecked, so note it in a comment.
    - `delete(conn, id)`
    - `delete_for_message(conn, message_id)`
    - `count_unfinished(conn, uploader_id) -> i64` (states uploading/processing)
    - `expired(conn, now: i64) -> Vec<i64>`: uploading with `updated_at < now − 24h`; ready with `updated_at < now − 24h`; failed with `updated_at < now − 1h`.
    - `in_state(conn, state: &str) -> Vec<AttachmentRow>`
    - `usage(conn) -> Vec<(i64, i64, i64)>`: (storage_id, count, bytes) over ready and sent.
    - `not_on(conn, storage_id: i64, after_id: i64, limit: i64) -> Vec<AttachmentRow>` (ready and sent, ordered by id)
    - `count_not_on(conn, storage_id) -> (i64, i64)`
    - `set_storage_if(conn, id, from: i64, to: i64) -> bool` (rows affected == 1)
    - `count_on(conn, storage_id) -> i64` (any state)
- **`db::storage`:**
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct StorageRow` (all columns).
  - `list`, `insert`, `update`, `set_active(conn, id)`: one transaction that clears every `active`, then sets this one.
  - `delete`, `set_last_error(conn, id, Option<&str>)`.
  - Blob queue: `queue_blob(conn, storage_id, key: &str)`, `due_blobs(conn, now, limit) -> Vec<BlobRow { id, storage_id, object_key, attempts }>`, `blob_done(conn, id)`, `blob_retry(conn, id, attempts: i64, next_at: i64)`.
- **`attachments::settings`:**
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct UploadSettings { pub blocked_extensions: Vec<String>, pub voice_message_max_seconds: u32 }`, with `Default` = `{ vec![], 300 }`.
  - `pub async fn load(conn) -> Result<Self, meta::MetaError>`, `pub async fn save(&self, conn) -> Result<(), sqlx::Error>`, `pub fn to_proto(&self, max_file_bytes: u64) -> proto::UploadSettings`.
  - Stored in `server_meta` as `uploads.blocked_extensions` (JSON array via `serde_json`) and `uploads.voice_message_max_seconds`. Follow `voice/settings.rs`; a missing key gives the default.
  - `to_proto` fills `max_attachments = 10` and `piece_bytes = PIECE_BYTES`.
  - `Guild` gets `pub upload_settings: UploadSettings`, and `pub fn upload_limit(&self, user_id: i64, ceiling: u64) -> u64` using `effective_upload_limit` (everyone = the @everyone role's `max_upload_bytes`, defaulting to `EVERYONE_DEFAULT_BYTES` if somehow missing).
- **`attachments::Attachments`** (in `mod.rs`), held as `AppState.attachments: crate::attachments::Attachments`:

```rust
pub struct Attachments {
    temp_dir: PathBuf,
    max_file_bytes: u64,
    backends: std::sync::RwLock<HashMap<i64, Arc<Backend>>>,
    active: std::sync::atomic::AtomicI64,
    jobs: Arc<tokio::sync::Semaphore>,          // 2 permits
    upload_locks: std::sync::Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
    posters: std::sync::Mutex<HashMap<i64, Vec<u8>>>,
    hold_processing: std::sync::atomic::AtomicBool, // test hook
    http: reqwest::Client,
}
impl Attachments {
    pub fn new(config: &Config, http: reqwest::Client) -> Self; // local backend id 1 at config.attachments_dir(), active 1
    pub async fn load(&self, conn: &mut SqliteConnection, secrets: &Secrets) -> Result<(), anyhow::Error>; // A3 adds S3 rows
    pub fn backend(&self, id: i64) -> Option<Arc<Backend>>;
    pub fn active(&self) -> (i64, Arc<Backend>);
    pub fn temp_path(&self, id: i64) -> PathBuf;   // temp_dir/{id}.part
    pub fn temp_dir(&self) -> &Path;
    pub fn max_file_bytes(&self) -> u64;
    pub async fn lock_upload(&self, id: i64) -> tokio::sync::OwnedMutexGuard<()>;
}
```

  - `AppState::new` builds `Attachments::new(&config, crate::tls::http_client()?)`. Mirror how `network` gets its client; read `state.rs`.
  - `server.rs` calls `state.attachments.load(…, state.certificates.secrets())` after building the state.
- **Server error constructors** (`error.rs`):
  - `file_too_large(limit_bytes: u64)`: "files can be at most {n} MB here", where n = limit / MiB rounded down, minimum 1.
  - `file_type_blocked(extension: &str)`: "{ext} files can't be uploaded here".
  - `upload_offset_mismatch()`
  - `storage_unavailable(message: impl Into<String>)`
  - `storage_full()`: "the server is out of space for uploads".
  - `voice_message_invalid(message)`
  - `voice_message_too_long(max_seconds: u32)`: "voice messages can be at most {m}:{ss} long".

- [x] **Step 1: Write tests** (unit, `test_pool()`):
  - `upload_rows_move_through_their_states`: insert uploading → set_received → processing → ready → mark_sent; `for_messages` returns them in position order.
  - `expired_picks_each_state_by_its_age`: 24 h uploading and ready, 1 h failed, and fresh rows are kept.
  - `one_backend_is_active`: `set_active(2)` leaves exactly one active.
  - `upload_settings_round_trip_and_default`.
  - `upload_limit_uses_roles_and_ceiling`: a guild built in the test with @everyone 50 MiB and a role with 200 MiB gives the member 200 MiB, a non-member of that role 50 MiB, and the owner the ceiling.
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement, then regenerate sqlx data. **Step 4:** Run → PASS.
- [x] **Step 5: Commit:** `feat: attachment queries, upload settings and the attachments service (A1)`.

### Task 8: Creating uploads and receiving pieces

**Files:**
- Create: `src/handlers/uploads.rs`, `src/attachments/temp.rs`, `src/attachments/http.rs`.
- Modify: `src/handlers/mod.rs` (dispatch `CreateUpload`, `CancelUpload`), `src/http.rs` (merge `attachments::http::routes()`), `src/media.rs` (make `authorize`, `reply`, `reply_error` `pub(crate)`; extend the status mapping), `src/rate_limit.rs` (`check_upload`), `tests/common/mod.rs`.
- Create: `tests/attachments.rs`.

**Interfaces:**
- **`handlers::uploads::create(ctx, proto::CreateUpload) -> Result<Response, ApiError>`.** Checks, in this order (each failure's error in brackets):
  1. kind is FILE or VOICE_MESSAGE [INVALID_ARGUMENT];
  2. `require_channel(VIEW_CHANNEL | SEND_MESSAGES)` plus the text-or-voice-with-chat rule `require_message_channel` uses (make that helper `pub(crate)` and reuse it);
  3. FILE needs `ATTACH_FILES`; VOICE needs `SEND_VOICE_MESSAGES` [FORBIDDEN];
  4. `filename = sanitize_filename(..)`; for VOICE the filename is always `voice-message.ogg`;
  5. FILE: blocked extension [FILE_TYPE_BLOCKED];
  6. FILE: `size_bytes > guild.upload_limit(user, ceiling)` [FILE_TOO_LARGE]; VOICE: `size_bytes > voice_max_bytes(max_seconds)` [VOICE_MESSAGE_TOO_LONG];
  7. `size_bytes == 0` is allowed for FILE only;
  8. `rate_limits.check_upload(user)` [RATE_LIMITED];
  9. `count_unfinished ≥ 10` [CONFLICT "finish or cancel your other uploads first"];
  10. `fs4::available_space(temp_dir) < size + 512 MiB` [STORAGE_FULL].

  Then:
  - `id = ids.next_id()`;
  - `key = BlobKey::random()`;
  - `key_enc = secrets.seal(&format!("attachment-key:{id}"), key.as_bytes())`;
  - create the temp file with the 8-byte header (`temp::create`);
  - insert the row (state uploading, storage_id = active id, kind `file` or `voice`, video hints into width/height/duration_ms clamped to 1–16,384 and ≤ 86,400,000);
  - answer `Response::Upload { upload_id, piece_bytes }`.
- **`handlers::uploads::cancel`:** own row in uploading, processing, ready or failed → delete the row and the temp file; else NOT_FOUND.
- **`attachments::temp`:**
  - `pub async fn create(path: &Path) -> io::Result<()>`
  - `pub async fn append_piece(path: &Path, key: &BlobKey, id: i64, offset: u64, total: u64, piece: &[u8]) -> io::Result<()>`:
    - truncate the file to `8 + (offset / 65536) × 65576` first, which discards half-written data from a failed attempt;
    - seal each 64 KiB chunk with index `offset/65536 + i` and `last = index == segment_count(total) − 1`;
    - for a zero-byte file, finish writes one empty last segment instead;
    - write, then `sync_data`.
  - `pub async fn read_plain(path: &Path, key: &BlobKey, id: i64, total: u64, limit: u64) -> io::Result<Vec<u8>>`: whole-file decrypt, refused when `total > limit`.
- **HTTP** (`attachments::http::routes() -> Router<Arc<AppState>>`):
  - **`PUT /media/uploads/{id}`**, header `Upload-Offset: <u64>`, body = the piece.
    - Read the body with `axum::body::to_bytes(body, PIECE_BYTES as usize + 1)`.
    - Authorize; the row must be the caller's and in uploading state.
    - Take `lock_upload(id)` for the whole handling.
    - `offset != received` → **409** with body `Response::UploadStatus` (current state and offset).
    - Piece length must equal `min(PIECE_BYTES, size − offset)`, and must be `> 0` unless `size == 0` [INVALID_ARGUMENT 400].
    - Then `append_piece`, `set_received`, and answer 200 `UploadStatus`.
  - **`GET /media/uploads/{id}`** → 200 `UploadStatus` for the caller's own row (any state; `attachment` filled when ready or sent; `error` when failed), else 404.
  - **`POST /media/uploads/{id}/finish`**: added in Task 9.
- **Status mapping in `reply_error`:**

  | Code | HTTP |
  |---|---|
  | FILE_TOO_LARGE | 413 |
  | FILE_TYPE_BLOCKED | 415 |
  | UPLOAD_OFFSET_MISMATCH | 409 |
  | STORAGE_UNAVAILABLE | 503 |
  | STORAGE_FULL | 507 |
  | VOICE_MESSAGE_INVALID, VOICE_MESSAGE_TOO_LONG | 400 |

- **`tests/common/mod.rs` gains:**
  - `pub async fn send_http_full(&self, method, path, headers, body) -> (u16, Vec<(String, String)>, Vec<u8>)`
  - `pub fn decode_response(bytes: &[u8]) -> proto::response::Result`
  - `pub fn png(width: u32, height: u32) -> Vec<u8>`: a grey RGB PNG made with the `image` crate.
  - `pub async fn upload(client: &mut TestClient, server: &TestServer, token: &str, channel_id: i64, name: &str, bytes: &[u8]) -> proto::Attachment`: CreateUpload, PUT every piece, POST finish, then `client.wait_for` UploadReady.

- [x] **Step 1: Write integration tests** in `tests/attachments.rs`:
  - `create_upload_checks_permissions_and_size`:
    - a member without ATTACH_FILES (the owner removes it from @everyone with UpdateRole first) → FORBIDDEN;
    - with it: 60 MiB → FILE_TOO_LARGE with "50 MB" in the message;
    - an 11th unfinished upload → CONFLICT;
    - a 21st create within a minute → RATE_LIMITED.

    (Blocked extensions are tested in Task 13, once the settings request exists.)
  - `pieces_must_arrive_in_order_and_have_the_right_size`: a 9 MiB upload; PUT offset 8 MiB first → 409 with `received_bytes == 0`; a 1-byte-short first piece → 400; the correct pieces → 200 with offsets 8 MiB then 9 MiB.
  - **`a_repeated_piece_gets_a_conflict_and_the_upload_continues`** (Review Focus 1): PUT piece 1 twice → second is 409 with offset 8 MiB; PUT piece 2 → 200; after finish (Task 9) the downloaded bytes equal the original (complete this assertion in Task 10).
  - `another_users_upload_is_not_found`.
  - `cancel_removes_the_row_and_temp_file`.
  - `nothing_plain_reaches_the_temp_folder`: upload a piece made of a repeated marker string; the temp file doesn't contain the marker.
- [x] **Step 2:** Run `cargo test -p opencord-server --test attachments`. Expected: FAIL.
- [x] **Step 3:** Implement. Regenerate sqlx data if queries changed.
- [x] **Step 4:** Run. Expected: PASS (except the parts marked for Task 9/10).
- [x] **Step 5: Commit:** `feat: creating uploads and receiving pieces (A1)`.

### Task 9: Finishing and processing

**Files:**
- Create: `src/attachments/process.rs`, `src/attachments/images.rs`, `src/attachments/voice.rs`.
- Modify: `src/attachments/http.rs` (finish route), `src/attachments/mod.rs` (`start`, `resume_processing`), `src/server.rs` (call `attachments::start(&state)`), `Cargo.toml` (`opus.workspace = true`; add `"gif"` to the workspace `image` features in the root `Cargo.toml`), `deploy/Dockerfile` (add `RUN apt-get update && apt-get install -y --no-install-recommends cmake && rm -rf /var/lib/apt/lists/*` to the build stage before `cargo build`), `src/server.rs` `ServerHandle` (test hooks).

**Interfaces:**
- **`POST /media/uploads/{id}/finish`**, optional body (poster ≤ 2 MiB, read with `to_bytes(body, POSTER_MAX_BYTES + 1)`).
  - The caller's own row.
  - State uploading with `received == size` → set processing, keep the poster in `posters`, spawn the job, answer **202** `UploadStatus{processing}`.
  - State processing, ready or failed → answer 200 with the current status.
  - Uploading but incomplete → 400.
- **`process::spawn_job(state: Arc<AppState>, id: i64)`:** `tokio::spawn`, acquire one of the 2 permits, then `run(&state, id)`. If `hold_processing` is set, return at once and leave the row processing.
- **`process::run(state, id) -> Result<(), ApiError>`:**
  1. Load the row and open the key: `Secrets::open(&format!("attachment-key:{id}"), &key_enc)` → `BlobKey`.
  2. Read the first segment's plaintext (`temp::read_plain` with a range, or decrypt segment 0 only) → `sniff`.
  3. Branch:
     - **voice** (row.kind == "voice"): `voice::check(&bytes, max_seconds) -> Result<VoiceInfo { duration_ms: u32, waveform: Vec<u8> }, ApiError>`. Kind voice, content type `audio/ogg`. If the sniff isn't Ogg → VOICE_MESSAGE_INVALID.
     - **image sniff and size ≤ 100 MiB**: `images::process(&bytes) -> Result<ImageInfo { width, height, animated, thumbnail: Vec<u8>, thumb_w, thumb_h }, ImageError>` inside `spawn_blocking`. `Err` → kind FILE with no thumbnail.
     - **video sniff**: kind VIDEO, width/height/duration from the row's hints. A poster (if any) → `images::thumbnail_from_poster(&poster)` in `spawn_blocking`; an error just means no thumbnail.
     - **audio sniff**: kind AUDIO.
     - **otherwise**: FILE. Content type from `sniff`.
  4. `(active_id, backend) = active()`.
  5. `backend.put_file(&object_key(File, id), &temp_path)`, then `put_bytes(&object_key(Thumbnail, id), seal_all(key, Thumbnail, id, &thumb))` when there is a thumbnail.
  6. `set_ready(…)`.
  7. Broadcast `UploadReady{attachment}` to `Audience::User(uploader)`.
  8. Remove the temp file if it still exists.

  Any `Err` → `set_failed(code, message)`, remove the temp file, broadcast `UploadFailed{upload_id, error}` to the uploader. A storage error maps to STORAGE_UNAVAILABLE or STORAGE_FULL.
- **`images::process(bytes: &[u8]) -> Result<ImageInfo, ImageError>`:**
  - `image::ImageReader::new(Cursor::new(bytes)).with_guessed_format()`, with `Limits` max width and height 16,384 and `max_alloc` 512 MiB.
  - Reject `w × h > 50,000,000` **before** decoding: read the dimensions with `into_dimensions()` on a second reader.
  - `into_decoder()`, `orientation()`, `DynamicImage::from_decoder`, `apply_orientation`.
  - Thumbnail: unchanged if both sides ≤ 800, else `resize(800, 800, FilterType::Triangle)`.
  - Encode JPEG q82 (`to_rgb8`) or PNG when `color().has_alpha()`.
  - `animated`: GIF → `GifDecoder` + `into_frames().take(2).count() == 2`; WebP → `WebPDecoder::has_animation()`.
  - Width and height reported are **after** orientation.
- **`images::thumbnail_from_poster(bytes) -> Result<(Vec<u8>, u32, u32), ImageError>`:** same limits (sides ≤ 8,192), always JPEG.
- **`voice::check(bytes: &[u8], max_seconds: u32) -> Result<VoiceInfo, ApiError>`:**
  - `opencord_common::sound::ogg::OpusFile::read(bytes)`; a read error → VOICE_MESSAGE_INVALID with its message.
  - `info.duration_ms > max_seconds × 1000` → VOICE_MESSAGE_TOO_LONG.
  - Decode every packet with `opus::Decoder::new(48_000, channels)` and `decode_float` into a 5,760 × channels buffer. Mix down to mono, drop the first `pre_skip` samples, keep `duration_ms × 48` samples.
  - `waveform(&pcm, 128)`.
- **`attachments::start(state: &Arc<AppState>)`:**
  - At boot: delete temp files with no uploading or processing row.
  - `spawn_job` every processing row (resume).
  - Start GC (Task 11).
- **Test hooks on `ServerHandle`** (doc-commented "Intended for tests", like the existing ones):
  - `pub fn hold_processing(&self, hold: bool)`
  - `pub async fn sweep_attachments(&self, now_ms: i64)` (Task 11)
  - `pub async fn drain_blob_deletions(&self, now_ms: i64)` (Task 11)

- [x] **Step 1: Write tests.**
  - **Unit, `images.rs`:**
    - `thumbnails_fit_800_and_keep_alpha_as_png`: a 1600×400 RGBA PNG → thumbnail 800×200 PNG.
    - `exif_rotation_is_applied`: build a JPEG with an Orientation=6 APP1 segment (use the 36-byte segment from Task 20, inserted after SOI). A 40×20 source reports 20×40.
  - **Unit, `voice.rs`:** build an Ogg Opus in the test with `opus::Encoder` (48 kHz mono, 20 ms frames: 1 s of a 440 Hz sine at amplitude 0.5, then 1 s of silence) and `ogg::write`. Put this builder in `tests/common/mod.rs` as `pub fn voice_ogg(seconds_tone: f32, seconds_silence: f32) -> Vec<u8>` so the integration tests reuse it.
    - `voice_check_measures_and_draws` gives `duration_ms ≈ 2000 (±40)`, the first half of the waveform > 150, the last 10 values < 20.
    - Garbage → VOICE_MESSAGE_INVALID; max 1 s → VOICE_MESSAGE_TOO_LONG.
  - **Integration (`tests/attachments.rs`):**
    - `an_image_becomes_ready_with_a_thumbnail`: `upload(png(1200, 900))` → kind IMAGE, 1200×900, `has_thumbnail`, thumb 800×600.
    - `a_zip_stays_a_file`.
    - `a_voice_message_gets_the_server_waveform`: CreateUpload VOICE with the test Ogg; ready attachment has `waveform.len() == 128`, `duration_ms` ≈ 2000, filename `voice-message.ogg`.
    - **`odd_images_never_crash_processing`** (Review Focus 3): a PNG header claiming 20,000×20,000 (craft the IHDR and fix the CRC; a truncated body is fine) → FILE; a truncated PNG → FILE; a 16-bit grey PNG → IMAGE; a 2-frame GIF → `animated == true`.
    - **`a_storage_failure_fails_the_upload_cleanly`** (Review Focus 5): start a server with `uploads.local_dir` pointing at an existing **file** (so directories can't be made). Upload → `UploadFailed` with STORAGE_UNAVAILABLE; GET status says failed; the temp folder is empty.
    - **`processing_resumes_after_a_restart`**: `hold_processing(true)`, finish (202), `server.restart(|_| {})`, reconnect, poll GET until ready (≤ 10 s).
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [x] **Step 5: Commit:** `feat: finishing uploads: thumbnails, voice checks and storage (A1)`.

### Task 10: Downloads

**Files:**
- Create: `src/attachments/range.rs`.
- Modify: `src/attachments/http.rs`, `tests/attachments.rs`.

**Interfaces:**
- **`range::parse(header: Option<&str>, len: u64) -> Result<Option<(u64, u64)>, RangeError>`** (inclusive end):
  - `None` → `Ok(None)`.
  - `bytes=a-b` (b clamped to len − 1), `bytes=a-`, `bytes=-n` (n ≥ len → whole).
  - `a ≥ len`, `len == 0`, a comma (several ranges), or anything malformed → `Err`.
- **`GET /media/attachments/{id}`:**
  1. Authorize the token.
  2. The row must be **sent**, and its message must exist with `deleted = FALSE`.
  3. `require_channel(VIEW_CHANNEL | READ_HISTORY)`: not found → 404; missing READ_HISTORY → 403.
  4. Parse the range: error → **416** with `Content-Range: bytes */{len}`.
  5. `blob::locate`.
  6. Stream the body: `futures_util::stream::unfold`, reading at most 16 segments per `get_range` call, decrypting with `open_span` logic segment by segment, trimming the first and last chunk, and yielding `Bytes`. `axum::body::Body::from_stream`.
  7. Status 200 or 206 with headers:
     - `Content-Type` (the row's), `Content-Length`;
     - `Accept-Ranges: bytes`;
     - `Content-Range: bytes a-b/len` on 206;
     - `Cache-Control: private, no-store`;
     - `X-Content-Type-Options: nosniff`;
     - `Content-Disposition: attachment; filename*=UTF-8''<percent-encoded filename>` (encode every byte outside `A-Za-z0-9-._~`).
  8. A backend error before the first byte → 502.
- **`GET /media/attachments/{id}/thumbnail`:** same checks; `has_thumbnail` must be true, else 404. Fetch the whole sealed thumbnail, `open_all`, `Content-Type: image/png` if the bytes start with the PNG magic, else `image/jpeg`.

- [x] **Step 1: Write tests.**
  - **Unit (`range.rs`):** `range_edges` (Review Focus 4) with every case above, including `len 0`, `bytes=0-0`, `bytes=-5` on a 3-byte file → (0, 2), `bytes=5-` on 5 bytes → Err, `bytes=0-1,3-4` → Err.
  - **Integration:**
    - `downloads_return_exact_bytes_and_ranges`: sizes 0, 65,536 and 200,000. Full GET equals the original; `bytes=65530-65540` equals the slice; `bytes=-10` on 200,000 equals the last 10. Also finish Task 8's repeated-piece test here: its download equals the original.
    - **`downloads_check_the_channel_at_request_time`** (Review Focus 2): after the owner hides the channel from @everyone with an overwrite, the member gets 404; with READ_HISTORY denied, 403; a deleted message → 404; a bad token → 401.
    - `a_ready_but_unsent_upload_cannot_be_downloaded_by_anyone`: GET → 404.
    - `headers_forbid_inline_use`: check `Cache-Control`, `nosniff`, and that `Content-Disposition` starts with `attachment;`.
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [x] **Step 5: Commit:** `feat: downloading attachments with ranges (A1)`.

### Task 11: Garbage collection and blob deletion

**Files:**
- Create: `src/attachments/gc.rs`.
- Modify: `src/attachments/mod.rs`, `src/server.rs` (hooks), `tests/attachments.rs`.

**Interfaces:**
- `pub fn start(state: &Arc<AppState>)`: two loops stopped by `state.shutdown`:
  - every 600 s `sweep(state, now_ms())`;
  - every 30 s `drain_blobs(state, now_ms())`.
- **`pub async fn sweep(state: &AppState, now: i64)`:** for each id in `expired(now)`, delete the row (the trigger queues its objects) and remove its temp file.
- **`pub async fn drain_blobs(state: &AppState, now: i64)`:**
  - Take up to 100 `due_blobs`.
  - For each, `backend(storage_id)?.delete(key)`. An unknown backend counts as done.
  - Ok → `blob_done`. Err → `blob_retry(attempts + 1, now + min(2^attempts minutes, 6 h))`.
- **Hooks:** `ServerHandle::sweep_attachments(now_ms)`, `ServerHandle::drain_blob_deletions(now_ms)`.

- [x] **Step 1: Write tests:**
  - `expired_uploads_are_swept`: an unfinished upload; `sweep_attachments(now + 24h + 1s)` → GET 404, temp file gone.
  - `deleted_rows_delete_their_objects`: a ready upload, `CancelUpload`, `drain_blob_deletions(now)` → the object files under `attachments/a/..` and `t/..` are gone.

  (Channel deletion is tested in Task 12, once messages carry attachments.)
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS.
- [x] **Step 5: Commit:** `feat: expiring uploads and deleting stored objects (A1)`.

**A1 stop.** Full checks, **D53** ("Storage, uploads, processing and downloads"), ledger, report.

---

# Milestone A2: Messages, settings and limits

Read first:
- `src/handlers/messages.rs` (`send`, `edit`, `delete`, `fetch`, `to_proto`);
- `src/handlers/settings.rs` (`update_voice_settings`, the pattern for Task 13);
- `src/handlers/roles.rs` (`create`, `update`);
- `src/db/attachments.rs`;
- `src/attachments/http.rs` (`attachment_to_proto`).

All paths below are under `crates/opencord-server/` unless they say otherwise.

### Task 12: Messages with attachments

**Files:**
- Modify:
  - `crates/opencord-common/src/validation.rs`;
  - `src/db/attachments.rs` (`mark_sent`);
  - `src/handlers/messages.rs`;
  - `src/server.rs` (remove the `db()` test hook in Step 4);
  - `tests/attachments.rs`.
- `src/db/messages.rs` stays unchanged.

**Interfaces:**

- **`opencord_common::validation::message_content_or_empty`**: `message_content` without the non-empty check.

  ```rust
  /// Message text that may be empty: a message with files needs no text.
  pub fn message_content_or_empty(input: &str) -> Result<String, ValidationError> {
      let trimmed = input.trim();
      require_max_chars("message", trimmed, MESSAGE_MAX_CHARS)?;
      Ok(trimmed.to_owned())
  }
  ```

- **`db::attachments::mark_sent`** becomes a conditional transition (Working Rule 16). Between `send`'s checks and its insert, the upload may be cancelled or sent by another request.

  ```rust
  /// Moves the caller's ready uploads in this channel onto a message, keeping
  /// their order. False if any is no longer ready, theirs or in this channel;
  /// the caller then drops its transaction, which undoes the message.
  pub async fn mark_sent(
      conn: &mut SqliteConnection,
      ids: &[i64],
      message_id: i64,
      uploader_id: i64,
      channel_id: i64,
  ) -> Result<bool, sqlx::Error>
  ```

  - For each id, with `position` = its index, run:

    ```sql
    UPDATE attachments SET message_id = $1, position = $2, state = 'sent'
    WHERE id = $3 AND state = 'ready' AND uploader_id = $4 AND channel_id = $5
    ```

  - Return `Ok(false)` as soon as one `rows_affected() != 1`.
  - Update the call in the unit test `upload_rows_move_through_their_states`. Then assert that calling `mark_sent` again with the same ids returns `false`.

- **`messages::send`**, in this order:
  1. Let `ids = request.attachment_ids`. If it isn't empty:
     - more than `upload::MAX_ATTACHMENTS` → INVALID_ARGUMENT "a message can have at most 10 files";
     - a duplicate (check with a `HashSet`) → INVALID_ARGUMENT "the same file is attached twice".
  2. Content: `validation::message_content` when `ids` is empty (today's rule), else `validation::message_content_or_empty`.
  3. The nonce check, `require_message_channel` and the message rate limit: all as today.
  4. If `ids` isn't empty, load each row with `attachments::find`, keeping the request's order. A row that is missing, not `uploader_id == ctx.user_id`, not in state `ready`, or not in `request.channel_id` → NOT_FOUND "upload".
  5. Permissions, in a block that holds `ctx.state.guild()` with no `.await` inside:
     - if any row's kind is `voice`:
       - `ids.len() == 1` and empty content, else INVALID_ARGUMENT "a voice message is sent on its own";
       - then `require_channel(&guild, ctx.user_id, channel_id, Permissions::SEND_VOICE_MESSAGES)`;
     - otherwise `require_channel(…, Permissions::ATTACH_FILES)`.
     - `require_channel` gives the FORBIDDEN error itself.
  6. Write in one transaction, as `roles::create` does:

     ```rust
     let mut tx = ctx.state.db.begin().await?;
     messages::insert(&mut tx, id, request.channel_id, ctx.user_id, &content).await?;
     if !ids.is_empty()
         && !attachments::mark_sent(&mut tx, &ids, id, ctx.user_id, request.channel_id).await?
     {
         // Dropping tx rolls the message back.
         return Err(ApiError::not_found("upload"));
     }
     tx.commit().await?;
     ```

  7. The message's `attachments` are `rows.iter().map(attachment_to_proto).collect()`, in request order.
     - Use the existing `crate::attachments::http::attachment_to_proto`. Don't write a second one.
  - With no attachments, behavior stays exactly as today, and the existing message tests must pass unchanged.

- **`to_proto(row: MessageRow, attachments: Vec<proto::Attachment>) -> proto::Message`.**
  - Update its callers.
  - `handlers/commands.rs` builds its own message with `attachments: Vec::new()`. Leave it.

- **`fetch`:** after `messages::list`, make one `for_messages` call and group its rows by message:

  ```rust
  let ids: Vec<i64> = rows.iter().map(|row| row.id).collect();
  let mut files: HashMap<i64, Vec<proto::Attachment>> = HashMap::new();
  for file in attachments::for_messages(&mut conn, &ids).await? {
      if let Some(message_id) = file.message_id {
          files.entry(message_id).or_default().push(attachment_to_proto(&file));
      }
  }
  // then: to_proto(row, files.remove(&row.id).unwrap_or_default())
  ```

- **`edit`**, in this order:
  1. `messages::find`, then `require_channel(VIEW_CHANNEL)` and the author check, all as today.
  2. `let files = attachments::for_messages(&mut conn, &[existing.id]).await?;`
  3. Content:
     - `message_content_or_empty` when `files` isn't empty, else `message_content`. Validation moves after the load.
     - If a file is a voice message and the new content isn't empty → INVALID_ARGUMENT "a voice message has no text".
  4. Update as today. The reply and the `MessageUpdate` carry the files (`attachment_to_proto`).

- **`delete`:** one transaction:

  ```rust
  let mut tx = ctx.state.db.begin().await?;
  // find + the permission check as today, reading through &mut tx
  messages::mark_deleted(&mut tx, existing.id).await?;
  attachments::delete_for_message(&mut tx, existing.id).await?;
  tx.commit().await?;
  ```

  - The `attachments_queue_blobs` trigger queues the objects, and the blob worker deletes them.
  - A channel's deletion needs no code. `ON DELETE CASCADE` deletes its attachment rows, and the trigger fires for each one.

- [x] **Step 1: Write tests.**
  - **Unit, `validation.rs`:** `message_content_or_empty("  ")` gives `""`; `MESSAGE_MAX_CHARS + 1` characters give `Err`.
  - **Integration (`tests/attachments.rs`):**
    - `a_message_carries_its_files_in_order`: two uploads; send `[b, a]` with empty content. Both MessageCreate and FetchMessages give `[b, a]`.
    - **`sending_checks_permissions_again_at_send_time`** (Review Focus 2): upload; the owner removes ATTACH_FILES from @everyone; send → FORBIDDEN.
    - `someone_elses_or_unready_uploads_cannot_be_sent`: each of these gives NOT_FOUND:
      - another user's upload;
      - one still uploading;
      - one made in another channel.
    - `voice_messages_stand_alone`: voice + text → INVALID_ARGUMENT; voice + file → INVALID_ARGUMENT; voice alone → ok.
    - `editing_keeps_files_and_allows_empty_text`:
      - a message with a file edited to `""` → ok, and its MessageUpdate carries the file;
      - a text-only message edited to `""` → INVALID_ARGUMENT;
      - a voice message edited to `"hi"` → INVALID_ARGUMENT.
    - `deleting_a_message_deletes_its_files`: delete, then `drain_blob_deletions` → the object files are gone; GET → 404.
    - `deleting_a_channel_deletes_its_files`: send a message with a file, then delete the channel; drain → the objects are gone.
    - `an_upload_cannot_be_sent_twice`: the second send → NOT_FOUND, and no second message exists (FetchMessages holds one).
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement and regenerate the sqlx data.
- [x] **Step 4: Remove the test hook.**
  - A1's tests link uploads to messages through `server.db()` and `mark_sent`. Change each of them to send a real message instead:

    ```rust
    SendMessage { channel_id, content: "…".into(), attachment_ids: vec![att.id], ..Default::default() }
    ```

  - Then delete `ServerHandle::db` from `src/server.rs`.
  - `grep -rn "\.db()" crates/opencord-server/tests` must print nothing.
  - Run → PASS.
- [x] **Step 5: Commit:** `feat: messages with attachments (A2)`.

### Task 13: Upload settings, role limits and Ready

**Files:**
- Modify:
  - `src/handlers/uploads.rs` (`update_settings`, and the blocked-extension error message);
  - `src/handlers/mod.rs` (dispatch);
  - `src/handlers/roles.rs`;
  - `src/gateway/identify.rs`;
  - `tests/attachments.rs`.
- No query changes: `Role::to_row` and `Role::to_proto` already carry `max_upload_bytes`.

**Interfaces:**

- **`handlers::uploads::update_settings(ctx, proto::UpdateUploadSettings) -> Result<Response, ApiError>`.**
  - Dispatch `Kind::UpdateUploadSettings(request)` to it. The storage kinds stay on "not implemented" until A3.
  - Mirror `settings::update_voice_settings`:

  ```rust
  let _writes = ctx.state.write_lock().await;
  let current = {
      let guild = ctx.state.guild();
      require_base(&guild, ctx.user_id, Permissions::MANAGE_SERVER)?;
      guild.upload_settings.clone()
  };
  let blocked_extensions = match &request.blocked_extensions {
      Some(list) => {
          if let Some(bad) = list
              .extensions
              .iter()
              .find(|entry| upload::normalize_extension(entry).is_err())
          {
              let shown: String = bad.trim().chars().take(30).collect();
              return Err(ApiError::invalid_argument(format!(
                  "\"{shown}\" isn't a file extension"
              )));
          }
          upload::normalize_blocked_list(&list.extensions)?
      }
      None => current.blocked_extensions,
  };
  let voice_message_max_seconds = match request.voice_message_max_seconds {
      Some(seconds) if upload::VOICE_MAX_SECONDS_RANGE.contains(&seconds) => seconds,
      Some(_) => {
          return Err(ApiError::invalid_argument(
              "voice messages can be limited to 30 seconds to 30 minutes",
          ));
      }
      None => current.voice_message_max_seconds,
  };
  let updated = UploadSettings { blocked_extensions, voice_message_max_seconds };
  updated.save(&mut *ctx.state.db.acquire().await?).await?;
  ctx.state.guild_mut().upload_settings = updated.clone();
  let settings = updated.to_proto(ctx.state.attachments.max_file_bytes());
  ctx.state.broadcast(
      Event::UploadSettingsUpdate(proto::UploadSettingsUpdate { settings: Some(settings.clone()) }),
      Audience::Everyone,
  );
  Ok(Response::UploadSettings(settings))
  ```

- **The blocked-extension error in `create`** names the list entry that matched, not the file's last extension. So `archive.tar.gz` blocked by `tar.gz` says "tar.gz files can't be uploaded here":

  ```rust
  let blocked = &guild.upload_settings.blocked_extensions;
  if let Some(entry) = blocked
      .iter()
      .find(|entry| upload::is_blocked(&filename, std::slice::from_ref(*entry)))
  {
      return Err(ApiError::file_type_blocked(entry));
  }
  ```

- **Role limits** (`roles.rs`): add one helper.

  ```rust
  /// A role's upload limit from a request; `None` keeps `current`.
  fn valid_upload_limit(
      guild: &Guild,
      caller: i64,
      is_everyone: bool,
      requested: Option<u64>,
      current: Option<u64>,
      ceiling: u64,
  ) -> Result<Option<u64>, ApiError> {
      match requested {
          None => Ok(current),
          Some(0) if is_everyone => Err(ApiError::invalid_argument("@everyone needs an upload limit")),
          Some(0) => Ok(None),
          Some(bytes) if bytes < upload::MIN_ROLE_LIMIT_BYTES || bytes > ceiling => {
              Err(ApiError::invalid_argument(format!(
                  "an upload limit can be 1 to {} MB here",
                  ceiling / upload::MIB
              )))
          }
          Some(bytes) => {
              let own = guild.upload_limit(caller, ceiling);
              if !guild.is_owner(caller) && bytes > own {
                  return Err(ApiError::forbidden(format!(
                      "you can't give a role a larger upload limit than your own ({} MB)",
                      own / upload::MIB
                  )));
              }
              Ok(Some(bytes))
          }
      }
  }
  ```

  - `ceiling` is `ctx.state.attachments.max_file_bytes()`.
  - **`create`:** call it inside the existing `let shifted = { … }` guild block, with `is_everyone = false` and `current = None`. Return the result from the block with `shifted`, and use it instead of `max_upload_bytes: None`.
  - **`update`:** call it inside the existing block, with `is_everyone`, `request.max_upload_bytes` and `role.max_upload_bytes`. Set `max_upload_bytes` in the `Role { … }` literal.

- **Ready** (`gateway/identify.rs`): replace `upload_settings: None` with:

  ```rust
  upload_settings: Some(guild.upload_settings.to_proto(state.attachments.max_file_bytes())),
  ```

- [x] **Step 1: Write tests** (`tests/attachments.rs`):
  - `upload_settings_are_shared_and_validated`:
    - the owner sets `["EXE", ".tar.gz"]` → everyone gets an UploadSettingsUpdate with `["exe", "tar.gz"]`;
    - a new session's Ready carries the same settings;
    - a member without MANAGE_SERVER → FORBIDDEN;
    - `29` seconds → INVALID_ARGUMENT;
    - `["ex e"]` → INVALID_ARGUMENT whose message contains `ex e`.
  - `blocked_extensions_stop_uploads`: after the owner blocks `exe` and `tar.gz`:
    - CreateUpload `setup.EXE` → FILE_TYPE_BLOCKED;
    - `archive.tar.gz` → FILE_TYPE_BLOCKED with `tar.gz` in the message;
    - `notes.txt` → ok;
    - a voice message is never blocked.
  - `role_limits_raise_members_and_respect_the_editor`:
    - the owner gives role R 200 MiB, then a member with R creates a 150 MiB upload → ok;
    - a moderator (MANAGE_ROLES, a role above the target, own limit 50 MiB) sets a lower role to 100 MiB → FORBIDDEN, and to 40 MiB → ok;
    - @everyone `Some(0)` → INVALID_ARGUMENT;
    - another role `Some(0)` clears its limit, and its RoleUpdate has no `max_upload_bytes`.
  - `the_owner_is_limited_only_by_the_ceiling`: start with `TestServer::start_with(|c| c.uploads.max_file_mb = 1)`. The owner's 2 MiB CreateUpload → FILE_TOO_LARGE.
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement. **Step 4:** Run → PASS. The existing role tests must pass unchanged.
- [x] **Step 5: Commit:** `feat: upload settings and role upload limits (A2)`.

**A2 stop.** Full checks, the ledger, and the report. The decisions entry, to copy into `docs/decisions.md` (see "Milestone stops"):

```markdown
## D54: Media uploads: messages, upload settings and role limits (Phase 4 A2, 2026-10-18)

- **Why.** Milestone A2 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) puts finished uploads on messages and lets the server's managers set the upload rules. Commits: <commits>.
- **Messages.**
  - `SendMessage.attachment_ids` takes up to 10 of the sender's own ready uploads from the same channel. The message keeps their order, and its text may then be empty (`validation::message_content_or_empty`).
  - Attach Files, or Send Voice Messages for a voice message, is checked again when the message is sent, not only when the upload starts.
  - A voice message stands alone: one attachment and no text, also after an edit.
  - `db::attachments::mark_sent` moves an upload only while it is still ready, the sender's and in that channel. Otherwise the whole send rolls back with "not found", so a cancel or a second send in between can't put a file on two messages.
  - `FetchMessages`, `EditMessage` and `MessageUpdate` carry the attachments.
  - Deleting a message deletes its attachment rows in the same transaction. The `attachments_queue_blobs` trigger then queues their objects, also when a channel's deletion cascades to them.
- **Upload settings.**
  - `UpdateUploadSettings` (Manage Server) normalizes the blocked list with `normalize_blocked_list` and takes a voice message limit of 30–1,800 seconds.
  - Everyone gets `UploadSettingsUpdate`, and `Ready.upload_settings` carries the settings at login.
  - A blocked upload's error names the list entry it matched ("tar.gz files can't be uploaded here").
- **Role limits.**
  - `CreateRole` and `UpdateRole` take `max_upload_bytes`. 0 clears a role's limit but is refused for @everyone.
  - Other values must be 1 MB up to the server's ceiling.
  - Only the owner may give a role a larger limit than their own.
- **Tests.** The A1 tests that put uploads on messages through `ServerHandle::db` now send real messages, and the hook is gone.
```

---

# Milestone A3: S3 storage

Read first:
- `src/attachments/mod.rs` (`Attachments`, `load`, `active`, `start`) and `src/attachments/storage/{mod,local}.rs`;
- `src/db/storage.rs`, and in `src/db/attachments.rs`: `usage`, `not_on`, `count_not_on`, `set_storage_if`, `count_on`;
- `src/attachments/process.rs` (`store`, `discard_stored`) and `src/attachments/gc.rs`;
- `src/handlers/uploads.rs` (`update_settings`: the pattern for a handler that takes the write lock);
- the storage messages in `proto/opencord/v1/models.proto` (`Storage` to `StorageMove`) and `proto/opencord/v1/requests.proto` (`GetStorage` to `StopStorageMove`).

All paths below are under `crates/opencord-server/` unless they say otherwise.

**The S3 test server.** MinIO no longer publishes Docker images, so the S3 tests run against RustFS, an S3-compatible server. Start it before Task 14 and leave it running until the A3 stop:

```bash
docker run -d --rm --name opencord-s3 -p 9000:9000 \
  -e RUSTFS_ACCESS_KEY=opencord -e RUSTFS_SECRET_KEY=opencord-secret \
  rustfs/rustfs:1.0.3 /data
```

The S3 tests are `#[ignore]`d, so the normal test run skips them. Run them with:

```bash
source .superpowers/sdd/media/env.sh && cargo test -p opencord-server --test s3 -- --ignored --test-threads=1
```

**The code blocks in A3 were compiled and clippy-checked before release, and the S3 code passed its tests against this server.** Copy them as given. If one doesn't compile against the code you have, stop and ask; don't rewrite it.

### Task 14: The S3 backend

**Files:**
- Create: `src/attachments/storage/s3.rs`, `tests/s3.rs`.
- Modify: `Cargo.toml`, `src/attachments/storage/mod.rs`, `src/attachments/mod.rs`, `src/attachments/process.rs`, `src/handlers/uploads.rs`.

**Interfaces:**

- **`Cargo.toml`:** add `rusty-s3 = "0.10.2"` after `reqwest.workspace = true`. Don't add the `url` crate: `reqwest::Url` is the same type.
- **`src/attachments/storage/s3.rs`**, exactly:

```rust
//! An S3-compatible bucket. Requests are presigned with `rusty-s3` and sent
//! with the server's own HTTP client. Signed URLs carry credentials, so they
//! are never logged or returned.

use std::path::Path;
use std::time::Duration;

use bytes::Bytes;
use opencord_proto::v1 as proto;
use reqwest::StatusCode;
use reqwest::header::{CONTENT_LENGTH, ETAG, RANGE};
use rusty_s3::actions::CreateMultipartUpload;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use tokio::io::AsyncReadExt;

use super::StorageError;

const MIB: u64 = 1024 * 1024;
/// How long a presigned request stays valid.
const SIGNED_FOR: Duration = Duration::from_secs(15 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
/// Larger files go up in parts.
const SINGLE_PUT_MAX: u64 = 64 * MIB;
const PART_BYTES: u64 = 16 * MIB;
/// S3's smallest part, used by the bucket test.
const TEST_PART_BYTES: u64 = 5 * MIB;

/// A bucket's settings. No `Debug`: it holds the secret.
#[derive(Clone)]
pub struct S3Settings {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub key_prefix: String,
    pub path_style: bool,
    pub allow_http: bool,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// `Credentials` keeps the secret out of `Debug`.
#[derive(Debug)]
pub struct S3Storage {
    bucket: Bucket,
    credentials: Credentials,
    key_prefix: String,
    http: reqwest::Client,
}

impl S3Storage {
    /// Errors are messages for the person who typed the settings.
    pub fn new(settings: &S3Settings, http: reqwest::Client) -> Result<Self, String> {
        let endpoint = reqwest::Url::parse(settings.endpoint.trim())
            .map_err(|_| "the endpoint isn't a web address".to_owned())?;
        match endpoint.scheme() {
            "https" => {}
            "http" if settings.allow_http => {}
            "http" => return Err("a plain http:// endpoint needs \"Allow plain HTTP\"".to_owned()),
            _ => return Err("the endpoint must start with https://".to_owned()),
        }
        let style = if settings.path_style {
            UrlStyle::Path
        } else {
            UrlStyle::VirtualHost
        };
        let bucket = Bucket::new(
            endpoint,
            style,
            settings.bucket.clone(),
            settings.region.clone(),
        )
        .map_err(|error| format!("the bucket settings don't work: {error}"))?;
        Ok(Self {
            bucket,
            credentials: Credentials::new(
                settings.access_key_id.clone(),
                settings.secret_access_key.clone(),
            ),
            key_prefix: settings.key_prefix.clone(),
            http,
        })
    }

    fn object(&self, key: &str) -> String {
        format!("{}{key}", self.key_prefix)
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, StorageError> {
        request
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(network_error)
    }

    /// Small files in one request, larger ones in parts. The file at `path`
    /// stays where it is; the caller removes it.
    pub async fn put_file(&self, key: &str, path: &Path) -> Result<(), StorageError> {
        let size = tokio::fs::metadata(path).await?.len();
        if size <= SINGLE_PUT_MAX {
            let bytes = tokio::fs::read(path).await?;
            return self.put_bytes(key, bytes).await;
        }
        self.put_parts(key, path, PART_BYTES).await
    }

    pub async fn put_bytes(&self, key: &str, bytes: Vec<u8>) -> Result<(), StorageError> {
        let object = self.object(key);
        let url = self
            .bucket
            .put_object(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let response = self.send(self.http.put(url).body(bytes)).await?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        Ok(())
    }

    pub async fn get_range(&self, key: &str, start: u64, end: u64) -> Result<Bytes, StorageError> {
        if end <= start {
            return Ok(Bytes::new());
        }
        let object = self.object(key);
        let url = self
            .bucket
            .get_object(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let range = format!("bytes={start}-{}", end - 1);
        let response = self.send(self.http.get(url).header(RANGE, range)).await?;
        match response.status() {
            StatusCode::PARTIAL_CONTENT => {}
            // Some servers answer a range covering the whole object in full.
            StatusCode::OK if start == 0 => {}
            StatusCode::NOT_FOUND => return Err(StorageError::NotFound),
            _ => return Err(failure(response).await),
        }
        let bytes = response.bytes().await.map_err(network_error)?;
        if bytes.len() as u64 != end - start {
            return Err(StorageError::Unavailable(format!(
                "asked for {} bytes and got {}",
                end - start,
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    pub async fn size(&self, key: &str) -> Result<Option<u64>, StorageError> {
        let object = self.object(key);
        let url = self
            .bucket
            .head_object(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let response = self.send(self.http.head(url)).await?;
        match response.status() {
            StatusCode::NOT_FOUND => Ok(None),
            status if status.is_success() => {
                // Read the header itself: a HEAD answer has no body to count.
                let length = response
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(|| {
                        StorageError::Unavailable("the storage didn't give the file's size".into())
                    })?;
                Ok(Some(length))
            }
            _ => Err(failure(response).await),
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let object = self.object(key);
        let url = self
            .bucket
            .delete_object(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let response = self.send(self.http.delete(url)).await?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(failure(response).await)
        }
    }

    /// A multipart upload in parts of `part_bytes`, aborted on any failure.
    async fn put_parts(&self, key: &str, path: &Path, part_bytes: u64) -> Result<(), StorageError> {
        let object = self.object(key);
        let url = self
            .bucket
            .create_multipart_upload(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let response = self.send(self.http.post(url)).await?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        let text = response.text().await.map_err(network_error)?;
        let created = CreateMultipartUpload::parse_response(&text).map_err(|error| {
            StorageError::Unavailable(format!("the storage's answer couldn't be read: {error}"))
        })?;
        let upload_id = created.upload_id();
        let result = self.send_parts(&object, upload_id, path, part_bytes).await;
        if result.is_err() {
            let url = self
                .bucket
                .abort_multipart_upload(Some(&self.credentials), &object, upload_id)
                .sign(SIGNED_FOR);
            if let Err(error) = self.send(self.http.delete(url)).await {
                tracing::warn!(%error, "an unfinished multipart upload couldn't be aborted");
            }
        }
        result
    }

    async fn send_parts(
        &self,
        object: &str,
        upload_id: &str,
        path: &Path,
        part_bytes: u64,
    ) -> Result<(), StorageError> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut etags = Vec::new();
        loop {
            let mut part = Vec::new();
            (&mut file).take(part_bytes).read_to_end(&mut part).await?;
            if part.is_empty() {
                break;
            }
            let number = u16::try_from(etags.len() + 1)
                .map_err(|_| StorageError::Unavailable("the file has too many parts".into()))?;
            let url = self
                .bucket
                .upload_part(Some(&self.credentials), object, number, upload_id)
                .sign(SIGNED_FOR);
            let response = self.send(self.http.put(url).body(part)).await?;
            if !response.status().is_success() {
                return Err(failure(response).await);
            }
            let etag = response
                .headers()
                .get(ETAG)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    StorageError::Unavailable("the storage gave a part no ETag".into())
                })?;
            etags.push(etag.to_owned());
        }
        let action = self.bucket.complete_multipart_upload(
            Some(&self.credentials),
            object,
            upload_id,
            etags.iter().map(String::as_str),
        );
        // Sign first: `body` consumes the action.
        let url = action.sign(SIGNED_FOR);
        let response = self.send(self.http.post(url).body(action.body())).await?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        // S3 can answer 200 and still report an error in the body.
        let text = response.text().await.map_err(network_error)?;
        if text.contains("<Error>") {
            return Err(StorageError::Unavailable(error_message(
                StatusCode::OK,
                &text,
            )));
        }
        Ok(())
    }

    /// Checks that the bucket takes writes, ranged reads, multipart uploads
    /// and deletes. Stops at the first failing step.
    pub async fn test(&self, temp_dir: &Path) -> proto::StorageTest {
        let run = format!("opencord-test/{}", hex::encode(crate::random::bytes::<8>()));
        let small = format!("{run}/small");
        let parts = format!("{run}/parts");
        let mut steps = Vec::new();
        let ok = self.run_test(&small, &parts, temp_dir, &mut steps).await;
        if !ok {
            // A failed step may leave a test file behind.
            for key in [&small, &parts] {
                if let Err(error) = self.delete(key).await {
                    tracing::debug!(%error, "a storage test file couldn't be deleted");
                }
            }
        }
        proto::StorageTest { steps, ok }
    }

    async fn run_test(
        &self,
        small: &str,
        parts: &str,
        temp_dir: &Path,
        steps: &mut Vec<proto::StorageTestStep>,
    ) -> bool {
        let data = crate::random::bytes::<1024>().to_vec();
        let written = self.put_bytes(small, data.clone()).await;
        if !record(steps, "Write a small file", written) {
            return false;
        }
        let read = match self.get_range(small, 100, 200).await {
            Ok(bytes) if bytes[..] == data[100..200] => Ok(()),
            Ok(_) => Err(StorageError::Unavailable(
                "the bytes read back differ".into(),
            )),
            Err(error) => Err(error),
        };
        if !record(steps, "Read part of it back", read) {
            return false;
        }
        let uploaded = self.test_parts(parts, temp_dir).await;
        if !record(steps, "Upload in parts", uploaded) {
            return false;
        }
        let deleted = match self.delete(small).await {
            Ok(()) => self.delete(parts).await,
            Err(error) => Err(error),
        };
        record(steps, "Delete the test files", deleted)
    }

    /// Two parts, 5 MiB + 1 KiB, through a temp file.
    async fn test_parts(&self, key: &str, temp_dir: &Path) -> Result<(), StorageError> {
        const SIZE: u64 = TEST_PART_BYTES + 1024;
        let name = format!(
            "storage-test-{}.tmp",
            hex::encode(crate::random::bytes::<8>())
        );
        let path = temp_dir.join(name);
        let result = async {
            tokio::fs::write(&path, vec![0x5a; SIZE as usize]).await?;
            self.put_parts(key, &path, TEST_PART_BYTES).await?;
            match self.size(key).await? {
                Some(SIZE) => Ok(()),
                other => Err(StorageError::Unavailable(format!(
                    "the uploaded file has the wrong size ({other:?} bytes)"
                ))),
            }
        }
        .await;
        if let Err(error) = tokio::fs::remove_file(&path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "a storage test's temp file couldn't be deleted");
        }
        result
    }
}

fn record(
    steps: &mut Vec<proto::StorageTestStep>,
    name: &str,
    result: Result<(), StorageError>,
) -> bool {
    let detail = match &result {
        Ok(()) => String::new(),
        Err(StorageError::Unavailable(message)) => message.clone(),
        Err(error) => error.to_string(),
    };
    steps.push(proto::StorageTestStep {
        name: name.to_owned(),
        ok: result.is_ok(),
        detail,
    });
    result.is_ok()
}

/// A network error without its URL: the URL carries the signature.
fn network_error(error: reqwest::Error) -> StorageError {
    StorageError::Unavailable(error.without_url().to_string())
}

async fn failure(response: reqwest::Response) -> StorageError {
    let status = response.status();
    // The body only improves the message; without it the status still says
    // what happened.
    let body = response.text().await.unwrap_or_default();
    StorageError::Unavailable(error_message(status, &body))
}

/// "Code: Message" from an S3 error body, or the HTTP status. At most 200
/// characters.
fn error_message(status: StatusCode, body: &str) -> String {
    let field = |name: &str| {
        let open = format!("<{name}>");
        let start = body.find(&open)? + open.len();
        let end = start + body[start..].find(&format!("</{name}>"))?;
        Some(body[start..end].trim().to_owned())
    };
    let message = match (field("Code"), field("Message")) {
        (Some(code), Some(message)) => format!("{code}: {message}"),
        (Some(code), None) => code,
        _ => format!("HTTP {status}"),
    };
    message.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(endpoint: &str, allow_http: bool) -> S3Settings {
        S3Settings {
            endpoint: endpoint.into(),
            region: "us-east-1".into(),
            bucket: "files".into(),
            key_prefix: String::new(),
            path_style: true,
            allow_http,
            access_key_id: "AKIAEXAMPLE".into(),
            secret_access_key: "a-very-secret-value".into(),
        }
    }

    #[test]
    fn http_endpoints_need_allow_http() {
        let http = crate::tls::http_client().unwrap();
        assert!(S3Storage::new(&settings("http://127.0.0.1:9000", false), http.clone()).is_err());
        assert!(S3Storage::new(&settings("http://127.0.0.1:9000", true), http.clone()).is_ok());
        assert!(S3Storage::new(&settings("https://s3.example.com", false), http.clone()).is_ok());
        assert!(S3Storage::new(&settings("ftp://s3.example.com", true), http.clone()).is_err());
        assert!(S3Storage::new(&settings("not a url", true), http).is_err());
    }

    #[test]
    fn debug_output_hides_the_secret() {
        let storage = S3Storage::new(
            &settings("https://s3.example.com", false),
            crate::tls::http_client().unwrap(),
        )
        .unwrap();
        assert!(!format!("{storage:?}").contains("a-very-secret-value"));
    }

    #[test]
    fn error_bodies_become_short_messages() {
        let body = "<?xml version=\"1.0\"?><Error><Code>NoSuchBucket</Code>\
                    <Message>The specified bucket does not exist</Message></Error>";
        assert_eq!(
            error_message(StatusCode::NOT_FOUND, body),
            "NoSuchBucket: The specified bucket does not exist"
        );
        assert_eq!(
            error_message(StatusCode::BAD_GATEWAY, "<html>proxy error</html>"),
            "HTTP 502 Bad Gateway"
        );
        let long = format!(
            "<Error><Code>X</Code><Message>{}</Message></Error>",
            "m".repeat(500)
        );
        assert_eq!(
            error_message(StatusCode::BAD_REQUEST, &long)
                .chars()
                .count(),
            200
        );
    }
}
```

  Don't "fix" these; each is deliberate:
  - Signed URLs carry the credentials. `network_error` strips them with `without_url()`, and no URL is ever logged or returned.
  - `put_file` leaves the source file where it is, while the local backend moves it. `process.rs` already removes its temp file either way.
  - `S3Settings` has no `Debug` because it holds the secret. `S3Storage` derives it safely: `rusty_s3::Credentials` leaves the secret out.
  - `size` reads the `Content-Length` header itself, because a HEAD answer has no body to count.

- **`src/attachments/storage/mod.rs`:**
  - Add `pub mod s3;` and `pub use s3::{S3Settings, S3Storage};`.
  - `Backend` gains `S3(Box<S3Storage>)`. The box keeps clippy's `large_enum_variant` quiet.
  - Each of the five methods gains the arm `Self::S3(s3) => s3.<the same call>.await,`, for example `Self::S3(s3) => s3.put_file(key, path).await,`.
- **`src/attachments/mod.rs`:**
  - Change the re-export to `pub use storage::{Backend, LocalStorage, ObjectKind, S3Settings, S3Storage, StorageError, object_key};`, and add `use crate::db::storage::StorageRow;`.
  - Replace `load` and `active` with the code below, which also adds `set_backend`, `remove_backend`, `set_active`, `active_id` and `http`:

```rust
    /// Builds the S3 backends and restores the active one. A storage that
    /// can't be built stops the server: its files would be unreadable.
    pub async fn load(&self, conn: &mut SqliteConnection, secrets: &Secrets) -> anyhow::Result<()> {
        let rows = crate::db::storage::list(conn).await?;
        let mut loaded = Vec::new();
        for row in rows.iter().filter(|row| row.kind == "s3") {
            let settings = s3_settings(row, secrets)
                .map_err(|error| anyhow::anyhow!("storage \"{}\": {error}", row.name))?;
            let storage = S3Storage::new(&settings, self.http.clone())
                .map_err(|error| anyhow::anyhow!("storage \"{}\": {error}", row.name))?;
            loaded.push((row.id, Arc::new(Backend::S3(Box::new(storage)))));
        }
        let active = rows.iter().find(|row| row.active).map_or(1, |row| row.id);
        self.backends
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .extend(loaded);
        self.active.store(active, Ordering::Release);
        Ok(())
    }

    /// Adds or replaces a backend. Callers write the database first.
    pub fn set_backend(&self, id: i64, backend: Backend) {
        self.backends
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, Arc::new(backend));
    }

    pub fn remove_backend(&self, id: i64) {
        self.backends
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    }

    /// New uploads go to `id` from now on. Its backend must be loaded.
    pub fn set_active(&self, id: i64) {
        self.active.store(id, Ordering::Release);
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn active_id(&self) -> i64 {
        self.active.load(Ordering::Acquire)
    }

    pub fn active(&self) -> Result<(i64, Arc<Backend>), ApiError> {
        let id = self.active_id();
        let backend = self.backend(id).ok_or_else(|| {
            tracing::error!(storage_id = id, "the active storage isn't loaded");
            ApiError::internal()
        })?;
        Ok((id, backend))
    }
```

  - Add these two functions next to `key_label`:

```rust
/// What an S3 storage's secret key is sealed under in the database.
pub(crate) fn storage_secret_label(id: i64) -> String {
    format!("storage-secret:{id}")
}

/// An S3 row's settings, with its secret opened.
pub(crate) fn s3_settings(row: &StorageRow, secrets: &Secrets) -> Result<S3Settings, String> {
    let sealed = row
        .secret_enc
        .as_deref()
        .ok_or_else(|| "it has no secret key".to_owned())?;
    let secret_access_key = secrets
        .open_string(&storage_secret_label(row.id), sealed)
        .map_err(|_| {
            "its secret key couldn't be opened with this server's secrets file".to_owned()
        })?;
    Ok(S3Settings {
        endpoint: row.endpoint.clone().unwrap_or_default(),
        region: row.region.clone().unwrap_or_default(),
        bucket: row.bucket.clone().unwrap_or_default(),
        key_prefix: row.key_prefix.clone(),
        path_style: row.path_style,
        allow_http: row.allow_http,
        access_key_id: row.access_key_id.clone().unwrap_or_default(),
        secret_access_key,
    })
}
```

  - Update the two callers of `active()`:
    - `handlers/uploads.rs` needs only the id: `let active_storage_id = ctx.state.attachments.active_id();`
    - `process.rs`: `let (storage_id, backend) = state.attachments.active()?;`
- **`tests/s3.rs`**, exactly:

```rust
//! Tests against a real S3-compatible server. They are ignored by default.
//!
//! ```bash
//! docker run -d --rm --name opencord-s3 -p 9000:9000 \
//!   -e RUSTFS_ACCESS_KEY=opencord -e RUSTFS_SECRET_KEY=opencord-secret \
//!   rustfs/rustfs:1.0.3 /data
//! cargo test -p opencord-server --test s3 -- --ignored --test-threads=1
//! docker stop opencord-s3
//! ```
//!
//! `OPENCORD_TEST_S3_ENDPOINT` overrides the endpoint (default
//! `http://127.0.0.1:9000`).

use std::time::Duration;

use opencord_server::attachments::storage::{S3Settings, S3Storage};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};

const ACCESS_KEY: &str = "opencord";
const SECRET_KEY: &str = "opencord-secret";

fn endpoint() -> String {
    std::env::var("OPENCORD_TEST_S3_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:9000".into())
}

/// A new, empty bucket for one test. `drop_bucket` removes it.
struct TestBucket {
    name: String,
    http: reqwest::Client,
}

impl TestBucket {
    async fn create() -> Self {
        let name = format!("opencord-test-{}", hex::encode(rand_bytes()));
        let http = opencord_server::tls::http_client().unwrap();
        let bucket = Bucket::new(
            endpoint().parse().unwrap(),
            UrlStyle::Path,
            name.clone(),
            "us-east-1",
        )
        .unwrap();
        let credentials = Credentials::new(ACCESS_KEY, SECRET_KEY);
        let url = bucket
            .create_bucket(&credentials)
            .sign(Duration::from_secs(60));
        let response = http.put(url).send().await.unwrap();
        assert!(
            response.status().is_success(),
            "{}",
            response.text().await.unwrap()
        );
        Self { name, http }
    }

    fn settings(&self, secret: &str) -> S3Settings {
        S3Settings {
            endpoint: endpoint(),
            region: "us-east-1".into(),
            bucket: self.name.clone(),
            key_prefix: "files/".into(),
            path_style: true,
            allow_http: true,
            access_key_id: ACCESS_KEY.into(),
            secret_access_key: secret.into(),
        }
    }

    fn storage(&self) -> S3Storage {
        S3Storage::new(&self.settings(SECRET_KEY), self.http.clone()).unwrap()
    }

    async fn drop_bucket(self) {
        let bucket = Bucket::new(
            endpoint().parse().unwrap(),
            UrlStyle::Path,
            self.name.clone(),
            "us-east-1",
        )
        .unwrap();
        let credentials = Credentials::new(ACCESS_KEY, SECRET_KEY);
        let url = bucket
            .delete_bucket(&credentials)
            .sign(Duration::from_secs(60));
        let response = self.http.delete(url).send().await.unwrap();
        assert!(
            response.status().is_success(),
            "{}",
            response.text().await.unwrap()
        );
    }
}

fn rand_bytes() -> [u8; 6] {
    opencord_server::random::bytes::<6>()
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn round_trip_ranges_and_delete() {
    let bucket = TestBucket::create().await;
    let storage = bucket.storage();
    let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    storage.put_bytes("a/1", data.clone()).await.unwrap();
    assert_eq!(storage.size("a/1").await.unwrap(), Some(1000));
    assert_eq!(
        storage.get_range("a/1", 10, 20).await.unwrap(),
        &data[10..20]
    );
    assert_eq!(storage.get_range("a/1", 0, 1000).await.unwrap(), &data[..]);
    storage.delete("a/1").await.unwrap();
    storage.delete("a/1").await.unwrap();
    assert_eq!(storage.size("a/1").await.unwrap(), None);
    bucket.drop_bucket().await;
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn multipart_put_file_of_70_mib() {
    let bucket = TestBucket::create().await;
    let storage = bucket.storage();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big");
    let data: Vec<u8> = (0..70 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    tokio::fs::write(&path, &data).await.unwrap();
    storage.put_file("a/2", &path).await.unwrap();
    assert!(path.exists(), "S3 leaves the source file for the caller");
    assert_eq!(storage.size("a/2").await.unwrap(), Some(data.len() as u64));
    let boundary = 16 * 1024 * 1024;
    assert_eq!(
        storage
            .get_range("a/2", boundary - 10, boundary + 10)
            .await
            .unwrap(),
        &data[boundary as usize - 10..boundary as usize + 10]
    );
    storage.delete("a/2").await.unwrap();
    bucket.drop_bucket().await;
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn the_bucket_test_passes_on_s3_and_fails_on_a_wrong_secret() {
    let bucket = TestBucket::create().await;
    let dir = tempfile::tempdir().unwrap();
    let passed = bucket.storage().test(dir.path()).await;
    assert!(passed.ok, "{passed:?}");
    assert_eq!(passed.steps.len(), 4);

    let wrong = S3Storage::new(&bucket.settings("wrong-secret"), bucket.http.clone()).unwrap();
    let failed = wrong.test(dir.path()).await;
    assert!(!failed.ok);
    assert_eq!(failed.steps.len(), 1);
    assert_eq!(failed.steps[0].name, "Write a small file");
    assert!(!failed.steps[0].detail.is_empty());
    println!("wrong secret: {}", failed.steps[0].detail);
    bucket.drop_bucket().await;
}
```

- [x] **Step 1: Write the tests:** the `#[cfg(test)]` module at the bottom of `s3.rs`, and `tests/s3.rs`.
- [x] **Step 2:** Run → FAIL (they don't compile yet).
- [x] **Step 3:** Implement as above.
- [x] **Step 4:** Run → PASS:
  - `cargo test -p opencord-server --lib -- attachments::storage::s3`;
  - the S3 tests (command at the top of this milestone);
  - `cargo test -p opencord-server --test attachments`, unchanged.
- [x] **Step 5: Commit:** `feat: the S3 storage backend (A3)`.

### Task 15: Managing storage (owner only)

**Files:**
- Create: `src/attachments/manage.rs`, `src/handlers/storage.rs`, `tests/storage.rs`.
- Modify:
  - `src/attachments/mod.rs` (`pub mod manage;`);
  - `src/db/storage.rs` (`find`, `count_blobs`);
  - `src/handlers/mod.rs` (dispatch);
  - `tests/common/mod.rs` and `tests/attachments.rs` (`media_token` moves to `common`);
  - `tests/s3.rs`.

**Interfaces:**

- **Queries** (`src/db/storage.rs`), then regenerate the sqlx data:

```rust
pub async fn count_blobs(conn: &mut SqliteConnection, storage_id: i64) -> Result<i64, sqlx::Error> {
    let count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) FROM blob_deletions WHERE storage_id = $1",
        storage_id
    )
    .fetch_one(conn)
    .await?;
    Ok(count)
}

pub async fn find(conn: &mut SqliteConnection, id: i64) -> Result<Option<StorageRow>, sqlx::Error> {
    sqlx::query_as!(
        StorageRow,
        r#"SELECT id, kind, name, endpoint, region, bucket,
                  key_prefix, path_style AS "path_style: bool",
                  allow_http AS "allow_http: bool", access_key_id,
                  secret_enc, active AS "active: bool", last_error,
                  created_at
           FROM storage_backends WHERE id = $1"#,
        id
    )
    .fetch_optional(conn)
    .await
}
```

- **`src/attachments/manage.rs`**, exactly. Until Task 16 adds the mover, write the `r#move` line as `r#move: None,`:

```rust
//! What the owner's storage page shows, and the checks its requests share.

use std::collections::HashMap;

use opencord_proto::v1 as proto;

use super::S3Settings;
use crate::db;
use crate::db::storage::StorageRow;
use crate::error::ApiError;
use crate::state::{AppState, Audience};

/// Every backend with its usage, the move's progress and the temp folder's
/// free space.
pub async fn snapshot(state: &AppState) -> Result<proto::Storage, ApiError> {
    let mut conn = state.db.acquire().await?;
    let rows = db::storage::list(&mut conn).await?;
    let usage: HashMap<i64, (i64, i64)> = db::attachments::usage(&mut conn)
        .await?
        .into_iter()
        .map(|(storage_id, files, bytes)| (storage_id, (files, bytes)))
        .collect();
    let backends = rows
        .iter()
        .map(|row| {
            let (files, bytes) = usage.get(&row.id).copied().unwrap_or((0, 0));
            backend_to_proto(row, files, bytes)
        })
        .collect();
    let temp_free_bytes = match fs4::available_space(state.attachments.temp_dir()) {
        Ok(bytes) => bytes,
        Err(error) => {
            // Shown on the page only; nothing is decided from it.
            tracing::warn!(%error, "the temp folder's free space couldn't be read");
            0
        }
    };
    Ok(proto::Storage {
        backends,
        r#move: state.attachments.mover().progress(),
        temp_free_bytes,
    })
}

fn backend_to_proto(row: &StorageRow, files: i64, bytes: i64) -> proto::StorageBackend {
    let is_s3 = row.kind == "s3";
    proto::StorageBackend {
        id: row.id,
        kind: if is_s3 {
            proto::StorageKind::S3
        } else {
            proto::StorageKind::Local
        }
        .into(),
        name: row.name.clone(),
        active: row.active,
        s3: is_s3.then(|| proto::S3Config {
            endpoint: row.endpoint.clone().unwrap_or_default(),
            region: row.region.clone().unwrap_or_default(),
            bucket: row.bucket.clone().unwrap_or_default(),
            key_prefix: row.key_prefix.clone(),
            path_style: row.path_style,
            allow_http: row.allow_http,
            access_key_id: mask(row.access_key_id.as_deref().unwrap_or_default()),
            secret_access_key: String::new(),
        }),
        has_secret: row.secret_enc.is_some(),
        file_count: u64::try_from(files).unwrap_or(0),
        bytes: u64::try_from(bytes).unwrap_or(0),
        last_error: row.last_error.clone(),
    }
}

/// "••••WXYZ": only the last 4 characters, and none of a key that short.
pub fn mask(access_key_id: &str) -> String {
    let chars: Vec<char> = access_key_id.chars().collect();
    if chars.len() <= 4 {
        return "••••".to_owned();
    }
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("••••{tail}")
}

/// Sends the owner the storage page's new state.
pub async fn broadcast(state: &AppState) -> Result<proto::Storage, ApiError> {
    let storage = snapshot(state).await?;
    let owner_id = state.guild().meta.owner_id;
    if let Some(owner_id) = owner_id {
        state.broadcast(
            proto::event::Kind::StorageUpdate(proto::StorageUpdate {
                storage: Some(storage.clone()),
            }),
            Audience::User(owner_id),
        );
    }
    Ok(storage)
}

pub fn valid_name(name: &str) -> Result<String, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(ApiError::invalid_argument(
            "a storage name is 1 to 100 characters on one line",
        ));
    }
    Ok(name.to_owned())
}

/// Checked, trimmed settings. `stored_secret` fills in an empty secret.
pub fn valid_s3(
    config: &proto::S3Config,
    stored_secret: Option<String>,
) -> Result<S3Settings, ApiError> {
    let field = |value: &str, what: &str, max: usize| {
        let value = value.trim();
        if value.is_empty() || value.chars().count() > max {
            return Err(ApiError::invalid_argument(format!(
                "the {what} is 1 to {max} characters"
            )));
        }
        Ok(value.to_owned())
    };
    let secret_access_key = match (config.secret_access_key.trim(), stored_secret) {
        ("", Some(stored)) => stored,
        (secret, _) => field(secret, "secret key", 200)?,
    };
    let key_prefix = config.key_prefix.trim();
    if key_prefix.chars().count() > 200 {
        return Err(ApiError::invalid_argument(
            "the key prefix is at most 200 characters",
        ));
    }
    Ok(S3Settings {
        endpoint: field(&config.endpoint, "endpoint", 500)?,
        region: field(&config.region, "region", 100)?,
        bucket: field(&config.bucket, "bucket name", 100)?,
        key_prefix: key_prefix.to_owned(),
        path_style: config.path_style,
        allow_http: config.allow_http,
        access_key_id: field(&config.access_key_id, "access key", 200)?,
        secret_access_key,
    })
}

/// The first failed step, for a STORAGE_UNAVAILABLE message and `last_error`.
pub fn test_failure(test: &proto::StorageTest) -> Option<String> {
    test.steps
        .iter()
        .find(|step| !step.ok)
        .map(|step| format!("the bucket test failed at “{}”: {}", step.name, step.detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_keys_show_only_their_last_four_characters() {
        assert_eq!(mask("AKIAIOSFODNN7EXAMPLE"), "••••MPLE");
        assert_eq!(mask("abcd"), "••••");
        assert_eq!(mask(""), "••••");
    }

    #[test]
    fn an_empty_secret_keeps_the_stored_one() {
        let config = proto::S3Config {
            endpoint: " https://s3.example.com ".into(),
            region: "auto".into(),
            bucket: "files".into(),
            access_key_id: "AKIAEXAMPLE".into(),
            ..Default::default()
        };
        let settings = valid_s3(&config, Some("stored".into())).unwrap();
        assert_eq!(settings.secret_access_key, "stored");
        assert_eq!(settings.endpoint, "https://s3.example.com");
        assert!(valid_s3(&config, None).is_err());
    }
}
```

- **`src/handlers/storage.rs`.** Start with these helpers, exactly:

```rust
fn require_owner(ctx: &Ctx<'_>) -> Result<(), ApiError> {
    if ctx.state.guild().is_owner(ctx.user_id) {
        Ok(())
    } else {
        Err(ApiError::forbidden("only the owner manages storage"))
    }
}

/// A saved S3 row's settings. This fails only if the secrets file changed.
fn saved_settings(ctx: &Ctx<'_>, row: &StorageRow) -> Result<S3Settings, ApiError> {
    s3_settings(row, ctx.state.certificates.secrets()).map_err(|error| {
        tracing::error!(storage_id = row.id, %error, "a storage's settings couldn't be read");
        ApiError::internal()
    })
}

/// Builds the bucket and runs its test. A failed step is STORAGE_UNAVAILABLE.
async fn tested(ctx: &Ctx<'_>, settings: &S3Settings) -> Result<S3Storage, ApiError> {
    let storage = S3Storage::new(settings, ctx.state.attachments.http().clone())
        .map_err(ApiError::invalid_argument)?;
    let test = storage.test(ctx.state.attachments.temp_dir()).await;
    match manage::test_failure(&test) {
        Some(failure) => Err(ApiError::storage_unavailable(failure)),
        None => Ok(storage),
    }
}

/// `row` with the bucket's settings and its secret sealed.
fn with_settings(row: StorageRow, settings: &S3Settings, secrets: &Secrets) -> StorageRow {
    let secret_enc = secrets.seal(
        &storage_secret_label(row.id),
        settings.secret_access_key.as_bytes(),
    );
    StorageRow {
        endpoint: Some(settings.endpoint.clone()),
        region: Some(settings.region.clone()),
        bucket: Some(settings.bucket.clone()),
        key_prefix: settings.key_prefix.clone(),
        path_style: settings.path_style,
        allow_http: settings.allow_http,
        access_key_id: Some(settings.access_key_id.clone()),
        secret_enc: Some(secret_enc),
        last_error: None,
        ..row
    }
}
```

- **The handlers**, each in the order given:
  - Every one follows Working Rules 13, 17 and 20:
    1. the owner check;
    2. validation;
    3. the bucket test, outside the write lock because it uses the network;
    4. then the write lock, and re-reading the row under it;
    5. the database write;
    6. the in-memory backends;
    7. the broadcast, last.
  - "Answer the storage" means `Ok(Response::Storage(manage::broadcast(ctx.state).await?))`.
  - **`get(ctx)`:** `require_owner`, then `Ok(Response::Storage(manage::snapshot(ctx.state).await?))`.
  - **`add(ctx, proto::AddStorage)`:**
    1. `require_owner`.
    2. `let name = manage::valid_name(&request.name)?;`
    3. `request.s3` missing → INVALID_ARGUMENT "the bucket's settings are missing". Otherwise `let settings = manage::valid_s3(config, None)?;`.
    4. `let storage = tested(ctx, &settings).await?;`
    5. `let _writes = ctx.state.write_lock().await;`
    6. `let id = ctx.state.ids.next_id();`
    7. Insert `with_settings(StorageRow { id, kind: "s3".into(), name, endpoint: None, region: None, bucket: None, key_prefix: String::new(), path_style: false, allow_http: false, access_key_id: None, secret_enc: None, active: false, last_error: None, created_at: now_ms() }, &settings, ctx.state.certificates.secrets())`.
    8. `ctx.state.attachments.set_backend(id, Backend::S3(Box::new(storage)));`
    9. Answer the storage.
  - **`update(ctx, proto::UpdateStorage)`:**
    1. `require_owner`.
    2. `db::storage::find` → NOT_FOUND "storage".
    3. `let name = request.name.as_deref().map(manage::valid_name).transpose()?;`
    4. If `request.s3` is set:
       - the row is local → INVALID_ARGUMENT "this server's own storage has no bucket settings";
       - `stored` is `Some(saved_settings(ctx, &row)?.secret_access_key)` when the request's secret is empty after trimming, else `None`;
       - `let settings = manage::valid_s3(config, stored)?;`
       - if the endpoint, bucket or key prefix differ from the row's, and `count_on(id) + count_blobs(id) > 0`: CONFLICT "this storage holds files; add a new storage and move them there instead";
       - `let storage = tested(ctx, &settings).await?;`
    5. Take the write lock, then `find` again → NOT_FOUND "storage" if it's gone. Call that row `current`.
    6. Build the new row from `current`: `StorageRow { name: name.unwrap_or(current.name.clone()), ..current }`. With new settings, pass it through `with_settings`. Then `db::storage::update`.
    7. With new settings: `set_backend(id, Backend::S3(Box::new(storage)))`.
    8. Answer the storage.
  - **`test(ctx, proto::TestStorage)`** answers `Response::StorageTest`:
    1. `require_owner`.
    2. Match `request.target`:
       - `None` → INVALID_ARGUMENT "name a storage or give its settings";
       - `Some(Target::S3(config))`: `valid_s3(&config, None)?`, then `S3Storage::new(..).map_err(ApiError::invalid_argument)?`, then return its `test(temp_dir)`. Nothing is saved.
       - `Some(Target::StorageId(id))`, in this order:
         1. `find` → NOT_FOUND "storage";
         2. a local row → INVALID_ARGUMENT "only S3 storage can be tested";
         3. `saved_settings`, `S3Storage::new`, `test`;
         4. take the write lock;
         5. `db::storage::set_last_error(conn, id, manage::test_failure(&test).as_deref())`, which also clears an old error;
         6. drop the lock and call `manage::broadcast`;
         7. return the test.
    - `Target` is `proto::test_storage::Target`.
  - **`activate(ctx, proto::ActivateStorage)`:**
    1. `require_owner`.
    2. `find` → NOT_FOUND "storage". Already active → answer the storage, changing nothing.
    3. For an S3 row, build it from `saved_settings` and run `test`.
       - A failed step:
         - take the write lock;
         - write `set_last_error` with the failure;
         - drop the lock;
         - `manage::broadcast`;
         - return STORAGE_UNAVAILABLE with the failure.
       - Otherwise keep the tested storage for step 6.
    4. Take the write lock, then `find` again → NOT_FOUND if it's gone.
    5. `db::storage::set_active(conn, id)`.
    6. For S3: `set_backend(id, Backend::S3(Box::new(storage)))` **before** the next step, so `active()` always finds the backend.
    7. `ctx.state.attachments.set_active(id)`.
    8. Answer the storage.
  - **`remove(ctx, proto::RemoveStorage)`:**
    1. `require_owner`.
    2. `id == 1` → INVALID_ARGUMENT "this server's own storage can't be removed".
    3. Take the write lock. `find` → NOT_FOUND "storage".
    4. Active → CONFLICT "activate another storage first".
    5. `count_on(id) = n > 0` → CONFLICT "{n} files are still stored there; move them first".
    6. `count_blobs(id) = n > 0` → CONFLICT "{n} files there are still waiting to be deleted; try again later".
    7. `db::storage::delete`, then `remove_backend(id)`.
    8. Answer the storage.
- **Dispatch** (`handlers/mod.rs`): one arm per request, `Kind::GetStorage(_) => storage::get(ctx).await` and so on. `StartStorageMove` and `StopStorageMove` stay on "not implemented" until Task 16.

- [x] **Step 1: Write tests.**
  - **Unit:**
    - the two tests at the bottom of `manage.rs`;
    - `the_secret_is_sealed` in `handlers/storage.rs`: `with_settings` with `Secrets::from_key(&[1; 32])` gives a `secret_enc` that doesn't contain the secret's bytes, and `s3_settings` opens it back.
  - **Integration, `tests/storage.rs`:**
    - `storage_requests_are_owner_only`: a member with an ADMINISTRATOR role sends all 8 storage requests → each is FORBIDDEN.
    - `the_local_storage_is_listed_with_its_usage`:
      - GetStorage → one backend: id 1, `LOCAL`, active, `has_secret == false`, no `s3`;
      - after one upload is sent: `file_count == 1` and `bytes` is the file's length;
      - `temp_free_bytes > 0`.
    - `the_local_storage_cannot_be_removed`: RemoveStorage(1) → INVALID_ARGUMENT.
    - `plain_http_needs_allow_http`: AddStorage with `http://127.0.0.1:1` and `allow_http: false` → INVALID_ARGUMENT, and GetStorage still lists one backend.
    - `a_failing_bucket_cannot_be_added`: AddStorage with `http://127.0.0.1:1` and `allow_http: true` → STORAGE_UNAVAILABLE whose message contains "Write a small file". GetStorage still lists one backend.
    - `testing_settings_saves_nothing`: TestStorage with that config → `ok == false`, one step, named "Write a small file". GetStorage still lists one backend.
    - `unknown_storage_is_not_found`: ActivateStorage(12345) and RemoveStorage(12345) → NOT_FOUND.
  - **S3, ignored (`tests/s3.rs`).**
    - Setup:
      - Add `mod common;` and `use common::*;`.
      - Build the `proto::S3Config` from the `TestBucket`'s values. The secret is `SECRET_KEY` and the key prefix is `files/`.
    - `the_secret_never_comes_back`: after AddStorage, both its answer and GetStorage show the new backend with an empty `secret_access_key`, `has_secret`, and `access_key_id == "••••cord"`.
    - `activating_s3_sends_new_uploads_there_and_survives_a_restart`:
      1. AddStorage, then ActivateStorage.
      2. Upload and send a file.
      3. The bucket holds `a/{id}`: check with `bucket.storage().size(..)`, which adds the prefix.
      4. The download equals the original.
      5. `server.restart(|_| {})` and reconnect.
      6. GetStorage shows the S3 storage active, the download still equals the original, and a new upload also lands in the bucket.
      7. RemoveStorage of the S3 storage → CONFLICT.
    - `a_storage_holding_files_keeps_its_bucket`: with one file on S3:
      - UpdateStorage with another bucket name → CONFLICT;
      - with only a new name → ok, and GetStorage shows the name.
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement and regenerate the sqlx data. **Step 4:** Run → PASS, including the S3 tests.
- [x] **Step 5: Commit:** `feat: owner storage management (A3)`.

### Task 16: Moving existing files

**Files:**
- Create: `src/attachments/move_job.rs`.
- Modify:
  - `src/attachments/mod.rs` (`pub mod move_job;`, the mover, startup cleanup);
  - `src/attachments/manage.rs` (the `r#move` line);
  - `src/db/storage.rs` (`queue_blob_after`);
  - `src/handlers/storage.rs`;
  - `src/handlers/mod.rs`;
  - `tests/storage.rs`;
  - `tests/s3.rs`.

**Interfaces:**

- **Query** (`src/db/storage.rs`), then regenerate the sqlx data:

```rust
/// Queues an object that must stay until `not_before` (ms).
pub async fn queue_blob_after(
    conn: &mut SqliteConnection,
    storage_id: i64,
    key: &str,
    not_before: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO blob_deletions (storage_id, object_key, next_attempt_at) VALUES ($1, $2, $3)",
        storage_id,
        key,
        not_before
    )
    .execute(conn)
    .await?;
    Ok(())
}
```

- **`src/attachments/move_job.rs`**, exactly:

```rust
//! Moves every file to the active storage. Ciphertext is copied as is, so
//! nothing is decrypted or re-encrypted.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use opencord_proto::v1 as proto;
use sqlx::Connection;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::{Backend, ObjectKind, StorageError, object_key};
use crate::db;
use crate::db::attachments::AttachmentRow;
use crate::error::ApiError;
use crate::state::{AppState, now_ms};

const PAGE: i64 = 50;
const AT_ONCE: usize = 2;
const RETRIES: u32 = 3;
const COPY_CHUNK: u64 = 8 * 1024 * 1024;
const PROGRESS_EVERY: Duration = Duration::from_secs(2);
/// Old copies outlive the switch by this much, so downloads that started
/// before it can finish.
const OLD_COPY_GRACE_MS: i64 = 60 * 60 * 1000;

#[derive(Debug, Default)]
pub struct Mover {
    /// The running or last move. Kept when it ends, so the page shows how it
    /// went.
    progress: Mutex<Option<proto::StorageMove>>,
    /// Present exactly while a move runs.
    cancel: Mutex<Option<CancellationToken>>,
    last_sent: Mutex<Option<Instant>>,
}

impl Mover {
    pub fn progress(&self) -> Option<proto::StorageMove> {
        *self.progress.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn is_running(&self) -> bool {
        self.cancel
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// The job stops before its next file.
    pub fn stop(&self) {
        if let Some(token) = self
            .cancel
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            token.cancel();
        }
    }

    /// Applies `change`; true when it's time to send the owner an update.
    fn update(&self, change: impl FnOnce(&mut proto::StorageMove)) -> bool {
        if let Some(progress) = self
            .progress
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
        {
            change(progress);
        }
        let mut last_sent = self
            .last_sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let due = last_sent.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY);
        if due {
            *last_sent = Some(Instant::now());
        }
        due
    }
}

/// Starts moving every file that isn't on the active storage. The caller
/// holds the write lock, so the active storage can't change meanwhile.
pub async fn start(state: &Arc<AppState>) -> Result<(), ApiError> {
    let mover = state.attachments.mover();
    if mover.is_running() {
        return Err(ApiError::conflict("files are already being moved"));
    }
    let target = state.attachments.active_id();
    let (files, bytes) = {
        let mut conn = state.db.acquire().await?;
        db::attachments::count_not_on(&mut conn, target).await?
    };
    let token = CancellationToken::new();
    *mover.cancel.lock().unwrap_or_else(PoisonError::into_inner) = Some(token.clone());
    *mover
        .progress
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(proto::StorageMove {
        to_storage_id: target,
        running: true,
        files_total: u64::try_from(files).unwrap_or(0),
        bytes_total: u64::try_from(bytes).unwrap_or(0),
        ..Default::default()
    });
    *mover
        .last_sent
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = None;
    tokio::spawn(run(Arc::clone(state), target, token));
    Ok(())
}

async fn run(state: Arc<AppState>, target: i64, cancel: CancellationToken) {
    let mut after_id = 0;
    while !cancel.is_cancelled() {
        let page = match next_page(&state, target, after_id).await {
            Ok(page) => page,
            Err(error) => {
                tracing::error!(%error, "the files to move couldn't be listed");
                break;
            }
        };
        let Some(last) = page.last() else { break };
        after_id = last.id;
        futures_util::stream::iter(page)
            .for_each_concurrent(AT_ONCE, |row| {
                let state = &state;
                let cancel = &cancel;
                async move {
                    if cancel.is_cancelled() {
                        return;
                    }
                    let moved = move_with_retries(state, &row, target, cancel).await;
                    let due = state.attachments.mover().update(|progress| {
                        if moved {
                            progress.files_done += 1;
                            progress.bytes_done += u64::try_from(row.size_bytes).unwrap_or(0);
                        } else {
                            progress.failed += 1;
                        }
                    });
                    if due {
                        send_progress(state).await;
                    }
                }
            })
            .await;
    }
    let mover = state.attachments.mover();
    mover.update(|progress| progress.running = false);
    *mover.cancel.lock().unwrap_or_else(PoisonError::into_inner) = None;
    send_progress(&state).await;
}

async fn next_page(
    state: &AppState,
    target: i64,
    after_id: i64,
) -> Result<Vec<AttachmentRow>, sqlx::Error> {
    let mut conn = state.db.acquire().await?;
    db::attachments::not_on(&mut conn, target, after_id, PAGE).await
}

async fn send_progress(state: &AppState) {
    if let Err(error) = super::manage::broadcast(state).await {
        tracing::warn!(?error, "the storage move's progress couldn't be sent");
    }
}

#[derive(Debug, thiserror::Error)]
enum MoveError {
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("storage {0} isn't loaded")]
    Missing(i64),
}

/// Three retries, 2, 4 and 8 seconds apart. False when the file stays put.
async fn move_with_retries(
    state: &AppState,
    row: &AttachmentRow,
    target: i64,
    cancel: &CancellationToken,
) -> bool {
    for attempt in 0..=RETRIES {
        match move_one(state, row, target).await {
            Ok(()) => return true,
            Err(error) if attempt < RETRIES && !cancel.is_cancelled() => {
                tracing::warn!(attachment_id = row.id, %error, attempt, "moving a file failed; trying again");
                tokio::time::sleep(Duration::from_secs(2 << attempt)).await;
            }
            Err(error) => {
                tracing::error!(attachment_id = row.id, %error, "a file couldn't be moved");
                return false;
            }
        }
    }
    false
}

async fn move_one(state: &AppState, row: &AttachmentRow, target: i64) -> Result<(), MoveError> {
    let from = row.storage_id;
    let source = state
        .attachments
        .backend(from)
        .ok_or(MoveError::Missing(from))?;
    let dest = state
        .attachments
        .backend(target)
        .ok_or(MoveError::Missing(target))?;
    let mut keys = vec![object_key(ObjectKind::File, row.id)];
    if row.has_thumbnail {
        keys.push(object_key(ObjectKind::Thumbnail, row.id));
    }
    for key in &keys {
        let path = state
            .attachments
            .temp_dir()
            .join(format!("move-{}.part", key.replace('/', "-")));
        let copied = copy(&source, &dest, key, &path).await;
        remove_temp(&path).await;
        copied?;
    }
    // Writes only (Working Rule 20): the row switches and the copies nobody
    // needs are queued together.
    let mut conn = state.db.acquire().await?;
    let mut tx = conn.begin().await?;
    let moved = db::attachments::set_storage_if(&mut tx, row.id, from, target).await?;
    if moved {
        let not_before = now_ms() + OLD_COPY_GRACE_MS;
        for key in &keys {
            db::storage::queue_blob_after(&mut tx, from, key, not_before).await?;
        }
    } else {
        // Deleted meanwhile: its old copies are already queued, and the new
        // ones are unused.
        for key in &keys {
            db::storage::queue_blob(&mut tx, target, key).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Through a temp file, in 8 MiB reads.
async fn copy(
    source: &Backend,
    dest: &Backend,
    key: &str,
    path: &Path,
) -> Result<(), StorageError> {
    let size = source.size(key).await?.ok_or(StorageError::NotFound)?;
    let mut file = tokio::fs::File::create(path).await?;
    let mut offset = 0;
    while offset < size {
        let end = (offset + COPY_CHUNK).min(size);
        file.write_all(&source.get_range(key, offset, end).await?)
            .await?;
        offset = end;
    }
    file.sync_all().await?;
    drop(file);
    dest.put_file(key, path).await
}

/// A local storage moved the file already; anything else is logged.
async fn remove_temp(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "a move's temp file couldn't be deleted");
    }
}
```

  - Why old copies wait an hour: a download that started before the switch reads the old copy, so it must outlive the switch.
  - A file deleted meanwhile already had its old copies queued by the `attachments_queue_blobs` trigger.
- **`src/attachments/mod.rs`:**
  - `Attachments` gains `mover: move_job::Mover`, built with `move_job::Mover::default()`, and:

    ```rust
    pub fn mover(&self) -> &move_job::Mover {
        &self.mover
    }
    ```

  - In `start()`, also delete what a cut-short move or bucket test leaves. Replace the `is_orphan` computation with:

    ```rust
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    // A move or a bucket test that was cut short leaves these.
    let is_leftover = name.starts_with("move-") || name.starts_with("storage-test-");
    let is_orphan = name
        .strip_suffix(".part")
        .and_then(|id| id.parse::<i64>().ok())
        .is_some_and(|id| !live.contains(&id));
    ```

    Then delete when `is_leftover || is_orphan`.
- **`manage.rs`:** `r#move: state.attachments.mover().progress(),`.
- **Handlers:**
  - `start_move(ctx)`:
    1. `require_owner`;
    2. take the write lock;
    3. `move_job::start(ctx.state).await?`;
    4. answer the storage.
  - `stop_move(ctx)`:
    1. `require_owner`;
    2. `ctx.state.attachments.mover().stop()`;
    3. return `Ok(Response::Storage(manage::snapshot(ctx.state).await?))`. The job sends its final update itself.
  - `activate`: right after taking the write lock, a running move → CONFLICT "wait for the move to finish, or stop it, first".
  - Dispatch both requests.

- [x] **Step 1: Write tests.**
  - **Integration, `tests/storage.rs`:** `a_move_with_nothing_to_move_finishes`:
    1. StartStorageMove on a fresh server answers a `move` with `files_total == 0`. Don't assert `running`: with nothing to move, the job can finish before the answer is built.
    2. A StorageUpdate whose `move` has `running == false` arrives.
    3. StopStorageMove then answers ok.
  - **S3, ignored:** `moving_files_copies_ciphertext_and_cleans_up`:
    1. Upload three files on the local storage, one of them a PNG so that a thumbnail moves too, and send them.
    2. Add and activate S3.
    3. StartStorageMove. Wait for a StorageUpdate whose `move` has `running == false`.
    4. `files_total == 3`, `files_done == 3`, `failed == 0`.
    5. The three downloads equal the originals, and the PNG's thumbnail downloads.
    6. The local object files still exist after `drain_blob_deletions(now)`, and are gone after `drain_blob_deletions(now + 3_600_000 + 1_000)`.
    7. GetStorage: local `file_count == 0`, S3 `file_count == 3`.
- [x] **Step 2:** Run → FAIL. **Step 3:** Implement and regenerate the sqlx data. **Step 4:** Run → PASS, including the S3 tests.
- [x] **Step 5: Commit:** `feat: moving files between storages (A3)`.

**A3 stop.**
1. Run the full checks and the S3 tests, then `docker stop opencord-s3`.
2. Update the ledger and write the report.
3. Copy this decisions entry into `docs/decisions.md` (see "Milestone stops"):

```markdown
## D55: Media uploads: S3 storage, storage management and moving files (Phase 4 A3, 2026-10-19)

- **Why.** Milestone A3 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) lets the owner keep files in S3-compatible buckets. Commits: <commits>.
- **The S3 backend** (`attachments/storage/s3.rs`).
  - Requests are presigned with `rusty-s3` 0.10.2 for 15 minutes and sent with the server's own `reqwest` client, with 300 seconds each. Network errors drop the signed URL (`without_url`), so no URL reaches a log or a reply.
  - Files up to 64 MiB go up in one request. Larger ones go up as multipart uploads in 16 MiB parts, aborted on failure.
  - Reads are ranged GETs. A missing object is `NotFound`, and a delete treats 404 as done.
  - S3 errors become "Code: Message" from the error body, at most 200 characters.
  - The bucket test writes 1 KiB, reads bytes 100–199 back, uploads 5 MiB + 1 KiB in two parts and deletes both. It stops at the first failed step.
- **Loading.** `Attachments::load` builds every S3 storage at startup and restores the active one. A storage whose secret can't be opened stops the server, rather than leaving its files unreadable.
- **Managing storage.**
  - Only the owner may send the storage requests; an administrator gets FORBIDDEN.
  - AddStorage, UpdateStorage with new bucket settings, and ActivateStorage of an S3 storage run the bucket test first. A failed step is STORAGE_UNAVAILABLE "the bucket test failed at “step”: detail".
  - The secret key is sealed as `storage-secret:<id>` and never sent back. An empty secret in UpdateStorage keeps the stored one. Answers show `has_secret` and the access key masked to its last 4 characters.
  - A storage that holds files can't change its endpoint, bucket or key prefix.
  - The local storage can't be removed. Another storage can be removed only when it's inactive, holds no files and has no deletions waiting.
  - Every change goes to the owner as StorageUpdate.
- **Moving files** (`attachments/move_job.rs`).
  - StartStorageMove copies every ready or sent file, with its thumbnail, from the other storages to the active one: 50 rows per page, two files at a time, through a temp file in 8 MiB reads. The ciphertext is copied as is.
  - The row switches with `set_storage_if` in the same transaction that queues the old copies for deletion. A file deleted meanwhile gets its new copies queued instead.
  - Old copies are deleted an hour after the switch, so downloads already under way can finish.
  - Each file gets three retries, 2, 4 and 8 seconds apart. After that it counts as failed and stays where it was.
  - Progress goes to the owner at most every 2 seconds, and once at the end. StopStorageMove stops before the next file, and activating another storage is refused while a move runs.
  - Startup deletes the temp files a move or a bucket test left behind.
- **Tests.** The S3 tests in `tests/s3.rs` are ignored by default and run against RustFS 1.0.3 in Docker, because MinIO no longer publishes images.
```

---

# Milestone A4: Client core

Read first:
- `crates/opencord-core/src/client.rs`: `Inner`, `Client::new`, `request`, `send_message`, `context`, the `expect_*` helpers and `unexpected`;
- `src/connection.rs`: `Context`, `Route`, `Routes`, `MediaTokens`, `on_event`;
- `src/http.rs` (`MediaHttp`), `src/tofu.rs` (`TlsTrust`) and `src/soundboard/cache.rs`, the cache the attachment cache follows;
- `src/api/client.rs`: `client`, `on_runtime`, and how an API function wraps a `Client` method;
- `crates/opencord-server/src/attachments/http.rs`: the endpoints the core talks to, and what each answers.

All paths below are under `crates/opencord-core/` unless they say otherwise.

**How A4's code is given.**
- All of A4 was written, compiled and tested task by task before release. After each task the tree built, `cargo clippy -p opencord_core --all-targets -- -D warnings` passed, and the task's tests passed. Task 21 also passed `app/tool/check_platforms.sh`, and the whole of A4 passed the full core suite and `flutter test`.
- A new file is given whole. Create it with exactly that content.
- A change to an existing file is a `diff` block in unified form: `-` lines go, `+` lines come in, the rest is context for finding the place.
  - Apply one by saving the block to `/tmp/a4.patch` and running `git apply /tmp/a4.patch` from the repository root, or make the same edits by hand.
  - If a block doesn't apply, stop and ask. Don't rewrite the code to make it fit.
- Don't change, reorder or "improve" the given code, comments included. A line not given is a deviation.

**Bindings.** Tasks 17, 18, 19 and 21 change `src/api/`. After each, regenerate:

```bash
cd app && source ../.superpowers/sdd/media/env.sh && flutter_rust_bridge_codegen generate
```

It takes about two minutes and rewrites `src/frb_generated.rs` and `app/lib/src/rust/`. Commit those files with the task. Never edit them by hand.

**The core's tests.** `tests/attachments.rs` starts a real server in the test process:

```bash
source .superpowers/sdd/media/env.sh && cargo test -p opencord_core --test attachments
```

The whole file takes about 10 seconds. A test that runs longer than a minute is hung, which is a bug: stop and report it, with the test's name.

### Task 17: Core types, conversions and simple requests

**What it does.**
- Messages carry their attachments, roles their upload limit, and Ready the upload settings.
- `RoleChanges.max_upload_bytes` `Some(0)` removes a role's limit, as the server's UpdateRole does. `create_role` gains a `max_upload_bytes` argument.
- The core gains the requests for upload settings and the owner's storage pages, and passes UploadSettingsUpdate and StorageUpdate events to the app.
- `UploadReady` and `UploadFailed` still map to nothing here. Task 18 hands them to the upload manager.
- The seven new error codes keep their names, and `CoreError` gains `Cancelled`.
- `client` and `on_runtime` in `api/client.rs` become `pub(crate)`, so `api/attachments.rs` can use them.

**Files.**
- Modify: `src/api/types.rs`, `src/convert.rs`, `src/client.rs`, `src/api/client.rs`, `src/api/mod.rs`, `tests/client.rs`, `examples/m6_peer.rs`.
- Create: `src/api/attachments.rs`, `tests/attachments.rs`.
- Dart, after regenerating: `app/lib/core/rust/core_mapping.dart`, `app/lib/core/rust/rust_repository.dart`, `app/test/support/fake_core.dart`, `app/test/unit/rust/core_mapping_test.dart`, `app/test/unit/rust/rust_repository_test.dart`, `app/test/unit/state/channel_messages_test.dart`.

**`src/api/types.rs`:**

```diff
--- a/crates/opencord-core/src/api/types.rs
+++ b/crates/opencord-core/src/api/types.rs
@@ -82,6 +82,9 @@
     pub permissions: i64,
     pub hoist: bool,
     pub mentionable: bool,
+    /// The largest file its members may upload; `None` leaves it to their
+    /// other roles.
+    pub max_upload_bytes: Option<u64>,
 }
 
 #[derive(Debug, Clone, Default, PartialEq, Eq)]
@@ -91,6 +94,8 @@
     pub permissions: Option<i64>,
     pub hoist: Option<bool>,
     pub mentionable: Option<bool>,
+    /// `Some(0)` removes the role's limit.
+    pub max_upload_bytes: Option<u64>,
 }
 
 #[derive(Debug, Clone, Copy, PartialEq, Eq)]
@@ -162,6 +167,8 @@
     pub nonce: Option<String>,
     /// A command's public result: the command run, by the author.
     pub command: Option<String>,
+    /// In the order they were attached.
+    pub attachments: Vec<Attachment>,
 }
 
 #[derive(Debug, Clone, PartialEq, Eq)]
@@ -353,6 +360,7 @@
     pub addresses: ServerAddresses,
     /// The chat commands this user may see (Phase 3 plan §17.1).
     pub commands: CommandList,
+    pub upload_settings: UploadSettings,
 }
 
 /// One address a server is reached at.
@@ -579,6 +587,29 @@
     CommandsUpdate(CommandList),
     /// A command's reply only this device sees.
     EphemeralMessage(EphemeralMessage),
+    UploadSettingsUpdate(UploadSettings),
+    /// To the owner only.
+    StorageUpdate(StorageInfo),
+    /// A file of a message being sent with `send_message_with_files`, at
+    /// most 10 times a second, and once at each phase.
+    UploadProgress {
+        channel_id: i64,
+        nonce: String,
+        /// Its place among the message's files.
+        file_index: u32,
+        sent_bytes: u64,
+        total_bytes: u64,
+        phase: UploadPhase,
+    },
+    /// A `download_attachment`, at most 10 times a second, and once at the
+    /// end with `done` or `error`.
+    DownloadProgress {
+        attachment_id: i64,
+        received_bytes: u64,
+        total_bytes: u64,
+        done: bool,
+        error: Option<String>,
+    },
 }
 
 /// Where the server is reached from the internet (Phase 3 plan §5.1).
@@ -926,6 +957,13 @@
     CommandDisabled,
     CommandNotAllowedHere,
     CommandInvalidOptions,
+    FileTooLarge,
+    FileTypeBlocked,
+    UploadOffsetMismatch,
+    StorageUnavailable,
+    StorageFull,
+    VoiceMessageInvalid,
+    VoiceMessageTooLong,
     Unknown,
 }
 
@@ -1349,6 +1387,8 @@
         problem: CameraProblem,
         message: String,
     },
+    #[error("cancelled")]
+    Cancelled,
 }
 
 /// Lavalink nodes (Phase 3 plan §12). Their passwords never reach clients.
@@ -1789,3 +1829,159 @@
     pub channel_ids: Option<Vec<i64>>,
     pub results_public: Option<bool>,
 }
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum AttachmentKind {
+    File,
+    Image,
+    Video,
+    Audio,
+    VoiceMessage,
+}
+
+/// A file in a message.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct Attachment {
+    pub id: i64,
+    pub filename: String,
+    /// Chosen by the server from the file's first bytes.
+    pub content_type: String,
+    pub size_bytes: u64,
+    pub kind: AttachmentKind,
+    /// Images: after EXIF rotation. Videos: the sender's hints.
+    pub width: Option<u32>,
+    pub height: Option<u32>,
+    /// Videos (hint) and voice messages (measured).
+    pub duration_ms: Option<u32>,
+    /// Voice messages: 128 values, 0–255. Empty otherwise.
+    pub waveform: Vec<u8>,
+    pub has_thumbnail: bool,
+    pub thumbnail_width: Option<u32>,
+    pub thumbnail_height: Option<u32>,
+    /// A GIF or WebP with more than one frame.
+    pub animated: bool,
+}
+
+/// What may be uploaded to a server.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct UploadSettings {
+    /// Lowercase, without the dot; may hold inner dots ("tar.gz").
+    pub blocked_extensions: Vec<String>,
+    pub voice_message_max_seconds: u32,
+    /// The server's ceiling: the owner's limit and the most any role allows.
+    pub max_file_bytes: u64,
+    pub max_attachments: u32,
+}
+
+/// Where a file being sent is.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub enum UploadPhase {
+    /// Reading it, and removing a photo's location.
+    Preparing,
+    Uploading,
+    /// The server checks it and makes its thumbnail.
+    Processing,
+    /// Every file is in; the message goes out.
+    Sending,
+    Done,
+    Failed {
+        message: String,
+    },
+    Cancelled,
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum StorageKind {
+    Local,
+    S3,
+}
+
+/// An S3-compatible bucket. In answers the access key is masked to its
+/// last 4 characters and the secret is empty.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct S3Config {
+    pub endpoint: String,
+    pub region: String,
+    pub bucket: String,
+    pub key_prefix: String,
+    pub path_style: bool,
+    pub allow_http: bool,
+    pub access_key_id: String,
+    /// Empty in an update keeps the saved one.
+    pub secret_access_key: String,
+}
+
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct StorageBackend {
+    pub id: i64,
+    pub kind: StorageKind,
+    pub name: String,
+    /// New files go here.
+    pub active: bool,
+    pub s3: Option<S3Config>,
+    pub has_secret: bool,
+    pub file_count: u64,
+    pub bytes: u64,
+    /// Why its last test failed.
+    pub last_error: Option<String>,
+}
+
+/// Moving every file to the active storage.
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub struct StorageMove {
+    pub to_storage_id: i64,
+    pub running: bool,
+    pub files_done: u64,
+    pub files_total: u64,
+    pub bytes_done: u64,
+    pub bytes_total: u64,
+    pub failed: u32,
+}
+
+/// Where a server keeps its files. Owner only.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct StorageInfo {
+    pub backends: Vec<StorageBackend>,
+    /// The running or last move.
+    pub move_status: Option<StorageMove>,
+    /// Free space where the server receives uploads.
+    pub temp_free_bytes: u64,
+}
+
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct StorageTestStep {
+    pub name: String,
+    pub ok: bool,
+    pub detail: String,
+}
+
+/// A bucket test: the steps run, up to the first that failed.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct StorageTestResult {
+    pub steps: Vec<StorageTestStep>,
+    pub ok: bool,
+}
+
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub enum StorageTestTarget {
+    /// A saved storage; its result is saved as its `last_error`.
+    Existing { storage_id: i64 },
+    /// Settings not saved yet; nothing is saved.
+    Config { s3: S3Config },
+}
+
+/// A file to send: a path on this device, or bytes (a pasted image).
+/// Exactly one of the two.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct OutgoingFile {
+    pub path: Option<String>,
+    pub bytes: Option<Vec<u8>>,
+    /// Empty uses the path's file name.
+    pub filename: String,
+}
+
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub enum AttachmentVariant {
+    Original,
+    Thumbnail,
+}
```

**`src/convert.rs`:**

```diff
--- a/crates/opencord-core/src/convert.rs
+++ b/crates/opencord-core/src/convert.rs
@@ -3,11 +3,13 @@
 use opencord_proto::v1 as proto;
 
 use crate::api::types::{
-    Ban, Channel, ChannelKind, ChannelPermissions, ConnectAddress, CoreError, CoreEventPayload,
-    EphemeralMessage, ErrorCode, Invite, Member, Message, OverwriteTargetKind, PermissionOverwrite,
-    Presence, PresenceStatus, ReadySnapshot, Role, ScreenShareResolution, ScreenStream,
-    ServerAddresses, ServerInfo, SoundEffect, SoundboardSound, StreamSourceKind, User,
-    VoiceSettings, VoiceSettingsChanges, VoiceState,
+    Attachment, AttachmentKind, Ban, Channel, ChannelKind, ChannelPermissions, ConnectAddress,
+    CoreError, CoreEventPayload, EphemeralMessage, ErrorCode, Invite, Member, Message,
+    OverwriteTargetKind, PermissionOverwrite, Presence, PresenceStatus, ReadySnapshot, Role,
+    S3Config, ScreenShareResolution, ScreenStream, ServerAddresses, ServerInfo, SoundEffect,
+    SoundboardSound, StorageBackend, StorageInfo, StorageKind, StorageMove, StorageTestResult,
+    StorageTestStep, StreamSourceKind, UploadSettings, User, VoiceSettings, VoiceSettingsChanges,
+    VoiceState,
 };
 use crate::identity::public_key_fingerprint;
 
@@ -47,6 +49,7 @@
         permissions: bits_to_api(role.permissions),
         hoist: role.hoist,
         mentionable: role.mentionable,
+        max_upload_bytes: role.max_upload_bytes,
     }
 }
 
@@ -213,6 +216,116 @@
         edited_at_ms: message.edited_at_ms,
         nonce: message.nonce,
         command: message.interaction.map(|interaction| interaction.command),
+        attachments: message.attachments.into_iter().map(attachment).collect(),
+    }
+}
+
+pub fn attachment(attachment: proto::Attachment) -> Attachment {
+    let kind = match proto::AttachmentKind::try_from(attachment.kind) {
+        Ok(proto::AttachmentKind::Image) => AttachmentKind::Image,
+        Ok(proto::AttachmentKind::Video) => AttachmentKind::Video,
+        Ok(proto::AttachmentKind::Audio) => AttachmentKind::Audio,
+        Ok(proto::AttachmentKind::VoiceMessage) => AttachmentKind::VoiceMessage,
+        _ => AttachmentKind::File,
+    };
+    Attachment {
+        id: attachment.id,
+        filename: attachment.filename,
+        content_type: attachment.content_type,
+        size_bytes: attachment.size_bytes,
+        kind,
+        width: attachment.width,
+        height: attachment.height,
+        duration_ms: attachment.duration_ms,
+        waveform: attachment.waveform,
+        has_thumbnail: attachment.has_thumbnail,
+        thumbnail_width: attachment.thumbnail_width,
+        thumbnail_height: attachment.thumbnail_height,
+        animated: attachment.animated,
+    }
+}
+
+pub fn upload_settings(settings: proto::UploadSettings) -> UploadSettings {
+    UploadSettings {
+        blocked_extensions: settings.blocked_extensions,
+        voice_message_max_seconds: settings.voice_message_max_seconds,
+        max_file_bytes: settings.max_file_bytes,
+        max_attachments: settings.max_attachments,
+    }
+}
+
+pub fn storage(storage: proto::Storage) -> StorageInfo {
+    StorageInfo {
+        backends: storage.backends.into_iter().map(storage_backend).collect(),
+        move_status: storage.r#move.map(|status| StorageMove {
+            to_storage_id: status.to_storage_id,
+            running: status.running,
+            files_done: status.files_done,
+            files_total: status.files_total,
+            bytes_done: status.bytes_done,
+            bytes_total: status.bytes_total,
+            failed: status.failed,
+        }),
+        temp_free_bytes: storage.temp_free_bytes,
+    }
+}
+
+fn storage_backend(backend: proto::StorageBackend) -> StorageBackend {
+    let kind = match proto::StorageKind::try_from(backend.kind) {
+        Ok(proto::StorageKind::S3) => StorageKind::S3,
+        _ => StorageKind::Local,
+    };
+    StorageBackend {
+        id: backend.id,
+        kind,
+        name: backend.name,
+        active: backend.active,
+        s3: backend.s3.map(s3_config),
+        has_secret: backend.has_secret,
+        file_count: backend.file_count,
+        bytes: backend.bytes,
+        last_error: backend.last_error,
+    }
+}
+
+fn s3_config(config: proto::S3Config) -> S3Config {
+    S3Config {
+        endpoint: config.endpoint,
+        region: config.region,
+        bucket: config.bucket,
+        key_prefix: config.key_prefix,
+        path_style: config.path_style,
+        allow_http: config.allow_http,
+        access_key_id: config.access_key_id,
+        secret_access_key: config.secret_access_key,
+    }
+}
+
+pub fn s3_config_to_proto(config: S3Config) -> proto::S3Config {
+    proto::S3Config {
+        endpoint: config.endpoint,
+        region: config.region,
+        bucket: config.bucket,
+        key_prefix: config.key_prefix,
+        path_style: config.path_style,
+        allow_http: config.allow_http,
+        access_key_id: config.access_key_id,
+        secret_access_key: config.secret_access_key,
+    }
+}
+
+pub fn storage_test(test: proto::StorageTest) -> StorageTestResult {
+    StorageTestResult {
+        steps: test
+            .steps
+            .into_iter()
+            .map(|step| StorageTestStep {
+                name: step.name,
+                ok: step.ok,
+                detail: step.detail,
+            })
+            .collect(),
+        ok: test.ok,
     }
 }
 
@@ -319,6 +432,7 @@
             .collect(),
         addresses: server_addresses(ready.server_addresses.unwrap_or_default()),
         commands: crate::commands::list_from(ready.commands.unwrap_or_default()),
+        upload_settings: upload_settings(ready.upload_settings.unwrap_or_default()),
     }
 }
 
@@ -469,15 +583,19 @@
             nonce: message.nonce,
             content: message.content,
         }),
+        Kind::UploadSettingsUpdate(update) => {
+            CoreEventPayload::UploadSettingsUpdate(upload_settings(update.settings?))
+        }
+        Kind::StorageUpdate(update) => CoreEventPayload::StorageUpdate(storage(update.storage?)),
         // With the server's clock: `music::event`.
         Kind::MusicSnapshot(_)
         | Kind::MusicUpdate(_)
         | Kind::MusicNotice(_)
         | Kind::MusicNowPlaying(_)
-        | Kind::UploadSettingsUpdate(_)
+        // For the send waiting on them: `connection.rs` hands them to
+        // `Uploads`.
         | Kind::UploadReady(_)
-        | Kind::UploadFailed(_)
-        | Kind::StorageUpdate(_) => return None,
+        | Kind::UploadFailed(_) => return None,
     })
 }
 
@@ -523,6 +641,14 @@
         Ok(proto::ErrorCode::CommandDisabled) => ErrorCode::CommandDisabled,
         Ok(proto::ErrorCode::CommandNotAllowedHere) => ErrorCode::CommandNotAllowedHere,
         Ok(proto::ErrorCode::CommandInvalidOptions) => ErrorCode::CommandInvalidOptions,
+        Ok(proto::ErrorCode::FileTooLarge) => ErrorCode::FileTooLarge,
+        Ok(proto::ErrorCode::FileTypeBlocked) => ErrorCode::FileTypeBlocked,
+        Ok(proto::ErrorCode::UploadOffsetMismatch) => ErrorCode::UploadOffsetMismatch,
+        Ok(proto::ErrorCode::StorageUnavailable) => ErrorCode::StorageUnavailable,
+        Ok(proto::ErrorCode::StorageFull) => ErrorCode::StorageFull,
+        Ok(proto::ErrorCode::VoiceMessageInvalid) => ErrorCode::VoiceMessageInvalid,
+        Ok(proto::ErrorCode::VoiceMessageTooLong) => ErrorCode::VoiceMessageTooLong,
+
         _ => ErrorCode::Unknown,
     }
 }
```

**`src/client.rs`:**

```diff
--- a/crates/opencord-core/src/client.rs
+++ b/crates/opencord-core/src/client.rs
@@ -19,9 +19,10 @@
     AddServerOutcome, AudioSettings, Ban, CameraStarted, Channel, ChannelChanges, ChannelKind,
     ChannelPosition, CoreError, CoreEvent, CoreEventPayload, ErrorCode, HotkeyBinding,
     HotkeySupport, IdentityInfo, Invite, MediaEvent, Member, Message, OverwriteTargetKind,
-    PermissionOverwrite, PresenceStatus, Role, RoleChanges, ScreenProblem, ScreenShareRequest,
-    ScreenShareStarted, ScreenStream, Server, ServerChanges, ServerInfo, SoundDraft,
-    SoundSelection, SoundboardSound, SoundboardSoundChanges, StreamSourceKind, User, VideoWant,
+    PermissionOverwrite, PresenceStatus, Role, RoleChanges, S3Config, ScreenProblem,
+    ScreenShareRequest, ScreenShareStarted, ScreenStream, Server, ServerChanges, ServerInfo,
+    SoundDraft, SoundSelection, SoundboardSound, SoundboardSoundChanges, StorageInfo,
+    StorageTestResult, StorageTestTarget, StreamSourceKind, UploadSettings, User, VideoWant,
     VoiceCue, VoiceSettings, VoiceSettingsChanges, VoiceState,
 };
 use crate::artwork::ArtworkCache;
@@ -530,6 +531,7 @@
         expect_channel(self.request(key, request).await?)
     }
 
+    #[allow(clippy::too_many_arguments)]
     pub async fn create_role(
         &self,
         key: &str,
@@ -538,6 +540,7 @@
         permissions: i64,
         hoist: bool,
         mentionable: bool,
+        max_upload_bytes: Option<u64>,
     ) -> Result<Role, CoreError> {
         let request = Request::CreateRole(proto::CreateRole {
             name,
@@ -545,7 +548,7 @@
             permissions: convert::bits_from_api(permissions),
             hoist,
             mentionable,
-            max_upload_bytes: None,
+            max_upload_bytes,
         });
         expect_role(self.request(key, request).await?)
     }
@@ -563,7 +566,7 @@
             permissions: changes.permissions.map(convert::bits_from_api),
             hoist: changes.hoist,
             mentionable: changes.mentionable,
-            max_upload_bytes: None,
+            max_upload_bytes: changes.max_upload_bytes,
         });
         expect_role(self.request(key, request).await?)
     }
@@ -1750,6 +1753,109 @@
         Arc::clone(&self.inner.artwork)
     }
 
+    /// Owner or MANAGE_SERVER. `None` leaves a setting as it is.
+    pub async fn update_upload_settings(
+        &self,
+        key: &str,
+        blocked_extensions: Option<Vec<String>>,
+        voice_message_max_seconds: Option<u32>,
+    ) -> Result<UploadSettings, CoreError> {
+        let request = Request::UpdateUploadSettings(proto::UpdateUploadSettings {
+            blocked_extensions: blocked_extensions
+                .map(|extensions| proto::BlockedExtensions { extensions }),
+            voice_message_max_seconds,
+        });
+        match self.request(key, request).await? {
+            Response::UploadSettings(settings) => Ok(convert::upload_settings(settings)),
+            other => Err(unexpected(&other)),
+        }
+    }
+
+    pub async fn storage_get(&self, key: &str) -> Result<StorageInfo, CoreError> {
+        expect_storage(
+            self.request(key, Request::GetStorage(proto::GetStorage {}))
+                .await?,
+        )
+    }
+
+    pub async fn storage_add(
+        &self,
+        key: &str,
+        name: String,
+        s3: S3Config,
+    ) -> Result<StorageInfo, CoreError> {
+        let request = Request::AddStorage(proto::AddStorage {
+            name,
+            s3: Some(convert::s3_config_to_proto(s3)),
+        });
+        expect_storage(self.request(key, request).await?)
+    }
+
+    pub async fn storage_update(
+        &self,
+        key: &str,
+        storage_id: i64,
+        name: Option<String>,
+        s3: Option<S3Config>,
+    ) -> Result<StorageInfo, CoreError> {
+        let request = Request::UpdateStorage(proto::UpdateStorage {
+            storage_id,
+            name,
+            s3: s3.map(convert::s3_config_to_proto),
+        });
+        expect_storage(self.request(key, request).await?)
+    }
+
+    pub async fn storage_test(
+        &self,
+        key: &str,
+        target: StorageTestTarget,
+    ) -> Result<StorageTestResult, CoreError> {
+        let target = match target {
+            StorageTestTarget::Existing { storage_id } => {
+                proto::test_storage::Target::StorageId(storage_id)
+            }
+            StorageTestTarget::Config { s3 } => {
+                proto::test_storage::Target::S3(convert::s3_config_to_proto(s3))
+            }
+        };
+        let request = Request::TestStorage(proto::TestStorage {
+            target: Some(target),
+        });
+        match self.request(key, request).await? {
+            Response::StorageTest(test) => Ok(convert::storage_test(test)),
+            other => Err(unexpected(&other)),
+        }
+    }
+
+    pub async fn storage_activate(
+        &self,
+        key: &str,
+        storage_id: i64,
+    ) -> Result<StorageInfo, CoreError> {
+        let request = Request::ActivateStorage(proto::ActivateStorage { storage_id });
+        expect_storage(self.request(key, request).await?)
+    }
+
+    pub async fn storage_remove(
+        &self,
+        key: &str,
+        storage_id: i64,
+    ) -> Result<StorageInfo, CoreError> {
+        let request = Request::RemoveStorage(proto::RemoveStorage { storage_id });
+        expect_storage(self.request(key, request).await?)
+    }
+
+    pub async fn storage_move_start(&self, key: &str) -> Result<StorageInfo, CoreError> {
+        let request = Request::StartStorageMove(proto::StartStorageMove {});
+        expect_storage(self.request(key, request).await?)
+    }
+
+    pub async fn storage_move_stop(&self, key: &str) -> Result<StorageInfo, CoreError> {
+        let request = Request::StopStorageMove(proto::StopStorageMove {});
+        expect_storage(self.request(key, request).await?)
+    }
+
     /// A request answered with an ack.
     pub(crate) async fn ack(&self, key: &str, kind: Request) -> Result<(), CoreError> {
         expect_ack(self.request(key, kind).await?)
@@ -1776,6 +1882,13 @@
         other => Err(unexpected(&other)),
     }
 }
+
+fn expect_storage(response: Response) -> Result<StorageInfo, CoreError> {
+    match response {
+        Response::Storage(storage) => Ok(convert::storage(storage)),
+        other => Err(unexpected(&other)),
+    }
+}
 
 fn expect_role(response: Response) -> Result<Role, CoreError> {
     match response {
```

**`src/api/client.rs`:**

```diff
--- a/crates/opencord-core/src/api/client.rs
+++ b/crates/opencord-core/src/api/client.rs
@@ -326,6 +326,8 @@
     .await
 }
 
+/// `max_upload_bytes`: `None` leaves uploads to the member's other roles.
+#[allow(clippy::too_many_arguments)]
 pub async fn create_role(
     server_key: String,
     name: String,
@@ -333,10 +335,19 @@
     permissions: i64,
     hoist: bool,
     mentionable: bool,
+    max_upload_bytes: Option<u64>,
 ) -> Result<Role, CoreError> {
     on_runtime(move |client| async move {
         client
-            .create_role(&server_key, name, color, permissions, hoist, mentionable)
+            .create_role(
+                &server_key,
+                name,
+                color,
+                permissions,
+                hoist,
+                mentionable,
+                max_upload_bytes,
+            )
             .await
     })
     .await
@@ -1463,12 +1474,12 @@
     Ok(())
 }
 
-fn client() -> Result<Client, CoreError> {
+pub(crate) fn client() -> Result<Client, CoreError> {
     CLIENT.get().cloned().ok_or(CoreError::NotInitialized)
 }
 
 /// Runs `work` on the core's runtime, where connections live.
-async fn on_runtime<T, F>(work: impl FnOnce(Client) -> F) -> Result<T, CoreError>
+pub(crate) async fn on_runtime<T, F>(work: impl FnOnce(Client) -> F) -> Result<T, CoreError>
 where
     F: Future<Output = Result<T, CoreError>> + Send + 'static,
     T: Send + 'static,
```

**`src/api/mod.rs`, `tests/client.rs` and `examples/m6_peer.rs`:**

```diff
--- a/crates/opencord-core/src/api/mod.rs
+++ b/crates/opencord-core/src/api/mod.rs
@@ -1,3 +1,5 @@
+pub mod attachments;
 pub mod client;
+
 pub mod system;
 pub mod types;
```

```diff
--- a/crates/opencord-core/tests/client.rs
+++ b/crates/opencord-core/tests/client.rs
@@ -403,9 +403,11 @@
             bits(Permissions::MANAGE_MESSAGES),
             false,
             false,
+            None,
         )
         .await
         .unwrap();
+
     owner
         .client
         .add_member_role(&key, member_ready.self_user.id, role.id)
```

```diff
--- a/crates/opencord-core/examples/m6_peer.rs
+++ b/crates/opencord-core/examples/m6_peer.rs
@@ -105,7 +105,7 @@
         .await?;
 
     let role = client
-        .create_role(&key, ROLE.to_owned(), 0x3B_A5_5C, 0, true, false)
+        .create_role(&key, ROLE.to_owned(), 0x3B_A5_5C, 0, true, false, None)
         .await?;
     client.add_member_role(&key, member_id, role.id).await?;
     client
```

**`src/api/attachments.rs`** (new):

```rust
//! Files in messages, upload settings and storage (media uploads plan,
//! milestone A4), for the Flutter app.

use super::client::on_runtime;
use super::types::{
    CoreError, S3Config, StorageInfo, StorageTestResult, StorageTestTarget, UploadSettings,
};

/// MANAGE_SERVER. `None` leaves a setting as it is.
pub async fn update_upload_settings(
    server_key: String,
    blocked_extensions: Option<Vec<String>>,
    voice_message_max_seconds: Option<u32>,
) -> Result<UploadSettings, CoreError> {
    on_runtime(move |client| async move {
        client
            .update_upload_settings(&server_key, blocked_extensions, voice_message_max_seconds)
            .await
    })
    .await
}

/// The storage requests are the owner's only.
pub async fn storage_get(server_key: String) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_get(&server_key).await }).await
}

/// Tests the bucket first; refused when the test fails.
pub async fn storage_add(
    server_key: String,
    name: String,
    s3: S3Config,
) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_add(&server_key, name, s3).await }).await
}

/// An empty secret keeps the saved one.
pub async fn storage_update(
    server_key: String,
    storage_id: i64,
    name: Option<String>,
    s3: Option<S3Config>,
) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move {
        client
            .storage_update(&server_key, storage_id, name, s3)
            .await
    })
    .await
}

pub async fn storage_test(
    server_key: String,
    target: StorageTestTarget,
) -> Result<StorageTestResult, CoreError> {
    on_runtime(move |client| async move { client.storage_test(&server_key, target).await }).await
}

/// New files go there from now on.
pub async fn storage_activate(
    server_key: String,
    storage_id: i64,
) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_activate(&server_key, storage_id).await })
        .await
}

pub async fn storage_remove(server_key: String, storage_id: i64) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_remove(&server_key, storage_id).await })
        .await
}

/// Moves every file to the active storage; StorageUpdate events follow it.
pub async fn storage_move_start(server_key: String) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_move_start(&server_key).await }).await
}

pub async fn storage_move_stop(server_key: String) -> Result<StorageInfo, CoreError> {
    on_runtime(move |client| async move { client.storage_move_stop(&server_key).await }).await
}
```

**`tests/attachments.rs`** (new). Tasks 18 to 20 add to it.

```rust
//! Files in messages, upload settings and storage, against a real server
//! (media uploads plan, milestone A4).

use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    CoreEvent, CoreEventPayload, ReadySnapshot, RoleChanges, StorageKind,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(20);
const MIB: usize = 1024 * 1024;

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        config.voice.udp_port = 0;
        config.music.audio_udp_port = 0;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }
}

struct TestClient {
    client: Client,
    events: UnboundedReceiver<CoreEvent>,
    _dir: tempfile::TempDir,
}

impl TestClient {
    /// The owner: trusts the certificate and claims the server.
    async fn owner(server: &TestServer) -> (Self, ReadySnapshot) {
        let dir = tempfile::tempdir().unwrap();
        let (client, events) = Client::new(dir.path(), tokio::runtime::Handle::current()).unwrap();
        client.set_identity(Identity::generate(), "Owner".to_owned());
        client
            .trust_fingerprint(
                &server.address(),
                &format_fingerprint(&server.handle.fingerprint),
            )
            .unwrap();
        client
            .add_server(&server.address(), server.handle.claim_token.clone())
            .await
            .unwrap();
        let mut owner = Self {
            client,
            events,
            _dir: dir,
        };
        let ready = owner
            .wait_for(|payload| match payload {
                CoreEventPayload::Ready(ready) => Some(ready.clone()),
                _ => None,
            })
            .await;
        (owner, ready)
    }

    /// Skips events until `pick` returns something.
    async fn wait_for<T>(&mut self, mut pick: impl FnMut(&CoreEventPayload) -> Option<T>) -> T {
        tokio::time::timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.expect("the event stream ended");
                if let Some(found) = pick(&event.payload) {
                    return found;
                }
            }
        })
        .await
        .expect("timed out waiting for an event")
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn ready_carries_upload_settings_and_role_limits() {
    let server = TestServer::start().await;
    let (_owner, ready) = TestClient::owner(&server).await;

    assert_eq!(ready.upload_settings.max_attachments, 10);
    assert_eq!(ready.upload_settings.max_file_bytes, 4096 * MIB as u64);
    assert_eq!(ready.upload_settings.voice_message_max_seconds, 300);
    let everyone = ready
        .roles
        .iter()
        .find(|role| role.name == "@everyone")
        .unwrap();
    assert_eq!(everyone.max_upload_bytes, Some(50 * MIB as u64));
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_updates_upload_settings_and_sees_the_event() {
    let server = TestServer::start().await;
    let (mut owner, _) = TestClient::owner(&server).await;
    let key = server.address();

    let settings = owner
        .client
        .update_upload_settings(&key, Some(vec!["psd".to_owned()]), Some(120))
        .await
        .unwrap();

    assert_eq!(settings.blocked_extensions, ["psd"]);
    assert_eq!(settings.voice_message_max_seconds, 120);
    let event = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::UploadSettingsUpdate(settings) => Some(settings.clone()),
            _ => None,
        })
        .await;
    assert_eq!(event, settings);
}

#[tokio::test(flavor = "multi_thread")]
async fn storage_get_lists_the_local_storage() {
    let server = TestServer::start().await;
    let (owner, _) = TestClient::owner(&server).await;

    let storage = owner.client.storage_get(&server.address()).await.unwrap();

    assert_eq!(storage.backends.len(), 1);
    assert_eq!(storage.backends[0].kind, StorageKind::Local);
    assert!(storage.backends[0].active);
    assert!(storage.move_status.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn role_limit_round_trips() {
    let server = TestServer::start().await;
    let (owner, _) = TestClient::owner(&server).await;
    let key = server.address();

    let role = owner
        .client
        .create_role(
            &key,
            "uploaders".to_owned(),
            0,
            0,
            false,
            false,
            Some(100 * MIB as u64),
        )
        .await
        .unwrap();
    assert_eq!(role.max_upload_bytes, Some(100 * MIB as u64));

    let changes = RoleChanges {
        max_upload_bytes: Some(0),
        ..RoleChanges::default()
    };
    let role = owner
        .client
        .update_role(&key, role.id, changes)
        .await
        .unwrap();
    assert_eq!(role.max_upload_bytes, None);
}
```

**Dart.** After regenerating, `flutter analyze` reports exactly these:
- three switches no longer cover every case (`ErrorCode`, `CoreError` and `CoreEventPayload`);
- eight test call sites miss the new required fields `attachments` and `uploadSettings`.

The app maps the new codes to existing kinds and ignores the new events until Task 22 gives them their own. `uploadSettings` is a `BigInt`-holding value, so the shared test fixture is `final`, not `const`.

```diff
--- a/app/lib/core/rust/core_mapping.dart
+++ b/app/lib/core/rust/core_mapping.dart
@@ -429,6 +429,14 @@
   core.ErrorCode.soundCooldown => RepoErrorKind.soundCooldown,
   core.ErrorCode.soundTooLong => RepoErrorKind.soundTooLong,
   core.ErrorCode.soundInvalid => RepoErrorKind.soundInvalid,
+  // Media uploads; Task 22 gives them kinds of their own.
+  core.ErrorCode.fileTooLarge ||
+  core.ErrorCode.fileTypeBlocked ||
+  core.ErrorCode.voiceMessageInvalid ||
+  core.ErrorCode.voiceMessageTooLong => RepoErrorKind.invalidArgument,
+  core.ErrorCode.uploadOffsetMismatch => RepoErrorKind.conflict,
+  core.ErrorCode.storageUnavailable ||
+  core.ErrorCode.storageFull => RepoErrorKind.other,
   core.ErrorCode.internal || core.ErrorCode.unknown => RepoErrorKind.other,
 };
 
@@ -483,6 +491,10 @@
     RepoErrorKind.other,
     message,
   ),
+  core.CoreError_Cancelled() => const RepoException(
+    RepoErrorKind.other,
+    'Cancelled.',
+  ),
   core.CoreError_Camera(:final problem, :final message) => RepoException(
     switch (problem) {
       core.CameraProblem.denied => RepoErrorKind.cameraDenied,
```

```diff
--- a/app/lib/core/rust/rust_repository.dart
+++ b/app/lib/core/rust/rust_repository.dart
@@ -622,6 +622,11 @@
           at: DateTime.fromMillisecondsSinceEpoch(field0.atMs),
         ),
       ),
+      // Uploads, downloads and storage reach the app from Task 22 on.
+      core.CoreEventPayload_UploadSettingsUpdate() ||
+      core.CoreEventPayload_StorageUpdate() ||
+      core.CoreEventPayload_UploadProgress() ||
+      core.CoreEventPayload_DownloadProgress() => null,
     };
     if (event != null) _emit(event);
   }
```

```diff
--- a/app/test/support/fake_core.dart
+++ b/app/test/support/fake_core.dart
@@ -362,6 +362,7 @@
       content: content,
       createdAtMs: 0,
       nonce: nonce,
+      attachments: const [],
     );
   }
 
@@ -478,6 +479,13 @@
 }
 
 /// The server's default voice settings, as the core reports them.
+final coreUploadSettings = core.UploadSettings(
+  blockedExtensions: const [],
+  voiceMessageMaxSeconds: 300,
+  maxFileBytes: BigInt.from(4096 * 1024 * 1024),
+  maxAttachments: 10,
+);
+
 const coreVoiceSettings = core.VoiceSettings(
   screenShareMaxResolution: core.ScreenShareResolution.p720,
   screenShareMaxFps: 30,
```

```diff
--- a/app/test/unit/rust/core_mapping_test.dart
+++ b/app/test/unit/rust/core_mapping_test.dart
@@ -30,6 +30,7 @@
         createdAtMs: 1760000000000,
         editedAtMs: 1760000005000,
         nonce: 'n1',
+        attachments: [],
       ),
     );
 
@@ -55,6 +56,7 @@
         authorId: 7,
         content: 'hi',
         createdAtMs: 9223372036854775807,
+        attachments: [],
       ),
     );
 
@@ -187,6 +189,7 @@
         ],
         addresses: const core.ServerAddresses(domains: []),
         commands: const core.CommandList(commands: [], resultsPublic: true),
+        uploadSettings: coreUploadSettings,
       ),
     );
 
```

```diff
--- a/app/test/unit/rust/rust_repository_test.dart
+++ b/app/test/unit/rust/rust_repository_test.dart
@@ -35,6 +35,7 @@
   authorId: author,
   content: content,
   createdAtMs: 1760000000000 + id,
+  attachments: const [],
 );
 
 core.Channel _text(int id, String name, {int position = 0}) => core.Channel(
@@ -83,6 +84,7 @@
   ],
   addresses: const core.ServerAddresses(domains: []),
   commands: const core.CommandList(commands: [], resultsPublic: true),
+  uploadSettings: coreUploadSettings,
 );
 
 /// A keychain that can be locked, refusing to save, or out of reach.
```

```diff
--- a/app/test/unit/state/channel_messages_test.dart
+++ b/app/test/unit/state/channel_messages_test.dart
@@ -26,6 +26,7 @@
   authorId: 2,
   content: 'm$id',
   createdAtMs: 1760000000000 + id,
+  attachments: const [],
 );
 
 /// Newest first, as the core answers.
@@ -76,6 +77,7 @@
     ],
     addresses: const core.ServerAddresses(domains: []),
     commands: const core.CommandList(commands: [], resultsPublic: true),
+    uploadSettings: coreUploadSettings,
   ),
 );
 
```

- [ ] **Step 1:** Create `tests/attachments.rs` as given.
- [ ] **Step 2:** Run it → FAIL: it doesn't compile yet.
- [ ] **Step 3:** Make the Rust changes, then regenerate the bindings.
- [ ] **Step 4:** Make the Dart changes.
- [ ] **Step 5:** Run → PASS:
  - `cargo test -p opencord_core --test attachments`: 4 tests;
  - `cargo test -p opencord_core --test client`;
  - `cargo clippy -p opencord_core --all-targets -- -D warnings`;
  - `cd app && flutter analyze && flutter test`.
- [ ] **Step 6: Commit:** `feat: core types and requests for uploads and storage (A4)`.

### Task 18: Transfers, sending files and saving them

**What it does.**
- **Transfers** (`attachments/transfer.rs`).
  - Uploads and downloads use one `reqwest` client per server. It is built with the rustls config of `TlsTrust`, the gateway's own certificate check, so a pinned certificate stays pinned.
  - A client is rebuilt when the server's route changes, and its entry leaves when the server has no route.
  - A changed certificate is `FingerprintMismatch`. A 401 renews the media token once. The renewal moves out of `MediaHttp` into `http::renew_media_token`, which both use (Working Rule 19).
  - Timeouts: 10 s to connect, and 60 s without a byte ends any request. A piece may take 120 s, a status or finish 30 s, and a download has no overall limit.
- **Sending** (`attachments/upload.rs`).
  - `send_message_with_files` takes 1 to 10 files, each by path or by bytes. Two go up at a time, each in the server's pieces of 8 MiB. Then SendMessage goes out with their ids in the files' order.
  - A network failure is tried again 1, 2, 4, 8 and 16 seconds later, six tries in all.
  - **A piece whose answer was lost goes again.** The server answers 409 with what it has, and the upload carries on from there (Review Focus 1). That is why a network failure never needs a status request.
  - Each file's verdict comes by its UploadReady or UploadFailed event. Its waiter is registered **before** the finish request, so a verdict that comes at once isn't missed. The status is also asked for every 5 seconds, in case the event went with a dropped connection.
  - **UploadProgress** follows each file:
    - Preparing, then Uploading at most 10 times a second, then Processing, Sending and Done;
    - on a failure the failing file is `Failed` and the others `Cancelled`.
  - `cancel_send` stops a send while it uploads. After a cancel or any failure, every upload the send made is cancelled on the server.
  - Both maps (`sends`, `waiters`) lose their entry through a guard's `Drop`, on every path (Working Rule 14). A second send with the same nonce is refused.
  - Test hook: `lose_upload_answers(n)` drops the next n piece answers after the server took the piece.
- **Saving** (`attachments/download.rs`). `download_attachment` streams a file to `path.part`, then renames it.
  - DownloadProgress follows it at most 10 times a second, and once at the end with `done` or `error`.
  - `cancel_download` stops it. On any failure the `.part` file is removed.
- **Server hook** (`crates/opencord-server/src/server.rs`): `ServerHandle::uploads_in_state`, for the cancel test.
- Test hook: `spoil_media_token(key)` makes the next transfer meet a 401.

**Files.**
- Create: `src/attachments/{mod,transfer,upload,download}.rs`.
- Modify: `Cargo.toml`, `src/lib.rs`, `src/http.rs`, `src/connection.rs`, `src/client.rs`, `src/api/attachments.rs`, `tests/attachments.rs`, `crates/opencord-server/src/server.rs`.

**`Cargo.toml`:**

```diff
--- a/crates/opencord-core/Cargo.toml
+++ b/crates/opencord-core/Cargo.toml
@@ -12,6 +12,7 @@
 crate-type = ["cdylib", "staticlib", "rlib"]
 
 [dependencies]
+bytes.workspace = true
 data-encoding.workspace = true
 flutter_rust_bridge.workspace = true
 futures-util.workspace = true
@@ -21,6 +22,7 @@
 opencord-media.workspace = true
 opencord-proto.workspace = true
 prost.workspace = true
+reqwest.workspace = true
 rustls.workspace = true
 rustls-platform-verifier.workspace = true
 serde.workspace = true
@@ -29,6 +31,7 @@
 thiserror.workspace = true
 tokio-rustls.workspace = true
 tokio-tungstenite.workspace = true
+tokio-util.workspace = true
 tokio.workspace = true
 tracing.workspace = true
 webpki-roots.workspace = true
@@ -46,6 +49,7 @@
 irondash_run_loop.workspace = true
 irondash_texture.workspace = true
 
+
 # One section per system (docs/plans/platforms.md): each session edits only
 # its own. A crate only one system needs goes here with its version, never
 # in [workspace.dependencies].
```

**`src/lib.rs`, `src/http.rs` and `src/connection.rs`:**

```diff
--- a/crates/opencord-core/src/lib.rs
+++ b/crates/opencord-core/src/lib.rs
@@ -2,6 +2,8 @@
 
 pub mod api;
 mod artwork;
+pub mod attachments;
+
 pub mod client;
 mod commands;
 mod connection;
```

```diff
--- a/crates/opencord-core/src/http.rs
+++ b/crates/opencord-core/src/http.rs
@@ -75,7 +75,7 @@
             .ok_or(CoreError::NotConnected)?;
         let mut renewed = false;
         loop {
-            let token = self.token(key).ok_or(CoreError::NotConnected)?;
+            let token = media_token(&self.tokens, key).ok_or(CoreError::NotConnected)?;
             let response = request(&address, pin, method, path, &token, body, limit)
                 .await
                 .map_err(|error| CoreError::Connection {
@@ -84,29 +84,40 @@
             if response.status != 401 || renewed {
                 return Ok(response);
             }
-            let renewal = client
-                .request(
-                    key,
-                    proto::request::Kind::RefreshMediaToken(proto::RefreshMediaToken {}),
-                )
-                .await?;
-            if let proto::response::Result::MediaToken(token) = renewal {
-                self.tokens
-                    .lock()
-                    .unwrap_or_else(PoisonError::into_inner)
-                    .insert(key.to_owned(), token.token);
-            }
+            renew_media_token(client, &self.tokens, key).await?;
             renewed = true;
         }
     }
+}
+
+/// This device's media token for the server at `key`.
+pub(crate) fn media_token(tokens: &MediaTokens, key: &str) -> Option<String> {
+    tokens
+        .lock()
+        .unwrap_or_else(PoisonError::into_inner)
+        .get(key)
+        .cloned()
+}
 
-    fn token(&self, key: &str) -> Option<String> {
-        self.tokens
+/// Asks the server at `key` for a new media token, after one expired.
+pub(crate) async fn renew_media_token(
+    client: &Client,
+    tokens: &MediaTokens,
+    key: &str,
+) -> Result<(), CoreError> {
+    let renewal = client
+        .request(
+            key,
+            proto::request::Kind::RefreshMediaToken(proto::RefreshMediaToken {}),
+        )
+        .await?;
+    if let proto::response::Result::MediaToken(token) = renewal {
+        tokens
             .lock()
             .unwrap_or_else(PoisonError::into_inner)
-            .get(key)
-            .cloned()
+            .insert(key.to_owned(), token.token);
     }
+    Ok(())
 }
 
 /// One request to the server at `address`, whose certificate must match
```

```diff
--- a/crates/opencord-core/src/connection.rs
+++ b/crates/opencord-core/src/connection.rs
@@ -26,8 +26,10 @@
 use crate::api::types::{
     ConnectionState, CoreError, CoreEvent, CoreEventPayload, ErrorCode, FailureReason,
 };
+use crate::attachments::upload::Uploads;
 use crate::convert;
 use crate::identity::Identity;
+
 use crate::mirror::{GuildMirror, PermissionSnapshot};
 use crate::server_clock::ServerClock;
 use crate::store::Store;
@@ -350,6 +352,8 @@
     pub media_tokens: MediaTokens,
     pub routes: Routes,
     pub clocks: Clocks,
+    /// Sends waiting for their uploads' verdicts.
+    pub uploads: Arc<Uploads>,
 }
 
 /// Each server's clock against this device's, by server key.
@@ -886,6 +890,22 @@
             proto::event::Kind::MusicNowPlaying(_) | proto::event::Kind::MusicNotice(_) => {
                 crate::music::event(kind)
             }
+            // For the send waiting on them, not for the app.
+            proto::event::Kind::UploadReady(ready) => {
+                if let Some(attachment) = ready.attachment {
+                    self.context.uploads.on_ready(&self.context.key, attachment);
+                }
+                None
+            }
+            proto::event::Kind::UploadFailed(failed) => {
+                self.context.uploads.on_failed(
+                    &self.context.key,
+                    failed.upload_id,
+                    failed.error.unwrap_or_default(),
+                );
+                None
+            }
+
             kind => convert::event(kind, session_id),
         };
         if let Some(payload) = payload {
```

**`src/attachments/mod.rs`** (new; later tasks add their modules):

```rust
//! Files in messages (media uploads plan, milestone A4): sending them,
//! downloading them, the encrypted cache, photo metadata and the clipboard.

pub(crate) mod download;
pub(crate) mod transfer;
pub(crate) mod upload;
```

**`src/attachments/transfer.rs`** (new):

```rust
//! HTTPS to a server's upload and download endpoints, checked like its
//! gateway (media uploads plan, Task 18). Unlike `http.rs`, 8 MiB pieces go
//! up and whole files come down, so each server gets one client whose
//! connections are kept, and answers are read as they arrive.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use opencord_common::address::format_fingerprint;
use opencord_proto::v1 as proto;
use prost::Message as _;
use reqwest::StatusCode;

use crate::api::types::CoreError;
use crate::client::Client;
use crate::connection::{MediaTokens, Route, Routes};
use crate::convert;
use crate::http::{media_token, renew_media_token};
use crate::tofu::{TlsTrust, Verdict};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest wait for more of an answer.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// One request to a media endpoint.
pub(crate) struct Call {
    pub method: reqwest::Method,
    /// From the root, like `/media/uploads/1`.
    pub path: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Bytes,
    /// For the whole request; `None` for downloads, which may take hours.
    pub timeout: Option<Duration>,
}

impl Call {
    pub fn get(path: String, timeout: Option<Duration>) -> Self {
        Self {
            method: reqwest::Method::GET,
            path,
            headers: Vec::new(),
            body: Bytes::new(),
            timeout,
        }
    }
}

pub(crate) struct Transfers {
    routes: Routes,
    tokens: MediaTokens,
    /// Each server's client, built for the route it was last reached by.
    /// An entry leaves when its server has no route any more.
    clients: Mutex<HashMap<String, Pooled>>,
}

#[derive(Clone)]
struct Pooled {
    route: Route,
    http: reqwest::Client,
    trust: Arc<TlsTrust>,
}

impl Transfers {
    pub fn new(routes: Routes, tokens: MediaTokens) -> Self {
        Self {
            routes,
            tokens,
            clients: Mutex::default(),
        }
    }

    /// Sends `call` to the server at `key` with this device's media token,
    /// renewing an expired token once. Any answer comes back, whatever its
    /// status; only failing to reach the server is an error.
    pub async fn send(
        &self,
        client: &Client,
        key: &str,
        call: Call,
    ) -> Result<reqwest::Response, CoreError> {
        let pooled = self.pooled(key)?;
        let url = format!("https://{}{}", pooled.route.address, call.path);
        let mut renewed = false;
        loop {
            let token = media_token(&self.tokens, key).ok_or(CoreError::NotConnected)?;
            let mut request = pooled
                .http
                .request(call.method.clone(), &url)
                .bearer_auth(token)
                .body(call.body.clone());
            for (name, value) in &call.headers {
                request = request.header(*name, value);
            }
            if let Some(timeout) = call.timeout {
                request = request.timeout(timeout);
            }
            let response = request
                .send()
                .await
                .map_err(|error| failure(&error, &pooled.trust))?;
            if response.status() != StatusCode::UNAUTHORIZED || renewed {
                return Ok(response);
            }
            renew_media_token(client, &self.tokens, key).await?;
            renewed = true;
        }
    }

    /// The client for the server at `key`, built again when the server
    /// was reached another way.
    fn pooled(&self, key: &str) -> Result<Pooled, CoreError> {
        let route = self
            .routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned();
        let mut clients = self.lock_clients();
        let Some(route) = route else {
            clients.remove(key);
            return Err(CoreError::NotConnected);
        };
        if let Some(pooled) = clients.get(key)
            && pooled.route == route
        {
            return Ok(pooled.clone());
        }
        let trust = Arc::new(TlsTrust::new(route.pin));
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured((*trust.config).clone())
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .pool_idle_timeout(IDLE_TIMEOUT)
            .build()
            .map_err(|error| network(&error))?;
        let pooled = Pooled { route, http, trust };
        clients.insert(key.to_owned(), pooled.clone());
        Ok(pooled)
    }

    fn lock_clients(&self) -> MutexGuard<'_, HashMap<String, Pooled>> {
        self.clients.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Why a request didn't reach the server: its certificate changed, or the
/// network.
fn failure(error: &reqwest::Error, trust: &TlsTrust) -> CoreError {
    match trust.verdict() {
        Some(Verdict::Mismatch {
            expected,
            presented,
        }) => CoreError::FingerprintMismatch {
            expected: format_fingerprint(&expected),
            presented: format_fingerprint(&presented),
        },
        _ => network(error),
    }
}

/// A network failure, said by its first cause: reqwest's own text only
/// names the request.
pub(crate) fn network(error: &reqwest::Error) -> CoreError {
    if error.is_timeout() {
        return CoreError::Connection {
            message: "the server stopped answering".to_owned(),
        };
    }
    let mut cause: &dyn std::error::Error = error;
    while let Some(inner) = cause.source() {
        cause = inner;
    }
    CoreError::Connection {
        message: cause.to_string(),
    }
}

/// A media endpoint's protobuf answer (they are small), or the error it
/// holds.
async fn answer(
    response: reqwest::Response,
) -> Result<(StatusCode, proto::response::Result), CoreError> {
    let status = response.status();
    let body = response.bytes().await.map_err(|error| network(&error))?;
    let result = proto::Response::decode(body)
        .ok()
        .and_then(|response| response.result)
        .ok_or_else(|| unexpected(status))?;
    match result {
        proto::response::Result::Error(error) => Err(convert::error(error)),
        result => Ok((status, result)),
    }
}

/// The upload status an upload endpoint answered with.
pub(crate) async fn upload_status(
    response: reqwest::Response,
) -> Result<(StatusCode, proto::UploadStatus), CoreError> {
    match answer(response).await? {
        (status, proto::response::Result::UploadStatus(upload)) => Ok((status, upload)),
        (status, _) => Err(unexpected(status)),
    }
}

/// Why a download was refused.
pub(crate) async fn refusal(response: reqwest::Response) -> CoreError {
    match answer(response).await {
        Ok((status, _)) => unexpected(status),
        Err(error) => error,
    }
}

fn unexpected(status: StatusCode) -> CoreError {
    CoreError::Connection {
        message: format!("the server answered {status}"),
    }
}
```

**`src/attachments/upload.rs`** (new; Task 20 adds photo stripping):

```rust
//! Sending a message with files (media uploads plan, Task 18). Each file
//! goes up in pieces, two files at a time. When every file is ready, the
//! message goes out with their ids.

use std::collections::HashMap;
use std::future::Future;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use opencord_common::upload::{MAX_ATTACHMENTS, PIECE_BYTES};
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use reqwest::StatusCode;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use super::transfer::{Call, upload_status};
use crate::api::types::{CoreError, CoreEventPayload, Message, OutgoingFile, UploadPhase};
use crate::client::{Client, unexpected};
use crate::convert;

/// Files going up at once, per message.
const AT_ONCE: usize = 2;
/// Uploading progress goes out at most this often, per file.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);
const PIECE_TIMEOUT: Duration = Duration::from_secs(120);
const STATUS_TIMEOUT: Duration = Duration::from_secs(30);
/// How often a file being processed is asked about, in case its event was
/// lost with the connection.
const POLL_EVERY: Duration = Duration::from_secs(5);
/// Tries after a network failure, 1, 2, 4, 8 and 16 seconds apart.
const TRIES: u32 = 6;

/// The server's verdict on a finished upload.
type Outcome = Result<proto::Attachment, proto::Error>;

#[derive(Default)]
pub(crate) struct Uploads {
    /// Sends that can still be cancelled, by server key and nonce. An entry
    /// leaves when its send returns.
    sends: Mutex<HashMap<(String, String), CancellationToken>>,
    /// Finished uploads waiting for their verdict, by server key and upload
    /// id. An entry leaves when the verdict comes or its send stops waiting.
    waiters: Mutex<HashMap<(String, i64), oneshot::Sender<Outcome>>>,
    /// Test hook: this many answers to pieces are dropped, as if the network
    /// lost them after the server took the piece.
    lose_answers: Mutex<u32>,
}

impl Uploads {
    /// From an `UploadReady` event.
    pub fn on_ready(&self, key: &str, attachment: proto::Attachment) {
        self.settle(key, attachment.id, Ok(attachment));
    }

    /// From an `UploadFailed` event.
    pub fn on_failed(&self, key: &str, upload_id: i64, error: proto::Error) {
        self.settle(key, upload_id, Err(error));
    }

    fn settle(&self, key: &str, upload_id: i64, outcome: Outcome) {
        let waiter = self.lock_waiters().remove(&(key.to_owned(), upload_id));
        if let Some(waiter) = waiter
            && waiter.send(outcome).is_err()
        {
            tracing::debug!(upload_id, "an upload was settled after its send stopped");
        }
    }

    /// Stops the send of the message with this nonce, if it is still
    /// uploading.
    pub fn cancel(&self, key: &str, nonce: &str) {
        if let Some(token) = self.lock_sends().get(&(key.to_owned(), nonce.to_owned())) {
            token.cancel();
        }
    }

    pub fn lose_answers(&self, count: u32) {
        *self
            .lose_answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = count;
    }

    fn answer_lost(&self) -> bool {
        let mut left = self
            .lose_answers
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let lost = *left > 0;
        *left = left.saturating_sub(1);
        lost
    }

    fn begin(&self, key: &str, nonce: &str) -> Result<SendGuard<'_>, CoreError> {
        let id = (key.to_owned(), nonce.to_owned());
        let token = CancellationToken::new();
        let mut sends = self.lock_sends();
        if sends.contains_key(&id) {
            return Err(invalid("this message is already being sent"));
        }
        sends.insert(id.clone(), token.clone());
        Ok(SendGuard {
            uploads: self,
            id,
            token,
        })
    }

    fn wait_for(&self, key: &str, upload_id: i64) -> WaitGuard<'_> {
        let id = (key.to_owned(), upload_id);
        let (sender, receiver) = oneshot::channel();
        self.lock_waiters().insert(id.clone(), sender);
        WaitGuard {
            uploads: self,
            id,
            receiver,
        }
    }

    fn lock_sends(&self) -> MutexGuard<'_, HashMap<(String, String), CancellationToken>> {
        self.sends.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_waiters(&self) -> MutexGuard<'_, HashMap<(String, i64), oneshot::Sender<Outcome>>> {
        self.waiters.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A send that can be cancelled until this is dropped.
struct SendGuard<'a> {
    uploads: &'a Uploads,
    id: (String, String),
    token: CancellationToken,
}

impl Drop for SendGuard<'_> {
    fn drop(&mut self) {
        self.uploads.lock_sends().remove(&self.id);
    }
}

/// A verdict to come; nobody waits for it once this is dropped.
struct WaitGuard<'a> {
    uploads: &'a Uploads,
    id: (String, i64),
    receiver: oneshot::Receiver<Outcome>,
}

impl Drop for WaitGuard<'_> {
    fn drop(&mut self) {
        self.uploads.lock_waiters().remove(&self.id);
    }
}

/// Why the uploads stopped.
enum Stop {
    Cancelled,
    Failed { index: usize, error: CoreError },
}

/// Uploads `files` to the server at `key`, then sends the message with
/// them. On a failure or a cancel, every upload made so far is cancelled.
pub(crate) async fn send(
    client: &Client,
    key: &str,
    channel_id: i64,
    content: String,
    nonce: String,
    files: Vec<OutgoingFile>,
) -> Result<Message, CoreError> {
    if files.is_empty() || files.len() > MAX_ATTACHMENTS {
        return Err(invalid(format!(
            "a message holds 1 to {MAX_ATTACHMENTS} files"
        )));
    }
    if let Some(file) = files
        .iter()
        .find(|file| file.path.is_some() == file.bytes.is_some())
    {
        return Err(invalid(format!(
            "{} needs a path or bytes, not both",
            file.filename
        )));
    }
    let uploads = client.uploads();
    let guard = uploads.begin(key, &nonce)?;
    let progress = Progress {
        client,
        key,
        channel_id,
        nonce: &nonce,
        files: Mutex::new(vec![FileProgress::default(); files.len()]),
    };
    let created = Mutex::new(Vec::new());
    let uploaded = tokio::select! {
        () = guard.token.cancelled() => Err(Stop::Cancelled),
        result = upload_all(client, key, channel_id, files, &progress, &created) => result,
    };
    drop(guard);
    let created = created.into_inner().unwrap_or_else(PoisonError::into_inner);
    let attachments = match uploaded {
        Ok(attachments) => attachments,
        Err(stop) => {
            cancel_uploads(client, key, &created).await;
            return Err(progress.stopped(stop));
        }
    };
    progress.all(UploadPhase::Sending);
    let ids = attachments.iter().map(|attachment| attachment.id).collect();
    match client
        .send_message_with_ids(key, channel_id, content, nonce.clone(), ids)
        .await
    {
        Ok(message) => {
            progress.all(UploadPhase::Done);
            Ok(message)
        }
        Err(error) => {
            cancel_uploads(client, key, &created).await;
            progress.all(UploadPhase::Failed {
                message: error.to_string(),
            });
            Err(error)
        }
    }
}

/// Every file, two at a time; their attachments in the files' order.
async fn upload_all(
    client: &Client,
    key: &str,
    channel_id: i64,
    files: Vec<OutgoingFile>,
    progress: &Progress<'_>,
    created: &Mutex<Vec<i64>>,
) -> Result<Vec<proto::Attachment>, Stop> {
    let mut done: Vec<(usize, proto::Attachment)> =
        futures_util::stream::iter(files.into_iter().enumerate())
            .map(|(index, file)| async move {
                upload_one(client, key, channel_id, index, file, progress, created)
                    .await
                    .map(|attachment| (index, attachment))
                    .map_err(|error| Stop::Failed { index, error })
            })
            .buffer_unordered(AT_ONCE)
            .try_collect()
            .await?;
    done.sort_by_key(|(index, _)| *index);
    Ok(done.into_iter().map(|(_, attachment)| attachment).collect())
}

async fn upload_one(
    client: &Client,
    key: &str,
    channel_id: i64,
    index: usize,
    file: OutgoingFile,
    progress: &Progress<'_>,
    created: &Mutex<Vec<i64>>,
) -> Result<proto::Attachment, CoreError> {
    progress.phase(index, UploadPhase::Preparing);
    let mut source = Source::prepare(file).await?;
    progress.sized(index, source.size);
    let request = Request::CreateUpload(proto::CreateUpload {
        channel_id,
        filename: source.filename.clone(),
        size_bytes: source.size,
        kind: proto::AttachmentKind::File as i32,
        width: None,
        height: None,
        duration_ms: None,
    });
    let upload = match client.request(key, request).await? {
        Response::Upload(upload) => upload,
        other => return Err(unexpected(&other)),
    };
    created
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(upload.upload_id);
    progress.phase(index, UploadPhase::Uploading);
    send_pieces(client, key, &mut source, &upload, |sent| {
        progress.sent(index, sent);
    })
    .await?;
    progress.phase(index, UploadPhase::Processing);
    finish(client, key, upload.upload_id).await
}

/// A file to upload, read from memory or from disk.
struct Source {
    filename: String,
    size: u64,
    body: Body,
}

enum Body {
    Bytes(Bytes),
    File(tokio::fs::File),
}

impl Source {
    async fn prepare(file: OutgoingFile) -> Result<Self, CoreError> {
        let OutgoingFile {
            path,
            bytes,
            filename,
        } = file;
        let named = |fallback: Option<&Path>| {
            let given = filename.trim();
            if !given.is_empty() {
                return given.to_owned();
            }
            fallback.and_then(Path::file_name).map_or_else(
                || "file".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            )
        };
        let (filename, body) = match (path, bytes) {
            (_, Some(bytes)) => (named(None), Body::Bytes(bytes.into())),
            (Some(path), None) => {
                let path = Path::new(&path);
                let unreadable = |error: std::io::Error| {
                    invalid(format!("{} can't be read: {error}", path.display()))
                };
                let file = tokio::fs::File::open(path).await.map_err(unreadable)?;
                (named(Some(path)), Body::File(file))
            }
            (None, None) => return Err(invalid("a file needs a path or bytes")),
        };
        let size = match &body {
            Body::Bytes(bytes) => bytes.len() as u64,
            Body::File(file) => file
                .metadata()
                .await
                .map_err(|error| invalid(format!("{filename} can't be read: {error}")))?
                .len(),
        };
        Ok(Self {
            filename,
            size,
            body,
        })
    }

    /// `len` bytes from `offset`.
    async fn piece(&mut self, offset: u64, len: u64) -> Result<Bytes, CoreError> {
        let unreadable =
            |error: std::io::Error| invalid(format!("the file can't be read: {error}"));
        match &mut self.body {
            Body::Bytes(bytes) => Ok(bytes.slice(offset as usize..(offset + len) as usize)),
            Body::File(file) => {
                file.seek(SeekFrom::Start(offset))
                    .await
                    .map_err(unreadable)?;
                let mut piece = vec![0; len as usize];
                file.read_exact(&mut piece).await.map_err(unreadable)?;
                Ok(piece.into())
            }
        }
    }
}

/// Sends the file from the start. A piece whose answer is lost is sent
/// again, and the server's 409 says where it really is.
async fn send_pieces(
    client: &Client,
    key: &str,
    source: &mut Source,
    upload: &proto::Upload,
    mut sent: impl FnMut(u64),
) -> Result<(), CoreError> {
    let piece_bytes = match upload.piece_bytes {
        0 => u64::from(PIECE_BYTES),
        bytes => u64::from(bytes),
    };
    let path = format!("/media/uploads/{}", upload.upload_id);
    let mut offset = 0;
    while offset < source.size {
        let len = (source.size - offset).min(piece_bytes);
        let piece = source.piece(offset, len).await?;
        let status = retrying(|| put_piece(client, key, &path, offset, piece.clone())).await?;
        if status.state != proto::UploadState::Uploading as i32
            || status.received_bytes > source.size
        {
            return Err(invalid("the server stopped taking this file"));
        }
        offset = status.received_bytes;
        sent(offset);
    }
    Ok(())
}

/// The upload's status after the piece: 200 when taken, 409 when the server
/// was elsewhere.
async fn put_piece(
    client: &Client,
    key: &str,
    path: &str,
    offset: u64,
    piece: Bytes,
) -> Result<proto::UploadStatus, CoreError> {
    let call = Call {
        method: reqwest::Method::PUT,
        path: path.to_owned(),
        headers: vec![("Upload-Offset", offset.to_string())],
        body: piece,
        timeout: Some(PIECE_TIMEOUT),
    };
    let response = client.transfers().send(client, key, call).await?;
    let (status, upload) = upload_status(response).await?;
    if status != StatusCode::OK && status != StatusCode::CONFLICT {
        return Err(invalid(format!("the server answered {status}")));
    }
    if client.uploads().answer_lost() {
        return Err(CoreError::Connection {
            message: "the answer was lost (test hook)".to_owned(),
        });
    }
    Ok(upload)
}

/// Finishes the upload and waits for the server's verdict: its event, or
/// what asking every 5 seconds says.
async fn finish(
    client: &Client,
    key: &str,
    upload_id: i64,
) -> Result<proto::Attachment, CoreError> {
    let uploads = client.uploads();
    // Before finishing, so a verdict that comes at once isn't missed.
    let mut waiter = uploads.wait_for(key, upload_id);
    let finish_path = format!("/media/uploads/{upload_id}/finish");
    let status = retrying(|| {
        let call = Call {
            method: reqwest::Method::POST,
            path: finish_path.clone(),
            headers: Vec::new(),
            body: Bytes::new(),
            timeout: Some(STATUS_TIMEOUT),
        };
        status_of(client, key, call)
    })
    .await?;
    if let Some(attachment) = settled(status)? {
        return Ok(attachment);
    }
    let status_path = format!("/media/uploads/{upload_id}");
    loop {
        tokio::select! {
            outcome = &mut waiter.receiver => {
                return match outcome {
                    Ok(Ok(attachment)) => Ok(attachment),
                    Ok(Err(error)) => Err(convert::error(error)),
                    Err(_) => Err(invalid("the upload's verdict was lost")),
                };
            }
            () = tokio::time::sleep(POLL_EVERY) => {
                let status = retrying(|| {
                    status_of(client, key, Call::get(status_path.clone(), Some(STATUS_TIMEOUT)))
                })
                .await?;
                if let Some(attachment) = settled(status)? {
                    return Ok(attachment);
                }
            }
        }
    }
}

async fn status_of(
    client: &Client,
    key: &str,
    call: Call,
) -> Result<proto::UploadStatus, CoreError> {
    let response = client.transfers().send(client, key, call).await?;
    upload_status(response).await.map(|(_, status)| status)
}

/// The attachment once the server is done with the file, `None` while it
/// still processes it.
fn settled(status: proto::UploadStatus) -> Result<Option<proto::Attachment>, CoreError> {
    match proto::UploadState::try_from(status.state) {
        Ok(proto::UploadState::Ready | proto::UploadState::Sent) => status
            .attachment
            .map(Some)
            .ok_or_else(|| invalid("the server's answer has no attachment")),
        Ok(proto::UploadState::Processing) => Ok(None),
        Ok(proto::UploadState::Failed) => Err(status.error.map_or_else(
            || invalid("the server couldn't take this file"),
            convert::error,
        )),
        _ => Err(invalid("the server's answer has no known state")),
    }
}

/// Runs `attempt` again after a network failure, up to six times in all.
async fn retrying<T, F, Fut>(mut attempt: F) -> Result<T, CoreError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, CoreError>>,
{
    let mut tries = 1;
    loop {
        match attempt().await {
            Err(CoreError::Connection { message }) if tries < TRIES => {
                tracing::debug!(%message, tries, "a transfer failed; trying again");
                tokio::time::sleep(Duration::from_secs(1 << (tries - 1))).await;
                tries += 1;
            }
            result => return result,
        }
    }
}

/// Uploads that won't be sent; the server would drop them within a day.
async fn cancel_uploads(client: &Client, key: &str, upload_ids: &[i64]) {
    for &upload_id in upload_ids {
        let request = Request::CancelUpload(proto::CancelUpload { upload_id });
        if let Err(error) = client.request(key, request).await {
            tracing::warn!(upload_id, %error, "an upload couldn't be cancelled");
        }
    }
}

#[derive(Debug, Clone, Default)]
struct FileProgress {
    sent: u64,
    total: u64,
    last: Option<Instant>,
}

/// `UploadProgress` events for one message's files.
struct Progress<'a> {
    client: &'a Client,
    key: &'a str,
    channel_id: i64,
    nonce: &'a str,
    files: Mutex<Vec<FileProgress>>,
}

impl Progress<'_> {
    fn sized(&self, index: usize, total: u64) {
        if let Some(file) = self.lock().get_mut(index) {
            file.total = total;
        }
    }

    /// Bytes sent so far, at most 10 times a second.
    fn sent(&self, index: usize, sent: u64) {
        let due = {
            let mut files = self.lock();
            let Some(file) = files.get_mut(index) else {
                return;
            };
            file.sent = sent;
            let due = sent == file.total
                || file
                    .last
                    .is_none_or(|last| last.elapsed() >= PROGRESS_EVERY);
            if due {
                file.last = Some(Instant::now());
            }
            due
        };
        if due {
            self.phase(index, UploadPhase::Uploading);
        }
    }

    fn phase(&self, index: usize, phase: UploadPhase) {
        let (sent_bytes, total_bytes) = self
            .lock()
            .get(index)
            .map_or((0, 0), |file| (file.sent, file.total));
        self.client.emit(
            self.key,
            CoreEventPayload::UploadProgress {
                channel_id: self.channel_id,
                nonce: self.nonce.to_owned(),
                file_index: index as u32,
                sent_bytes,
                total_bytes,
                phase,
            },
        );
    }

    fn all(&self, phase: UploadPhase) {
        // Counted first: a guard in the loop's head would be held through it.
        let count = self.lock().len();
        for index in 0..count {
            self.phase(index, phase.clone());
        }
    }

    /// The file that failed says why; the others were cancelled.
    fn stopped(&self, stop: Stop) -> CoreError {
        match stop {
            Stop::Cancelled => {
                self.all(UploadPhase::Cancelled);
                CoreError::Cancelled
            }
            Stop::Failed { index, error } => {
                let count = self.lock().len();
                for other in (0..count).filter(|other| *other != index) {
                    self.phase(other, UploadPhase::Cancelled);
                }
                self.phase(
                    index,
                    UploadPhase::Failed {
                        message: error.to_string(),
                    },
                );
                error
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<FileProgress>> {
        self.files.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidInput {
        message: message.into(),
    }
}
```

**`src/attachments/download.rs`** (new; Task 19 adds attachment bytes and the cache):

```rust
//! Attachments coming down (media uploads plan, Task 18): any file streamed
//! to where the user saves it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::transfer::{Call, network, refusal};
use crate::api::types::{AttachmentVariant, CoreError, CoreEventPayload};
use crate::client::Client;

/// Progress goes out at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[derive(Default)]
pub(crate) struct Downloads {
    /// Saves that can still be cancelled, by server key and attachment id.
    /// An entry leaves when its save returns.
    running: Mutex<HashMap<(String, i64), CancellationToken>>,
}

impl Downloads {
    pub fn cancel(&self, key: &str, attachment_id: i64) {
        if let Some(token) = self.lock().get(&(key.to_owned(), attachment_id)) {
            token.cancel();
        }
    }

    fn begin(&self, key: &str, attachment_id: i64) -> Result<DownloadGuard<'_>, CoreError> {
        let id = (key.to_owned(), attachment_id);
        let token = CancellationToken::new();
        let mut running = self.lock();
        if running.contains_key(&id) {
            return Err(CoreError::InvalidInput {
                message: "this file is already being saved".to_owned(),
            });
        }
        running.insert(id.clone(), token.clone());
        Ok(DownloadGuard {
            downloads: self,
            id,
            token,
        })
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<(String, i64), CancellationToken>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A save that can be cancelled until this is dropped.
struct DownloadGuard<'a> {
    downloads: &'a Downloads,
    id: (String, i64),
    token: CancellationToken,
}

impl Drop for DownloadGuard<'_> {
    fn drop(&mut self) {
        self.downloads.lock().remove(&self.id);
    }
}

fn path_of(attachment_id: i64, variant: AttachmentVariant) -> String {
    match variant {
        AttachmentVariant::Original => format!("/media/attachments/{attachment_id}"),
        AttachmentVariant::Thumbnail => format!("/media/attachments/{attachment_id}/thumbnail"),
    }
}

/// Streams the attachment to `path`, through `path.part` until it is all
/// there. `DownloadProgress` events follow it, the last with `done` or
/// `error`.
pub(crate) async fn save(
    client: &Client,
    key: &str,
    attachment_id: i64,
    path: String,
) -> Result<(), CoreError> {
    let downloads = client.downloads();
    let guard = downloads.begin(key, attachment_id)?;
    let progress = Progress {
        client,
        key,
        attachment_id,
        last: Mutex::new(None),
    };
    let part = PathBuf::from(format!("{path}.part"));
    let streamed = tokio::select! {
        () = guard.token.cancelled() => Err(CoreError::Cancelled),
        result = stream_to(client, key, attachment_id, &part, &progress) => result,
    };
    drop(guard);
    let saved = match streamed {
        Ok(total) => tokio::fs::rename(&part, &path)
            .await
            .map(|()| total)
            .map_err(|error| CoreError::Storage {
                message: error.to_string(),
            }),
        Err(error) => Err(error),
    };
    match saved {
        Ok(total) => {
            progress.end(total, total, None);
            Ok(())
        }
        Err(error) => {
            if let Err(remove) = tokio::fs::remove_file(&part).await
                && remove.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(error = %remove, "a partial download couldn't be removed");
            }
            progress.end(0, 0, Some(error.to_string()));
            Err(error)
        }
    }
}

/// The file's size once it is all in `part`.
async fn stream_to(
    client: &Client,
    key: &str,
    attachment_id: i64,
    part: &Path,
    progress: &Progress<'_>,
) -> Result<u64, CoreError> {
    let call = Call::get(path_of(attachment_id, AttachmentVariant::Original), None);
    let mut response = client.transfers().send(client, key, call).await?;
    if !response.status().is_success() {
        return Err(refusal(response).await);
    }
    let total = response.content_length().unwrap_or(0);
    let unwritable = |error: std::io::Error| CoreError::Storage {
        message: error.to_string(),
    };
    let mut file = tokio::fs::File::create(part).await.map_err(unwritable)?;
    let mut received = 0;
    while let Some(chunk) = response.chunk().await.map_err(|error| network(&error))? {
        file.write_all(&chunk).await.map_err(unwritable)?;
        received += chunk.len() as u64;
        progress.received(received, total);
    }
    file.sync_all().await.map_err(unwritable)?;
    Ok(received)
}

/// `DownloadProgress` events for one save.
struct Progress<'a> {
    client: &'a Client,
    key: &'a str,
    attachment_id: i64,
    last: Mutex<Option<Instant>>,
}

impl Progress<'_> {
    /// At most 10 times a second.
    fn received(&self, received_bytes: u64, total_bytes: u64) {
        {
            let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
            if last.is_some_and(|last| last.elapsed() < PROGRESS_EVERY) {
                return;
            }
            *last = Some(Instant::now());
        }
        self.emit(received_bytes, total_bytes, false, None);
    }

    fn end(&self, received_bytes: u64, total_bytes: u64, error: Option<String>) {
        let done = error.is_none();
        self.emit(received_bytes, total_bytes, done, error);
    }

    fn emit(&self, received_bytes: u64, total_bytes: u64, done: bool, error: Option<String>) {
        self.client.emit(
            self.key,
            CoreEventPayload::DownloadProgress {
                attachment_id: self.attachment_id,
                received_bytes,
                total_bytes,
                done,
                error,
            },
        );
    }
}
```

**`src/client.rs`:**

```diff
--- a/crates/opencord-core/src/client.rs
+++ b/crates/opencord-core/src/client.rs
@@ -18,14 +18,17 @@
 use crate::api::types::{
     AddServerOutcome, AudioSettings, Ban, CameraStarted, Channel, ChannelChanges, ChannelKind,
     ChannelPosition, CoreError, CoreEvent, CoreEventPayload, ErrorCode, HotkeyBinding,
-    HotkeySupport, IdentityInfo, Invite, MediaEvent, Member, Message, OverwriteTargetKind,
-    PermissionOverwrite, PresenceStatus, Role, RoleChanges, S3Config, ScreenProblem,
-    ScreenShareRequest, ScreenShareStarted, ScreenStream, Server, ServerChanges, ServerInfo,
-    SoundDraft, SoundSelection, SoundboardSound, SoundboardSoundChanges, StorageInfo,
+    HotkeySupport, IdentityInfo, Invite, MediaEvent, Member, Message, OutgoingFile,
+    OverwriteTargetKind, PermissionOverwrite, PresenceStatus, Role, RoleChanges, S3Config,
+    ScreenProblem, ScreenShareRequest, ScreenShareStarted, ScreenStream, Server, ServerChanges,
+    ServerInfo, SoundDraft, SoundSelection, SoundboardSound, SoundboardSoundChanges, StorageInfo,
     StorageTestResult, StorageTestTarget, StreamSourceKind, UploadSettings, User, VideoWant,
     VoiceCue, VoiceSettings, VoiceSettingsChanges, VoiceState,
 };
 use crate::artwork::ArtworkCache;
+use crate::attachments::download::{self, Downloads};
+use crate::attachments::transfer::Transfers;
+use crate::attachments::upload::{self, Uploads};
 use crate::connection::{
     self, Clocks, Command, Connection, Context, Credentials, Established, HandshakeError,
     MediaTokens, Route, Routes,
@@ -86,6 +89,9 @@
     clocks: Clocks,
     /// How this person hears music, kept for media that starts later.
     music_listening: Mutex<MusicListening>,
+    transfers: Transfers,
+    uploads: Arc<Uploads>,
+    downloads: Downloads,
 }
 
 /// The app's id, as its desktop file names it; the system's portals ask for
@@ -167,12 +173,15 @@
                 voice: Mutex::new(Voice::default()),
                 media: OnceLock::new(),
                 hotkeys: tokio::sync::Mutex::new(None),
-                media_tokens,
-                routes,
+                media_tokens: Arc::clone(&media_tokens),
+                routes: Arc::clone(&routes),
                 soundboard,
                 artwork,
                 clocks: Clocks::default(),
                 music_listening: Mutex::default(),
+                transfers: Transfers::new(Arc::clone(&routes), Arc::clone(&media_tokens)),
+                uploads: Arc::default(),
+                downloads: Downloads::default(),
             }),
         };
         client.spawn_wake_watch();
@@ -388,15 +397,46 @@
         content: String,
         nonce: String,
     ) -> Result<Message, CoreError> {
+        self.send_message_with_ids(key, channel_id, content, nonce, Vec::new())
+            .await
+    }
+
+    pub(crate) async fn send_message_with_ids(
+        &self,
+        key: &str,
+        channel_id: i64,
+        content: String,
+        nonce: String,
+        attachment_ids: Vec<i64>,
+    ) -> Result<Message, CoreError> {
         let request = Request::SendMessage(proto::SendMessage {
             channel_id,
             content,
             nonce,
-            attachment_ids: Vec::new(),
+            attachment_ids,
         });
         expect_message(self.request(key, request).await?)
     }
 
+    /// Uploads the files, then sends the message with them.
+    /// `UploadProgress` events follow each file.
+    pub async fn send_message_with_files(
+        &self,
+        key: &str,
+        channel_id: i64,
+        content: String,
+        nonce: String,
+        files: Vec<OutgoingFile>,
+    ) -> Result<Message, CoreError> {
+        upload::send(self, key, channel_id, content, nonce, files).await
+    }
+
+    /// Stops a `send_message_with_files` that is still uploading; it
+    /// returns `Cancelled`.
+    pub fn cancel_send(&self, key: &str, nonce: &str) {
+        self.inner.uploads.cancel(key, nonce);
+    }
+
     pub async fn edit_message(
         &self,
         key: &str,
@@ -1617,6 +1657,7 @@
             media_tokens: Arc::clone(&self.inner.media_tokens),
             routes: Arc::clone(&self.inner.routes),
             clocks: Arc::clone(&self.inner.clocks),
+            uploads: Arc::clone(&self.inner.uploads),
         }
     }
 
@@ -1856,6 +1897,62 @@
         expect_storage(self.request(key, request).await?)
     }
 
+    pub(crate) fn transfers(&self) -> &Transfers {
+        &self.inner.transfers
+    }
+
+    pub(crate) fn uploads(&self) -> Arc<Uploads> {
+        Arc::clone(&self.inner.uploads)
+    }
+
+    pub(crate) fn downloads(&self) -> &Downloads {
+        &self.inner.downloads
+    }
+
+    /// An event of this client's own, for the app.
+    pub(crate) fn emit(&self, key: &str, payload: CoreEventPayload) {
+        let event = CoreEvent {
+            server_key: key.to_owned(),
+            payload,
+        };
+        if self.inner.outward.send(event).is_err() {
+            tracing::debug!("an event was dropped: the app stopped listening");
+        }
+    }
+
+    /// Saves the attachment at `path`; `DownloadProgress` events follow it.
+    pub async fn download_attachment(
+        &self,
+        key: &str,
+        attachment_id: i64,
+        path: String,
+    ) -> Result<(), CoreError> {
+        download::save(self, key, attachment_id, path).await
+    }
+
+    /// Stops a `download_attachment`; it returns `Cancelled`.
+    pub fn cancel_download(&self, key: &str, attachment_id: i64) {
+        self.inner.downloads.cancel(key, attachment_id);
+    }
+
+    /// Test hook: the next `count` answers to upload pieces are dropped, as
+    /// if the network lost them after the server took the piece.
+    #[doc(hidden)]
+    pub fn lose_upload_answers(&self, count: u32) {
+        self.inner.uploads.lose_answers(count);
+    }
+
+    /// Test hook: the server at `key` will refuse this device's media token
+    /// until it is renewed.
+    #[doc(hidden)]
+    pub fn spoil_media_token(&self, key: &str) {
+        self.inner
+            .media_tokens
+            .lock()
+            .unwrap_or_else(PoisonError::into_inner)
+            .insert(key.to_owned(), "spoiled".to_owned());
+    }
+
     /// A request answered with an ack.
     pub(crate) async fn ack(&self, key: &str, kind: Request) -> Result<(), CoreError> {
         expect_ack(self.request(key, kind).await?)
```

**`src/api/attachments.rs`:**

```diff
--- a/crates/opencord-core/src/api/attachments.rs
+++ b/crates/opencord-core/src/api/attachments.rs
@@ -1,9 +1,12 @@
 //! Files in messages, upload settings and storage (media uploads plan,
 //! milestone A4), for the Flutter app.
 
-use super::client::on_runtime;
+use flutter_rust_bridge::frb;
+
+use super::client::{client, on_runtime};
 use super::types::{
-    CoreError, S3Config, StorageInfo, StorageTestResult, StorageTestTarget, UploadSettings,
+    CoreError, Message, OutgoingFile, S3Config, StorageInfo, StorageTestResult, StorageTestTarget,
+    UploadSettings,
 };
 
 /// MANAGE_SERVER. `None` leaves a setting as it is.
@@ -78,3 +81,48 @@
 pub async fn storage_move_stop(server_key: String) -> Result<StorageInfo, CoreError> {
     on_runtime(move |client| async move { client.storage_move_stop(&server_key).await }).await
 }
+
+/// Uploads the files, two at a time, then sends the message with them.
+/// UploadProgress events follow each file. On a failure or a cancel, the
+/// uploads made so far are cancelled.
+pub async fn send_message_with_files(
+    server_key: String,
+    channel_id: i64,
+    content: String,
+    nonce: String,
+    files: Vec<OutgoingFile>,
+) -> Result<Message, CoreError> {
+    on_runtime(move |client| async move {
+        client
+            .send_message_with_files(&server_key, channel_id, content, nonce, files)
+            .await
+    })
+    .await
+}
+
+/// Stops a send that is still uploading; it returns `Cancelled`.
+#[frb(sync)]
+pub fn cancel_send(server_key: String, nonce: String) -> Result<(), CoreError> {
+    client()?.cancel_send(&server_key, &nonce);
+    Ok(())
+}
+
+/// Saves the attachment at `path`; DownloadProgress events follow it.
+pub async fn download_attachment(
+    server_key: String,
+    attachment_id: i64,
+    path: String,
+) -> Result<(), CoreError> {
+    on_runtime(move |client| async move {
+        client
+            .download_attachment(&server_key, attachment_id, path)
+            .await
+    })
+    .await
+}
+
+#[frb(sync)]
+pub fn cancel_download(server_key: String, attachment_id: i64) -> Result<(), CoreError> {
+    client()?.cancel_download(&server_key, attachment_id);
+    Ok(())
+}
```

**`crates/opencord-server/src/server.rs`:**

```diff
--- a/crates/opencord-server/src/server.rs
+++ b/crates/opencord-server/src/server.rs
@@ -298,6 +298,15 @@
     pub async fn drain_blob_deletions(&self, now_ms: i64) {
         crate::attachments::gc::drain_blobs(&self.state, now_ms).await;
     }
+
+    /// Intended for tests: how many uploads are in `state` ("uploading",
+    /// "processing", "ready", "failed" or "sent").
+    pub async fn uploads_in_state(&self, state: &str) -> Result<usize, sqlx::Error> {
+        let mut conn = self.state.db.acquire().await?;
+        Ok(crate::db::attachments::in_state(&mut conn, state)
+            .await?
+            .len())
+    }
 }
 
 /// The voice node inside this server, unless voice is off or on external
```

**`tests/attachments.rs`:**

```diff
--- a/crates/opencord-core/tests/attachments.rs
+++ b/crates/opencord-core/tests/attachments.rs
@@ -5,12 +5,14 @@
 
 use opencord_common::address::format_fingerprint;
 use opencord_core::api::types::{
-    CoreEvent, CoreEventPayload, ReadySnapshot, RoleChanges, StorageKind,
+    CoreError, CoreEvent, CoreEventPayload, ErrorCode, OutgoingFile, ReadySnapshot, RoleChanges,
+    StorageKind, UploadPhase,
 };
 use opencord_core::client::Client;
 use opencord_core::identity::Identity;
 use opencord_server::config::Config;
 use opencord_server::server::{self, ServerHandle};
+use sha2::{Digest, Sha256};
 use tokio::sync::mpsc::UnboundedReceiver;
 
 const WAIT: Duration = Duration::from_secs(20);
@@ -93,6 +95,51 @@
     }
 }
 
+fn general(ready: &ReadySnapshot) -> i64 {
+    ready
+        .channels
+        .iter()
+        .find(|channel| channel.name == "general")
+        .unwrap()
+        .id
+}
+
+/// Bytes that don't repeat within a piece.
+fn data(len: usize) -> Vec<u8> {
+    let mut state = 0x2545_F491_u32;
+    (0..len)
+        .map(|_| {
+            state ^= state << 13;
+            state ^= state >> 17;
+            state ^= state << 5;
+            state as u8
+        })
+        .collect()
+}
+
+fn bytes_file(name: &str, bytes: Vec<u8>) -> OutgoingFile {
+    OutgoingFile {
+        path: None,
+        bytes: Some(bytes),
+        filename: name.to_owned(),
+    }
+}
+
+fn sha256(bytes: &[u8]) -> Vec<u8> {
+    Sha256::digest(bytes).to_vec()
+}
+
+async fn download(owner: &TestClient, key: &str, attachment_id: i64) -> Vec<u8> {
+    let dir = tempfile::tempdir().unwrap();
+    let path = dir.path().join("saved");
+    owner
+        .client
+        .download_attachment(key, attachment_id, path.to_string_lossy().into_owned())
+        .await
+        .unwrap();
+    std::fs::read(path).unwrap()
+}
+
 #[tokio::test(flavor = "multi_thread")]
 async fn ready_carries_upload_settings_and_role_limits() {
     let server = TestServer::start().await;
@@ -177,3 +224,232 @@
         .unwrap();
     assert_eq!(role.max_upload_bytes, None);
 }
+
+#[tokio::test(flavor = "multi_thread")]
+async fn send_message_with_files_uploads_and_sends() {
+    let server = TestServer::start().await;
+    let (mut owner, ready) = TestClient::owner(&server).await;
+    let key = server.address();
+    let dir = tempfile::tempdir().unwrap();
+    let big = data(9 * MIB);
+    let path = dir.path().join("big.bin");
+    std::fs::write(&path, &big).unwrap();
+    let small = data(1024);
+    let files = vec![
+        OutgoingFile {
+            path: Some(path.to_string_lossy().into_owned()),
+            bytes: None,
+            filename: String::new(),
+        },
+        bytes_file("small.bin", small.clone()),
+    ];
+
+    let message = owner
+        .client
+        .send_message_with_files(
+            &key,
+            general(&ready),
+            "two files".to_owned(),
+            "n1".to_owned(),
+            files,
+        )
+        .await
+        .unwrap();
+
+    let names: Vec<_> = message
+        .attachments
+        .iter()
+        .map(|a| a.filename.as_str())
+        .collect();
+    assert_eq!(names, ["big.bin", "small.bin"]);
+    assert_eq!(message.attachments[0].size_bytes, big.len() as u64);
+    let mut done = [false, false];
+    while done != [true, true] {
+        let (index, phase) = owner
+            .wait_for(|payload| match payload {
+                CoreEventPayload::UploadProgress {
+                    file_index, phase, ..
+                } => Some((*file_index as usize, phase.clone())),
+                _ => None,
+            })
+            .await;
+        if phase == UploadPhase::Done {
+            done[index] = true;
+        }
+    }
+    let saved = download(&owner, &key, message.attachments[0].id).await;
+    assert_eq!(sha256(&saved), sha256(&big));
+    assert_eq!(
+        download(&owner, &key, message.attachments[1].id).await,
+        small
+    );
+}
+
+/// Review Focus 1: the server took a piece but its answer was lost. The
+/// piece goes again, the server answers 409 with its count, and the upload
+/// carries on.
+#[tokio::test(flavor = "multi_thread")]
+async fn uploads_resume_after_a_lost_answer() {
+    let server = TestServer::start().await;
+    let (owner, ready) = TestClient::owner(&server).await;
+    let key = server.address();
+    let file = data(20 * MIB);
+    owner.client.lose_upload_answers(1);
+
+    let message = owner
+        .client
+        .send_message_with_files(
+            &key,
+            general(&ready),
+            String::new(),
+            "n1".to_owned(),
+            vec![bytes_file("file.bin", file.clone())],
+        )
+        .await
+        .unwrap();
+
+    let saved = download(&owner, &key, message.attachments[0].id).await;
+    assert_eq!(sha256(&saved), sha256(&file));
+}
+
+#[tokio::test(flavor = "multi_thread")]
+async fn cancel_send_cancels_server_uploads() {
+    let server = TestServer::start().await;
+    let (mut owner, ready) = TestClient::owner(&server).await;
+    let key = server.address();
+    server.handle.hold_processing(true);
+    let client = owner.client.clone();
+    let (send_key, channel_id) = (key.clone(), general(&ready));
+    let send = tokio::spawn(async move {
+        client
+            .send_message_with_files(
+                &send_key,
+                channel_id,
+                String::new(),
+                "n1".to_owned(),
+                vec![bytes_file("file.bin", data(1024))],
+            )
+            .await
+    });
+    owner
+        .wait_for(|payload| match payload {
+            CoreEventPayload::UploadProgress {
+                phase: UploadPhase::Processing,
+                ..
+            } => Some(()),
+            _ => None,
+        })
+        .await;
+
+    owner.client.cancel_send(&key, "n1");
+
+    assert!(matches!(send.await.unwrap(), Err(CoreError::Cancelled)));
+    owner
+        .wait_for(|payload| match payload {
+            CoreEventPayload::UploadProgress {
+                phase: UploadPhase::Cancelled,
+                ..
+            } => Some(()),
+            _ => None,
+        })
+        .await;
+    for state in ["uploading", "processing", "ready"] {
+        assert_eq!(server.handle.uploads_in_state(state).await.unwrap(), 0);
+    }
+}
+
+#[tokio::test(flavor = "multi_thread")]
+async fn a_blocked_file_fails_with_the_servers_reason() {
+    let server = TestServer::start().await;
+    let (owner, ready) = TestClient::owner(&server).await;
+    let key = server.address();
+    // The list starts empty; the owner picks what to block.
+    owner
+        .client
+        .update_upload_settings(&key, Some(vec!["exe".to_owned()]), None)
+        .await
+        .unwrap();
+
+    let sent = owner
+        .client
+        .send_message_with_files(
+            &key,
+            general(&ready),
+            String::new(),
+            "n1".to_owned(),
+            vec![bytes_file("setup.exe", data(1024))],
+        )
+        .await;
+
+    assert!(matches!(
+        sent,
+        Err(CoreError::Server {
+            code: ErrorCode::FileTypeBlocked,
+            ..
+        })
+    ));
+}
+
+/// An expired media token is renewed once, then the transfer goes
+/// ahead.
+#[tokio::test(flavor = "multi_thread")]
+async fn an_expired_media_token_is_renewed() {
+    let server = TestServer::start().await;
+    let (owner, ready) = TestClient::owner(&server).await;
+    let key = server.address();
+    owner.client.spoil_media_token(&key);
+
+    let message = owner
+        .client
+        .send_message_with_files(
+            &key,
+            general(&ready),
+            String::new(),
+            "n1".to_owned(),
+            vec![bytes_file("file.bin", data(1024))],
+        )
+        .await
+        .unwrap();
+
+    owner.client.spoil_media_token(&key);
+    assert_eq!(
+        download(&owner, &key, message.attachments[0].id).await,
+        data(1024)
+    );
+}
+
+#[tokio::test(flavor = "multi_thread")]
+async fn download_attachment_writes_the_exact_file() {
+    let server = TestServer::start().await;
+    let (mut owner, ready) = TestClient::owner(&server).await;
+    let key = server.address();
+    let file = data(3 * MIB + 7);
+    let message = owner
+        .client
+        .send_message_with_files(
+            &key,
+            general(&ready),
+            String::new(),
+            "n1".to_owned(),
+            vec![bytes_file("file.bin", file.clone())],
+        )
+        .await
+        .unwrap();
+    let attachment_id = message.attachments[0].id;
+
+    let saved = download(&owner, &key, attachment_id).await;
+
+    assert_eq!(sha256(&saved), sha256(&file));
+    let (received, total) = owner
+        .wait_for(|payload| match payload {
+            CoreEventPayload::DownloadProgress {
+                received_bytes,
+                total_bytes,
+                done: true,
+                ..
+            } => Some((*received_bytes, *total_bytes)),
+            _ => None,
+        })
+        .await;
+    assert_eq!((received, total), (file.len() as u64, file.len() as u64));
+}
```

- [ ] **Step 1:** Add the tests to `tests/attachments.rs`.
- [ ] **Step 2:** Run → FAIL: it doesn't compile yet.
- [ ] **Step 3:** Make the other changes, then regenerate the bindings.
- [ ] **Step 4:** Run → PASS:
  - `cargo test -p opencord_core --test attachments`: 10 tests;
  - `cargo clippy -p opencord_core -p opencord-server --all-targets -- -D warnings`;
  - `cd app && flutter analyze`.
- [ ] **Step 5: Commit:** `feat: sending and saving files in the core (A4)`.

### Task 19: Attachment bytes and the encrypted cache

**What it does.**
- **`attachment_bytes`** reads a thumbnail, or an original of at most 64 MiB, for the image viewer and the voice and audio players. Thumbnails, and originals up to 20 MiB, are cached.
  - Only those callers use it, so this rule caches what the Global Constraints list: images up to 20 MiB, voice messages and audio up to 20 MiB.
  - Videos use the media player's ranged reads (Task 33), and other files use `download_attachment`.
- **The cache** (`attachments/cache.rs`) follows `SoundCache`:
  - 1 GiB for every server, the least recently used going first, a file's modification time being when it was last used;
  - each entry sealed as a blob of kind 2 (client cache) under a key from the system keychain;
  - an entry's name is the SHA-256 of server key, attachment id and variant, and its first 8 bytes are the blob id. So a file copied over another entry's name doesn't open, and is removed;
  - cache reads and writes run in `spawn_blocking`, since they read and seal up to 20 MiB (Working Rule 18).
- **The key** (`app/lib/core/rust/media_cache_key_store.dart`):
  - 32 random bytes in the keychain under `opencord.[<profile>.]media-cache.key`, with `SecureIdentityStore`'s options;
  - made once, then reused, and replaced if damaged;
  - `app_start.dart` calls `media_cache_init` with it right after `core.init`. If the keychain fails, the error is logged and nothing is cached on disk.

**Files.**
- Create: `src/attachments/cache.rs`, `app/lib/core/rust/media_cache_key_store.dart`, `app/test/unit/rust/media_cache_key_store_test.dart`.
- Modify: `src/attachments/{mod,download}.rs`, `src/client.rs`, `src/api/attachments.rs`, `tests/attachments.rs`, `app/lib/app_start.dart`.

**`src/attachments/cache.rs`** (new, with its unit tests):

```rust
//! Attachments kept on this device (media uploads plan, Task 19): one cache
//! for every server, at most 1 GiB, the least recently shown going first.
//! Each file is sealed with a key the app keeps in the system keychain, so
//! the folder holds no readable file. A file's modification time is when
//! it was last shown or fetched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use opencord_common::blob::{self, BlobKey, BlobKind};
use sha2::{Digest, Sha256};

use crate::api::types::AttachmentVariant;

/// Inside the app's data directory.
pub(crate) const DIR: &str = "media-cache";
pub(crate) const LIMIT: u64 = 1024 * 1024 * 1024;

/// One attachment's place in the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The hash's 64 hex digits.
    name: String,
    /// What it is sealed under: the hash's first 8 bytes. A file copied
    /// over another entry's name doesn't open.
    blob_id: i64,
}

impl Entry {
    pub fn new(server_key: &str, attachment_id: i64, variant: AttachmentVariant) -> Self {
        let variant = match variant {
            AttachmentVariant::Original => "original",
            AttachmentVariant::Thumbnail => "thumbnail",
        };
        let hash = Sha256::digest(format!("{server_key}\n{attachment_id}\n{variant}"));
        let mut id = [0u8; 8];
        id.copy_from_slice(&hash[..8]);
        Self {
            name: hex::encode(hash),
            blob_id: i64::from_be_bytes(id),
        }
    }
}

pub(crate) struct MediaCache {
    dir: PathBuf,
    key: BlobKey,
    limit: u64,
}

impl MediaCache {
    pub fn open(dir: PathBuf, key: BlobKey, limit: u64) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        Ok(Self { dir, key, limit })
    }

    /// The entry's bytes, if it is here and opens. One that doesn't open is
    /// removed. Its time is updated, so it is kept longer.
    pub fn get(&self, entry: &Entry) -> Option<Vec<u8>> {
        let path = self.dir.join(&entry.name);
        let sealed = fs::read(&path).ok()?;
        match blob::open_all(&self.key, BlobKind::Cache, entry.blob_id, &sealed) {
            Ok(plain) => {
                touch(&path);
                Some(plain)
            }
            Err(error) => {
                tracing::debug!(?error, "a cached file didn't open; removing it");
                remove(&path);
                None
            }
        }
    }

    /// Keeps `plain` sealed, then trims the cache to its limit.
    pub fn put(&self, entry: &Entry, plain: &[u8]) -> io::Result<()> {
        let sealed = blob::seal_all(&self.key, BlobKind::Cache, entry.blob_id, plain);
        let path = self.dir.join(&entry.name);
        let partial = self.dir.join(format!("{}.partial", entry.name));
        fs::write(&partial, sealed)?;
        fs::rename(&partial, &path)?;
        self.trim(&path)
    }

    pub fn size(&self) -> io::Result<u64> {
        Ok(self.entries()?.iter().map(|(_, size, _)| size).sum())
    }

    pub fn clear(&self) -> io::Result<()> {
        for (_, _, path) in self.entries()? {
            remove_file(&path)?;
        }
        Ok(())
    }

    /// Removes the least recently used entries until the rest fit, never
    /// `keep`.
    fn trim(&self, keep: &Path) -> io::Result<()> {
        let mut entries = self.entries()?;
        let mut total: u64 = entries.iter().map(|(_, size, _)| size).sum();
        entries.sort();
        for (_, size, path) in entries {
            if total <= self.limit {
                break;
            }
            if path == keep {
                continue;
            }
            remove_file(&path)?;
            total -= size;
        }
        Ok(())
    }

    /// Every entry with its time and size. Files being written (`.partial`)
    /// aren't entries yet.
    fn entries(&self) -> io::Result<Vec<(SystemTime, u64, PathBuf)>> {
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.dir)? {
            let item = item?;
            let is_entry = item.file_name().to_str().is_some_and(|name| {
                name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit())
            });
            let metadata = item.metadata()?;
            if is_entry && metadata.is_file() {
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                entries.push((modified, metadata.len(), item.path()));
            }
        }
        Ok(entries)
    }
}

/// Removed already counts as removed.
fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn remove(path: &Path) {
    if let Err(error) = remove_file(path) {
        tracing::warn!(%error, "a cached file couldn't be removed");
    }
}

/// Marks the file as just used.
fn touch(path: &Path) {
    let touched = fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()));
    if let Err(error) = touched {
        tracing::debug!(%error, "a cached file's time couldn't be updated");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn cache(dir: &Path, limit: u64) -> MediaCache {
        MediaCache::open(dir.join(DIR), BlobKey::from_bytes([7; 32]), limit).unwrap()
    }

    fn entry(id: i64) -> Entry {
        Entry::new("127.0.0.1:8443", id, AttachmentVariant::Original)
    }

    fn age(cache: &MediaCache, entry: &Entry, seconds: u64) {
        fs::File::options()
            .write(true)
            .open(cache.dir.join(&entry.name))
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(seconds))
            .unwrap();
    }

    #[test]
    fn cache_round_trips_and_evicts_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let one = blob::sealed_len(1_000);
        let cache = cache(dir.path(), one * 3);
        for id in 1..=3 {
            cache.put(&entry(id), &[id as u8; 1_000]).unwrap();
            age(&cache, &entry(id), 100 - id as u64);
        }
        assert_eq!(cache.get(&entry(2)), Some(vec![2; 1_000]));

        cache.put(&entry(4), &[4; 1_000]).unwrap();

        assert_eq!(cache.get(&entry(1)), None);
        assert_eq!(cache.get(&entry(2)), Some(vec![2; 1_000]));
        assert_eq!(cache.get(&entry(4)), Some(vec![4; 1_000]));
        assert_eq!(cache.size().unwrap(), one * 3);
    }

    #[test]
    fn cache_files_hold_no_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path(), LIMIT);
        let marker = b"a very recognisable marker string";
        cache.put(&entry(1), marker).unwrap();

        for item in fs::read_dir(&cache.dir).unwrap() {
            let bytes = fs::read(item.unwrap().path()).unwrap();
            assert!(!bytes.windows(marker.len()).any(|window| window == marker));
        }
    }

    #[test]
    fn a_cache_file_cannot_be_renamed_to_another_entry() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path(), LIMIT);
        let (a, b) = (entry(1), entry(2));
        cache.put(&a, b"picture a").unwrap();
        cache.put(&b, b"picture b").unwrap();

        fs::copy(cache.dir.join(&a.name), cache.dir.join(&b.name)).unwrap();

        assert_eq!(cache.get(&b), None);
        assert!(!cache.dir.join(&b.name).exists());
        assert_eq!(cache.get(&a), Some(b"picture a".to_vec()));
    }

    #[test]
    fn clear_empties_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = cache(dir.path(), LIMIT);
        cache.put(&entry(1), b"one").unwrap();
        cache.put(&entry(2), b"two").unwrap();

        cache.clear().unwrap();

        assert_eq!(cache.size().unwrap(), 0);
        assert_eq!(cache.get(&entry(1)), None);
    }
}
```

**`src/attachments/mod.rs` and `src/attachments/download.rs`:**

```diff
--- a/crates/opencord-core/src/attachments/mod.rs
+++ b/crates/opencord-core/src/attachments/mod.rs
@@ -1,6 +1,7 @@
 //! Files in messages (media uploads plan, milestone A4): sending them,
 //! downloading them, the encrypted cache, photo metadata and the clipboard.
 
+pub(crate) mod cache;
 pub(crate) mod download;
 pub(crate) mod transfer;
 pub(crate) mod upload;
```

```diff
--- a/crates/opencord-core/src/attachments/download.rs
+++ b/crates/opencord-core/src/attachments/download.rs
@@ -1,20 +1,26 @@
-//! Attachments coming down (media uploads plan, Task 18): any file streamed
-//! to where the user saves it.
+//! Attachments coming down (media uploads plan, Tasks 18 and 19): small ones whole,
+//! for the image viewer and the players, through the cache; any file
+//! streamed to where the user saves it.
 
 use std::collections::HashMap;
 use std::path::{Path, PathBuf};
-use std::sync::{Mutex, MutexGuard, PoisonError};
+use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
 use std::time::{Duration, Instant};
 
 use tokio::io::AsyncWriteExt;
 use tokio_util::sync::CancellationToken;
 
+use super::cache::{Entry, MediaCache};
 use super::transfer::{Call, network, refusal};
 use crate::api::types::{AttachmentVariant, CoreError, CoreEventPayload};
 use crate::client::Client;
 
 /// Progress goes out at most this often.
 const PROGRESS_EVERY: Duration = Duration::from_millis(100);
+/// The most `bytes` reads into memory.
+const SHOW_MAX_BYTES: u64 = 64 * 1024 * 1024;
+/// Originals up to this size are cached; thumbnails always are.
+const CACHE_MAX_BYTES: u64 = 20 * 1024 * 1024;
 
 #[derive(Default)]
 pub(crate) struct Downloads {
@@ -188,3 +194,97 @@
         );
     }
 }
+
+/// A thumbnail, or an original of at most 64 MiB, from the cache when it
+/// is there. Only images, voice messages and audio are read this way.
+pub(crate) async fn bytes(
+    client: &Client,
+    key: &str,
+    attachment_id: i64,
+    variant: AttachmentVariant,
+) -> Result<Vec<u8>, CoreError> {
+    let entry = Entry::new(key, attachment_id, variant);
+    let cache = client.media_cache();
+    if let Some(cache) = &cache {
+        let (cache, entry) = (Arc::clone(cache), entry.clone());
+        if let Some(bytes) = blocking(move || Ok(cache.get(&entry))).await? {
+            return Ok(bytes);
+        }
+    }
+    let call = Call::get(path_of(attachment_id, variant), None);
+    let mut response = client.transfers().send(client, key, call).await?;
+    if !response.status().is_success() {
+        return Err(refusal(response).await);
+    }
+    let too_large = || CoreError::InvalidInput {
+        message: "the file is too large to show".to_owned(),
+    };
+    if response
+        .content_length()
+        .is_some_and(|length| length > SHOW_MAX_BYTES)
+    {
+        return Err(too_large());
+    }
+    let mut body = Vec::new();
+    while let Some(chunk) = response.chunk().await.map_err(|error| network(&error))? {
+        if (body.len() + chunk.len()) as u64 > SHOW_MAX_BYTES {
+            return Err(too_large());
+        }
+        body.extend_from_slice(&chunk);
+    }
+    let worth_keeping =
+        variant == AttachmentVariant::Thumbnail || body.len() as u64 <= CACHE_MAX_BYTES;
+    if let Some(cache) = cache
+        && worth_keeping
+    {
+        let plain = body.clone();
+        let kept = blocking(move || {
+            cache
+                .put(&entry, &plain)
+                .map_err(|error| CoreError::Storage {
+                    message: error.to_string(),
+                })
+        })
+        .await;
+        if let Err(error) = kept {
+            tracing::warn!(%error, "an attachment couldn't be cached");
+        }
+    }
+    Ok(body)
+}
+
+/// Runs cache work (file reads and sealing) off the async threads.
+async fn blocking<T: Send + 'static>(
+    work: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
+) -> Result<T, CoreError> {
+    tokio::task::spawn_blocking(work)
+        .await
+        .map_err(|error| CoreError::Storage {
+            message: error.to_string(),
+        })?
+}
+
+/// The cache's size, or 0 without one.
+pub(crate) async fn cache_size(cache: Option<Arc<MediaCache>>) -> Result<u64, CoreError> {
+    let Some(cache) = cache else {
+        return Ok(0);
+    };
+    blocking(move || {
+        cache.size().map_err(|error| CoreError::Storage {
+            message: error.to_string(),
+        })
+    })
+    .await
+}
+
+pub(crate) async fn cache_clear(cache: Option<Arc<MediaCache>>) -> Result<(), CoreError> {
+    let Some(cache) = cache else {
+        return Ok(());
+    };
+    blocking(move || {
+        cache.clear().map_err(|error| CoreError::Storage {
+            message: error.to_string(),
+        })
+    })
+    .await
+}
```

**`src/client.rs` and `src/api/attachments.rs`:**

```diff
--- a/crates/opencord-core/src/client.rs
+++ b/crates/opencord-core/src/client.rs
@@ -2,7 +2,7 @@
 //! server. Every event from every server arrives on one channel.
 
 use std::collections::HashMap;
-use std::path::Path;
+use std::path::{Path, PathBuf};
 use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock, Weak};
 use std::time::{Duration, SystemTime, UNIX_EPOCH};
 
@@ -16,16 +16,17 @@
 use tokio::sync::{broadcast, mpsc, oneshot};
 
 use crate::api::types::{
-    AddServerOutcome, AudioSettings, Ban, CameraStarted, Channel, ChannelChanges, ChannelKind,
-    ChannelPosition, CoreError, CoreEvent, CoreEventPayload, ErrorCode, HotkeyBinding,
-    HotkeySupport, IdentityInfo, Invite, MediaEvent, Member, Message, OutgoingFile,
-    OverwriteTargetKind, PermissionOverwrite, PresenceStatus, Role, RoleChanges, S3Config,
-    ScreenProblem, ScreenShareRequest, ScreenShareStarted, ScreenStream, Server, ServerChanges,
-    ServerInfo, SoundDraft, SoundSelection, SoundboardSound, SoundboardSoundChanges, StorageInfo,
-    StorageTestResult, StorageTestTarget, StreamSourceKind, UploadSettings, User, VideoWant,
-    VoiceCue, VoiceSettings, VoiceSettingsChanges, VoiceState,
+    AddServerOutcome, AttachmentVariant, AudioSettings, Ban, CameraStarted, Channel,
+    ChannelChanges, ChannelKind, ChannelPosition, CoreError, CoreEvent, CoreEventPayload,
+    ErrorCode, HotkeyBinding, HotkeySupport, IdentityInfo, Invite, MediaEvent, Member, Message,
+    OutgoingFile, OverwriteTargetKind, PermissionOverwrite, PresenceStatus, Role, RoleChanges,
+    S3Config, ScreenProblem, ScreenShareRequest, ScreenShareStarted, ScreenStream, Server,
+    ServerChanges, ServerInfo, SoundDraft, SoundSelection, SoundboardSound, SoundboardSoundChanges,
+    StorageInfo, StorageTestResult, StorageTestTarget, StreamSourceKind, UploadSettings, User,
+    VideoWant, VoiceCue, VoiceSettings, VoiceSettingsChanges, VoiceState,
 };
 use crate::artwork::ArtworkCache;
+use crate::attachments::cache::{self, MediaCache};
 use crate::attachments::download::{self, Downloads};
 use crate::attachments::transfer::Transfers;
 use crate::attachments::upload::{self, Uploads};
@@ -89,9 +90,12 @@
     clocks: Clocks,
     /// How this person hears music, kept for media that starts later.
     music_listening: Mutex<MusicListening>,
+    data_dir: PathBuf,
     transfers: Transfers,
     uploads: Arc<Uploads>,
     downloads: Downloads,
+    /// Attachments kept on this device, once the app gave the cache's key.
+    media_cache: RwLock<Option<Arc<MediaCache>>>,
 }
 
 /// The app's id, as its desktop file names it; the system's portals ask for
@@ -179,9 +183,11 @@
                 artwork,
                 clocks: Clocks::default(),
                 music_listening: Mutex::default(),
+                data_dir: data_dir.to_owned(),
                 transfers: Transfers::new(Arc::clone(&routes), Arc::clone(&media_tokens)),
                 uploads: Arc::default(),
                 downloads: Downloads::default(),
+                media_cache: RwLock::new(None),
             }),
         };
         client.spawn_wake_watch();
@@ -1953,6 +1959,53 @@
             .insert(key.to_owned(), "spoiled".to_owned());
     }
 
+    pub(crate) fn media_cache(&self) -> Option<Arc<MediaCache>> {
+        self.inner
+            .media_cache
+            .read()
+            .unwrap_or_else(PoisonError::into_inner)
+            .clone()
+    }
+
+    /// A thumbnail, or an original of at most 64 MiB, through the cache.
+    pub async fn attachment_bytes(
+        &self,
+        key: &str,
+        attachment_id: i64,
+        variant: AttachmentVariant,
+    ) -> Result<Vec<u8>, CoreError> {
+        download::bytes(self, key, attachment_id, variant).await
+    }
+
+    /// Turns the attachment cache on, sealed with this 32-byte key.
+    pub fn media_cache_init(&self, key: &[u8]) -> Result<(), CoreError> {
+        let key: [u8; 32] = key.try_into().map_err(|_| CoreError::InvalidInput {
+            message: "the media cache's key must be 32 bytes".to_owned(),
+        })?;
+        let cache = MediaCache::open(
+            self.inner.data_dir.join(cache::DIR),
+            opencord_common::blob::BlobKey::from_bytes(key),
+            cache::LIMIT,
+        )
+        .map_err(|error| CoreError::Storage {
+            message: error.to_string(),
+        })?;
+        *self
+            .inner
+            .media_cache
+            .write()
+            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(cache));
+        Ok(())
+    }
+
+    pub async fn media_cache_size(&self) -> Result<u64, CoreError> {
+        download::cache_size(self.media_cache()).await
+    }
+
+    pub async fn media_cache_clear(&self) -> Result<(), CoreError> {
+        download::cache_clear(self.media_cache()).await
+    }
+
     /// A request answered with an ack.
     pub(crate) async fn ack(&self, key: &str, kind: Request) -> Result<(), CoreError> {
         expect_ack(self.request(key, kind).await?)
```

```diff
--- a/crates/opencord-core/src/api/attachments.rs
+++ b/crates/opencord-core/src/api/attachments.rs
@@ -5,8 +5,8 @@
 
 use super::client::{client, on_runtime};
 use super::types::{
-    CoreError, Message, OutgoingFile, S3Config, StorageInfo, StorageTestResult, StorageTestTarget,
-    UploadSettings,
+    AttachmentVariant, CoreError, Message, OutgoingFile, S3Config, StorageInfo, StorageTestResult,
+    StorageTestTarget, UploadSettings,
 };
 
 /// MANAGE_SERVER. `None` leaves a setting as it is.
@@ -126,3 +126,34 @@
     client()?.cancel_download(&server_key, attachment_id);
     Ok(())
 }
+
+/// A thumbnail, or an original of at most 64 MiB, through the cache. Only
+/// for images, voice messages and audio; other files use
+/// `download_attachment`, videos the media player.
+pub async fn attachment_bytes(
+    server_key: String,
+    attachment_id: i64,
+    variant: AttachmentVariant,
+) -> Result<Vec<u8>, CoreError> {
+    on_runtime(move |client| async move {
+        client
+            .attachment_bytes(&server_key, attachment_id, variant)
+            .await
+    })
+    .await
+}
+
+/// Turns the attachment cache on, with the 32-byte key the app keeps in
+/// the system keychain. Without it nothing is cached on disk.
+#[frb(sync)]
+pub fn media_cache_init(key: Vec<u8>) -> Result<(), CoreError> {
+    client()?.media_cache_init(&key)
+}
+
+pub async fn media_cache_size() -> Result<u64, CoreError> {
+    on_runtime(move |client| async move { client.media_cache_size().await }).await
+}
+
+pub async fn media_cache_clear() -> Result<(), CoreError> {
+    on_runtime(move |client| async move { client.media_cache_clear().await }).await
+}
```

**`tests/attachments.rs`:**

```diff
--- a/crates/opencord-core/tests/attachments.rs
+++ b/crates/opencord-core/tests/attachments.rs
@@ -5,8 +5,8 @@
 
 use opencord_common::address::format_fingerprint;
 use opencord_core::api::types::{
-    CoreError, CoreEvent, CoreEventPayload, ErrorCode, OutgoingFile, ReadySnapshot, RoleChanges,
-    StorageKind, UploadPhase,
+    AttachmentVariant, CoreError, CoreEvent, CoreEventPayload, ErrorCode, OutgoingFile,
+    ReadySnapshot, RoleChanges, StorageKind, UploadPhase,
 };
 use opencord_core::client::Client;
 use opencord_core::identity::Identity;
@@ -453,3 +453,56 @@
         .await;
     assert_eq!((received, total), (file.len() as u64, file.len() as u64));
 }
+
+#[tokio::test(flavor = "multi_thread")]
+async fn attachment_bytes_uses_the_cache_second_time() {
+    let server = TestServer::start().await;
+    let (owner, ready) = TestClient::owner(&server).await;
+    let key = server.address();
+    owner.client.media_cache_init(&[9; 32]).unwrap();
+    let mut png = Vec::new();
+    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64, 64, image::Rgb([1, 2, 3])))
+        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
+        .unwrap();
+    let message = owner
+        .client
+        .send_message_with_files(
+            &key,
+            general(&ready),
+            String::new(),
+            "n1".to_owned(),
+            vec![bytes_file("picture.png", png.clone())],
+        )
+        .await
+        .unwrap();
+    let id = message.attachments[0].id;
+    let original = owner
+        .client
+        .attachment_bytes(&key, id, AttachmentVariant::Original)
+        .await
+        .unwrap();
+    let thumbnail = owner
+        .client
+        .attachment_bytes(&key, id, AttachmentVariant::Thumbnail)
+        .await
+        .unwrap();
+    assert_eq!(original, png);
+    assert!(owner.client.media_cache_size().await.unwrap() > 0);
+
+    server.handle.shutdown().await;
+
+    let cached = owner
+        .client
+        .attachment_bytes(&key, id, AttachmentVariant::Original)
+        .await
+        .unwrap();
+    assert_eq!(cached, png);
+    let cached = owner
+        .client
+        .attachment_bytes(&key, id, AttachmentVariant::Thumbnail)
+        .await
+        .unwrap();
+    assert_eq!(cached, thumbnail);
+    owner.client.media_cache_clear().await.unwrap();
+    assert_eq!(owner.client.media_cache_size().await.unwrap(), 0);
+}
```

**`app/lib/core/rust/media_cache_key_store.dart`** (new):

```dart
import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

/// The key the attachment cache on disk is sealed with (media uploads plan,
/// Task 19). It stays in the system keychain, beside the identity, so the
/// cache's files can't be read without this user's login. [profile] keeps
/// a second instance's key apart, as [SecureIdentityStore] does.
class MediaCacheKeyStore {
  MediaCacheKeyStore({String profile = ''})
    : _name = profile.isEmpty
          ? 'opencord.media-cache.key'
          : 'opencord.$profile.media-cache.key';

  static const keyLength = 32;

  final String _name;

  // The same options as SecureIdentityStore: on macOS the login keychain.
  final _storage = const FlutterSecureStorage(
    mOptions: MacOsOptions(usesDataProtectionKeychain: false),
  );

  /// The saved key, or a new random one, saved first. Throws the plugin's
  /// PlatformException when the keychain can't be used.
  Future<Uint8List> load() async {
    final saved = await _storage.read(key: _name);
    if (saved != null) {
      try {
        final key = base64Decode(saved);
        if (key.length == keyLength) return key;
      } on FormatException {
        // Replaced below.
      }
    }
    final random = Random.secure();
    final key = Uint8List.fromList([
      for (var i = 0; i < keyLength; i++) random.nextInt(256),
    ]);
    await _storage.write(key: _name, value: base64Encode(key));
    return key;
  }
}
```

**`app/test/unit/rust/media_cache_key_store_test.dart`** (new):

```dart
import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/rust/media_cache_key_store.dart';

const _plugin = MethodChannel('plugins.it_nomads.com/flutter_secure_storage');

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final keychain = <String, String>{};

  setUp(() {
    keychain.clear();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(_plugin, (call) async {
          final arguments = call.arguments as Map<Object?, Object?>;
          final key = arguments['key']! as String;
          return switch (call.method) {
            'read' => keychain[key],
            'write' => keychain[key] = arguments['value']! as String,
            _ => null,
          };
        });
  });

  tearDown(
    () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(_plugin, null),
  );

  test('the key is made once, then reused', () async {
    final first = await MediaCacheKeyStore().load();
    final second = await MediaCacheKeyStore().load();

    expect(first, hasLength(32));
    expect(second, first);
    expect(keychain.keys, ['opencord.media-cache.key']);
  });

  test('a damaged key is replaced', () async {
    keychain['opencord.media-cache.key'] = base64Encode([1, 2, 3]);

    final key = await MediaCacheKeyStore().load();

    expect(key, hasLength(32));
    expect(base64Decode(keychain['opencord.media-cache.key']!), key);
  });

  test('a profile keeps its own key', () async {
    final main = await MediaCacheKeyStore().load();
    final other = await MediaCacheKeyStore(profile: 'b').load();

    expect(other, isNot(main));
    expect(keychain.keys, contains('opencord.b.media-cache.key'));
  });
}
```

**`app/lib/app_start.dart`:**

```diff
--- a/app/lib/app_start.dart
+++ b/app/lib/app_start.dart
@@ -18,6 +18,7 @@
 import 'package:opencord/core/rust/core_key_value_store.dart';
 import 'package:opencord/core/rust/core_mapping.dart';
 import 'package:opencord/core/rust/identity_store.dart';
+import 'package:opencord/core/rust/media_cache_key_store.dart';
 import 'package:opencord/core/rust/rust_repository.dart';
 import 'package:opencord/core/settings/app_settings.dart';
 import 'package:opencord/core/settings/key_value_store.dart';
@@ -35,6 +36,7 @@
 import 'package:opencord/features/window/native_window.dart';
 import 'package:opencord/features/window/window_providers.dart';
 import 'package:opencord/features/window/window_startup.dart';
+import 'package:opencord/src/rust/api/attachments.dart' as attachments;
 import 'package:opencord/src/rust/api/client.dart' as core;
 import 'package:opencord/src/rust/api/system.dart';
 import 'package:opencord/src/rust/api/types.dart' show CoreError;
@@ -64,6 +66,7 @@
   final environment = Platform.environment;
   final dataDir = await _dataDir(profile);
   core.init(appDataDir: dataDir.path);
+  if (!mock) await _startMediaCache(profile);
   registerFontLicenses();
 
   const store = CoreKeyValueStore();
@@ -165,6 +168,19 @@
   }
 }
 
+/// Seals the attachment cache with a key from the system keychain (media
+/// uploads plan, Task 19). Without the keychain nothing is cached on disk.
+Future<void> _startMediaCache(String profile) async {
+  try {
+    final key = await MediaCacheKeyStore(profile: profile).load();
+    attachments.mediaCacheInit(key: key);
+  } on PlatformException catch (error) {
+    developer.log('no media cache: ${error.message}', name: 'opencord');
+  } on CoreError catch (error) {
+    developer.log('no media cache: $error', name: 'opencord');
+  }
+}
+
 /// Lets the core draw video into this engine's textures (Phase 2 plan
 /// §7.11). Without it, video still flows but nothing is drawn.
 Future<void> _startVideo() async {
```

- [ ] **Step 1:** Write the tests: `cache.rs`'s unit tests, the new test in `tests/attachments.rs`, and the Dart test.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Make the other changes, then regenerate the bindings.
- [ ] **Step 4:** Run → PASS:
  - `cargo test -p opencord_core --lib -- attachments::cache`: 4 tests;
  - `cargo test -p opencord_core --test attachments`: 11 tests;
  - clippy as before;
  - `cd app && flutter analyze && flutter test test/unit/rust/media_cache_key_store_test.dart`.
- [ ] **Step 5: Commit:** `feat: attachment bytes and the encrypted media cache (A4)`.

### Task 20: Removing photo metadata

**What it does.** Before a JPEG, PNG or WebP of at most 100 MiB leaves the device, its metadata goes. A path is read into memory only when its first 12 bytes say it's one of those.
- **JPEG:**
  - APP1 Exif and XMP segments are dropped, and so is APP13 (IPTC).
  - An orientation other than 1 comes back as a 36-byte APP1 Exif segment holding only that tag, right after SOI: the length field says 34 (0x22), plus the 2 marker bytes.
  - Everything from the start of scan on is copied as it is.
- **PNG:** the `eXIf`, `tEXt`, `zTXt`, `iTXt` and `tIME` chunks are dropped. Text chunks carry XMP and camera notes, and the image doesn't need them.
- **WebP:** the `EXIF` and `XMP ` chunks are dropped, their VP8X flags are cleared, and the RIFF size is rewritten. Odd-sized chunks keep their pad byte.
- A file that isn't well formed comes back `None` and is sent as it is. The server's processing rejects what it must.

**Files.**
- Create: `src/attachments/exif.rs`.
- Modify: `src/attachments/{mod,upload}.rs`, `tests/attachments.rs`.

**`src/attachments/exif.rs`** (new, with its unit tests):

```rust
//! Removing a photo's metadata before it is sent (media uploads plan,
//! Task 20): Exif, with its GPS position and camera serial numbers, XMP,
//! and PNG text. A JPEG keeps its orientation in a minimal Exif block, so
//! it still shows the right way up.

const JPEG_START: [u8; 3] = [0xFF, 0xD8, 0xFF];
const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const EXIF: &[u8] = b"Exif\0\0";
const XMP: &[u8] = b"http://ns.adobe.com/xap/1.0/\0";
/// The TIFF orientation tag.
const ORIENTATION: u16 = 0x0112;
/// VP8X flags for Exif and XMP chunks.
const VP8X_EXIF: u8 = 0x08;
const VP8X_XMP: u8 = 0x04;

/// Whether a file starting with `head` (its first 12 bytes or more) is a
/// JPEG, PNG or WebP, which may carry metadata.
pub fn is_photo(head: &[u8]) -> bool {
    head.starts_with(&JPEG_START)
        || head.starts_with(&PNG_SIGNATURE)
        || (head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP")
}

/// The bytes without metadata, or `None` when they aren't a well-formed
/// JPEG, PNG or WebP; the caller then sends the file as it is.
pub fn strip_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.starts_with(&JPEG_START) {
        jpeg(bytes)
    } else if bytes.starts_with(&PNG_SIGNATURE) {
        png(bytes)
    } else if is_photo(bytes) {
        webp(bytes)
    } else {
        None
    }
}

/// Copies the segments before the image data except APP1 Exif or XMP and
/// APP13 (IPTC), then the image data as it is.
fn jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[..2]);
    let mut orientation = None;
    let mut at = 2;
    loop {
        let start = at;
        if *bytes.get(at)? != 0xFF {
            return None;
        }
        // Fill bytes may come before a marker.
        while *bytes.get(at + 1)? == 0xFF {
            at += 1;
        }
        let marker = *bytes.get(at + 1)?;
        at += 2;
        match marker {
            // Start of scan: the rest is image data.
            0xDA => {
                out.extend_from_slice(&bytes[start..]);
                break;
            }
            // Markers without a length don't belong before the scan.
            0x01 | 0xD0..=0xD9 => return None,
            _ => {
                let length =
                    usize::from(u16::from_be_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]));
                if length < 2 {
                    return None;
                }
                let end = at.checked_add(length)?;
                let payload = bytes.get(at + 2..end)?;
                let metadata = match marker {
                    0xE1 if payload.starts_with(EXIF) => {
                        orientation =
                            orientation.or_else(|| exif_orientation(&payload[EXIF.len()..]));
                        true
                    }
                    0xE1 => payload.starts_with(XMP),
                    0xED => true,
                    _ => false,
                };
                if !metadata {
                    out.extend_from_slice(&bytes[start..end]);
                }
                at = end;
            }
        }
    }
    if let Some(turn @ 2..=8) = orientation {
        out.splice(2..2, orientation_segment(turn));
    }
    Some(out)
}

/// An APP1 Exif segment with only the orientation: big-endian TIFF, one
/// IFD, one SHORT entry.
fn orientation_segment(turn: u16) -> [u8; 36] {
    let [high, low] = turn.to_be_bytes();
    [
        0xFF, 0xE1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0x00, 0x00, b'M', b'M', 0x00, 0x2A, 0x00,
        0x00, 0x00, 0x08, 0x00, 0x01, 0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, high, low,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]
}

/// The orientation tag in a TIFF block's first IFD, in either byte order.
fn exif_orientation(tiff: &[u8]) -> Option<u16> {
    let big = match tiff.get(..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let pair = [*tiff.get(at)?, *tiff.get(at + 1)?];
        Some(if big {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let quad = [
            *tiff.get(at)?,
            *tiff.get(at + 1)?,
            *tiff.get(at + 2)?,
            *tiff.get(at + 3)?,
        ];
        Some(if big {
            u32::from_be_bytes(quad)
        } else {
            u32::from_le_bytes(quad)
        })
    };
    if u16_at(2)? != 42 {
        return None;
    }
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let count = usize::from(u16_at(ifd)?);
    (0..count).find_map(|index| {
        let entry = ifd.checked_add(2 + index * 12)?;
        // A SHORT sits at the start of the entry's 4-byte value.
        (u16_at(entry)? == ORIENTATION).then(|| u16_at(entry + 8))?
    })
}

/// Copies the chunks up to IEND except `eXIf`, the text chunks (`tEXt`,
/// `zTXt`, `iTXt`, which hold XMP and camera notes) and `tIME`.
fn png(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = PNG_SIGNATURE.to_vec();
    let mut at = PNG_SIGNATURE.len();
    loop {
        let length =
            usize::try_from(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?)).ok()?;
        let kind: [u8; 4] = bytes.get(at + 4..at + 8)?.try_into().ok()?;
        // Length, type, data and CRC.
        let end = at.checked_add(12)?.checked_add(length)?;
        let chunk = bytes.get(at..end)?;
        if !matches!(&kind, b"eXIf" | b"tEXt" | b"zTXt" | b"iTXt" | b"tIME") {
            out.extend_from_slice(chunk);
        }
        at = end;
        if &kind == b"IEND" {
            return Some(out);
        }
    }
}

/// Copies the RIFF chunks except `EXIF` and `XMP `, clears their VP8X
/// flags, and writes the new RIFF size.
fn webp(bytes: &[u8]) -> Option<Vec<u8>> {
    let riff_size = usize::try_from(u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?)).ok()?;
    let end = riff_size.checked_add(8)?;
    if end > bytes.len() {
        return None;
    }
    let mut out = bytes[..12].to_vec();
    let mut at = 12;
    while at < end {
        let kind: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
        let length = usize::try_from(u32::from_le_bytes(
            bytes.get(at + 4..at + 8)?.try_into().ok()?,
        ))
        .ok()?;
        // An odd-sized chunk carries a pad byte.
        let next = at
            .checked_add(8)?
            .checked_add(length)?
            .checked_add(length & 1)?;
        let chunk = bytes.get(at..next)?;
        match &kind {
            b"EXIF" | b"XMP " => {}
            b"VP8X" => {
                let mut chunk = chunk.to_vec();
                *chunk.get_mut(8)? &= !(VP8X_EXIF | VP8X_XMP);
                out.extend_from_slice(&chunk);
            }
            _ => out.extend_from_slice(chunk),
        }
        at = next;
    }
    let size = u32::try_from(out.len() - 8).ok()?;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::metadata::Orientation;
    use image::{ImageDecoder, ImageFormat, RgbImage, RgbaImage};

    use super::*;

    const SECRET: &[u8] = b"GPS 52.5200N 13.4050E";

    fn encoded(format: ImageFormat) -> Vec<u8> {
        let mut out = Vec::new();
        let picture = RgbaImage::from_fn(16, 8, |x, y| {
            image::Rgba([x as u8 * 16, y as u8 * 32, 90, 255])
        });
        let picture = match format {
            ImageFormat::Jpeg => {
                image::DynamicImage::ImageRgb8(RgbImage::from_fn(16, 8, |x, y| {
                    image::Rgb([x as u8 * 16, y as u8 * 32, 90])
                }))
            }
            _ => image::DynamicImage::ImageRgba8(picture),
        };
        picture
            .write_to(&mut Cursor::new(&mut out), format)
            .unwrap();
        out
    }

    /// Little-endian TIFF: orientation `turn` in IFD0, then `SECRET` where
    /// a GPS block would be.
    fn exif_block(turn: u16) -> Vec<u8> {
        let mut tiff = b"II\x2A\x00\x08\x00\x00\x00".to_vec();
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&ORIENTATION.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&turn.to_le_bytes());
        tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        tiff.extend_from_slice(SECRET);
        let mut block = EXIF.to_vec();
        block.extend_from_slice(&tiff);
        block
    }

    fn jpeg_segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut segment = vec![0xFF, marker];
        segment.extend_from_slice(&u16::try_from(payload.len() + 2).unwrap().to_be_bytes());
        segment.extend_from_slice(payload);
        segment
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut chunk = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
        chunk.extend_from_slice(kind);
        chunk.extend_from_slice(data);
        // The CRC is copied, never checked here; image checks it below.
        let mut crc = crc32(kind);
        crc = crc32_more(crc, data);
        chunk.extend_from_slice(&(!crc).to_be_bytes());
        chunk
    }

    fn crc32(bytes: &[u8]) -> u32 {
        crc32_more(0xFFFF_FFFF, bytes)
    }

    fn crc32_more(mut crc: u32, bytes: &[u8]) -> u32 {
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        crc
    }

    #[test]
    fn jpeg_gps_exif_is_removed_and_orientation_kept() {
        let plain = encoded(ImageFormat::Jpeg);
        let mut photo = plain[..2].to_vec();
        photo.extend(jpeg_segment(0xE1, &exif_block(6)));
        photo.extend(jpeg_segment(0xE1, &[XMP, SECRET].concat()));
        photo.extend(jpeg_segment(0xED, SECRET));
        photo.extend_from_slice(&plain[2..]);

        let stripped = strip_metadata(&photo).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert_eq!(&stripped[2..38], &orientation_segment(6));
        assert_eq!(stripped.len(), plain.len() + 36);
        let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(&stripped)).unwrap();
        assert_eq!(decoder.orientation().unwrap(), Orientation::Rotate90);
        image::load_from_memory(&stripped).unwrap();
    }

    #[test]
    fn jpeg_orientation_1_needs_no_exif() {
        let plain = encoded(ImageFormat::Jpeg);
        let mut photo = plain[..2].to_vec();
        photo.extend(jpeg_segment(0xE1, &exif_block(1)));
        photo.extend_from_slice(&plain[2..]);

        assert_eq!(strip_metadata(&photo).unwrap(), plain);
    }

    #[test]
    fn jpeg_without_exif_is_unchanged() {
        let plain = encoded(ImageFormat::Jpeg);

        assert_eq!(strip_metadata(&plain).unwrap(), plain);
    }

    #[test]
    fn a_truncated_jpeg_is_left_alone() {
        let plain = encoded(ImageFormat::Jpeg);
        let mut photo = plain[..2].to_vec();
        photo.extend(jpeg_segment(0xE1, &exif_block(6)));

        assert_eq!(strip_metadata(&photo[..photo.len() - 4]), None);
    }

    #[test]
    fn png_metadata_chunks_are_removed_and_the_png_decodes() {
        let plain = encoded(ImageFormat::Png);
        // After the signature and IHDR (8 + 25 bytes).
        let mut picture = plain[..33].to_vec();
        picture.extend(png_chunk(b"eXIf", &exif_block(1)[EXIF.len()..]));
        picture.extend(png_chunk(
            b"iTXt",
            &[b"XML:com.adobe.xmp\0\0\0\0\0", SECRET].concat(),
        ));
        picture.extend(png_chunk(b"tEXt", &[b"Comment\0", SECRET].concat()));
        picture.extend_from_slice(&plain[33..]);
        image::load_from_memory(&picture).unwrap();

        let stripped = strip_metadata(&picture).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert_eq!(stripped, plain);
        image::load_from_memory(&stripped).unwrap();
    }

    #[test]
    fn webp_exif_is_removed_and_riff_size_fixed() {
        let plain = encoded(ImageFormat::WebP);
        // Extended format: VP8X (Exif and XMP flags set), the image, EXIF
        // and XMP chunks, one of odd size.
        let mut vp8x = b"VP8X".to_vec();
        vp8x.extend_from_slice(&10u32.to_le_bytes());
        vp8x.extend_from_slice(&[VP8X_EXIF | VP8X_XMP | 0x10, 0, 0, 0, 15, 0, 0, 7, 0, 0]);
        let mut picture = plain[..12].to_vec();
        picture.extend_from_slice(&vp8x);
        picture.extend_from_slice(&plain[12..]);
        for (kind, data) in [
            (b"EXIF", exif_block(6)[EXIF.len()..].to_vec()),
            (b"XMP ", [SECRET, b"!"].concat()),
        ] {
            picture.extend_from_slice(kind);
            picture.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
            picture.extend_from_slice(&data);
            if data.len() % 2 == 1 {
                picture.push(0);
            }
        }
        let size = u32::try_from(picture.len() - 8).unwrap();
        picture[4..8].copy_from_slice(&size.to_le_bytes());
        image::load_from_memory(&picture).unwrap();

        let stripped = strip_metadata(&picture).unwrap();

        assert!(!contains(&stripped, SECRET));
        assert_eq!(stripped[20] & (VP8X_EXIF | VP8X_XMP), 0);
        assert_eq!(stripped[20] & 0x10, 0x10);
        assert_eq!(
            u32::from_le_bytes(stripped[4..8].try_into().unwrap()) as usize,
            stripped.len() - 8
        );
        assert_eq!(stripped.len(), plain.len() + vp8x.len());
        image::load_from_memory(&stripped).unwrap();
    }

    #[test]
    fn other_files_are_left_alone() {
        assert_eq!(strip_metadata(b"PK\x03\x04 a zip file"), None);
        assert_eq!(strip_metadata(b""), None);
        assert_eq!(strip_metadata(&PNG_SIGNATURE), None);
    }
}
```

**`src/attachments/mod.rs` and `src/attachments/upload.rs`:**

```diff
--- a/crates/opencord-core/src/attachments/mod.rs
+++ b/crates/opencord-core/src/attachments/mod.rs
@@ -3,5 +3,6 @@
 
 pub(crate) mod cache;
 pub(crate) mod download;
+pub mod exif;
 pub(crate) mod transfer;
 pub(crate) mod upload;
```

```diff
--- a/crates/opencord-core/src/attachments/upload.rs
+++ b/crates/opencord-core/src/attachments/upload.rs
@@ -11,7 +11,7 @@
 
 use bytes::Bytes;
 use futures_util::{StreamExt, TryStreamExt};
-use opencord_common::upload::{MAX_ATTACHMENTS, PIECE_BYTES};
+use opencord_common::upload::{IMAGE_PROCESS_MAX_BYTES, MAX_ATTACHMENTS, PIECE_BYTES};
 use opencord_proto::v1 as proto;
 use proto::request::Kind as Request;
 use proto::response::Result as Response;
@@ -20,6 +20,7 @@
 use tokio::sync::oneshot;
 use tokio_util::sync::CancellationToken;
 
+use super::exif;
 use super::transfer::{Call, upload_status};
 use crate::api::types::{CoreError, CoreEventPayload, Message, OutgoingFile, UploadPhase};
 use crate::client::{Client, unexpected};
@@ -335,6 +336,7 @@
             }
             (None, None) => return Err(invalid("a file needs a path or bytes")),
         };
+        let body = without_metadata(body).await?;
         let size = match &body {
             Body::Bytes(bytes) => bytes.len() as u64,
             Body::File(file) => file
@@ -368,6 +370,37 @@
     }
 }
 
+/// A photo of at most 100 MiB without its metadata (Task 20), read into
+/// memory to strip it. Anything else stays as it is.
+async fn without_metadata(body: Body) -> Result<Body, CoreError> {
+    let unreadable = |error: std::io::Error| invalid(format!("the file can't be read: {error}"));
+    let bytes = match body {
+        Body::Bytes(bytes) => bytes,
+        Body::File(mut file) => {
+            let size = file.metadata().await.map_err(unreadable)?.len();
+            let mut head = [0; 12];
+            let read = file.read(&mut head).await.map_err(unreadable)?;
+            if size > IMAGE_PROCESS_MAX_BYTES || !exif::is_photo(&head[..read]) {
+                return Ok(Body::File(file));
+            }
+            file.seek(SeekFrom::Start(0)).await.map_err(unreadable)?;
+            let mut bytes = Vec::with_capacity(size as usize);
+            file.read_to_end(&mut bytes).await.map_err(unreadable)?;
+            Bytes::from(bytes)
+        }
+    };
+    if bytes.len() as u64 > IMAGE_PROCESS_MAX_BYTES || !exif::is_photo(&bytes) {
+        return Ok(Body::Bytes(bytes));
+    }
+    let stripped = tokio::task::spawn_blocking(move || match exif::strip_metadata(&bytes) {
+        Some(stripped) => Bytes::from(stripped),
+        None => bytes,
+    })
+    .await
+    .map_err(|error| invalid(format!("the file can't be prepared: {error}")))?;
+    Ok(Body::Bytes(stripped))
+}
+
 /// Sends the file from the start. A piece whose answer is lost is sent
 /// again, and the server's 409 says where it really is.
 async fn send_pieces(
```

**`tests/attachments.rs`:**

```diff
--- a/crates/opencord-core/tests/attachments.rs
+++ b/crates/opencord-core/tests/attachments.rs
@@ -506,3 +506,62 @@
     owner.client.media_cache_clear().await.unwrap();
     assert_eq!(owner.client.media_cache_size().await.unwrap(), 0);
 }
+
+const SECRET: &[u8] = b"GPS 52.5200N 13.4050E";
+
+/// A JPEG whose Exif holds orientation 6 and a GPS position.
+fn photo_with_location() -> Vec<u8> {
+    let mut jpeg = Vec::new();
+    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
+        32,
+        16,
+        image::Rgb([200, 80, 40]),
+    ))
+    .write_to(
+        &mut std::io::Cursor::new(&mut jpeg),
+        image::ImageFormat::Jpeg,
+    )
+    .unwrap();
+    let mut exif = b"Exif\0\0II\x2A\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00".to_vec();
+    exif.extend_from_slice(SECRET);
+    let mut photo = jpeg[..2].to_vec();
+    photo.extend_from_slice(&[0xFF, 0xE1]);
+    photo.extend_from_slice(&u16::try_from(exif.len() + 2).unwrap().to_be_bytes());
+    photo.extend_from_slice(&exif);
+    photo.extend_from_slice(&jpeg[2..]);
+    photo
+}
+
+/// Task 20: the photo's GPS position never reaches the server, and it still
+/// shows the right way up.
+#[tokio::test(flavor = "multi_thread")]
+async fn a_photos_location_never_leaves_the_device() {
+    let server = TestServer::start().await;
+    let (owner, ready) = TestClient::owner(&server).await;
+    let key = server.address();
+    let dir = tempfile::tempdir().unwrap();
+    let path = dir.path().join("photo.jpg");
+    std::fs::write(&path, photo_with_location()).unwrap();
+
+    let message = owner
+        .client
+        .send_message_with_files(
+            &key,
+            general(&ready),
+            String::new(),
+            "n1".to_owned(),
+            vec![OutgoingFile {
+                path: Some(path.to_string_lossy().into_owned()),
+                bytes: None,
+                filename: String::new(),
+            }],
+        )
+        .await
+        .unwrap();
+
+    let attachment = &message.attachments[0];
+    let sent = download(&owner, &key, attachment.id).await;
+    assert!(!sent.windows(SECRET.len()).any(|window| window == SECRET));
+    // Orientation 6 turns the 32×16 photo on its side.
+    assert_eq!((attachment.width, attachment.height), (Some(16), Some(32)));
+}
```

- [ ] **Step 1:** Write the tests: `exif.rs`'s unit tests and `a_photos_location_never_leaves_the_device`.
- [ ] **Step 2:** Run → FAIL.
- [ ] **Step 3:** Make the changes. The API doesn't change, so don't regenerate.
- [ ] **Step 4:** Run → PASS:
  - `cargo test -p opencord_core --lib -- attachments::exif`: 7 tests;
  - `cargo test -p opencord_core --test attachments`: 12 tests;
  - clippy as before.
- [ ] **Step 5: Commit:** `feat: photos lose their metadata before upload (A4)`.

### Task 21: Clipboard images

**What it does.**
- `clipboard_image` returns the clipboard's image as PNG. It returns `None` when the clipboard holds text or nothing, so a paste of text stays a paste of text.
- `clipboard_set_image` decodes PNG, JPEG, GIF or WebP with the server's image limits and puts it on the clipboard. It backs "Copy image" in Task 24.
- **One `arboard::Clipboard` is kept for the app's life.** On Linux a copied image is served by this process and vanishes when its clipboard is dropped.
- Both run in `spawn_blocking` on the core's runtime (`on_blocking`), since frb's async functions don't run on Tokio.
- **Linux only for now.**
  - `arboard` 3.7.0 goes in the Linux section of `Cargo.toml`, with Wayland support.
  - On Windows, `arboard` brings in `clipboard-win` and `error-code`, both BSL-1.0, which the Global Constraints' licence list doesn't include. P1 settles Windows.
  - macOS is added in P2, on the Mac. Other systems return `None`, or an error for a copy.

**Files.**
- Create: `src/attachments/clipboard.rs`.
- Modify: `Cargo.toml`, `src/attachments/mod.rs`, `src/api/client.rs`, `src/api/attachments.rs`.

**`Cargo.toml`** (`image` moves from the dev-dependencies to the dependencies):

```diff
--- a/crates/opencord-core/Cargo.toml
+++ b/crates/opencord-core/Cargo.toml
@@ -18,6 +18,7 @@
 futures-util.workspace = true
 getrandom.workspace = true
 hex.workspace = true
+image.workspace = true
 opencord-common.workspace = true
 opencord-media.workspace = true
 opencord-proto.workspace = true
@@ -37,7 +38,6 @@
 webpki-roots.workspace = true
 
 [dev-dependencies]
-image.workspace = true
 opencord-server.workspace = true
 tempfile.workspace = true
 
@@ -58,6 +58,9 @@
 # GPU into Flutter's GL textures. EGL loaded at run time, like Flutter's.
 glow = "0.18.0"
 khronos-egl = { version = "6.0.0", features = ["dynamic"] }
+# Pasting and copying images (media uploads plan, Task 21), on X11 and
+# Wayland. macOS gets it in P2; on Windows it brings Boost-licensed crates.
+arboard = { version = "3.7.0", default-features = false, features = ["image-data", "wayland-data-control"] }
 
 [target.'cfg(target_os = "macos")'.dependencies]
 
```

**`src/attachments/clipboard.rs`** (new):

```rust
//! Images on the system clipboard (media uploads plan, Task 21): pasted into
//! the composer as PNG, and put there by "Copy image".

use crate::api::types::CoreError;

pub use system::{image, set_image};

#[cfg(target_os = "linux")]
mod system {
    use std::borrow::Cow;
    use std::io::Cursor;
    use std::sync::{Mutex, PoisonError};

    use arboard::{Clipboard, Error, ImageData};

    use super::{CoreError, failed};

    /// The same limits as the server's image processing.
    const MAX_SIDE: u32 = 16_384;
    const MAX_PIXELS: u64 = 50_000_000;
    const MAX_ALLOC: u64 = 512 * 1024 * 1024;

    /// Kept for the app's life: on Linux a copied image is served from this
    /// process, and goes when its clipboard is dropped.
    static CLIPBOARD: Mutex<Option<Clipboard>> = Mutex::new(None);

    fn with_clipboard<T>(
        work: impl FnOnce(&mut Clipboard) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let mut held = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
        let clipboard = match &mut *held {
            Some(clipboard) => clipboard,
            empty => empty.insert(Clipboard::new().map_err(failed)?),
        };
        work(clipboard)
    }

    /// The clipboard's image as PNG; `None` when it holds text or nothing.
    pub fn image() -> Result<Option<Vec<u8>>, CoreError> {
        with_clipboard(|clipboard| {
            if clipboard.get_text().is_ok_and(|text| !text.is_empty()) {
                return Ok(None);
            }
            let picture = match clipboard.get_image() {
                Ok(picture) => picture,
                Err(Error::ContentNotAvailable) => return Ok(None),
                Err(error) => return Err(failed(error)),
            };
            let width = u32::try_from(picture.width).map_err(failed)?;
            let height = u32::try_from(picture.height).map_err(failed)?;
            let rgba = image::RgbaImage::from_raw(width, height, picture.bytes.into_owned())
                .ok_or_else(|| failed("the clipboard's image has the wrong size"))?;
            let mut png = Vec::new();
            rgba.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(failed)?;
            Ok(Some(png))
        })
    }

    /// Puts a PNG, JPEG, GIF or WebP on the clipboard.
    pub fn set_image(bytes: &[u8]) -> Result<(), CoreError> {
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(MAX_ALLOC);
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(failed)?;
        reader.limits(limits);
        let picture = reader.decode().map_err(failed)?;
        if u64::from(picture.width()) * u64::from(picture.height()) > MAX_PIXELS {
            return Err(failed("the image is too large"));
        }
        let rgba = picture.to_rgba8();
        let data = ImageData {
            width: rgba.width() as usize,
            height: rgba.height() as usize,
            bytes: Cow::Owned(rgba.into_raw()),
        };
        with_clipboard(|clipboard| clipboard.set_image(data).map_err(failed))
    }
}

/// Systems without a clipboard backend yet: macOS gets one in P2, Windows
/// in P1.
#[cfg(not(target_os = "linux"))]
mod system {
    use super::CoreError;

    pub fn image() -> Result<Option<Vec<u8>>, CoreError> {
        Ok(None)
    }

    pub fn set_image(_bytes: &[u8]) -> Result<(), CoreError> {
        Err(super::failed("this system's clipboard isn't supported"))
    }
}

fn failed(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidInput {
        message: format!("the clipboard couldn't be used: {error}"),
    }
}
```

**`src/attachments/mod.rs`, `src/api/client.rs` and `src/api/attachments.rs`:**

```diff
--- a/crates/opencord-core/src/attachments/mod.rs
+++ b/crates/opencord-core/src/attachments/mod.rs
@@ -2,6 +2,7 @@
 //! downloading them, the encrypted cache, photo metadata and the clipboard.
 
 pub(crate) mod cache;
+pub mod clipboard;
 pub(crate) mod download;
 pub mod exif;
 pub(crate) mod transfer;
```

```diff
--- a/crates/opencord-core/src/api/client.rs
+++ b/crates/opencord-core/src/api/client.rs
@@ -1494,6 +1494,19 @@
         })?
 }
 
+/// Runs blocking `work` (the clipboard) on the core's runtime.
+pub(crate) async fn on_blocking<T: Send + 'static>(
+    work: impl FnOnce() -> Result<T, CoreError> + Send + 'static,
+) -> Result<T, CoreError> {
+    let runtime = RUNTIME.get().ok_or(CoreError::NotInitialized)?;
+    runtime
+        .spawn_blocking(work)
+        .await
+        .map_err(|error| CoreError::Connection {
+            message: error.to_string(),
+        })?
+}
+
 fn deliver(event: CoreEvent) {
     let mut events = lock_events();
     let undelivered = match &events.sink {
```

```diff
--- a/crates/opencord-core/src/api/attachments.rs
+++ b/crates/opencord-core/src/api/attachments.rs
@@ -3,11 +3,12 @@
 
 use flutter_rust_bridge::frb;
 
-use super::client::{client, on_runtime};
+use super::client::{client, on_blocking, on_runtime};
 use super::types::{
     AttachmentVariant, CoreError, Message, OutgoingFile, S3Config, StorageInfo, StorageTestResult,
     StorageTestTarget, UploadSettings,
 };
+use crate::attachments::clipboard;
 
 /// MANAGE_SERVER. `None` leaves a setting as it is.
 pub async fn update_upload_settings(
@@ -157,3 +158,13 @@
 pub async fn media_cache_clear() -> Result<(), CoreError> {
     on_runtime(move |client| async move { client.media_cache_clear().await }).await
 }
+
+/// The clipboard's image as PNG; `None` when it holds text or nothing.
+pub async fn clipboard_image() -> Result<Option<Vec<u8>>, CoreError> {
+    on_blocking(clipboard::image).await
+}
+
+/// Puts an image (PNG, JPEG, GIF or WebP) on the clipboard.
+pub async fn clipboard_set_image(bytes: Vec<u8>) -> Result<(), CoreError> {
+    on_blocking(move || clipboard::set_image(&bytes)).await
+}
```

- [ ] **Step 1:** Make the changes, then regenerate the bindings. There are no automated tests: the clipboard needs a desktop session. The live check is in A5.
- [ ] **Step 2:** Run → PASS: clippy as before, and `app/tool/check_platforms.sh` ("The client crates build off Linux.").
- [ ] **Step 3: Commit:** `feat: clipboard images in the core (A4)`.

**A4 stop.**
1. Run the full checks (Milestone stops). Also run `app/tool/check_platforms.sh`.
2. Write **D56** into `docs/decisions.md` from the text below, then report.

The decisions entry, copied into `docs/decisions.md` after D55 with `<commits>` replaced by the A4 commit range:

```markdown
## D56: Media uploads: the client core (Phase 4 A4, 2026-10-19)

- **Why.** Milestone A4 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) gives the client core what the app needs to send, show and save files. Commits: <commits>.
- **Types and requests.**
  - Messages carry their attachments, roles their upload limit, and Ready the upload settings. In RoleChanges, a limit of 0 removes a role's limit.
  - The core has the requests for upload settings and the owner's storage, and passes UploadSettingsUpdate and StorageUpdate to the app. CoreError gains Cancelled.
- **Transfers** (`attachments/transfer.rs`).
  - Uploads and downloads use one `reqwest` client per server, built with the gateway's own certificate check (`TlsTrust`), so a pinned certificate stays pinned. It is rebuilt when the server's route changes.
  - A changed certificate is FingerprintMismatch. A 401 renews the media token once, with the renewal now shared with `http.rs`.
  - Connecting may take 10 seconds, and 60 seconds without a byte ends a request. A piece may take 120 seconds, a status or finish 30, and a download has no overall limit.
- **Sending files** (`attachments/upload.rs`).
  - `send_message_with_files` takes 1 to 10 files, by path or by bytes. Two go up at a time, in the server's 8 MiB pieces, then SendMessage goes out with their ids in the files' order.
  - A network failure is tried again 1, 2, 4, 8 and 16 seconds later, six tries in all. A piece whose answer was lost goes again, and the server's 409 says where the upload really is.
  - Each file's verdict comes by its UploadReady or UploadFailed event, waited for from before the finish request. The status is also asked every 5 seconds, in case the event was lost.
  - UploadProgress follows each file: Preparing, Uploading (at most 10 a second), Processing, Sending, Done. On a failure the failing file is Failed and the others Cancelled.
  - `cancel_send` stops a send. After a cancel or a failure, every upload the send made is cancelled on the server.
- **Photo metadata** (`attachments/exif.rs`). A JPEG, PNG or WebP of at most 100 MiB loses its metadata before it leaves the device.
  - JPEG loses its Exif, XMP and IPTC segments. An orientation other than 1 comes back as a minimal Exif block, so the photo still shows the right way up.
  - PNG loses its eXIf, text and tIME chunks. WebP loses its EXIF and XMP chunks, their VP8X flags are cleared, and its RIFF size is rewritten.
  - A file that isn't well formed is sent as it is.
- **Downloads and the cache** (`attachments/{download,cache}.rs`).
  - `download_attachment` streams a file through `path.part` to `path`, with DownloadProgress; `cancel_download` stops it.
  - `attachment_bytes` reads a thumbnail, or an original of at most 64 MiB, for the image viewer and the voice and audio players. Thumbnails and originals up to 20 MiB are cached.
  - The cache is 1 GiB for every server, the least recently used going first. Each entry is sealed as a client-cache blob under a key the app keeps in the system keychain (`media_cache_key_store.dart`). The entry's name is a SHA-256 of server, attachment and variant, and its first 8 bytes are the blob id, so a file copied over another entry doesn't open. Without the keychain nothing is cached on disk.
- **Clipboard** (`attachments/clipboard.rs`).
  - `clipboard_image` pastes an image as PNG; with text on the clipboard it returns nothing. `clipboard_set_image` copies an image.
  - It uses `arboard` 3.7.0 on Linux, X11 and Wayland, with one clipboard kept for the app's life so a copied image stays.
  - macOS follows in P2. On Windows `arboard` needs BSL-1.0 crates, outside the allowed licences, so P1 settles Windows.
- **Tests.** `tests/attachments.rs` runs the core against a real server: sending, a lost piece answer, cancelling, a blocked file, an expired token, saving, the cache with the server stopped, and a photo's location. ServerHandle gains `uploads_in_state` for them.
```

---

# Milestone A5: The app (sending, viewing, settings)

### Task 22: Dart models, rules, repository, mock, tokens and icons

**Files:**
- Create: `app/lib/core/model/attachment.dart`, `app/lib/core/model/storage.dart`, `app/lib/core/upload_rules.dart`, `app/test/unit/model/upload_rules_test.dart`.
- Modify:
  - `core/model/message.dart` (`attachments`, `uploads`), `core/model/user.dart` (`Role.maxUploadBytes`), `core/model/snapshot.dart`, `core/providers/server_state.dart` (`uploadSettings`, `myUploadLimit`);
  - `core/repository/{repository,events}.dart`, `core/rust/{core_mapping,rust_repository}.dart`, `core/mock/{mock_repository,mock_world}.dart`, `core/format.dart` (`formatFileSize`);
  - `ui/theme/oc_colors.dart` (`voice`, `recording`), `tool/build_icons.dart` and the regenerated icons;
  - `docs/plans/desktop-ui.md` §1 (a one-paragraph note naming the two exceptions).

**Interfaces (Dart):**
- **`attachment.dart`:**
  - `enum AttachmentKind { file, image, video, audio, voiceMessage }`
  - `@immutable class Attachment` (fields as the Rust type, in camelCase, sizes as `int`)
  - `@immutable class UploadSettings { blockedExtensions, voiceMessageMaxSeconds, maxFileBytes, maxAttachments; static const fallback = UploadSettings(blockedExtensions: [], voiceMessageMaxSeconds: 300, maxFileBytes: 4294967296, maxAttachments: 10); }`
  - `enum UploadPhase { preparing, uploading, processing, sending, done, failed, cancelled }`
  - `@immutable class PendingUpload { final String name; final int size; final int sent; final UploadPhase phase; final String? error; final Uint8List? preview; }`
  - `@immutable class StagedFile { final String id; final String name; final int size; final String? path; final Uint8List? bytes; final Uint8List? preview; final StageProblem? problem; }`
  - `enum StageProblem { tooLarge, blocked, tooMany }`
- **`message.dart`:** `Message` gets `final List<Attachment> attachments` (default `const []`) and `final List<PendingUpload> uploads` (default `const []`), both in `copyWith`.
- **`upload_rules.dart`:** `int effectiveUploadLimit({required bool isOwner, required int everyone, required Iterable<int?> roles, required int ceiling})`, `String sanitizeFilename(String)`, `String? normalizeExtension(String)`, `bool isBlocked(String sanitizedName, List<String> blocked)`, `StageProblem? stageProblem({required String name, required int size, required int limit, required List<String> blocked})`. Same rules as Task 2.
- **`ServerData`:** `final UploadSettings uploadSettings;` and `int get myUploadLimit` (from `self` member roles plus @everyone via the existing role lookup).
- **`format.dart`:** `String formatFileSize(int bytes)`: `0 B`, `999 B`, `1.0 KB`, `12.5 MB`, `1.2 GB` (1 KB = 1024 bytes, one decimal from KB up, no decimal for whole numbers ≥ 100).
- **`OpencordRepository`** additions:

```dart
Future<Message> sendMessageWithFiles(String serverKey, int channelId, String content,
    {required String nonce, required List<StagedFile> files});
void cancelSend(String serverKey, String nonce);
Future<Uint8List> attachmentBytes(String serverKey, int attachmentId, {bool thumbnail = false});
Future<void> downloadAttachment(String serverKey, int attachmentId, String path);
void cancelDownload(String serverKey, int attachmentId);
Future<Uint8List?> clipboardImage();
Future<UploadSettings> updateUploadSettings(String serverKey, {List<String>? blockedExtensions, int? voiceMessageMaxSeconds});
Future<StorageInfo> storage(String serverKey);
Future<StorageInfo> addStorage(String serverKey, String name, S3Config s3);
Future<StorageInfo> updateStorage(String serverKey, int storageId, {String? name, S3Config? s3});
Future<StorageTestResult> testStorage(String serverKey, {int? storageId, S3Config? s3});
Future<StorageInfo> activateStorage(String serverKey, int storageId);
Future<StorageInfo> removeStorage(String serverKey, int storageId);
Future<StorageInfo> startStorageMove(String serverKey);
Future<StorageInfo> stopStorageMove(String serverKey);
Future<int> mediaCacheSize();
Future<void> clearMediaCache();
```

  - `updateRole`/`createRole` gain `int? maxUploadBytes` (0 clears).
- **The core API these call** (A4, Tasks 17–21; the generated Dart is in `app/lib/src/rust/api/attachments.dart`):
  - `send_message_with_files` takes a list of `OutgoingFile` (`path` or `bytes`, plus `filename`; an empty filename uses the path's);
  - `attachment_bytes` takes an `AttachmentVariant` (`original`, `thumbnail`);
  - `storage_test` takes a `StorageTestTarget`: `existing(storageId)` or `config(s3)`;
  - the other storage calls are `storage_get`, `storage_add`, `storage_update`, `storage_activate`, `storage_remove`, `storage_move_start` and `storage_move_stop`;
  - the cache calls are `media_cache_size` and `media_cache_clear`; the clipboard calls are `clipboard_image` and `clipboard_set_image`;
  - the core's `u64` values arrive as `BigInt`. Convert them the way `core_mapping.dart` already does.
- **Already done in A4; don't redo:** `core/rust/media_cache_key_store.dart` and its hook in `app_start.dart`; the optional `maxUploadBytes` on the generated `createRole`.
- **`events.dart`:** add `UploadProgressEvent`, `DownloadProgressEvent`, `UploadSettingsEvent`, `StorageEvent` to the existing event hierarchy.
  - Map them in `rust_repository.dart`, replacing the A4 placeholder that drops `CoreEventPayload_UploadSettingsUpdate`, `_StorageUpdate`, `_UploadProgress` and `_DownloadProgress`.
  - `UploadProgress` carries `channelId`, `nonce`, `fileIndex`, `sentBytes`, `totalBytes` and an `UploadPhase`, whose `failed` holds the message.
  - `DownloadProgress` carries `attachmentId`, `receivedBytes`, `totalBytes`, `done` and `error`.
- **Error kinds** (`RepoErrorKind` in `repository.dart`, mapped in `core_mapping.dart`). A4 left a placeholder there, "Task 22 gives them kinds of their own". Replace it:
  - add `fileTooLarge`, `fileTypeBlocked`, `voiceMessageInvalid`, `voiceMessageTooLong`, `storageUnavailable`, `storageFull` and `cancelled`, each with a doc comment, and map the matching `ErrorCode`s and `CoreError_Cancelled` to them;
  - `uploadOffsetMismatch` stays `conflict`, since the core resumes from it itself;
  - a cancelled send or download shows no error toast.
- **Mock:**
  - `MockRepository` implements everything in memory (fake progress with a short `Timer`; the cache size is 0).
  - `mock_world.dart` adds to an existing text channel: one message with a **grey** 640×360 PNG (generate the bytes in code with `dart:ui` or embed a tiny grey PNG as a base64 constant), one with a `report.pdf` file, and one voice message (waveform of 128 deterministic values, 7 s).
- **Colors:** `OcColors` gets `required this.voice, required this.recording` with the values in Global Constraints, plus `copyWith` and `lerp` entries and doc comments: "Voice messages only (the waveform). One of the app's two hues; see desktop-ui.md §1." and "The recording dot and the cancel state only."
- **Icons** (`tool/build_icons.dart`, Dart name → Material Symbols name):
  - `image` → `image`, `movie` → `movie`, `description` → `description`, `folderZip` → `folder_zip`, `article` → `article`;
  - `speed` → `speed`, `lockOpen` → `lock_open` (`volumeUp`, `volumeOff`, `upload`, `audioFile`, `check`, `close` and `error` exist already);
  - `fullscreen` → `fullscreen`, `zoomIn` → `zoom_in`, `chevronLeft` → `chevron_left`, `storage` → `storage`, `cloudUpload` → `cloud_upload`.

  Then run the icon build.

- [ ] **Step 1: Write tests:**
  - `upload_rules_test.dart` reads `File('../crates/opencord-common/tests/upload_rules.json')` and checks every case (the same table as Rust), plus `stageProblem` cases.
  - `format_test.dart` adds `formatFileSize` cases.
  - `mock_repository_test.dart` (in `test/unit/mock/`): `sendMessageWithFiles` emits progress and resolves with attachments.
- [ ] **Step 2:** `cd app && flutter test test/unit` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `flutter analyze && flutter test` → PASS. `monochrome_test.dart` must still pass with the new mock messages; if it fails, the mock image isn't grey.
- [ ] **Step 5: Commit:** `feat: app models, rules and repository for attachments (A5)`.

### Task 23: Composer: attaching, staging and sending

**Files:**
- Create: `app/lib/features/chat/attachments/staged_tiles.dart`, `attachments/upload_progress.dart`.
- Modify: `features/chat/composer.dart`, `features/chat/composer_state.dart` (`staged` list and add/remove/clear), `features/chat/attachment_drop.dart`, `core/providers/providers.dart` (the channel messages notifier: `sendWithFiles`, progress handling, retry, cancel), `core/providers/messages_state.dart` (`updatePendingUploads(state, nonce, fileIndex, PendingUpload)`).

**Behavior:**
- **Attach button** (exists; replace its toast):
  - tooltip `Attach files · up to ${formatFileSize(limit)} each`;
  - disabled with tooltip "You can't attach files here" without `attachFiles` in the channel;
  - opens `openFiles()` from `file_selector` (multi-select).
- **Drop zone:** overlay title `Drop to send to #${channelName}`, subtitle `Up to ${formatFileSize(limit)} each`. Dropping stages the files (paths from `desktop_drop`).
- **Paste:** in `_checkPaste` (composer), when the clipboard has no text, call `repository.clipboardImage()`; non-null → stage `pasted-image-YYYYMMDD-HHMMSS.png` (local time via `clockProvider`).
- **Staging:**
  - each file gets `stageProblem(...)`;
  - a file beyond 10 gets `tooMany`;
  - images (by extension `png/jpg/jpeg/gif/webp`) get a preview via `File.readAsBytes` when ≤ 20 MiB;
  - tiles are 72 px tall in a horizontal list above the field, each with: thumbnail or icon by extension, name (ellipsized in the middle), `formatFileSize`, a × button;
  - a problem tile has a 1 px `text` border, an `OcIcons.error` icon and the reason in `textSecondary`: "Larger than your ${limit} limit", "${ext} files are blocked here", "Only 10 files per message".
- **Send:**
  - disabled while any tile has a problem;
  - with staged files → `sendWithFiles(text, files)`: a pending `Message` with `uploads` = one `PendingUpload` per file (phase preparing), then `repository.sendMessageWithFiles`;
  - progress events whose nonce matches update `uploads`;
  - success → `resolvePending`; failure → `failPending` (the bubble's existing Retry re-sends the same files; Remove discards); the pending bubble's per-file × → `repository.cancelSend` then `removePending`;
  - clear the staged list right after the send starts.
- **Pending bubble:** below the text, each upload shows a row: icon, name, a 4 px progress bar (`text` on `border`), `sent / total`, and ×. Processing shows an indeterminate bar.

- [ ] **Step 1: Write widget tests** (`app/test/widget/chat/composer_attach_test.dart`, MockApp):
  - `staging_shows_tiles_and_blocks_send_on_problems` (a 60 MiB fake file → tooLarge text; Send disabled; remove it → enabled);
  - `pasting_an_image_stages_it` (fake clipboard image);
  - `sending_shows_progress_then_the_sent_message` (fake async progress);
  - `cancelling_a_pending_upload_removes_the_message`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS, plus `flutter analyze`.
- [ ] **Step 5: Commit:** `feat: attaching, staging and sending files (A5)`.

### Task 24: Attachments in messages

**Files:**
- Create: `app/lib/features/chat/attachments/{attachment_view.dart,image_grid.dart,file_card.dart,text_preview_dialog.dart,attachment_bytes.dart,download.dart,summary.dart}`.
- Modify: `features/chat/bubble.dart`, `features/chat/compact_message.dart` (render `AttachmentsView` below the markdown), the pins, reply and notification summary call sites (search for where message content is shortened to one line; use `summary.dart`).

**Interfaces:**
- `final attachmentBytesProvider = FutureProvider.autoDispose.family<Uint8List, ({String server, int id, bool thumbnail})>` calling `repository.attachmentBytes`.
- `Widget AttachmentsView({required String serverKey, required Message message})` dispatches by kind:
  - images → `ImageGrid`;
  - file and audio (audio until Task 35) → `FileCard`;
  - video → `FileCard` until Task 35;
  - voice → `VoiceBubble` (Task 31; until then `FileCard`).
- **`ImageGrid` layout:**
  - **1 image:** fit inside 400×300 keeping its stored `width`/`height` (fall back to the thumbnail size, then 4:3).
  - **2 images:** two equal cells side by side in a 400×200 box.
  - **3:** one 2/3 left cell and two stacked right cells (400×266).
  - **4+:** a 2×2 grid in 400×300; the 4th cell shows `+${n − 4}` over a `scrim` when there are more than 4.
  - Each cell shows the thumbnail with `BoxFit.cover`, radius `OcRadius.bubble`. The box size is fixed **before** loading, which is what keeps the scroll from jumping.
  - Animated images with `size ≤ 15 MiB` load the original instead of the thumbnail.
  - Loading → a `surface` box; error → an `OcIcons.error` box with "Couldn't load · Retry" (Retry invalidates the provider).
- **`FileCard`:**
  - Width 320, height 56. Icon by kind or extension (`image`, `movie`, `audioFile`, `folderZip` for zip/7z/rar/tar/gz, `article` for text, `description` otherwise).
  - Name (ellipsized in the middle), `formatFileSize`, a Download button.
  - A **Preview** button when the extension is one of `txt md log json csv xml yaml yml toml ini cfg conf rs py js ts dart c h cpp java go sh` and size ≤ 256 KiB.
- **`TextPreviewDialog`:** shows `utf8.decode(bytes, allowMalformed: true)` in `SelectableText` with a monospace style and a scroll view, max 900×640. **Never render HTML or markdown.**
- **`download.dart`:**
  - `Future<void> saveAttachment(BuildContext, WidgetRef, String serverKey, Attachment)`: `getSaveLocation(suggestedName: attachment.filename)`; null → return; otherwise `repository.downloadAttachment`.
  - Progress from `DownloadProgressEvent` shows on the card (a bar plus a cancel ×); done → toast "Saved ${filename}"; error → toast with the message.
- **Right-click menu on an attachment:** Download; Copy image (images only: the original bytes through `repository.copyImage(bytes)`, which calls the core's `clipboard_set_image`; add it to the repository and mock); Delete message (if allowed). **No link items.**
- **`summary.dart`:** `String messageSummary(Message m)`:
  - content non-empty → content;
  - one image → "Photo"; several → "${n} photos";
  - video → "Video";
  - voice → "Voice message (${m:ss})";
  - otherwise the first filename (+ " and ${n−1} more").

- [ ] **Step 1: Write tests:**
  - **Widget** `attachments_view_test.dart`: `one_image_reserves_its_box_before_loading` (the size equals 400×225 for a 1600×900 image before the future completes); `four_plus_images_show_the_counter`; `file_card_shows_preview_only_for_small_text`.
  - **Unit:** `message_summary_test.dart`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS.
- [ ] **Step 5: Commit:** `feat: images, files, previews and downloads in messages (A5)`.

### Task 25: The media viewer (images)

**Files:**
- Create: `app/lib/features/chat/attachments/media_viewer.dart`.
- Modify: `image_grid.dart` (tap opens the viewer).

**Interfaces:**
- `Future<void> showMediaViewer(BuildContext context, {required String serverKey, required List<Attachment> media, required int index})`.
- **Layout:**
  - a full-window route over `scrim`;
  - the top bar shows the filename, an index ("2 / 5"), Download and Close;
  - the image (original bytes; `InteractiveViewer` with min scale 1, max 8, wheel zoom through `Listener.onPointerSignal`);
  - ←/→ buttons and keys move through `media`; Esc closes.
  - Videos are a placeholder tile until Task 35.

- [ ] **Step 1: Widget test:** `viewer_navigates_and_closes` (arrow keys change the index text; Esc pops).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS.
- [ ] **Step 5: Commit:** `feat: the in-app media viewer (A5)`.

### Task 26: Settings pages

**Files:**
- Create: `app/lib/features/settings/server/{storage_page.dart,storage_dialogs.dart,uploads_page.dart}`.
- Modify: `features/settings/server_settings.dart` (pages), `features/settings/server/roles_page.dart` (limit field), `features/settings/user/app_pages.dart` (cache).

**Behavior:**
- **Uploads page** (`id: 'uploads'`, label "Uploads", icon `upload`, group "Server", shown with `manageServer`):
  - blocked extensions as removable chips, plus a text field (Enter adds, normalized with `normalizeExtension`; invalid shows an inline error);
  - a "Block programs" button that adds the preset (copy the Rust list into Dart as a constant);
  - voice maximum as a slider from 30 s to 30:00 in 30-second steps, labelled `m:ss`;
  - Save calls `updateUploadSettings`.
- **Storage page** (`id: 'storage'`, label "Storage", icon `storage`, group "Server", `data.isOwner` only):
  - **One row per backend:** name, kind, "Active" pill, `file_count` and `formatFileSize(bytes)`, `last_error` in `textSecondary`, and menu actions: Activate, Test, Edit (S3), Remove.
  - **"Connect a bucket"** opens `S3Dialog`:
    - a provider dropdown with presets that fill the fields:
      - AWS: endpoint `https://s3.{region}.amazonaws.com`, virtual-hosted;
      - Cloudflare R2: endpoint `https://{account}.r2.cloudflarestorage.com`, region `auto`, path style, with an "Account ID" helper field;
      - Backblaze B2: `https://s3.{region}.backblazeb2.com`;
      - Wasabi: `https://s3.{region}.wasabisys.com`;
      - MinIO / custom: path style;
    - fields: name, endpoint, region, bucket, key prefix, access key ID, secret (obscured; in edit mode empty means "keep"), path style, "Allow plain HTTP" (with a warning line);
    - **Test** shows each step with ✓/✗ (icons `check`/`close`, monochrome) and its detail;
    - **Save** is enabled only after a passing Test of the current field values.
  - **Move section:** when any backend other than the active one has files, "Move existing files here" starts the move; then a progress bar (files and bytes) with Stop; failures count shown.
  - Free temp space at the bottom: "Room for uploads in progress: X".
  - Listens to `StorageEvent`.
- **Roles page:** in the role editor, "Max upload size": a number field plus an MB/GB dropdown.
  - @everyone: required.
  - Others: a "Not set" checkbox.
  - Validation: ≥ 1 MB; ≤ `uploadSettings.maxFileBytes`; and, when not the owner, ≤ `data.myUploadLimit` ("You can't set more than your own limit (X)").
  - Saves through `updateRole(maxUploadBytes:)` (0 clears), sent **only when the field changed**. The server refuses any value above the editor's own limit, even one left as it was.
- **User settings** ("Windows & behavior", `BehaviorPage` in `app_pages.dart`): a "Media cache" section with one row with the current size (`mediaCacheSize`) and a "Clear" button.

- [ ] **Step 1: Write widget tests:**
  - `uploads_page_normalizes_and_saves`;
  - `storage_page_is_owner_only_and_requires_a_passing_test`;
  - `role_limit_field_validates_against_my_limit`;
  - `cache_row_clears`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS, plus the whole `flutter test`.
- [ ] **Step 5: Commit:** `feat: storage, uploads, role limit and cache settings (A5)`.

**A5 stop.** Full checks. Run the app against a local server (`OPENCORD_PROFILE` for two users) and try: attach, drop, paste, send, view, save, and the settings pages. Ledger, then **D57** ("Media uploads: the app (Phase 4 A5, <date>)"), written from `git diff` as `GEMINI.md` → "Write docs from the code" says, then report.

---

# Milestone A6: Voice messages

### Task 27: Audio engine hooks: capture tap and local media

**Files:**
- Modify: `crates/opencord-media/src/audio/processor.rs`, `crates/opencord-media/src/audio/engine.rs`.

**Interfaces:**
- `Processor::set_capture_tap(&mut self, tap: Option<rtrb::Producer<f32>>, silence_call: bool)`. In `capture()`, after the input volume is applied:
  - push the 480 tick samples into the tap (drop silently when full);
  - if `silence_call`, zero the tick **before** `self.capture.push` (the call gets silence; the level meter still sees the real level).
- `Processor::set_local_media(&mut self, source: Option<rtrb::Consumer<f32>>)` (interleaved stereo, 48 kHz):
  - in `play()`, right after `self.playback.tick_stereo(...)`, add up to `2 × TICK` samples popped from the source into `self.stereo` (missing samples = 0). The echo canceller's reference, computed from `self.stereo` afterwards, then includes them;
  - in `play_unheard`, pop and drop the same amount.
- `AudioEngine::set_capture_tap(&self, tap: Option<rtrb::Producer<f32>>, silence_call: bool)` and `AudioEngine::set_local_media(&self, source: Option<rtrb::Consumer<f32>>)`, through new `Command` variants.

- [ ] **Step 1: Write unit tests** (in `processor.rs` tests; reuse the module's helpers):
  - `the_tap_gets_processed_ticks_and_silence_reaches_the_call` (feed a sine; the tap receives 480-sample ticks; with `silence_call` every encoded frame decodes to energy < −60 dBFS);
  - `local_media_is_played_and_heard_by_the_echo_canceller` (output contains the samples; the AEC reference sees them; follow `the_sound_bob_shares_is_not_sent_again_through_his_microphone` and its helper `sent_while_sharing`, which check the reference for shared sound).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS (`cargo test -p opencord-media audio::processor`).
- [ ] **Step 5: Commit:** `feat: audio engine tap and local media mix (A6)`.

### Task 28: The voice recorder (core)

**Files:**
- Create: `crates/opencord-core/src/voice_message/{mod.rs,recorder.rs}`.
- Modify: `src/media.rs` (expose `pub(crate) fn call_engine(&self) -> Option<Arc<AudioEngine>>` and `pub(crate) fn audio_settings(&self) -> AudioSettings`), `src/api/types.rs`, `src/api/attachments.rs`, `src/client.rs`, bindings.

**Interfaces:**
- `pub struct VoiceDraft { pub draft_id: u64, pub duration_ms: u32, pub waveform: Vec<u8> }` (api type)
- `pub enum VoiceStopReason { MaxLength, DeviceLost }`
- `MediaEvent` += `VoiceRecordLevel { level: f32 }` (0..1, `waveform::level_unit` of the loudest 20 ms frame since the last event, at most every 33 ms) and `VoiceRecordStopped { reason: VoiceStopReason }`.
- **API:**
  - `pub fn voice_record_start(max_seconds: u32) -> Result<(), CoreError>` (`InvalidInput` if already recording)
  - `pub fn voice_record_stop() -> Result<VoiceDraft, CoreError>`
  - `pub fn voice_record_cancel()`
  - `pub fn voice_draft_discard(draft_id: u64)`
  - `pub async fn send_voice_message(server_key: String, channel_id: i64, nonce: String, draft_id: u64) -> Result<Message, CoreError>`: uses Task 18's manager, `attachments::upload::send(client, key, channel_id, content, nonce, files)`. It gains the upload kind as an argument: its per-file helper writes `kind: proto::AttachmentKind::File` into `CreateUpload` today. Send the draft as `OutgoingFile { path: None, bytes: Some(ogg), filename }`, kind VOICE_MESSAGE, filename `voice-message.ogg`, empty content; the draft is removed after success.
- **Recorder thread** (`opencord-voice-recorder`), with the input source chosen at start:
  - **In a call** (`call_engine()` is Some): `engine.set_capture_tap(Some(producer), true)`; the ticks are already processed. On stop or cancel, `set_capture_tap(None, false)`.
    - The recorder sees the engine only through `pub(crate) trait TapEngine: Send + Sync { fn set_capture_tap(&self, tap: Option<rtrb::Producer<f32>>, silence_call: bool); }`, implemented for `AudioEngine`, so tests can pass a fake.
  - **Otherwise:** `Microphone::open(settings.input_device.as_deref())` **on this thread**; `FromDevice::new(mic.rate, mic.channels)`; `VoiceProcessing::new(processing settings built the way Media::engine_settings builds them)`; each tick `processing.capture(&mut tick)`.
  - Every 2 ticks (20 ms): `opus::Encoder` (48 kHz, Mono, Voip; bitrate 32,000; VBR on; DTX off; mirror `opencord-media/src/audio/codec.rs`) `encode_float` → a packet.
  - Also keep each 10 ms tick's mean square (`Σx²/480`) in `tick_power: Vec<f32>`. At stop, the draft waveform is computed by this rule:
    1. split `tick_power` into 128 slices the way `waveform()` splits samples;
    2. `rms = sqrt(mean of the slice)` (an empty slice gives 0);
    3. `level_byte(rms)`.

    This equals the server's formula up to tick-sized slice edges and Opus loss. Memory stays at 4 bytes per 10 ms.
  - **Stop:**
    - `duration = frames × 20 ms`;
    - Ogg via `opencord_common::sound::ogg::write(1, pre_skip = encoder lookahead, &packets, samples)`; read `ogg::write`'s docs for what `samples` means;
    - store the draft in `drafts: Mutex<HashMap<u64, (Vec<u8>, VoiceDraft)>>` and return it.
  - Reaching `max_seconds` stops by itself and emits `VoiceRecordStopped{MaxLength}`; the draft is ready for `voice_record_stop()`, which returns it.
  - A lost device → `VoiceRecordStopped{DeviceLost}`.
- The voice panel status "Recording a voice message" comes from the app (Task 30) knowing that the recorder runs during a call; the core needs nothing more.

- [ ] **Step 1: Write tests** (core unit; inject a fake tick source: make the recorder's input a trait `TickSource` with `next_tick(&mut [f32; 480]) -> Option<()>`, implemented by the microphone path, the tap path and a test sine source):
  - `a_recording_becomes_valid_ogg_opus` (`OpusFile::read` succeeds, 48 kHz mono, duration ±20 ms of the input);
  - `the_draft_waveform_matches_the_server_formula` (2 s input: 1 s sine of amplitude 0.5, then 1 s silence; compare against `waveform()` over the same raw PCM: max difference ≤ 6 per value);
  - `recording_stops_at_the_maximum` (max 1 s, 2 s of input → duration 1000 ±20, `MaxLength` event);
  - `in_call_recording_silences_the_call` (with a real `AudioEngine` is too heavy; assert instead that `start` calls `set_capture_tap(Some, true)` on a fake engine trait object).
- [ ] **Step 2:** FAIL. **Step 3:** Implement and regenerate bindings. **Step 4:** PASS.
- [ ] **Step 5: Commit:** `feat: recording voice messages in the core (A6)`.

### Task 29: Voice playback (core)

**Files:**
- Create: `crates/opencord-core/src/voice_message/player.rs`, `crates/opencord-core/src/now_playing.rs`.
- Modify: `Cargo.toml` (`wsola = "0.2.0"`), `src/api/types.rs`, `src/api/attachments.rs`, bindings.

**Interfaces:**
- `pub enum PlaybackItem { Attachment { server_key: String, attachment_id: i64 }, Draft { draft_id: u64 } }` (api type)
- `MediaEvent` +=
  - `PlaybackUpdate { item: PlaybackItem, position_ms: u32, duration_ms: u32, playing: bool, buffering: bool }` (about 20 per second while playing, and on every state change)
  - `PlaybackEnded { item: PlaybackItem }`
  - `PlaybackFailed { item: PlaybackItem, message: String }`
- **`now_playing.rs`:**
  - `pub(crate) trait Stoppable { fn stop(&self); }`
  - `pub(crate) struct NowPlaying { current: Mutex<Option<Box<dyn Stoppable + Send>>> }` with `pub fn replace(&self, next: Box<dyn Stoppable + Send>)`, which calls `stop()` on the previous one first, and `pub fn clear(&self)`.
  - One instance lives on `Client`, shared by the voice player and the media player (Task 34).
- **API:**
  - `pub async fn voice_play(item: PlaybackItem, position_ms: u32, speed: f32) -> Result<(), CoreError>`: bytes from `attachment_bytes(Original)` (cached) or the draft.
  - `pub fn voice_pause()`, `pub fn voice_resume()`, `pub fn voice_seek(position_ms: u32)`, `pub fn voice_set_speed(speed: f32)` (1.0, 1.5 or 2.0 only, else InvalidInput), `pub fn voice_stop()`.
- **Player thread:**
  - `OpusFile::read` → the packet list.
  - Sample position of each packet via `opus::packet::get_nb_samples(packet, 48_000)`.
  - Seek to the packet containing `position − 80 ms`; decode from there, discarding samples until `position`.
  - Decode mono f32 → `wsola::TimeStretch` → output. Its API (0.2.0, MIT): `TimeStretch::new(NonZeroU32 rate, NonZeroU16 channels)`, `set_tempo(f32)`, `push(&[f32])`, `pull(max) -> impl Iterator<Item = f32>`, `flush()`, `reset()`. First verify in a test that tempo 2.0 halves the duration.
  - **Output:**
    - in a call: `engine.set_local_media(Some(consumer))` with interleaved stereo 48 kHz (duplicate mono), and `None` when done;
    - otherwise `Speaker::open(settings.output_device)` on the player thread with `ToDevice::new(rate, channels)`, applying the user's output volume.
  - `position_ms` counts **source** time (before stretching).

- [ ] **Step 1: Write unit tests:**
  - `tempo_two_halves_the_output` (wsola contract);
  - `seek_lands_within_a_frame` (seek to 1500 ms on a 3 s sine file; the first `PlaybackUpdate` position is 1500 ±20);
  - `starting_another_item_stops_the_first` (`NowPlaying`);
  - `a_draft_plays_without_network`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement and regenerate. **Step 4:** PASS.
- [ ] **Step 5: Commit:** `feat: voice message playback with speed (A6)`.

### Task 30: The recording bar (app)

**Files:**
- Create: `app/lib/features/chat/voice/{mic_button.dart,recording_bar.dart,waveform_painter.dart,voice_recorder_state.dart}`.
- Modify: `features/chat/composer.dart` (replace the toast mic with `MicButton`; swap the field for `RecordingBar` while recording), `core/repository/repository.dart` (+ rust and mock: `voiceRecordStart(int maxSeconds)`, `voiceRecordStop() -> VoiceDraft`, `voiceRecordCancel()`, `voiceDraftDiscard(int id)`, `sendVoiceMessage(server, channel, nonce, draftId)`, a stream of `VoiceRecordLevel`/`VoiceRecordStopped` from the existing media event wiring: `rust_repository.dart` listens with `_core.mediaEventStream().listen(_onMediaEvent)`), `core/model/permissions.dart` and `features/settings/server/permission_info.dart` (below), the voice panel (find the widget showing the call's own status; show "Recording a voice message" while recording in a call).

**Interfaces:**
- `enum RecordingPhase { idle, holding, cancelling, locked, preview }`
- `class VoiceRecorderState { RecordingPhase phase; Duration elapsed; List<double> bars; VoiceDraft? draft; }` with a Notifier `voiceRecorderProvider(ChannelRef)` exposing `press()`, `move(Offset delta)`, `release()`, `cancel()`, `lock()`, `stopForPreview()`, `send()`, `discard()`.
- **Gestures** (`MicButton`, a raw `Listener`, not `GestureDetector`):
  - pointer down → `press()`, which calls `voiceRecordStart(maxSeconds)` and sets the phase to holding;
  - moving: `dx < −120` → cancelling (`dx > −120` returns to holding); `dy < −60` → locked;
  - pointer up: holding → if elapsed < 500 ms, cancel and show the tooltip "Hold to record"; otherwise `send()`. Cancelling → `cancel()`. Locked → nothing.
  - Keyboard: with the mic focused, Space down/up behave the same.
  - Esc cancels while holding or locked.
  - The button is hidden without `sendVoiceMessages` in the channel, and shows only when the field is empty (as today).
  - The Dart model has no such permission yet. Add `static const sendVoiceMessages = Permissions(1 << 14)` (Rust's `SEND_VOICE_MESSAGES`), add the bit to `defaultEveryone` as Rust's `DEFAULT_EVERYONE` has it, and add a "Send voice messages" row after "Attach files" in `permission_info.dart`.
- **`RecordingBar`** (same height as the field):
  - left: a 10 px dot in `recording` (opacity pulsing 1.0 ↔ 0.4 every 600 ms) and the timer `m:ss`;
  - middle: the live waveform (`WaveformPainter.live`), one bar per 100 ms (the max level received in that window), 3 px bars with 2 px gaps, drawn in `voice`, the newest at the right, scrolling left;
  - right: the hint "Release to send · Esc to cancel" in `textMuted`.
  - **Cancelling:** the background becomes `recording` at 15% opacity (`withValues(alpha: 0.15)`), the dot is static, the hint "Release to cancel" is in `recording`.
  - **Locked:** the hint is replaced by buttons Delete (`delete`), Stop (`stop`) and Send (`send`, inverted). Enter = send, Esc = delete.
  - **Preview** (after Stop): play/pause plus the static waveform from `draft.waveform` (played part in `voice`) plus the duration, then Delete and Send.
  - `VoiceRecordStopped{maxLength}`: holding → send; locked → go to preview.
- **Sending:** a pending message with a local voice attachment (the draft waveform and duration) shows immediately; then `sendVoiceMessage`; success → resolvePending; failure → failPending (Retry re-sends the same draft).
  - Follow A5's file sending in `ChannelMessagesNotifier` (`providers.dart`): `sendWithFiles`, `_deliverWithFiles` and `retry` keep each message's files by nonce for Retry. Keep the draft id the same way, and call `voiceDraftDiscard` when the pending message is discarded or sent.
- **Icons:** `mic`, `stop`, `send`, `delete`, `pause` and `playArrow` exist; there is no `play`.

- [ ] **Step 1: Write widget tests** (`app/test/widget/chat/voice_recording_test.dart`, mock repository with a fake level stream):
  - `hold_shows_the_bar_and_release_sends`;
  - `a_short_press_shows_hold_to_record`;
  - `dragging_left_cancels`;
  - `dragging_up_locks_and_enter_sends`;
  - `max_length_while_locked_opens_the_preview`;
  - `no_permission_hides_the_mic`.
  - Plus a golden of the bar in both themes (`app/test/golden/voice_golden_test.dart` has the pattern).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS; `monochrome_test.dart` still passes (the bar isn't on the resting screen).
- [ ] **Step 5: Commit:** `feat: holding the mic to record a voice message (A6)`.

### Task 31: The voice bubble (app)

**Files:**
- Create: `app/lib/features/chat/voice/voice_bubble.dart`.
- Modify: `attachments/attachment_view.dart`, the repository (+ rust, mock: `voicePlay`, `voicePause`, `voiceResume`, `voiceSeek`, `voiceSetSpeed`, `voiceStop`, and `PlaybackUpdate`/`Ended`/`Failed` events), a `playbackProvider` (the current item and position).

**Behavior:**
- Width = `lerp(200, 320, clamp((durationSeconds − 10) / 50, 0, 1))`.
- Play/pause button.
- `WaveformPainter.static(values: attachment.waveform, progress: position / duration)`: resample the 128 values to `floor(width_for_bars / 5)` bars by max-pooling; played bars in `voice`, the rest in `textMuted`. At rest (progress 0) everything is `textMuted`, so resting screens stay monochrome.
- Time: idle `m:ss` of the total; playing `m:ss / m:ss`.
- A speed chip cycling 1× → 1.5× → 2× (`voiceSetSpeed` while playing; remembered per bubble).
- Tap or drag on the bars seeks.
- The right-click menu has Download: reuse `showAttachmentMenu` from `features/chat/attachments/download.dart`.
- `AttachmentsView` sends every non-image to `FileCard` today; route `voiceMessage` to `VoiceBubble`.

- [ ] **Step 1: Write tests:**
  - **Widget:** `voice_bubble_plays_seeks_and_changes_speed`;
  - **Unit:** `waveform_resampling_uses_max` (`[0, 255, 0, 0]` → 2 bars → `[255, 0]`).
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS.
- [ ] **Step 5: Commit:** `feat: voice message bubbles (A6)`.

**A6 stop.** Full checks. Live test on Linux with two app instances:
- record and send;
- play at 1.5× and 2×;
- record during a call: the other side hears silence, and the recording has no echo;
- play during a call on speakers: the other side doesn't hear it.

Then the ledger, then **D58** ("Media uploads: voice messages (Phase 4 A6, <date>)"), written from `git diff` as `GEMINI.md` → "Write docs from the code" says, then report.

---

# Milestone A7: Video and audio playback

### Task 32: FFmpeg with the players' decoders (Linux) and CI

**Files:** Modify `app/tool/build_ffmpeg.sh`, `.github/workflows/ci.yml`, `docs/packaging.md`.

- [ ] **Step 1: Build dav1d first** in the script:
  - pinned `dav1d_version=1.5.4`, `dav1d_sha256=686616b7c69eb88d44459391ab25cac13b6647a3b288835c5784e71c1514a5c5`, URL `https://downloads.videolan.org/pub/videolan/dav1d/$dav1d_version/dav1d-$dav1d_version.tar.xz` (fetched with the script's `fetch`);
  - build: `meson setup build --prefix="$prefix" --libdir=lib --buildtype=release --default-library=static -Denable_tools=false -Denable_tests=false && ninja -C build install`.
  - The script header must list meson and ninja as needs.
- [ ] **Step 2: Change FFmpeg's configure:**
  - add `--enable-libdav1d`;
  - decoders: `--enable-decoder=h264,mjpeg,hevc,vp8,vp9,libdav1d,mpeg4,aac,mp3float,opus,vorbis,flac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_u8`;
  - `--enable-demuxer=mov,matroska,ogg,mp3,flac,wav,aac`;
  - `--enable-parser=h264,hevc,vp8,vp9,av1,mpeg4video,aac,mpegaudio,opus,vorbis,flac`;
  - `--enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,vp9_superframe_split`;
  - keep everything else (network stays disabled: the player uses custom I/O).
  - Update the header comment.
- [ ] **Step 3: CI:** add `meson ninja-build` to the apt installs of both jobs that build FFmpeg. The cache key already hashes the script.
- [ ] **Step 4: Verify locally:** `app/tool/build_ffmpeg.sh target/ffmpeg && ls target/ffmpeg/lib/*.so*`. `ldd target/ffmpeg/lib/libavcodec.so.*` must show no `libdav1d.so` (it is linked statically).
- [ ] **Step 5:** `docs/packaging.md` Linux "Carried" line lists the new decoders and dav1d (BSD-2).
- [ ] **Step 6: Commit:** `build: FFmpeg with the media player's decoders and dav1d (A7)`.

### Task 33: The media player engine

**Files:**
- Create: `crates/opencord-media/src/player/{mod.rs,decode.rs,sync.rs,ffmpeg.rs,unsupported.rs}`.
- Modify: `crates/opencord-media/src/lib.rs` (`pub mod player;`, with `ffmpeg.rs` under `cfg(any(target_os = "linux", target_os = "windows"))` and `unsupported.rs` otherwise; macOS switches over in P2).

**Interfaces** (`opencord_media::player`):

```rust
pub trait MediaSource: std::io::Read + std::io::Seek + Send + 'static {}
impl<T: std::io::Read + std::io::Seek + Send + 'static> MediaSource for T {}
pub trait VideoOut: Send + Sync + 'static { fn present(&self, width: u32, height: u32, rgba: Vec<u8>); }
pub trait AudioOut: Send + 'static { fn write(&mut self, stereo_48k: &[f32]) -> usize; fn queued_frames(&self) -> usize; }
pub enum PlayerEvent { Opened { duration_ms: u32, width: u32, height: u32, has_video: bool, has_audio: bool },
    Position { position_ms: u32, buffering: bool }, Ended, Failed(String) }
pub struct MediaPlayer { /* command channel + thread */ }
impl MediaPlayer {
    pub fn open(source: Box<dyn MediaSource>, video: Option<Arc<dyn VideoOut>>, audio: Box<dyn AudioOut>,
        events: impl FnMut(PlayerEvent) + Send + 'static) -> Result<Self, PlayerError>;
    pub fn play(&self); pub fn pause(&self); pub fn seek(&self, position_ms: u32);
    pub fn set_volume(&self, volume: f32, muted: bool); pub fn close(self);
}
pub fn supported() -> bool; // false in unsupported.rs
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Probe { pub width: u32, pub height: u32, pub duration_ms: u32, pub has_video: bool }
pub fn probe_and_poster(path: &std::path::Path, max_side: u32) -> Result<(Probe, Option<Vec<u8>>), PlayerError>;
```

`probe_and_poster` returns `poster` = JPEG ≤ `max_side`.

**How it works:**
- **Opening:** `ffmpeg::format::input_from_stream_with_interrupt(ffmpeg::format::context::StreamIo::from_read_seek(source)?, move || cancelled.load(Ordering::Relaxed))`. Read the function in `~/.cargo/registry/src/*/ffmpeg-next-9.0.0/src/format/mod.rs`. Best video and audio streams.
- **Threads:** one thread per player for demux + decode, with decoders on frame threads (`decoder.set_threading`).
- **Video:**
  - frames → `software::scaling` to RGBA at the displayed size (≤ 1920 on the long side);
  - **rotation:** read the display matrix side data (`AV_PKT_DATA_DISPLAYMATRIX` / stream side data) and rotate 90, 180 or 270 when present;
  - queue up to 8 frames.
- **Audio:**
  - decoded frames converted in Rust (no swresample) from any of `u8`, `i16`, `i32`, `f32`, `f64`, planar or packed, to f32 stereo (mono duplicated, more than 2 channels → the first two);
  - resampled to 48 kHz with `rubato` (as `audio/convert.rs` does);
  - written to `AudioOut`.
- **Clock** (`sync.rs`): `audio_clock_ms = samples_written − queued_frames` at 48 kHz. Without audio, a monotonic clock from play start. Present a video frame when `pts ≤ clock + 10 ms`; drop frames later than `clock − 80 ms`.
- **Seek:** `input.seek(ts, ..ts)`; flush the decoders; drop frames before the target; Position update. The `buffering` flag is set while the source reader is waiting for network.
- **Volume:** applied to the samples before `AudioOut`.

- [ ] **Step 1: Write tests** (`crates/opencord-media/tests/player.rs`, run where FFmpeg exists; skip with an early return when `!supported()`).
  - **Fixtures:** commit three small files to `crates/opencord-media/tests/fixtures/`, made once with the system `ffmpeg` CLI (the app's own FFmpeg has no software encoders). Put these exact commands in `fixtures/README.md`:

```bash
ffmpeg -f lavfi -i testsrc=size=320x240:rate=25 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 2 -c:v libx264 -pix_fmt yuv420p -c:a aac -b:a 64k -movflags +faststart h264_aac.mp4
ffmpeg -f lavfi -i testsrc=size=320x240:rate=25 -f lavfi -i sine=frequency=440:sample_rate=48000 -t 2 -c:v libvpx-vp9 -b:v 200k -c:a libopus vp9_opus.webm
ffmpeg -display_rotation 90 -i h264_aac.mp4 -c copy rotated_90.mp4
```

  - Other crates' tests reach them with `concat!(env!("CARGO_MANIFEST_DIR"), "/../opencord-media/tests/fixtures/…")`.
  - `opens_and_reports_duration_and_size`;
  - `plays_to_the_end_in_real_time` (±15%, with a fake `AudioOut` consuming at 48 kHz);
  - `seeks_accurately` (seek 1000 → the next Position is within 50 ms);
  - `rotation_is_applied` (`rotated_90.mp4` presents a portrait frame);
  - `probe_and_poster_makes_a_jpeg`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS on Linux (with the Task 32 FFmpeg: `PKG_CONFIG_PATH=$PWD/target/ffmpeg/lib/pkgconfig`).
- [ ] **Step 5: Commit:** `feat: the media player engine (A7)`.

### Task 34: Player glue in the core, and video posters on upload

**Files:**
- Create: `crates/opencord-core/src/media_player.rs`, `crates/opencord-core/src/attachments/range_reader.rs`.
- Modify: `src/attachments/upload.rs` (probe videos), `src/api/attachments.rs`, `src/api/types.rs`, bindings.

**Interfaces:**
- **`RangeReader`** implements `Read + Seek`:
  - size from a HEAD-like first GET with `Range: bytes=0-0` (read `Content-Range` total);
  - 1 MiB blocks fetched through `Transfers` by blocking on the core runtime handle (`Handle::block_on` is allowed: this runs on the player thread, never on a runtime worker);
  - LRU of 8 blocks; reads ahead 2 blocks in the background.
- **API:**
  - `pub async fn media_open(server_key: String, attachment_id: i64) -> Result<MediaHandle, CoreError>` with `pub struct MediaHandle { pub texture_id: Option<i64>, pub duration_ms: u32, pub width: u32, pub height: u32, pub has_video: bool }`:
    - the texture comes from the core's existing `Textures` (`video.rs`), wrapping the `FrameSink` as `VideoOut`;
    - audio uses a `Speaker` outside calls, or the call engine's `set_local_media` in a call;
    - registers with `NowPlaying`.
  - `media_play()`, `media_pause()`, `media_seek(position_ms: u32)`, `media_set_volume(volume: f32, muted: bool)`, `media_close()`.
  - Events reuse `MediaEvent::PlaybackUpdate`/`Ended`/`Failed` with `PlaybackItem::Attachment`.
  - `#[frb(sync)] pub fn media_supported() -> bool`.
- **Upload:** for a path whose sniff is a video and where `media_supported()`, `probe_and_poster(path, 800)`. CreateUpload gets the hints; finish gets the poster.

- [ ] **Step 1: Write tests** (core integration):
  - `range_reader_reads_any_span` (random seeks equal the source bytes);
  - `video_uploads_get_hints_and_a_poster` (upload `h264_aac.mp4` → attachment width and height set, `has_thumbnail`).
- [ ] **Step 2:** FAIL. **Step 3:** Implement and regenerate. **Step 4:** PASS.
- [ ] **Step 5: Commit:** `feat: playing attachments through the media player (A7)`.

### Task 35: Video and audio in the app

**Files:**
- Create: `app/lib/features/chat/attachments/{video_tile.dart,video_player_view.dart,audio_card.dart}`.
- Modify: `attachment_view.dart`, `media_viewer.dart`, the repository (+ rust, mock).

**Behavior:**
- **`VideoTile`** in messages: the box from the stored width and height (same rules as one image, 400×300); the thumbnail (poster) or a `surface` box with `movie`; a centered play button; a duration badge `m:ss`. Tap opens the viewer at this item.
- **Viewer video:**
  - `mediaOpen` → `Texture(textureId: id)` sized to fit;
  - a control bar: play/pause, seek slider (drag → `mediaSeek`), `m:ss / m:ss`, volume slider plus mute;
  - Space toggles play; ←/→ still move between items; closing or switching calls `mediaClose`;
  - `!mediaSupported()` → a message "This video can't play on this computer yet" with Download.
- **`AudioCard`** (audio kind): the `FileCard` layout plus play/pause, a seek slider and the time, through the same media API (no texture).
- Starting any media stops a playing voice message, and vice versa (`NowPlaying` in the core; the app updates from events).

- [ ] **Step 1: Write widget tests** (mock repository): `video_tile_opens_the_viewer_and_plays`, `audio_card_plays_and_seeks`, `unsupported_video_offers_download`.
- [ ] **Step 2:** FAIL. **Step 3:** Implement. **Step 4:** PASS.
- [ ] **Step 5: Commit:** `feat: video and audio playback in the app (A7)`.

**A7 stop.** Full checks. Live: play an MP4, a WebM and a phone video (rotated) inside the app; seek; play during a call. **D59**, ledger, report.

---

# Milestone A8: Hardening and docs

### Task 36: Docs, deploy, security pass, performance

**Files:**
- Create `docs/attachments.md`.
- Modify `docs/protocol.md` (requests, events, HTTP endpoints with status codes), `docs/domains.md` (reverse proxies: nginx `client_max_body_size 16m;` in the Opencord location; Caddy needs nothing; Cloudflare's 100 MB request limit is why pieces are 8 MiB), `docs/packaging.md` (FFmpeg on all three systems once P2 lands), `README.md` (a short "Files and storage" paragraph linking `docs/attachments.md`), `docs/decisions.md`, `deploy/` (Docker comment: mount or secret for `secrets_file`; systemd: `LoadCredential=opencord-secrets.key:/etc/opencord/secrets.key` plus `OPENCORD_SECRETS_FILE=%d/opencord-secrets.key`).

**`docs/attachments.md` sections:**
1. **Where files go:** local storage, `local_dir`, S3 providers (one setup recipe each for AWS, R2, B2, Wasabi and MinIO, with this minimal IAM policy JSON):

```json
{"Version": "2012-10-17", "Statement": [{"Effect": "Allow", "Action": ["s3:PutObject", "s3:GetObject", "s3:DeleteObject", "s3:AbortMultipartUpload", "s3:ListMultipartUploadParts"], "Resource": "arn:aws:s3:::BUCKET/*"}, {"Effect": "Allow", "Action": ["s3:ListBucket"], "Resource": "arn:aws:s3:::BUCKET"}]}
```

2. **Encryption and `secrets.key`:** what is protected (a leaked bucket, a copied attachments folder, backups without the key) and what is not (a whole disk holding the key, unless `secrets_file` lives elsewhere); **back it up**; how to move it (stop the server, move the file, set `secrets_file`); the guard error and what it means.
3. **Limits and blocked extensions.**
4. **"Only in the app":** what it does (no links; authenticated, permission-checked serving; an encrypted app cache) and doesn't (a member can save a copy).
5. **Moving between storages.**
6. **Testing:** the S3 test server and command from the top of Milestone A3, and a manual checklist (the live checks listed at A5, A6 and A7).

**Steps:**
- [ ] **Step 1: Security pass.** Run the `security-review` checklist over the diff of `phase4-media` against `pre`. Fix every finding, adding a regression test for each. Must-checks:
  - no signed URL or secret in logs (grep `tracing::` calls in `attachments/`);
  - every media endpoint authorizes and checks permissions;
  - body limits on every upload read;
  - filenames in `Content-Disposition` percent-encoded;
  - owner checks on all 8 storage requests;
  - image limits;
  - the `local.rs` key parsing.
- [ ] **Step 2: Performance.**
  - Upload and download a 1 GiB file to a local server: throughput ≥ 80 MB/s on loopback, server RSS growth < 100 MB during the transfer.
  - Scrolling a channel with 200 image messages stays at 60 fps in profile mode.
  - Record the results in `docs/performance.md`.
- [ ] **Step 3:** Write the docs. Add `cmake` to the Docker build stage if Task 9 didn't.
- [ ] **Step 4:** Full checks, plus the S3 tests (Milestone A3) and `app/tool/check_platforms.sh`.
- [ ] **Step 5: Commit:** `docs: attachments, storage and voice messages; hardening (A8)`.

**A8 stop.** Write **D60** (summary and measured numbers) and report.

---

# P1: Windows (on the Windows PC; follow `docs/plans/platforms.md` for branches)

Branch `phase4-windows` from `phase4-media`.
1. Check that the shipped FFmpeg DLLs contain the decoders and demuxers from Task 32 (`ffmpeg -decoders` from the same build, or the build's configure line), including `libdav1d`. If any is missing, switch to an LGPL shared build that has them and update `docs/packaging.md` (Windows).
2. `cargo test --workspace --lib --bins --tests`, `flutter test`, then every live check from A5, A6 and A7.
3. Fix Windows-only problems in Windows files only. Shared fixes go back through the Linux session.
4. The clipboard (Task 21). `arboard` on Windows needs `clipboard-win` and `error-code`, both BSL-1.0, which the licence list doesn't include. Ask the user: allow BSL-1.0 (then add `arboard` to the Windows section of `crates/opencord-core/Cargo.toml` and widen `clipboard.rs`'s `cfg`), or write `image` and `set_image` for Windows with `windows-sys`. Then check pasting and "Copy image".
5. Ledger, report, push the branch when asked.

# P2: macOS (on the Mac)

Branch `phase4-macos` from `phase4-media`.
1. Build a decode-only universal FFmpeg:
   - a new `app/tool/macos/build_ffmpeg.sh`, the same flags as Task 32 plus `--enable-videotoolbox` and dav1d;
   - built for arm64 and x86_64, then merged with `lipo`;
   - the dylibs go in `Opencord.app/Contents/Frameworks` with `install_name_tool` ids `@rpath/…`, signed with the app.
2. `crates/opencord-media/Cargo.toml` `[target.'cfg(target_os = "macos")'.dependencies]` gains `ffmpeg-next.workspace = true`; `player`'s cfg includes macOS.
3. Write the decision entry superseding D34's "no FFmpeg on macOS" for playback only; calls keep VideoToolbox.
4. Microphone permission: the existing `NSMicrophoneUsageDescription` covers voice messages. Check the first-record prompt.
5. The clipboard (Task 21): add `arboard = { version = "3.7.0", default-features = false, features = ["image-data"] }` to the macOS section of `crates/opencord-core/Cargo.toml` and widen `clipboard.rs`'s `cfg` to `any(target_os = "linux", target_os = "macos")`. Check pasting and "Copy image".
6. Tests and every live check from A5, A6 and A7. Ledger, report.
