# Private Messages, Friends and Offline Delivery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. If your harness has neither skill, work through the tasks in order yourself and tick the boxes as you go.

**Goal:** People can add friends from profiles, and send each other end-to-end encrypted private messages that wait on a shared server while the other person is offline. It all lives behind a Private messages button at the top of the server rail.

**Architecture:**
- **Servers** relay and never read. Each routed event (a message copy, a friend request, an answer) is stored once per target device until that device acknowledges it. Privacy settings and blocks are synced to every server, and the server enforces them. Permission slips (signatures) let any server check "may A message B" without keeping friend lists.
- **The client core** encrypts with vodozemac (Olm: the Double Ratchet with a triple Diffie-Hellman start). It encrypts once per device of both people, keeps everything in its own encrypted SQLite database, and processes deliveries one at a time in a single worker. It saves before sending and before acknowledging.
- **The Flutter app** adds the home view (conversations, Friends page), the conversation view, profile buttons, a Privacy page, and notifications.

**Tech Stack:**
- Rust 2024: axum 0.8, sqlx 0.9 (SQLite; the server uses compile-time-checked queries, the core runtime queries), prost/protox, vodozemac 0.11.1, chacha20poly1305 0.11, hmac 0.13, ed25519-dalek 3.
- Flutter: Riverpod 3, flutter_rust_bridge 2.13, flutter_secure_storage.

**Spec:** `docs/superpowers/specs/2026-10-22-private-messages-design.md`.
- Read it fully before Task 1. The spec says *what* and *why*; this plan says *where* and *how*.
- If the two disagree, or a step is impossible as written, stop and ask the user. Do not improvise.

## Global Constraints

- **Platforms:** Linux, Windows and macOS desktops. Built and tested on Linux. `app/tool/check_platforms.sh` checks the rest from Linux, and CI builds them natively.
- **Branch:** create `private-messages` from `beta` (`git switch -c private-messages beta`). Commit there after every task. Never commit to `beta`, `canary` or `main`. Push only when the user asks.
- **Decisions:** one entry per milestone in `docs/decisions.md`, **D61** (E0) to **D66** (E5), in the existing style.
- **Encryption:** vodozemac **0.11.1**, `SessionConfig::version_1()` everywhere. Never write ratchet, key agreement or message encryption code yourself.
- **Signing payloads** (exact bytes; the context strings have no terminator):
  - device: `b"opencord-device-v1"` (18) ‖ device id (16) ‖ Curve25519 key (32) ‖ Ed25519 key (32) = **98 bytes**, signed by the person's identity key;
  - prekey: `b"opencord-prekey-v1"` (18) ‖ device id (16) ‖ key (32) ‖ fallback (1 byte, `0` or `1`) = **67 bytes**, signed by the device's Ed25519 key;
  - permission slip: `b"opencord-dm-allow-v1"` (20) ‖ granter's public key (32) ‖ grantee's public key (32) = **84 bytes**, signed by the granter's identity key.
  - Verify with ed25519-dalek's `verify_strict`. Every length is checked first; a wrong length is "doesn't verify", never a panic.
- **Sizes and counts:**
  - device id 16 bytes, send id 16 bytes, message id 16 bytes;
  - **10** devices per person per server (the one seen longest ago is dropped beyond that);
  - **20** unclaimed one-time keys kept per device per server (the client tops up), at most **100** stored per device;
  - **20** encrypted copies per message, each **1–32,768** bytes;
  - `FetchDevices` **1–2** user ids; `ClaimKeys` **1–20** devices; `AckPendingEvents` at most **500** ids;
  - a pending page: **100** events or **786,432** bytes (768 KiB) of encoded events, whichever comes first, always at least one;
  - message text **1–4,000** characters (`validation::message_content`);
  - at most **100** outgoing pending friend requests per sender per server; at most **10,000** blocked ids in one `UpdatePrivacy`.
- **Rates:** friend requests **10 per 600 s** per sender; private messages **5 per 5 s** per (sender, recipient); key claims **30 per 60 s** per user. Everything else counts against the existing request rate.
- **Times:**
  - pending events live **30 days** (`undelivered_days`); devices unseen for **60 days** are dropped (`device_expiry_days`); recent send ids last **24 h**; the server sweeps every **1 h**;
  - the fallback key rotates every **30 days**;
  - device lists are cached **10 minutes**;
  - unsent messages are resent for up to **24 h** after a restart;
  - received-event records are kept **35 days**;
  - acknowledgements are batched at most **200 ms**;
  - at most **one session reset per peer device per hour**;
  - typing is sent at most every **8 s** (the channel rule).
- **Config:** `[private_messages]` with `enabled = true`, `undelivered_days = 30` (1–365), `max_pending_per_device = 5000` (100–100,000) and `device_expiry_days = 60` (7–365). Environment overrides: `OPENCORD_PRIVATE_ENABLED`, `OPENCORD_PRIVATE_UNDELIVERED_DAYS`, `OPENCORD_PRIVATE_MAX_PENDING_PER_DEVICE` and `OPENCORD_PRIVATE_DEVICE_EXPIRY_DAYS`.
- **Spam:** an incoming request from someone whose requests this device declined **3** or more times.
- **Never log** plaintext, keys, pickles, slips or ciphertext. Log ids, counts and error kinds.
- **Colors:** monochrome, as everywhere. `voice` and `recording` stay the only hues; `monochrome_test.dart` and `hardcoded_colors_test.dart` keep passing unchanged.
- **Licences:** server code AGPL-3.0-or-later; `opencord-common`, `opencord-core`, `opencord-proto`, `proto/` and `app/` MPL-2.0. New dependencies must be MIT, Apache-2.0, BSD, ISC or Zlib. vodozemac 0.11.1 and its tree are already checked (spec §3); run the check in `GEMINI.md` anyway when adding it.

## Working Rules (read before Task 1)

### Environment

Each shell command runs in a fresh shell. Start **every** `cargo`, `flutter`, `dart`, `sqlx` and `flutter_rust_bridge_codegen` command with `source .superpowers/sdd/media/env.sh &&` (that path stays; `GEMINI.md` gives its contents). Check `df -h /` before the full checks; `GEMINI.md` says what to delete when space runs short. Never run `cargo clean` without asking.

### Commands

| What | Command |
|---|---|
| Format | `cargo fmt --all` and `cd app && dart format lib test tool` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust tests | `cargo test --workspace --lib --bins --tests --no-fail-fast` |
| Server private tests | `cargo test -p opencord-server --test private` (one: add the test name) |
| Core private tests | `cargo test -p opencord_core --test private` (the core's package name has an underscore) |
| One unit test | `cargo test -p <crate> --lib -- --exact <module::path::test_name>` |
| Off-Linux type check | `app/tool/check_platforms.sh` |
| Flutter | `cd app && flutter analyze && flutter test` |
| sqlx data (after **any** server query or migration change) | see below |
| Dart bindings (after **any** change under `crates/opencord-core/src/api/`) | `cd app && flutter_rust_bridge_codegen generate` |

**sqlx data**, for the server only:

```bash
export DATABASE_URL="sqlite://$PWD/target/sqlx-dev.db"
rm -f target/sqlx-dev.db
sqlx database create
sqlx migrate run --source crates/opencord-server/migrations
cargo sqlx prepare --workspace -- --all-targets
```

Commit the changed `.sqlx/*.json` with the task. CI builds with `SQLX_OFFLINE=true`.

**The core's database uses runtime queries** (`sqlx::query`, `sqlx::query_as` with `.bind(..)`), never the `query!` macros. The offline data only covers the server's schema.

### Generated files: never edit by hand

`app/lib/src/rust/**`, `crates/opencord-core/src/frb_generated.rs`, `*.freezed.dart`, `.sqlx/*.json`, `app/lib/ui/theme/oc_icons.dart` and the icon font. This plan needs no new icon: it uses `chat`, `group`, `mail`, `block`, `lock`, `personAdd`, `personRemove`, `check`, `close`, `moreHoriz`, `notificationsOff` and `fingerprint`.

### Task owners

Every task has an **Owner** and a **Score** (1–5, how much a mistake costs: cryptography, delivery guarantees, concurrency and data loss score 4–5).

- **Owner: Gemini.** You implement it. Score 3 means read its **Review focus** line twice: that's what the review checks hardest.
- **Owner: Claude.** Don't implement it. Hand the task to Claude, naming the task number and this plan's path, and wait for its report. Don't edit files or run git while it works: you share the working copy.
  - Then read the report, run the task's **Checks** yourself, and record the task in the ledger with the report's commits.
  - Claude's tasks come first in their milestone, so yours build on them.
  - Each Claude task is written to be done in one sitting, from this section alone.
- **Claude, when handed a task:** follow the task exactly. Run its checks; finish with one commit per task using the message given, then report the files, commits, checks and anything left undone.

### Rules reviews will enforce

`GEMINI.md` → "How to write code here" applies everywhere. Read it before each milestone. These come on top:

1. **No `unwrap()`/`expect()` in non-test code**, except where the codebase already does the same (fixed-size key conversions after a length check are fine as `try_into()` with an error).
2. **Never hold `state.guild()`, `state.guild_mut()` or any `std::sync` lock across `.await`.** Copy what you need out of the guard in a block, then await.
3. **Server writes:** write the database first, then update memory, then push events.
4. **Proto field numbers are permanent.** Add only those Task 1 gives.
5. **Every new or changed server `sqlx::query!`** needs the sqlx data regenerated in the same commit.
6. **Every new public function under `crates/opencord-core/src/api/`** needs the bindings regenerated in the same commit (that only happens in E4). Before E4, the core's private API is `Client` methods, used by the core's tests.
7. **`DEFAULT_CONFIG_TOML` must equal `Config::default()`** (`default_file_matches_default_config`). Don't edit the root `opencord.toml`.
8. **Dart:** every new `OpencordRepository` method is implemented in both `RustRepository` and `MockRepository`. No hard-coded colors outside `oc_colors.dart`. Mock images stay grey.
9. **Errors to users** are the spec's sentences (§6.7), shown with the person's name. The server's own `Error.message` is for logs; the app never shows it for the private codes.
10. **Tests first:** write the failing test, run it, and see it fail for the right reason. Never weaken or delete an existing test.
11. **One crypto lock in the core:** the device (`tokio::sync::Mutex<Device>`, Task 11) is held from loading sessions until their transaction commits, in both the send and the receive pipelines. It is never held across a network request.
12. **Save before you send, save before you acknowledge.**
    - A send saves the advanced sessions and the encrypted request (the outbox) in one transaction before it goes out.
    - A delivery is acknowledged only after its transaction commits.
13. **Every in-memory map's task says when its entries leave.** Implement exactly that, and test it where the task says.

### Progress ledger

Keep `.superpowers/sdd/private/progress.md`. After each task, append:

```
## Task N — <title> (owner: <Gemini|Claude>)
- Commit: <hash>
- Tests added: <test names>
- Commands run: <command> → <result summary>
- Deviations from the plan: <none | what and why>
- Left undone / doubts: <none | list>
```

What counts as a deviation is in `GEMINI.md` → "Reports and the ledger".

### When a step keeps failing

Follow `GEMINI.md` → "When you're stuck: ask Claude", with the question files in `.superpowers/sdd/private/questions/`.

### Milestone stops

At the end of each milestone (E0 … E5):
1. run the full checks (format, clippy, Rust tests with `--no-fail-fast`, the platform check, flutter analyze and test);
2. write the decisions entry from `git diff <first commit of the milestone>^..HEAD`, as `GEMINI.md` → "Write docs from the code" says, with **Why**, a bullet per part and **Tests**;
3. **stop and report** to the user. Continue only when the user releases the next milestone.

## Review Focus

These are the inputs most likely to break for real people while no ordinary test would catch them. Each has a pinned test in the task named.

1. **The same delivery twice:** a resume replay, a catch-up page, and a push after a lost acknowledgement. Expected: one message shown, never "couldn't be decrypted". Pinned in Task 13 (`a_message_delivered_twice_shows_once`).
2. **The app killed between decrypting and saving.** Expected: the next delivery decrypts and shows it. Pinned in Task 13 (`a_crash_before_saving_decrypts_on_redelivery`).
3. **A lost answer to a send:** the server stored it, the client timed out and resends. Expected: one copy for the recipient. Pinned in Task 6 (`a_repeated_send_id_stores_nothing`) and Task 12 (`a_send_retried_after_a_lost_answer_arrives_once`).
4. **The recipient adds a device between "fetch devices" and "send".** Expected: a refusal, then a re-encrypted send that every device gets. Pinned in Task 6 (`a_stale_device_set_is_refused`) and Task 12 (`a_device_added_meanwhile_gets_the_message_after_the_refusal`).
5. **Thousands of events waiting for a device that comes back** (more than a page, and more than 1 MiB). Expected: all arrive, in order, without the connection being closed. Pinned in Task 5 (`catch_up_pages_by_count_and_by_bytes`), Task 11 (`a_device_back_after_many_events_acknowledges_them_all`) and Task 13 (`messages_wait_for_an_offline_device`, for the order).

## File Map

### Protocol and shared

| File | Status | Contents |
|---|---|---|
| `proto/opencord/v1/private.proto` | new | private messages, friends, pending events, the encrypted payload |
| `proto/opencord/v1/{requests,events,gateway,models}.proto` | modify | oneof entries, `Ready.private_messages`, error codes |
| `crates/opencord-proto/build.rs` | modify | compile `private.proto` |
| `crates/opencord-proto/tests/private.rs` | new | round trips, pinned field numbers |
| `crates/opencord-common/src/private.rs` | new | signing payloads, sign and verify, limits |
| `crates/opencord-common/src/limits.rs` | modify | three rate limits |

### Server (`crates/opencord-server/`)

| File | Status | Contents |
|---|---|---|
| `migrations/0010_private_messages.sql` | new | schema |
| `src/private/mod.rs` | new | `PrivateMessages` service: privacy cache, pushing, delivering, sweeping, forgetting a member |
| `src/db/private.rs` | new | queries |
| `src/handlers/private.rs` | new | devices, keys, pending events, privacy, messages, typing |
| `src/handlers/friends.rs` | new | friend requests, answers, cancellations, removals |
| existing | modify | `config.rs`, `state.rs`, `error.rs`, `rate_limit.rs`, `gateway/session.rs`, `gateway/identify.rs`, `handlers/{mod,members}.rs`, `db/mod.rs`, `server.rs`, `lib.rs`, `tests/common/mod.rs` |
| `tests/private.rs` | new | everything above, against a real server |

### Client core (`crates/opencord-core/`)

| File | Status | Contents |
|---|---|---|
| `migrations/0001_private.sql` | new | the private database |
| `src/private/{mod,store,seal,device,sessions,contacts,members,service,outbox,inbox,actions}.rs` | new | see each task |
| `src/api/private.rs` | new (E4) | Dart-facing functions |
| existing | modify | `Cargo.toml`, `lib.rs`, `identity.rs`, `client.rs`, `connection.rs`, `convert.rs`, `api/{mod,types}.rs` |
| `tests/private.rs` | new | several clients against real servers |

### App (`app/lib/`)

| File | Status | Contents |
|---|---|---|
| `core/model/private.dart` | new | models |
| `core/rust/private_key_store.dart`, `core/rust/private_mapping.dart` | new | keychain key; core ↔ app mapping |
| `core/mock/mock_private.dart` | new | mock contacts and messages |
| `core/providers/private_state.dart` | new | contacts, conversations, typing, names, presence, shared servers, unread |
| `features/home/*.dart` | new | home sidebar, Friends page, conversation, header, divider, move card, request bar, error dialogs |
| `features/settings/user/privacy_page.dart` | new | Privacy page |
| existing | modify | `app_start.dart`, `core/repository/{repository,events}.dart`, `core/rust/{core_api,rust_repository,core_mapping}.dart`, `core/mock/mock_repository.dart`, `core/providers/providers.dart`, `features/servers/server_rail.dart`, `features/shell/{navigation,desktop_shell}.dart`, `features/members/{member_profile,member_actions}.dart`, `features/settings/user_settings.dart`, `features/desktop/{notification_binding,tray_binding}.dart`, `features/links/app_links.dart` |

### Docs

`docs/private-messages.md` (new), `docs/protocol.md`, `docs/decisions.md`, `README.md`, `docs/plans/desktop-ui.md` §4.1.

---

# Milestone E0: Foundations

### Task 1: Protocol messages

**Owner:** Gemini. **Score:** 2.

**Files:**
- Create: `proto/opencord/v1/private.proto`, `crates/opencord-proto/tests/private.rs`
- Modify: `proto/opencord/v1/requests.proto`, `events.proto`, `gateway.proto`, `models.proto`, `crates/opencord-proto/build.rs`, `crates/opencord-core/src/api/types.rs` (`ErrorCode`), `crates/opencord-core/src/convert.rs` (`error_code`), `app/lib/core/repository/repository.dart` (`RepoErrorKind`), `app/lib/core/rust/core_mapping.dart` (`errorFrom`)

**Interfaces:**
- Produces: every proto message below (Rust: `opencord_proto::v1::*`); `ErrorCode::{FriendRequestsClosed, MessageRequestsClosed, Undeliverable, DeviceListChanged, PrivateMessagesUnavailable}` in the core; `RepoErrorKind.{friendRequestsClosed, messageRequestsClosed, undeliverable, privateMessagesUnavailable}` in Dart.

- [ ] **Step 1: Write the failing test** `crates/opencord-proto/tests/private.rs`:
  - `field_numbers_are_the_planned_ones`. Assert these exact encodings:
    - `Request { kind: Some(RegisterDevice(Default::default())) }` → `[0xA2, 0x0B, 0x00]` (field 180);
    - `Event { kind: Some(PendingEvent(Default::default())) }` → `[0xC2, 0x07, 0x00]` (field 120);
    - `Response { result: Some(DeviceRegistration(Default::default())) }` → `[0x92, 0x02, 0x00]` (field 34);
    - a `Ready` with only `private_messages: true` → `[0xA8, 0x01, 0x01]` (field 21);
    - `ErrorCode::FriendRequestsClosed as i32 == 48` through `PrivateMessagesUnavailable as i32 == 52`.
  - `pending_events_and_payloads_round_trip`: one `PendingEvent` per `kind` variant and one `PrivatePayload` per `body` variant, each with non-default fields, encode then decode equal.
- [ ] **Step 2: Run it:** `cargo test -p opencord-proto --test private`. Expect: compile errors (the types don't exist).
- [ ] **Step 3: Write `proto/opencord/v1/private.proto` exactly as follows**, and add `"opencord/v1/private.proto"` to `PROTO_FILES` in `build.rs` (the array becomes `[&str; 10]`):

```proto
// SPDX-License-Identifier: MPL-2.0
syntax = "proto3";

package opencord.v1;

import "opencord/v1/models.proto";

// Private messages, friends and offline delivery (design 2026-10-22).

// A device's long-term keys, signed by its owner's identity key over
// "opencord-device-v1" || device_id || curve25519 || ed25519.
message DeviceKeys {
  // 16 random bytes, chosen by the device.
  bytes device_id = 1;
  // 32 bytes each.
  bytes curve25519 = 2;
  bytes ed25519 = 3;
  // 64 bytes, by the user's identity key.
  bytes signature = 4;
}

// A one-time key or a fallback key (32 bytes), signed by the device's
// Ed25519 key over "opencord-prekey-v1" || device_id || key || fallback.
message Prekey {
  bytes key = 1;
  bytes signature = 2;
}

message UserDevices {
  int64 user_id = 1;
  repeated DeviceKeys devices = 2;
}

message DeviceRef {
  int64 user_id = 1;
  bytes device_id = 2;
}

// What a sender needs to start a session with one device.
message DeviceBundle {
  int64 user_id = 1;
  DeviceKeys device = 2;
  // A one-time key, used up by this claim; the fallback key when none are
  // left.
  Prekey prekey = 3;
  bool fallback = 4;
}

// One device's encrypted copy of a private message.
message DeviceCiphertext {
  int64 user_id = 1;
  bytes device_id = 2;
  // 0: a prekey message (it starts a session); 1: a normal message.
  uint32 olm_type = 3;
  bytes body = 4;
}

// Requests.

// After Ready, once per session: binds the session to this device.
message RegisterDevice {
  DeviceKeys device = 1;
  repeated Prekey one_time_keys = 2;
  Prekey fallback_key = 3;
}

message UploadKeys {
  repeated Prekey one_time_keys = 1;
  // A new fallback key; absent keeps the current one.
  Prekey fallback_key = 2;
}

message UnregisterDevice {}

message DeviceRegistration {
  uint32 one_time_keys_left = 1;
  uint32 pending_events = 2;
}

// One or two members: the caller and the person they write to.
message FetchDevices {
  repeated int64 user_ids = 1;
}

message DeviceLists {
  repeated UserDevices users = 1;
}

message ClaimKeys {
  repeated DeviceRef devices = 1;
}

message ClaimedKeys {
  repeated DeviceBundle bundles = 1;
}

message FetchPendingEvents {
  // Events with a larger id; 0 for the oldest.
  int64 after_id = 1;
  // 1 to 100; 0 means 100.
  uint32 limit = 2;
}

message PendingEvents {
  // Oldest first.
  repeated PendingEvent events = 1;
  // More are waiting after the last one.
  bool more = 2;
}

message AckPendingEvents {
  repeated int64 ids = 1;
}

message SendPrivateMessage {
  int64 recipient_id = 1;
  // 16 bytes: the message's id. A repeat within 24 hours stores nothing.
  bytes send_id = 2;
  // One per device of the recipient, and one per other device of the caller.
  repeated DeviceCiphertext ciphertexts = 3;
  // The recipient's permission slip for the caller, when held.
  bytes slip = 4;
}

message PrivateMessageSent {
  // The server's time: the message's time on every device.
  int64 sent_at_ms = 1;
}

message StartPrivateTyping {
  int64 recipient_id = 1;
  bytes slip = 2;
}

// Replaces the caller's settings and block list on this server.
message UpdatePrivacy {
  bool allow_friend_requests = 1;
  bool allow_message_requests = 2;
  repeated int64 blocked_user_ids = 3;
}

message SendFriendRequest {
  int64 user_id = 1;
  // The caller's permission slip for the target.
  bytes slip = 2;
}

message FriendRequestCreated {
  int64 request_id = 1;
  int64 created_at_ms = 2;
}

message AnswerFriendRequest {
  int64 request_id = 1;
  bool accept = 2;
  // With an accept: the caller's permission slip for the requester.
  bytes slip = 3;
}

message CancelFriendRequest {
  int64 request_id = 1;
}

message RemoveFriend {
  int64 user_id = 1;
}

// Events.

// Something waiting on the server for one device until it acknowledges it.
message PendingEvent {
  int64 id = 1;
  int64 created_at_ms = 2;
  oneof kind {
    FriendRequestReceived friend_request_received = 10;
    FriendRequestSent friend_request_sent = 11;
    FriendRequestAnswered friend_request_answered = 12;
    FriendRequestCancelled friend_request_cancelled = 13;
    FriendRemoved friend_removed = 14;
    PrivateMessage private_message = 15;
  }
}

// To the target's devices.
message FriendRequestReceived {
  int64 request_id = 1;
  User from = 2;
  // The requester's permission slip for the target.
  bytes slip = 3;
}

// To the requester's other devices.
message FriendRequestSent {
  int64 request_id = 1;
  User to = 2;
}

// To the requester's devices and the answerer's other devices.
message FriendRequestAnswered {
  int64 request_id = 1;
  User requester = 2;
  User answerer = 3;
  bool accepted = 4;
  // With an accept: the answerer's permission slip for the requester.
  bytes slip = 5;
}

// To the target's devices and the requester's other devices.
message FriendRequestCancelled {
  int64 request_id = 1;
  User requester = 2;
  User target = 3;
}

// To the removed person's devices and the remover's other devices.
message FriendRemoved {
  User remover = 1;
  User removed = 2;
}

// One device's copy of a private message.
message PrivateMessage {
  User sender = 1;
  bytes sender_device_id = 2;
  uint32 olm_type = 3;
  bytes body = 4;
  // The server's time when it accepted the message.
  int64 sent_at_ms = 5;
}

// To the recipient's sessions; never stored.
message PrivateTypingStart {
  int64 user_id = 1;
}

// Inside the encryption: servers never decode these.

// An Olm message's plaintext.
message PrivatePayload {
  // 16 random bytes from the author's device.
  bytes message_id = 1;
  // The author's clock, for information only.
  int64 sent_at_ms = 2;
  // The conversation's other person (32 bytes).
  bytes recipient_public_key = 3;
  // The author's permission slip for the recipient (64 bytes), or empty.
  bytes slip = 4;
  oneof body {
    PrivateText text = 10;
    PrivateEdit edit = 11;
    PrivateDelete delete = 12;
    RelayMoved relay_moved = 13;
    RequestAccepted request_accepted = 14;
    SessionReset session_reset = 15;
  }
}

message PrivateText {
  string content = 1;
}

// Of the author's own message.
message PrivateEdit {
  bytes message_id = 1;
  string content = 2;
}

// Of the author's own message, for everyone.
message PrivateDelete {
  bytes message_id = 1;
}

// The author now writes through this server.
message RelayMoved {
  // The server's id from Hello (16 bytes).
  bytes server_id = 1;
  string server_name = 2;
}

message RequestAccepted {}

message SessionReset {}
```

- [ ] **Step 4: Add the oneof entries.**
  - `requests.proto`: import `"opencord/v1/private.proto"`. In `Request.kind`, after `stop_storage_move = 170`, under a comment `// Private messages and friends (design 2026-10-22).`, add:
    - `RegisterDevice register_device = 180`;
    - `UploadKeys upload_keys = 181`;
    - `UnregisterDevice unregister_device = 182`;
    - `FetchDevices fetch_devices = 183`;
    - `ClaimKeys claim_keys = 184`;
    - `FetchPendingEvents fetch_pending_events = 185`;
    - `AckPendingEvents ack_pending_events = 186`;
    - `SendPrivateMessage send_private_message = 187`;
    - `StartPrivateTyping start_private_typing = 188`;
    - `UpdatePrivacy update_privacy = 189`;
    - `SendFriendRequest send_friend_request = 190`;
    - `AnswerFriendRequest answer_friend_request = 191`;
    - `CancelFriendRequest cancel_friend_request = 192`;
    - `RemoveFriend remove_friend = 193`.

    In `Response.result`, after `upload_status = 33`:
    - `DeviceRegistration device_registration = 34`;
    - `DeviceLists device_lists = 35`;
    - `ClaimedKeys claimed_keys = 36`;
    - `PendingEvents pending_events = 37`;
    - `PrivateMessageSent private_message_sent = 38`;
    - `FriendRequestCreated friend_request_created = 39`.
  - `events.proto`: import `"opencord/v1/private.proto"`. After `storage_update = 113`, under the same comment, add:
    - `PendingEvent pending_event = 120;` with the comment `// To the sessions holding that device.`;
    - `PrivateTypingStart private_typing_start = 121;` with the comment `// To the recipient's sessions; never stored.`.
  - `gateway.proto` `Ready`: `// Whether this server relays private messages (design 2026-10-22).` then `bool private_messages = 21;`.
  - `models.proto` `ErrorCode`, after `ERROR_CODE_VOICE_MESSAGE_TOO_LONG = 47`, under `// Private messages and friends (design 2026-10-22).`, add:
    - `ERROR_CODE_FRIEND_REQUESTS_CLOSED = 48`;
    - `ERROR_CODE_MESSAGE_REQUESTS_CLOSED = 49`;
    - `ERROR_CODE_UNDELIVERABLE = 50`;
    - `ERROR_CODE_DEVICE_LIST_CHANGED = 51`;
    - `ERROR_CODE_PRIVATE_MESSAGES_UNAVAILABLE = 52`.
- [ ] **Step 5: Error codes on both sides of the bridge** (a value kept on both sides changes on both sides):
  - core `ErrorCode` (`api/types.rs`): add `FriendRequestsClosed`, `MessageRequestsClosed`, `Undeliverable`, `DeviceListChanged` and `PrivateMessagesUnavailable` after `VoiceMessageTooLong`;
  - `convert::error_code`: map the five proto codes to them, above the `_ => ErrorCode::Unknown` arm;
  - regenerate the bindings;
  - `RepoErrorKind`: add `friendRequestsClosed`, `messageRequestsClosed`, `undeliverable` and `privateMessagesUnavailable` (one doc line each, from spec §6.7);
  - `errorFrom` in `core_mapping.dart`: map the four, and `deviceListChanged` to `RepoErrorKind.other`.
- [ ] **Step 6:** If clippy reports `large_enum_variant` for a new oneof variant, box it in `build.rs` with `.boxed(..)` as the existing entries do, and name it in the ledger. Run the Task 1 test, `cargo clippy --workspace --all-targets -- -D warnings`, and `cd app && flutter analyze`. Expect: all pass.
- [ ] **Step 7: Commit** `feat(proto): private messages, friends and pending events`.

### Task 2: Shared signing payloads and limits

**Owner:** Gemini. **Score:** 3.
**Review focus:** every byte of the three payloads; wrong lengths never panic; `verify_strict`.

**Files:**
- Create: `crates/opencord-common/src/private.rs`
- Modify: `crates/opencord-common/src/lib.rs`, `crates/opencord-common/src/limits.rs`

**Interfaces:**
- Produces, in `opencord_common::private`:
  - constants (values from Global Constraints):
    - `DEVICE_ID_LEN = 16`, `KEY_LEN = 32`, `SIGNATURE_LEN = 64`, `SEND_ID_LEN = 16`, `MESSAGE_ID_LEN = 16`;
    - `MAX_DEVICES_PER_USER: usize = 10`, `ONE_TIME_KEYS_TARGET: usize = 20`, `MAX_STORED_ONE_TIME_KEYS: usize = 100`;
    - `MAX_CIPHERTEXTS: usize = 20`, `MAX_CIPHERTEXT_BYTES: usize = 32_768`;
    - `MAX_FETCH_DEVICES_USERS: usize = 2`, `MAX_CLAIM_DEVICES: usize = 20`, `MAX_ACK_IDS: usize = 500`;
    - `PENDING_PAGE_EVENTS: u32 = 100`, `PENDING_PAGE_BYTES: usize = 786_432`;
    - `MAX_OUTGOING_FRIEND_REQUESTS: i64 = 100`, `MAX_BLOCKED_IDS: usize = 10_000`;
    - `SPAM_AFTER_DECLINES: u32 = 3`;
    - `RECENT_SEND_TTL_MS: i64 = 86_400_000`, `FALLBACK_ROTATION_MS: i64 = 2_592_000_000`, `DEVICE_LIST_TTL_MS: i64 = 600_000`, `UNSENT_RESEND_MS: i64 = 86_400_000`, `RECEIVED_RECORD_TTL_MS: i64 = 3_024_000_000`, `TYPING_INTERVAL_MS: i64 = 8_000`.
  - `pub fn device_payload(device_id: &[u8; 16], curve25519: &[u8; 32], ed25519: &[u8; 32]) -> [u8; 98]`
  - `pub fn prekey_payload(device_id: &[u8; 16], key: &[u8; 32], fallback: bool) -> [u8; 67]`
  - `pub fn slip_payload(granter: &[u8; 32], grantee: &[u8; 32]) -> [u8; 84]`
  - `pub fn sign_device(identity: &SigningKey, device_id: &[u8; 16], curve25519: &[u8; 32], ed25519: &[u8; 32]) -> [u8; 64]`
  - `pub fn sign_slip(identity: &SigningKey, grantee: &[u8; 32]) -> [u8; 64]` (the granter is `identity`'s own public key)
  - `pub fn verify_device(identity_public_key: &[u8], device_id: &[u8], curve25519: &[u8], ed25519: &[u8], signature: &[u8]) -> bool`
  - `pub fn verify_prekey(device_ed25519: &[u8], device_id: &[u8], key: &[u8], fallback: bool, signature: &[u8]) -> bool`
  - `pub fn verify_slip(granter: &[u8], grantee: &[u8], slip: &[u8]) -> bool`
  - `SigningKey` is the one `opencord_common::auth` already uses.
- Produces, in `limits.rs`:
  - `FRIEND_REQUEST_RATE` (burst 10, 600 s);
  - `PRIVATE_MESSAGE_RATE` (burst 5, 5 s);
  - `KEY_CLAIM_RATE` (burst 30, 60 s).

- [ ] **Step 1: Write failing unit tests** in `private.rs`:
  - `payload_layouts_are_fixed`: each payload starts with its context string; lengths 98, 67, 84; the fallback byte is `1` or `0` in the last position; the slip has the granter before the grantee.
  - `device_signatures_bind_every_field`: verify succeeds; flipping one byte of the device id, the Curve25519 key, the Ed25519 key or the signature fails; another identity's key fails.
  - `prekey_signatures_bind_the_fallback_flag`: a one-time key's signature fails as a fallback key, and the reverse; another device id fails.
  - `slips_name_both_people_in_order`: A's slip for B verifies as (A, B), not as (B, A), and not as (A, C).
  - `wrong_lengths_never_verify`: 31- or 33-byte keys, a 15-byte device id, a 63-byte signature each return `false`.
  - Sign with `SigningKey::from_bytes(&[n; 32])` keys, as `auth.rs`'s tests do.
- [ ] **Step 2: Run** `cargo test -p opencord-common --lib private`. Expect: compile errors.
- [ ] **Step 3: Implement.** The verify functions convert slices with `try_into()`, and return `false` on any length or key error, using `VerifyingKey::verify_strict`. Add `pub mod private;` to `lib.rs`. Add the three rates to `limits.rs`, in the existing style with a doc line each.
- [ ] **Step 4: Run** the tests and `cargo clippy -p opencord-common --all-targets -- -D warnings`. Expect: pass.
- [ ] **Step 5: Commit** `feat(common): signing payloads and limits for private messages`.

### Task 3: Config, migration and the Ready flag

**Owner:** Gemini. **Score:** 2.

**Files:**
- Create: `crates/opencord-server/migrations/0010_private_messages.sql`, `crates/opencord-server/tests/private.rs`
- Modify: `crates/opencord-server/src/config.rs`, `crates/opencord-server/src/gateway/identify.rs`

**Interfaces:**
- Produces:
  - `Config.private_messages: PrivateMessagesSection { enabled: bool, undelivered_days: u32, max_pending_per_device: u32, device_expiry_days: u32 }`, with defaults `true`, `30`, `5000`, `60`;
  - the tables below;
  - `Ready.private_messages`.

- [ ] **Step 1: Write failing tests.**
  - In `config.rs`'s tests:
    - extend `environment_overrides_the_file` with the four `OPENCORD_PRIVATE_*` variables;
    - `private_message_limits_out_of_range_are_refused`, one case per bound: 0 and 366 days, 99 and 100,001 pending, 6 and 366 expiry days.
  - New `tests/private.rs` with `mod common;` and `ready_says_whether_private_messages_are_on`. It is `true` by default, and `false` with `common::TestServer::start_with(|config| config.private_messages.enabled = false)`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Config.** Add the section to `Config`, its `Default`, the environment overrides in `with_env`, and the range checks in `load_or_create` next to the `max_file_mb` one, with keys `private_messages.undelivered_days`, `private_messages.max_pending_per_device` and `private_messages.device_expiry_days`. Add this block to `DEFAULT_CONFIG_TOML`, after `[uploads]`:

```toml
[private_messages]
# Relay end-to-end encrypted private messages and friend requests between
# members. The server can't read the messages; it keeps one only until the
# device it's for fetches it. OPENCORD_PRIVATE_ENABLED
enabled = true
# Days an undelivered message or friend event waits for an offline device.
# 1 to 365. OPENCORD_PRIVATE_UNDELIVERED_DAYS
undelivered_days = 30
# Most events waiting for one device; past it, the oldest are dropped.
# 100 to 100000. OPENCORD_PRIVATE_MAX_PENDING_PER_DEVICE
max_pending_per_device = 5000
# Days a device may stay away before the server forgets it and its waiting
# events. 7 to 365. OPENCORD_PRIVATE_DEVICE_EXPIRY_DAYS
device_expiry_days = 60
```

- [ ] **Step 4: Migration** `0010_private_messages.sql`, exactly:

```sql
-- Private messages, friends and offline delivery (design 2026-10-22).

CREATE TABLE private_privacy (
    user_id INTEGER PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    allow_friend_requests INTEGER NOT NULL,
    allow_message_requests INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE private_blocks (
    owner_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    blocked_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (owner_id, blocked_id)
);

CREATE TABLE private_devices (
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    device_id BLOB NOT NULL,
    curve25519 BLOB NOT NULL,
    ed25519 BLOB NOT NULL,
    signature BLOB NOT NULL,
    fallback_key BLOB NOT NULL,
    fallback_signature BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, device_id)
);

CREATE TABLE private_one_time_keys (
    user_id INTEGER NOT NULL,
    device_id BLOB NOT NULL,
    key BLOB NOT NULL,
    signature BLOB NOT NULL,
    PRIMARY KEY (user_id, device_id, key),
    FOREIGN KEY (user_id, device_id)
        REFERENCES private_devices (user_id, device_id) ON DELETE CASCADE
);

CREATE TABLE private_friend_requests (
    id INTEGER PRIMARY KEY,
    from_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    to_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
);
-- One request per pair, whichever way it goes.
CREATE UNIQUE INDEX private_friend_requests_pair
    ON private_friend_requests (min(from_id, to_id), max(from_id, to_id));
CREATE INDEX private_friend_requests_to ON private_friend_requests (to_id);

CREATE TABLE private_pending (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    device_id BLOB NOT NULL,
    -- An encoded PendingEvent, id included.
    event BLOB NOT NULL,
    -- Set on FriendRequestReceived rows, so a cancel can take them back.
    friend_request_id INTEGER,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (user_id, device_id)
        REFERENCES private_devices (user_id, device_id) ON DELETE CASCADE
);
CREATE INDEX private_pending_by_device ON private_pending (user_id, device_id, id);
CREATE INDEX private_pending_by_request
    ON private_pending (friend_request_id) WHERE friend_request_id IS NOT NULL;
CREATE INDEX private_pending_by_age ON private_pending (created_at);

CREATE TABLE private_recent_sends (
    user_id INTEGER NOT NULL,
    device_id BLOB NOT NULL,
    send_id BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, device_id, send_id)
);
CREATE INDEX private_recent_sends_by_age ON private_recent_sends (created_at);
```

- [ ] **Step 5: Ready flag.** In `gateway/identify.rs`'s `ready()`, set `private_messages: state.config.private_messages.enabled`.
- [ ] **Step 6: Run** the config tests, `cargo test -p opencord-server --test private`, and the existing `cargo test -p opencord-server --test handshake`. Expect: pass. No queries changed, so no sqlx data yet.
- [ ] **Step 7: Commit** `feat(server): private messages config, schema and Ready flag`.

**Milestone E0 stop:** full checks, then D61 "Private messages: protocol, shared rules, config and schema (E0, …)" from `git diff`, then stop and report.

---

# Milestone E1: Server

### Task 4: The service, devices and keys, privacy and blocks

**Owner:** Claude. **Score:** 5 (identity binding of devices, key handout).

**Files:**
- Create: `crates/opencord-server/src/private/mod.rs`, `src/db/private.rs`, `src/handlers/private.rs`
- Modify: `src/lib.rs` (`pub mod private;`), `src/db/mod.rs`, `src/state.rs` (`pub private: crate::private::PrivateMessages`), `src/server.rs` (load at startup; test hooks), `src/error.rs`, `src/rate_limit.rs`, `src/gateway/session.rs`, `src/handlers/mod.rs`, `tests/common/mod.rs`, `tests/private.rs`, `.sqlx/`

**Interfaces:**
- Consumes: Task 1's protos, Task 2's `opencord_common::private`, Task 3's tables and config.
- Produces:
  - **`Session`**: a device slot, `pub fn private_device(&self) -> Option<[u8; 16]>` and `pub fn set_private_device(&self, device: Option<[u8; 16]>)`. It lives in the session, so it goes when the session goes; no map.
  - **`PrivateMessages`**, built in `AppState::new`:
    - `pub fn enabled(&self) -> bool`;
    - `pub async fn load(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error>` (privacy and blocks into memory, at startup);
    - `pub fn privacy(&self, user_id: i64) -> Privacy`, where `Privacy { allow_friend_requests: bool, allow_message_requests: bool, blocked: HashSet<i64> }` and the default allows both with no blocks;
    - `pub fn has_blocked(&self, owner: i64, other: i64) -> bool`;
    - `pub fn set_privacy(&self, user_id: i64, privacy: Privacy)`;
    - `pub fn forget(&self, user_id: i64)`.
    - The cache is `std::sync::RwLock<HashMap<i64, Privacy>>`. An entry is replaced by `UpdatePrivacy` and removed by `forget`, which Task 5 calls when a member is kicked or banned.
  - **`crate::handlers::private::require_device(ctx) -> Result<[u8; 16], ApiError>`**: the session's device, or `private_messages_unavailable("register this device first")` (also when `enabled()` is false).
  - **`ApiError`** constructors with these log texts:
    - `friend_requests_closed()`: "they aren't accepting friend requests";
    - `message_requests_closed()`: "they only accept messages from friends";
    - `undeliverable()`: "this could not be delivered";
    - `device_list_changed()`: "the devices changed; fetch them again";
    - `private_messages_unavailable(message)`.
  - **`RateLimits`**: `check_friend_request(user_id)`, `check_private_message(sender, recipient)` and `check_key_claim(user_id)`, over Task 2's rates and pruned like the others.
  - **Test helpers** in `tests/common/mod.rs`:
    - `pub struct TestDevice { pub id: [u8; 16], pub ed: SigningKey, pub curve: [u8; 32] }` with `TestDevice::new(seed: u8)`;
    - `pub fn device_keys(identity: &SigningKey, device: &TestDevice) -> proto::DeviceKeys`;
    - `pub fn prekey(device: &TestDevice, key: [u8; 32], fallback: bool) -> proto::Prekey`;
    - `pub fn slip(granter: &SigningKey, grantee: &[u8; 32]) -> Vec<u8>`;
    - `TestClient::register_device(&mut self, identity: &SigningKey, device: &TestDevice, one_time_keys: u8) -> proto::DeviceRegistration` (one-time keys `[device seed, i, 0…]`, fallback `[device seed, 255, 0…]`).
  - **`ServerHandle`** test hooks ("Intended for tests"):
    - `pub fn private_privacy(&self, user_id: i64) -> Option<(bool, bool, Vec<i64>)>` (sorted ids);
    - `pub async fn private_counts(&self, user_id: i64) -> PrivateCounts { devices, one_time_keys, pending, friend_requests }` (`pending` counts all of the user's devices; `friend_requests` counts both directions).

Handlers, each step in this order. Every handler starts with `enabled()`; when off, `private_messages_unavailable("private messages are turned off on this server")`.

- **`RegisterDevice`**:
  1. Shape: device present; the lengths from Global Constraints; a fallback key present; at most 100 one-time keys. Otherwise `INVALID_ARGUMENT`.
  2. The session holds a different device: `CONFLICT` "this connection already registered another device".
  3. `verify_device` against the caller's public key, taken from the in-memory guild. Otherwise `INVALID_ARGUMENT` "the device's signature does not match your identity".
  4. `verify_prekey` for each one-time key (`fallback = false`) and the fallback key (`true`) against the device's Ed25519 key. Otherwise `INVALID_ARGUMENT` "a prekey's signature does not match the device".
  5. Read the existing device row on a plain connection, then one transaction:
     - upsert the device (new keys replace old ones, and the old one-time keys are deleted with them);
     - `INSERT OR IGNORE` the one-time keys, keeping at most 100 stored;
     - set the fallback key and `last_seen_at`;
     - delete this user's devices beyond 10, the smallest `last_seen_at` first, never this one, in one `DELETE … WHERE device_id IN (SELECT …)` statement.
  6. Unbind this device from the user's other sessions, then bind this session.
  7. Answer `DeviceRegistration` with the device's unclaimed key count and pending count, read after the commit.
- **`UploadKeys`**:
  1. `require_device`.
  2. Shape, as above.
  3. Read the device's Ed25519 key, and verify each prekey as above.
  4. More than 100 stored after adding: `INVALID_ARGUMENT` "too many one-time keys".
  5. Insert the keys; replace the fallback key when one is given; touch `last_seen_at`.
  6. Answer `DeviceRegistration`.
- **`UnregisterDevice`**: with a device, delete it (its keys and pending events go by cascade) and unbind; without one, do nothing. Answer `ack`.
- **`FetchDevices`**:
  1. `require_device`.
  2. One or two ids, each a member: else `INVALID_ARGUMENT` / `NOT_FOUND` "member".
  3. Answer `DeviceLists` in the order asked, devices ordered by `created_at`.
- **`ClaimKeys`**:
  1. `require_device`.
  2. `check_key_claim`.
  3. 1–20 refs, each user a member.
  4. Per ref, one statement: `DELETE FROM private_one_time_keys WHERE rowid = (SELECT rowid … WHERE user_id = ? AND device_id = ? LIMIT 1) RETURNING key, signature`. It takes a key atomically, so two claims never get the same one. With no row, use the device's fallback key with `fallback: true`. An unknown device is left out of the answer.
- **`UpdatePrivacy`** (no device needed):
  1. At most 10,000 ids: else `INVALID_ARGUMENT`.
  2. Keep ids that are members and not the caller.
  3. One transaction: upsert `private_privacy`, delete the caller's `private_blocks`, insert the new ones.
  4. Then `set_privacy`. Answer `ack`.

- [ ] **Step 1: Write the failing tests** in `tests/private.rs`:
  - `a_device_registers_and_reports_its_keys`: 5 keys → `one_time_keys_left: 5, pending_events: 0`. `FetchDevices([self])` returns the same bytes.
  - `a_device_signed_by_someone_else_is_refused`: `INVALID_ARGUMENT`, and `FetchDevices` from a second device shows none.
  - `a_prekey_signed_by_another_key_is_refused`, including a one-time key signed as a fallback key.
  - `claims_use_up_one_time_keys_then_the_fallback`: register 2; a member claims 3 times. That gives two different keys with `fallback: false`, then the fallback key with `fallback: true`. `UploadKeys` with no keys answers `one_time_keys_left: 0`.
  - `a_session_registers_one_device`: a second device on the same session → `CONFLICT`; the same device again → success.
  - `an_eleventh_device_pushes_out_the_one_seen_longest_ago`: 11 sessions of one identity, each registering a new device, sequentially. `FetchDevices` lists 10, without the first.
  - `unregistering_removes_the_device_and_its_keys` (`private_counts`).
  - `requests_need_a_registered_device`: `FetchDevices`, `ClaimKeys` and `UploadKeys` before registering → `PRIVATE_MESSAGES_UNAVAILABLE`.
  - `a_disabled_server_refuses_registration`.
  - `privacy_and_blocks_replace_earlier_ones_and_survive_a_restart`: block A, then block B only. `private_privacy` shows only B, and does after `restart` too. Non-member ids are dropped.
- [ ] **Step 2: Run them** (`cargo test -p opencord-server --test private`). Expect: compile errors, then failures.
- [ ] **Step 3: Implement** the files above. Wire the six requests into `handlers::dispatch`. Call `state.private.load(..)` in `server::start` after the attachments load.
- [ ] **Step 4: Regenerate the sqlx data**, then run the tests, `cargo clippy -p opencord-server --all-targets -- -D warnings`, and the existing `cargo test -p opencord-server --test handshake`. Expect: pass.
- [ ] **Step 5: Commit** `feat(server): private message devices, keys and privacy`.

**Checks (for Gemini, after Claude's report):** `cargo test -p opencord-server --test private` and the clippy command above.

### Task 5: Pending events

**Owner:** Claude. **Score:** 5 (delivery guarantee, flow control).

**Files:**
- Modify: `src/private/mod.rs`, `src/db/private.rs`, `src/handlers/private.rs`, `src/handlers/members.rs`, `src/server.rs`, `tests/private.rs`, `.sqlx/`

**Interfaces:**
- Consumes: Task 4.
- Produces, in `crate::private`:
  - `pub struct Pending { pub user_id: i64, pub device_id: [u8; 16], pub kind: proto::pending_event::Kind, pub friend_request_id: Option<i64> }`
  - `pub async fn insert_pending(tx: &mut Transaction<'_, Sqlite>, state: &AppState, items: Vec<Pending>, now_ms: i64) -> Result<Vec<(i64, [u8; 16], proto::PendingEvent)>, sqlx::Error>`
    - gives each item a Snowflake id and `created_at_ms`, and stores the encoded event;
    - then deletes, per device touched, the rows beyond `max_pending_per_device`, oldest first;
    - returns what to push. A foreign-key failure (the device went meanwhile) comes back as the `sqlx::Error`.
  - `pub fn push_pending(state: &AppState, pushed: &[(i64, [u8; 16], proto::PendingEvent)])`: each event goes to the sessions of that user whose `private_device()` is that device, as `Event::PendingEvent`.
  - `pub async fn deliver(state: &AppState, items: Vec<Pending>) -> Result<(), ApiError>`: one transaction around `insert_pending`, commit, then `push_pending`.
  - `pub async fn devices_of(conn: &mut SqliteConnection, user_id: i64) -> Result<Vec<[u8; 16]>, sqlx::Error>`
  - `pub async fn sweep(state: &AppState, now_ms: i64) -> Result<(), sqlx::Error>`: deletes
    - pending rows older than `undelivered_days`;
    - devices with `last_seen_at` older than `device_expiry_days` (their keys and events go by cascade);
    - recent sends older than 24 h.
  - `pub async fn forget_member(tx: &mut Transaction<'_, Sqlite>, user_id: i64) -> Result<(), sqlx::Error>`: deletes the member's devices, privacy, own blocks, and friend requests to or from them, with the pending rows carrying those requests' ids.
  - `ServerHandle` test hooks:
    - `pub async fn sweep_private(&self, now_ms: i64)`;
    - `pub async fn queue_private_events(&self, user_id: i64, device_id: [u8; 16], kinds: Vec<proto::pending_event::Kind>)`: delivers these events to that device through `deliver`, as a sender would. `tests/common/mod.rs` gets `pub fn private_message_kind(body_len: usize) -> proto::pending_event::Kind`, a `PrivateMessage` from a fake sender with a body of that many bytes, for the tests below;
    - `pub async fn private_pending_for(&self, user_id: i64, device_id: [u8; 16]) -> Vec<proto::PendingEvent>` (Tasks 12 and 13 use it).

Handlers:
- **`FetchPendingEvents`**:
  1. `require_device`.
  2. A limit of 0 means 100; over 100 means 100.
  3. Select the device's rows with `id > after_id`, ordered by id, `limit + 1` of them.
  4. Take rows while the encoded total stays ≤ 786,432 bytes, always at least one.
  5. `more` = rows remain after the last taken one.
  6. Touch `last_seen_at`. Answer `PendingEvents` with the stored events decoded.
- **`AckPendingEvents`**:
  1. `require_device`.
  2. At most 500 ids.
  3. `DELETE … WHERE user_id = ? AND device_id = ? AND id IN (…)`, so only this device's rows go.
  4. Touch `last_seen_at`. Answer `ack`.
- **Kick and ban** (`handlers/members.rs`): call `forget_member` inside the existing transaction, before `commit`. After the commit, call `state.private.forget(user_id)` next to `remove_member`.
- **The sweep loop:** `server::start` spawns a loop on a 1-hour `tokio::time::interval` calling `sweep(&state, now_ms())`. A failure is logged with `tracing::warn!` and the loop goes on.

- [ ] **Step 1: Write the failing tests** in `tests/private.rs`:
  - `an_offline_device_gets_its_events_after_reconnecting_in_order`: queue 3 while B is offline. B connects and registers → `pending_events: 3`; a fetch gives 3, ids rising, `more: false`.
  - `events_stay_until_acknowledged`: fetch without acknowledging, reconnect, fetch → the same 3. Acknowledge 2, reconnect → 1 left. An acknowledgement naming another device's id deletes nothing.
  - `catch_up_pages_by_count_and_by_bytes`:
    - 250 events of 10 bytes → pages of 100, 100 and 50, with `more` true, true, false;
    - 30 events of 32,768 bytes → a first page of fewer than 30 whose encoded total is ≤ 786,432, with `more: true`; every page after that also fits, and all 30 arrive.
  - `a_live_event_reaches_the_session_holding_the_device`: B is online and registered; queue one → B gets `Event::PendingEvent` without fetching. B's second device, on another session, gets nothing.
  - `a_new_session_takes_the_device_from_the_old_one`: device D registers on connection 1, then on connection 2. A queued event reaches connection 2 only.
  - `the_cap_drops_the_oldest`: with `max_pending_per_device = 100`, queue 120 → 100 left, the oldest 20 gone.
  - `expiry_removes_old_events_and_unseen_devices`:
    - `sweep_private(now + 31 days)` → the events are gone and the device stays;
    - `sweep_private(now + 61 days)` → the device is gone (`private_counts`).
  - `a_kicked_member_leaves_nothing_behind`: B has a device, keys, events and privacy. The owner kicks B → `private_counts(B)` all zero, and `private_privacy(B)` is `None`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Regenerate the sqlx data**, then run the tests and clippy for the server, and `cargo test -p opencord-server --test features` (kick and ban). Expect: pass.
- [ ] **Step 5: Commit** `feat(server): pending events for private messages and friends`.

**Checks (for Gemini):** `cargo test -p opencord-server --test private` and server clippy.

### Task 6: Private messages and typing

**Owner:** Claude. **Score:** 5 (privacy enforcement, idempotency, device-set race).

**Files:**
- Modify: `src/handlers/private.rs`, `src/db/private.rs`, `tests/private.rs`, `.sqlx/`

**Interfaces:**
- Consumes: Tasks 4 and 5.
- Produces: the `SendPrivateMessage` and `StartPrivateTyping` handlers, wired into `dispatch`.

**`SendPrivateMessage`**, in this order:
1. `require_device` gives the caller's device.
2. `check_private_message(caller, recipient)`.
3. The recipient is a member (`NOT_FOUND` "member") and not the caller (`INVALID_ARGUMENT` "you can't message yourself").
4. `has_blocked(recipient, caller)` → `undeliverable()`.
5. The recipient doesn't allow message requests, and the slip fails `verify_slip(recipient's key, caller's key, slip)` → `message_requests_closed()`.
6. Shape: a 16-byte send id; 1–20 copies; each with a 16-byte device id, `olm_type` 0 or 1, a body of 1–32,768 bytes; no target twice. Otherwise `INVALID_ARGUMENT`.
7. A `private_recent_sends` row for (caller, device, send id) → answer `PrivateMessageSent` with that row's `created_at`, and store nothing.
8. Devices: the recipient's (`devices_of`) and the caller's other ones. A recipient with none → `private_messages_unavailable("they can't get private messages on this server")`. The copies' (user, device) set must equal recipient devices ∪ (caller devices − this device). Otherwise `device_list_changed()`.
9. One transaction:
   - `insert_pending` with one `PrivateMessage` per copy: `sender` = the caller's `User`, `sender_device_id`, `olm_type`, `body`, `sent_at_ms = now`;
   - insert the recent send with `created_at = now`.

   A foreign-key failure answers `device_list_changed()`. Commit, `push_pending`, answer `PrivateMessageSent { sent_at_ms: now }`.

**`StartPrivateTyping`**:
1. The recipient is a member and not the caller, as above.
2. Blocked, or message requests off with a failing slip → answer `ack` and send nothing.
3. Otherwise broadcast `PrivateTypingStart { user_id: caller }` to `Audience::User(recipient)`, and `ack`.

- [ ] **Step 1: Write the failing tests:**
  - `a_message_reaches_every_device_of_both_people_but_the_senders_own`: A has A1 (sending) and A2; B has B1 and B2. Three copies → each device's event carries its own body, the sender's user and device, and `sent_at_ms` equal to the answer's. A1 gets nothing.
  - `a_stale_device_set_is_refused`: B2 registers after A fetched; copies for B1 and A2 only → `DEVICE_LIST_CHANGED`, and nothing is stored for anyone.
  - `a_repeated_send_id_stores_nothing`: the same request twice → both succeed with the same `sent_at_ms`; B1 has one event.
  - `blocked_senders_get_undeliverable` (and nothing is stored).
  - `closed_message_requests_need_a_slip`: no slip, a slip signed by A, or B's slip for C → `MESSAGE_REQUESTS_CLOSED`; B's slip for A → success.
  - `a_recipient_without_devices_is_unavailable`.
  - `malformed_copies_are_refused`: a 32,769-byte body, 21 copies, a repeated target, a 15-byte send id, `olm_type` 2.
  - `typing_never_reveals_a_block`: B blocked A → A's typing gets `ack`, and B gets nothing within 500 ms. After B unblocks, B gets `PrivateTypingStart { user_id: A }`.
  - `messages_are_rate_limited_per_recipient`: the 6th within 5 s → `RATE_LIMITED`; a message to C still goes.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Regenerate the sqlx data**, then run the tests and server clippy. Expect: pass.
- [ ] **Step 5: Commit** `feat(server): relay end-to-end encrypted private messages`.

**Checks (for Gemini):** as Task 5.

### Task 7: Friend requests, answers, cancellations and removals

**Owner:** Gemini. **Score:** 3.
**Review focus:** the order of checks; conditional deletes; which devices each event goes to.

**Files:**
- Create: `crates/opencord-server/src/handlers/friends.rs`
- Modify: `src/handlers/mod.rs`, `src/db/private.rs`, `tests/private.rs`, `.sqlx/`

**Interfaces:**
- Consumes:
  - `require_device`, `deliver`, `Pending`, `devices_of`;
  - `PrivateMessages::{privacy, has_blocked}`;
  - `check_friend_request`;
  - the `ApiError` constructors (Task 4);
  - `verify_slip`.
- Produces: four handlers wired into `dispatch`.

"Target's devices" means all of that user's devices on this server. "The caller's other devices" means the caller's devices except the one this session holds. `User` values come from the in-memory guild (`Member::to_proto().user`).

- **`SendFriendRequest`**, in this order:
  1. `require_device`.
  2. `check_friend_request(caller)`.
  3. The target is a member, and not the caller.
  4. `has_blocked(target, caller)` → `undeliverable()`.
  5. The target doesn't allow friend requests → `friend_requests_closed()`.
  6. The target has no devices → `private_messages_unavailable("they can't get friend requests on this server")`.
  7. `verify_slip(caller's key, target's key, slip)` fails → `INVALID_ARGUMENT` "the permission slip is not valid".
  8. Read: the caller's outgoing count ≥ 100 → `INVALID_ARGUMENT` "you have too many friend requests waiting". A request between the pair exists:
     - from the caller → `CONFLICT` "you already sent them a friend request";
     - from the target → `CONFLICT` "they already sent you a friend request".
  9. One transaction:
     - insert the request (Snowflake id);
     - `insert_pending`: `FriendRequestReceived { request_id, from: caller, slip }` to the target's devices, with `friend_request_id` set; `FriendRequestSent { request_id, to: target }` to the caller's other devices.

     A unique-index failure → the `CONFLICT` for the caller. Commit, push, answer `FriendRequestCreated`.
- **`AnswerFriendRequest`**:
  1. `require_device`.
  2. Read the request: missing, or the caller isn't its target → `NOT_FOUND` "friend request".
  3. An accept with a failing `verify_slip(caller, requester, slip)` → `INVALID_ARGUMENT`.
  4. Transaction: `DELETE … WHERE id = ? AND to_id = ?`, which returns `rows_affected() == 1`. When `false` → `NOT_FOUND`, sending nothing. Then `insert_pending` `FriendRequestAnswered { request_id, requester, answerer: caller, accepted, slip (empty on a decline) }` to the requester's devices and the caller's other devices.
  5. Commit, push, `ack`. A requester who is no longer a member has no devices, so only the caller's other devices hear.
- **`CancelFriendRequest`**:
  1. `require_device`.
  2. The request exists and the caller sent it, else `NOT_FOUND`.
  3. Transaction:
     - `DELETE … WHERE id = ? AND from_id = ?` (`false` → `NOT_FOUND`);
     - delete the pending rows with this `friend_request_id`;
     - `insert_pending` `FriendRequestCancelled { request_id, requester: caller, target }` to the target's devices and the caller's other devices.
  4. Commit, push, `ack`.
- **`RemoveFriend`**:
  1. `require_device`.
  2. The target is a member and not the caller.
  3. `deliver` `FriendRemoved { remover: caller, removed: target }` to the target's devices and the caller's other devices.
  4. `ack`. The server keeps no friend list.

- [ ] **Step 1: Write the failing tests** (each person registers a device first; A2 is A's second device):
  - `a_friend_request_waits_for_an_offline_target`: B registered earlier and is offline. A sends. B reconnects, registers and fetches → `FriendRequestReceived` with A's user and a slip that verifies. A2 has `FriendRequestSent`.
  - `an_online_target_gets_the_request_at_once`.
  - `accepting_reaches_the_requester_and_the_answerers_other_devices` (`accepted: true`, B's slip for A verifies, both `User`s right).
  - `declining_reaches_the_requester_too` (`accepted: false`, empty slip).
  - `answering_twice_is_not_found`, and `answering_someone_elses_request_is_not_found`.
  - `cancelling_takes_back_the_undelivered_request`: B offline; A sends, then cancels. B's fetch has `FriendRequestCancelled` and no `FriendRequestReceived`.
  - `closed_friend_requests_give_their_error`.
  - `blocked_requesters_get_undeliverable`.
  - `a_second_request_between_the_pair_conflicts` (A→B twice, and B→A).
  - `a_request_needs_a_valid_slip`.
  - `removing_a_friend_reaches_both_sides`.
  - `friend_requests_are_rate_limited` (the 11th → `RATE_LIMITED`).
  - `a_kick_removes_the_kicked_persons_requests`: A requested B, B offline, the owner kicks A → B's fetch has no `FriendRequestReceived`, and `private_counts(A).friend_requests == 0`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** Each write that depends on an earlier read is the conditional statement given, never "read, then delete by id".
- [ ] **Step 4: Regenerate the sqlx data**, then run the tests and server clippy. Expect: pass.
- [ ] **Step 5: Commit** `feat(server): friend requests through pending events`.

**Milestone E1 stop:** full checks, then D62 from `git diff`, then stop and report.

# Milestone E2: Client storage, keys and contacts

The core's private module lives in `crates/opencord-core/src/private/`, declared `pub mod private;` in `lib.rs`, so the core's tests can reach its parts. Errors inside it are a `PrivateError` enum (Task 8). It has `impl From<PrivateError> for CoreError`: storage problems become `CoreError::Storage`; bad input becomes `CoreError::InvalidInput` with the spec's sentence.

### Task 8: The private database and sealing

**Owner:** Claude. **Score:** 5 (data at rest, losing a database).

**Files:**
- Create: `crates/opencord-core/migrations/0001_private.sql`, `src/private/mod.rs`, `src/private/seal.rs`, `src/private/store.rs`
- Modify: `crates/opencord-core/Cargo.toml` (`sqlx`, `chacha20poly1305`, `hmac`, all `.workspace = true`), `src/lib.rs`

**Interfaces:**
- Produces:
  - `seal::derive_keys(master: &[u8; 32]) -> DerivedKeys { pickle: [u8; 32], seal: [u8; 32] }`: HMAC-SHA256 of the labels `"opencord-private-pickle-v1"` and `"opencord-private-seal-v1"`, with `master` as the key.
  - `seal::Sealer` (no `Debug` that shows keys):
    - `Sealer::new(key: &[u8; 32])`;
    - `seal(&self, label: &str, row: &[u8], plaintext: &[u8]) -> Vec<u8>`: XChaCha20-Poly1305 with a random 24-byte nonce. The output is `nonce ‖ ciphertext+tag`; the associated data is `b"opencord-private:" ‖ label ‖ b":" ‖ row`;
    - `open(&self, label: &str, row: &[u8], sealed: &[u8]) -> Result<Vec<u8>, PrivateError>`.
  - `store::PrivateStore`:
    - `async fn open(data_dir: &Path, identity_public_key: &[u8; 32], master_key: &[u8; 32], now_ms: i64) -> Result<Opened, PrivateError>`, where `Opened { store: PrivateStore, started_fresh: bool }`;
    - `fn pool(&self) -> &SqlitePool`, `fn sealer(&self) -> &Sealer`, `fn pickle_key(&self) -> &[u8; 32]`;
    - `async fn setting(&self, key: &str) -> Result<Option<String>, PrivateError>`, `async fn set_setting(&self, key: &str, value: &str) -> Result<(), PrivateError>`;
    - `async fn close(self)`.
  - `PrivateError` in `mod.rs` (`thiserror`): `Storage(String)`, `Invalid(String)`, `Crypto(String)`; plus `From<sqlx::Error>`, `From<std::io::Error>` and `From<PrivateError> for CoreError`.

The database:
- **Where:** `<data_dir>/private/<identity public key, lowercase hex>.db`.
- **How it's opened:** `SqliteConnectOptions` with `create_if_missing`, WAL, `synchronous(Normal)`, `foreign_keys(true)` and `busy_timeout(5 s)`; at most 2 connections; `sqlx::migrate!("./migrations")`.
- **The key check:** a new database gets the setting `key_check` = hex of `seal("settings.key_check", b"", b"opencord-private-key-check")`. Opening an existing one opens that value.
- **Moving aside:** when the key check doesn't open, or opening or migrating fails, close the pool, rename the `.db`, `.db-wal` and `.db-shm` files (whichever exist) by appending `.unreadable-<now_ms>`, create a fresh database, and answer `started_fresh: true`. Log the move with `tracing::warn!`, without the key.

`migrations/0001_private.sql`, exactly:

```sql
-- The private messages database (design 2026-10-22 §11), one per identity.
-- Sealed columns hold nonce || XChaCha20-Poly1305 ciphertext.

CREATE TABLE device (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    device_id BLOB NOT NULL,
    -- vodozemac account pickle, encrypted with the pickle key.
    account TEXT NOT NULL,
    fallback_created_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

-- Other people's devices whose signatures this device checked.
CREATE TABLE peer_devices (
    public_key BLOB NOT NULL,
    device_id BLOB NOT NULL,
    curve25519 BLOB NOT NULL,
    ed25519 BLOB NOT NULL,
    first_seen_at INTEGER NOT NULL,
    PRIMARY KEY (public_key, device_id)
);

CREATE TABLE sessions (
    peer_curve25519 BLOB NOT NULL,
    session_id TEXT NOT NULL,
    -- vodozemac session pickle, encrypted with the pickle key.
    pickle TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    -- When it last decrypted something; NULL until then.
    last_received_at INTEGER,
    PRIMARY KEY (peer_curve25519, session_id)
);

CREATE TABLE contacts (
    public_key BLOB PRIMARY KEY,
    -- Sealed: the name last seen.
    name BLOB,
    -- 0 none, 1 outgoing request, 2 incoming request, 3 friends.
    friend_state INTEGER NOT NULL DEFAULT 0,
    request_server_id TEXT,
    request_id INTEGER,
    request_spam INTEGER NOT NULL DEFAULT 0,
    friend_server_id TEXT,
    blocked INTEGER NOT NULL DEFAULT 0,
    declines INTEGER NOT NULL DEFAULT 0,
    -- 0 none, 1 request, 2 accepted.
    conversation INTEGER NOT NULL DEFAULT 0,
    relay_server_id TEXT,
    -- Their permission slip for this person (64 bytes).
    their_slip BLOB,
    last_activity_at INTEGER,
    last_read_at INTEGER NOT NULL DEFAULT 0,
    muted INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

CREATE TABLE messages (
    -- 16 bytes from the author's device.
    id BLOB PRIMARY KEY,
    -- The conversation's other person.
    peer BLOB NOT NULL,
    outgoing INTEGER NOT NULL,
    -- 0 text, 1 divider, 2 moved card, 3 undecryptable line.
    kind INTEGER NOT NULL,
    -- Sealed: the text, or the server's name for a divider or card.
    body BLOB,
    -- A divider's or card's server, hex.
    server_id TEXT,
    -- The server's time; the local time while sending.
    sent_at INTEGER NOT NULL,
    edited_at INTEGER,
    -- 0 received, 1 sending, 2 sent, 3 failed.
    state INTEGER NOT NULL,
    -- Sealed encoded SendPrivateMessage while unsent, and its server.
    outbox BLOB,
    outbox_server_id TEXT
);
CREATE INDEX messages_by_conversation ON messages (peer, sent_at, id);
CREATE INDEX messages_sending ON messages (state) WHERE state = 1;

CREATE TABLE received_events (
    server_id TEXT NOT NULL,
    event_id INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (server_id, event_id)
);
CREATE INDEX received_events_by_age ON received_events (received_at);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

- [ ] **Step 1: Write the failing unit tests**, in temp dirs:
  - `sealed_values_open_only_with_their_key_label_and_row`: another key, label or row, or a flipped byte, all fail.
  - `every_seal_uses_a_new_nonce`.
  - `derived_keys_differ_and_repeat`.
  - `a_database_opens_again_with_its_key`.
  - `a_wrong_key_moves_the_database_aside`: the `.unreadable-` file exists, the new database lacks the old setting, and `started_fresh` is set.
  - `a_damaged_file_is_moved_aside`: garbage bytes in the `.db`.
  - `each_identity_has_its_own_database`.
  - `migrations_create_every_table`: `sqlite_master` lists the seven tables.
- [ ] **Step 2: Run** `cargo test -p opencord_core --lib private::`. Expect: failures.
- [ ] **Step 3: Implement.** Run the licence check from `GEMINI.md` for the new dependencies.
- [ ] **Step 4: Run** the tests and `cargo clippy -p opencord_core --all-targets -- -D warnings`, then `app/tool/check_platforms.sh`. Expect: pass.
- [ ] **Step 5: Commit** `feat(core): the private messages database, sealed at rest`.

**Checks (for Gemini):** the tests and the two check commands above.

### Task 9: The device and its sessions

**Owner:** Claude. **Score:** 5 (cryptography).

**Files:**
- Create: `src/private/device.rs`, `src/private/sessions.rs`
- Modify: `src/private/mod.rs`, `src/identity.rs`, `crates/opencord-core/Cargo.toml` (`vodozemac = "0.11.1"` in this crate's `[dependencies]`, default features)

**Interfaces:**
- Consumes: Task 8; `opencord_common::private`.
- Produces:
  - `Identity::sign_device(&self, device_id: &[u8; 16], curve25519: &[u8; 32], ed25519: &[u8; 32]) -> [u8; 64]` and `Identity::slip_for(&self, grantee: &[u8; 32]) -> [u8; 64]`.
  - `device::Device`:
    - `async fn load_or_create(store: &PrivateStore, now_ms: i64) -> Result<Device, PrivateError>`;
    - `fn id(&self) -> [u8; 16]`, `fn curve25519(&self) -> [u8; 32]`, `fn ed25519(&self) -> [u8; 32]`;
    - `fn keys(&self, identity: &Identity) -> proto::DeviceKeys`;
    - `fn fallback_prekey(&self) -> proto::Prekey`;
    - `fn new_one_time_keys(&mut self, count: usize) -> Vec<proto::Prekey>`: generates, signs each (`Account::sign` over `prekey_payload`), and marks them published;
    - `fn rotate_fallback_if_due(&mut self, now_ms: i64) -> bool`: due 30 days after `fallback_created_at`;
    - `fn account_mut(&mut self) -> &mut vodozemac::olm::Account`;
    - `async fn save(&self, conn: &mut SqliteConnection, pickle_key: &[u8; 32]) -> Result<(), PrivateError>`: writes the encrypted pickle inside the caller's transaction.
  - `sessions::PeerSessions`, for one peer device's sessions:
    - `async fn load(conn: &mut SqliteConnection, pickle_key: &[u8; 32], peer_curve25519: [u8; 32]) -> Result<Self, PrivateError>`;
    - `fn has_session(&self) -> bool`;
    - `fn add_outbound(&mut self, device: &Device, prekey: [u8; 32], now_ms: i64) -> Result<(), PrivateError>`: `create_outbound_session(SessionConfig::version_1(), peer, prekey)`;
    - `fn encrypt(&mut self, plaintext: &[u8]) -> Result<(u32, Vec<u8>), PrivateError>`: `(olm_type, body)` from `OlmMessage::to_parts`;
    - `fn decrypt(&mut self, device: &mut Device, olm_type: u32, body: &[u8], now_ms: i64) -> Decrypted`, where `Decrypted::{Plaintext(Vec<u8>), Duplicate, Failed(String)}`. The string is for logs only;
    - `async fn save(&mut self, conn: &mut SqliteConnection, pickle_key: &[u8; 32]) -> Result<(), PrivateError>`: upserts the changed sessions, then deletes all but the 5 newest by (`last_received_at` NULLs last, `created_at`).

Rules:
- **Sending** uses the session with the latest `last_received_at`, else the latest `created_at`.
- **Receiving a normal message** (`olm_type` 1) tries the sessions newest first. The first success sets that session's `last_received_at = now_ms`. When none succeeds and one said `DecryptionError::MissingMessageKey`, answer `Duplicate`; otherwise `Failed`.
- **Receiving a prekey message** (`olm_type` 0):
  - a session whose `session_keys()` equal the message's decrypts it (or reports `Duplicate`);
  - otherwise `device.account_mut().create_inbound_session(SessionConfig::version_1(), peer_curve25519, &message)`, which adds the session and changes the account (the caller saves both).
- The caller always saves the sessions, and the device when the account changed, **in the same transaction** as what it does with the plaintext.

- [ ] **Step 1: Write the failing unit tests**, with two devices on temp stores:
  - `device_keys_carry_a_valid_identity_signature` (`verify_device`).
  - `one_time_keys_are_signed_and_never_repeat`: two batches are disjoint and verify with `fallback = false`; the fallback key verifies with `true`.
  - `a_session_carries_messages_both_ways_and_survives_reloading`.
  - `a_duplicate_is_reported_as_a_duplicate`, for both a prekey and a normal message.
  - `crossed_sessions_settle_on_the_one_that_last_received`.
  - `at_most_five_sessions_are_kept_per_peer_device`.
  - `a_rotated_fallback_key_still_opens_sessions_made_with_the_previous_one`: and after a second rotation, the first no longer does (`Failed`).
  - `the_account_survives_reopening` (same id and keys).
  - `a_prekey_message_from_another_identity_key_fails`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** Run the licence check for vodozemac.
- [ ] **Step 4: Run** the tests, core clippy and `app/tool/check_platforms.sh`. Expect: pass.
- [ ] **Step 5: Commit** `feat(core): private message device keys and Olm sessions`.

**Checks (for Gemini):** as Task 8.

### Task 10: Contacts and their rules

**Owner:** Gemini. **Score:** 3.
**Review focus:** each rule's table of cases; a stale event changes nothing; names sealed.

**Files:**
- Create: `crates/opencord-core/src/private/contacts.rs`
- Modify: `src/private/mod.rs`

**Interfaces:**
- Consumes: Task 8 (`PrivateStore`, `Sealer`, `PrivateError`); `opencord_common::private::{verify_slip, SPAM_AFTER_DECLINES}`.
- Produces:
  - `enum FriendState { None = 0, Outgoing = 1, Incoming = 2, Friends = 3 }` and `enum Conversation { None = 0, Request = 1, Accepted = 2 }`.
  - `struct RequestRef { server_id: String, request_id: i64, spam: bool }`.
  - `struct Contact`:
    - `public_key: [u8; 32]`, `name: Option<String>`;
    - `friend_state: FriendState`, `request: Option<RequestRef>`, `friend_server_id: Option<String>`;
    - `blocked: bool`, `declines: u32`;
    - `conversation: Conversation`, `relay_server_id: Option<String>`, `their_slip: Option<[u8; 64]>`;
    - `last_activity_at: Option<i64>`, `last_read_at: i64`, `muted: bool`, `hidden: bool`;
    - `Contact::new(public_key)` has everything empty, `None` or zero.
  - `struct PrivacySettings { allow_friend_requests: bool, allow_message_requests: bool }`, `Default` both `true`.
  - Storage, each taking `&mut SqliteConnection` (so it runs inside a caller's transaction):
    - `load(conn, sealer, public_key) -> Result<Option<Contact>, PrivateError>`;
    - `load_all(conn, sealer) -> Result<Vec<Contact>, PrivateError>`;
    - `save(conn, sealer, &Contact, now_ms)` (upsert; the name sealed with label `"contacts.name"` and the public key as row);
    - `delete(conn, public_key)`;
    - `privacy(conn) -> PrivacySettings` and `set_privacy(conn, &PrivacySettings)`, on settings keys `privacy.allow_friend_requests` and `privacy.allow_message_requests`, values `"1"` or `"0"`.
  - Rules (pure, no I/O):
    - `is_accepted(&Contact) -> bool`: friends, or conversation `Accepted`.
    - `incoming_message(&Contact, &PrivacySettings) -> IncomingMessage { Drop, Request, Conversation }`. In this order: blocked → `Drop`; `is_accepted` → `Conversation`; message requests off → `Drop`; else `Request`.
    - `incoming_request(&Contact, &PrivacySettings) -> IncomingRequest { Drop, AutoAccept, CrossedAccept, Spam, Pending }`. In this order: blocked or friend requests off → `Drop`; friends → `AutoAccept`; `Outgoing` → `CrossedAccept`; `declines >= 3` → `Spam`; else `Pending`.
    - `after_my_request(&mut Contact, server_id, request_id)`: `Outgoing`, and the conversation becomes `Accepted` if it wasn't.
    - `after_my_answer(&mut Contact, accept: bool, server_id)`:
      - accept → `Friends`, `friend_server_id`, conversation `Accepted`;
      - decline → `None`, `declines += 1`;
      - the request is cleared either way.
    - `after_my_cancel(&mut Contact)`: `None`, request cleared.
    - `after_block(&mut Contact)`: blocked, `friend_state` `None`, request cleared.
  - Applying a friend event:
    - `apply_friend_event(conn, sealer, me: &[u8; 32], server_id: &str, event: FriendEvent<'_>, privacy: &PrivacySettings, now_ms) -> Result<FriendOutcome, PrivateError>`;
    - `FriendEvent<'a>` is one of `Received(&'a proto::FriendRequestReceived)`, `Sent(&'a proto::FriendRequestSent)`, `Answered(&'a proto::FriendRequestAnswered)`, `Cancelled(&'a proto::FriendRequestCancelled)` and `Removed(&'a proto::FriendRemoved)`;
    - `FriendOutcome { changed: Option<Contact>, new_request: Option<bool /* spam */>, follow_ups: Vec<FollowUp> }`, with `FollowUp::{Answer { server_id, request_id, accept: bool }, Cancel { server_id, request_id }}`.

`apply_friend_event` decides from which side this device is on (`me` compared with the `User.public_key`s). A `User` whose public key isn't 32 bytes makes the event change nothing. Every case updates the contact's name to the `User`'s display name.

| Event | This device is | Effect |
|---|---|---|
| `Received` | the target | slip valid (requester → me) → `their_slip`. Then by `incoming_request`: `Drop` → nothing; `AutoAccept` → follow-up `Answer { accept: true }`; `CrossedAccept` → follow-ups `Answer { this, accept: true }` and `Cancel { contact.request }`; `Spam` → `Incoming` with `spam: true`, `new_request: Some(true)`; `Pending` → `Incoming`, `new_request: Some(false)` |
| `Sent` | the requester's other device | `after_my_request` |
| `Answered` | the requester | only if `request` matches (server id and request id) and `Outgoing`: accepted → `Friends`, `friend_server_id`, conversation `Accepted`, and a valid slip (answerer → me) kept; declined → `None`, request cleared |
| `Answered` | the answerer's other device | only if `request` matches and `Incoming`: `after_my_answer(accepted)` |
| `Cancelled` | the target | only if `request` matches and `Incoming`: `None`, request cleared |
| `Cancelled` | the requester's other device | only if `request` matches and `Outgoing`: `None`, request cleared |
| `Removed` | the removed person, or the remover's other device | only if `Friends`: `None` |

- [ ] **Step 1: Write the failing unit tests**, in a temp store:
  - `incoming_messages_follow_blocks_acceptance_and_privacy`: one assertion per row of the order above.
  - `incoming_requests_follow_blocks_privacy_friendship_crossing_and_spam`: one per row; 2 declines → `Pending`, 3 → `Spam`.
  - `a_received_request_is_pending_and_keeps_a_valid_slip`: and an invalid slip isn't kept.
  - `the_third_decline_makes_the_next_request_spam`.
  - `an_accept_makes_friends_on_the_requesters_side_and_on_the_answerers_other_device`.
  - `a_decline_is_silent_for_the_requester`.
  - `an_answer_for_another_request_changes_nothing`.
  - `cancellations_and_removals_only_undo_what_exists`.
  - `crossed_requests_ask_for_an_accept_and_a_cancel`.
  - `names_are_sealed_on_disk`: the raw `name` column doesn't contain the name's bytes.
  - `privacy_settings_default_to_open_and_persist`.
- [ ] **Step 2: Run** `cargo test -p opencord_core --lib private::contacts` and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and core clippy. Expect: pass.
- [ ] **Step 5: Commit** `feat(core): private message contacts and their rules`.

**Milestone E2 stop:** full checks, then D63 from `git diff`, then stop and report.

---

# Milestone E3: Client delivery and the Rust API

From here on, a `Client` (`client.rs`) owns one `PrivateService` (in `src/private/service.rs`). The service is a cheap `Clone` around an `Arc`, and the connection tasks reach it through `Context.private`. Tests drive everything through `Client` methods, against real servers, in `crates/opencord-core/tests/private.rs`. That file gets its own `TestServer` and `TestClient` helpers, modelled on `tests/client.rs`'s, plus:
- `TestClient::open_private(&mut self, key: u8)`, which calls `private_open([key; 32])`;
- `TestClient::private_event(&mut self, pick) -> T`, which waits up to 10 s on `private_events()`.

The Rust API types below go in `crates/opencord-core/src/api/types.rs`. They reach Dart only in Task 16, so **don't regenerate the bindings in E3**.

```rust
pub struct PrivateOpened { pub started_fresh: bool }
pub enum FriendStatus { None, Outgoing, Incoming, Friends }
pub enum ConversationStatus { None, Request, Accepted }
pub enum PrivateMessageKind { Text, Divider, Moved, Undecryptable }
pub enum PrivateSendState { Received, Sending, Sent, Failed }
pub struct PrivateContact {
    pub peer: String,                    // public key, lowercase hex
    pub fingerprint: String,             // identity::public_key_fingerprint
    pub name: Option<String>,            // the name last seen
    pub friend: FriendStatus,
    pub request_spam: bool,
    pub blocked: bool,
    pub conversation: ConversationStatus,
    pub relay_server_key: Option<String>,// this device's key for the relay, if saved
    pub last_activity_ms: Option<i64>,
    pub unread: u32,                     // incoming text after last_read_at
    pub muted: bool,
    pub hidden: bool,
    pub preview: Option<String>,         // first line of the latest text, at most 100 characters
}
pub struct PrivateMessage {
    pub id: String,                      // hex
    pub peer: String,
    pub outgoing: bool,
    pub kind: PrivateMessageKind,
    pub content: String,                 // text; a divider's or card's server name; empty otherwise
    pub server_key: Option<String>,      // a divider's or card's server, if this device saved it
    pub sent_at_ms: i64,
    pub edited_at_ms: Option<i64>,
    pub state: PrivateSendState,
}
pub struct SharedServer {
    pub server_key: String,
    pub name: String,
    pub connected: bool,
    pub private_messages: bool,
    pub peer_user_id: i64,
}
pub struct PrivacySettingsData { pub allow_friend_requests: bool, pub allow_message_requests: bool }
pub enum PrivateEvent {
    ContactChanged(PrivateContact),
    ContactRemoved { peer: String },
    MessageAdded(PrivateMessage),
    MessageChanged(PrivateMessage),
    MessageDeleted { peer: String, message_id: String },
    Typing { peer: String },
    FriendRequestReceived { peer: String, spam: bool },
}
```

### Task 11: The service: members, registration, catch-up, acknowledgements

**Owner:** Claude. **Score:** 5 (delivery guarantee, concurrency).

**Files:**
- Create: `src/private/members.rs`, `src/private/service.rs`, `crates/opencord-core/tests/private.rs`
- Modify: `src/private/mod.rs`, `src/client.rs`, `src/connection.rs`, `src/api/types.rs`

**Interfaces:**
- Consumes: Tasks 8–10; the server from E1.
- Produces:
  - **`Client` methods:**
    - `pub async fn private_open(&self, key: [u8; 32]) -> Result<PrivateOpened, CoreError>`: needs an identity (else `CoreError::NoIdentity`). Opening again for another identity closes the old database first; for the same identity, it does nothing.
    - `pub fn private_events(&self) -> mpsc::UnboundedReceiver<PrivateEvent>`: replaces any earlier receiver.
    - `pub async fn private_contacts(&self) -> Result<Vec<PrivateContact>, CoreError>`.
    - `pub async fn private_messages(&self, peer: &str, before: Option<String>, limit: u32) -> Result<Vec<PrivateMessage>, CoreError>`: newest first, before the given message id; `limit` capped at 100.
    - `remove_server` first calls `PrivateService::before_remove(key)`.
  - **`PrivateService`** (crate-visible), used by later tasks:
    - `transport()`: the `Transport` trait, implemented over a `Weak` to the client's inner state. It has `request(&self, server_key: &str, kind) -> BoxFuture<'static, Result<proto::response::Result, CoreError>>`, `server_key_for(&self, server_id: &str) -> Option<String>`, `server_id_of(&self, server_key: &str) -> Option<String>`, `server_name(&self, server_key: &str) -> Option<String>` and `identity(&self) -> Option<Identity>`.
    - `members()`: the `MemberIndex` (below).
    - `device()`: the crypto lock, a `tokio::sync::Mutex<Device>` (Working Rule 11).
    - `store()`; `emit(PrivateEvent)`; `contact_view(&Contact) -> PrivateContact`; `message_view(row) -> PrivateMessage`.
  - **`MemberIndex`** (`members.rs`), per server key:
    - `self_id`, `server_id`, `connected: bool`, `private_messages: bool`, and members as `user_id ↔ public key` plus display names;
    - `shared_servers(peer: &[u8; 32]) -> Vec<SharedServer>` and `user_id_on(server_key, peer) -> Option<i64>`.
    - **When entries leave:** each `Ready` replaces its server's entry; `MemberLeave` removes a member; `before_remove` drops the server's entry. A dropped connection only sets `connected = false`.
  - **`connection.rs`**:
    - `on_ready` calls `context.private.on_ready(key, server_id_hex, &ready)` before emitting `Ready`;
    - `on_event` passes `MemberJoin`/`MemberUpdate`/`MemberLeave` to `context.private.on_member_event(key, &kind)` and still emits them;
    - it hands `PendingEvent` to `context.private.on_pending(key, event)` and `PrivateTypingStart` to `context.private.on_typing(key, user_id)`, emitting **neither** to Dart;
    - `emit_state` tells `on_connection(key, connected)`.

Behavior:
- **Opening:** open the store (Task 8) and `Device::load_or_create`. Delete received-event records older than 35 days. Start the worker. Then register on every server already ready with `private_messages`. Answer `started_fresh`.
- **Registration**, per server, in a spawned task, at most one at a time per server. On each `Ready` with `private_messages` while open:
  1. Under the device lock: `rotate_fallback_if_due`, and save the device if it rotated.
  2. `RegisterDevice` with the device keys, no one-time keys, and the fallback key.
  3. If `one_time_keys_left < 20`: under the lock, `new_one_time_keys(20 − left)` and save the device; then `UploadKeys`.
  4. `UpdatePrivacy`: this device's settings, and the user ids on that server of every blocked contact.
  5. **Catch-up:** `FetchPendingEvents` from `after_id` 0, then from each page's last id. Each page's events go into the worker queue, followed by a barrier the task awaits before fetching the next page, so at most one page is ever held. Stop when `more` is false.

  A failed step is logged (`tracing::warn!`, the error kind only), and the rest waits for the next `Ready`.
- **The worker:** one task, one event at a time, from an unbounded queue.
  - **Each pending event:**
    1. If (server id, event id) is in `received_events`, queue an acknowledgement and stop.
    2. Friend events: `apply_friend_event` (Task 10). In the same transaction, save the contact and record the event. After the commit, emit `ContactChanged`, and `FriendRequestReceived { spam }` when `new_request` is set. Queue the acknowledgement and run the follow-ups (below).
    3. `PrivateMessage` events are Task 13's. In this task, the worker leaves them alone: no record, no acknowledgement. They stay on the server until Task 13 handles them.
  - **Acknowledgements:** batched per server key. They're sent when 100 are waiting, or on a 200 ms tick while any are waiting. A failed acknowledgement is logged and dropped: the server will deliver those events again, and step 1 skips them. The map's entry for a server leaves when it's flushed.
- **Follow-ups** run after the commit, through the same server:
  - `Answer` sends `AnswerFriendRequest` with `identity.slip_for(peer)` when accepting. On success: `after_my_answer`, save, emit.
  - `Cancel` sends `CancelFriendRequest`. On success: `after_my_cancel`, save, emit.
  - A failure leaves the contact as it was, logged.
- **Typing:** `on_typing` → the member index gives the public key. If that contact isn't blocked, emit `Typing { peer }`.
- **Names:** on `Ready` and member events, each member who is a contact gets their last seen name updated, if it changed, and emits `ContactChanged`.
- **`before_remove(key)`:** if registered there, `UnregisterDevice` with a 5 s timeout (failure logged), then drop the server from the member index.

- [ ] **Step 1: Write the failing tests** in `tests/private.rs`. The server hooks are from Tasks 4 and 5.
  - `opening_registers_this_device_on_every_server_with_private_messages`: two servers, the second with private messages off → `private_counts` shows 1 device and 20 one-time keys on the first, none on the second.
  - `opening_after_ready_registers_too`.
  - `privacy_reaches_each_server_on_registration`: `private_privacy(B) == Some((true, true, []))`.
  - `a_queued_friend_request_becomes_a_pending_contact_and_is_acknowledged`. `queue_private_events` gives B's device a `FriendRequestReceived` from A, with A's real slip. B gets `ContactChanged` (`friend: Incoming`) and `FriendRequestReceived { spam: false }`, and the server's pending count reaches 0.
  - `a_device_back_after_many_events_acknowledges_them_all`: 250 `FriendRemoved` events are queued while B's client is closed. B reopens (same directory, identity and key) → pending reaches 0 within 10 s, and the connection was never closed by the server (one `Ready`).
  - `removing_a_server_unregisters_the_device_there`.
- [ ] **Step 2: Run** `cargo test -p opencord_core --test private` and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** these tests, `cargo test -p opencord_core --test client` (unchanged behavior), core clippy and `app/tool/check_platforms.sh`. Expect: pass.
- [ ] **Step 5: Commit** `feat(core): private message service, registration and catch-up`.

**Checks (for Gemini):** the four commands in Step 4.

### Task 12: Sending

**Owner:** Claude. **Score:** 5 (never reuse a message key; the outbox; fan-out).

**Files:**
- Create: `src/private/payload.rs`, `src/private/outbox.rs`
- Modify: `src/private/service.rs` (resend after catch-up), `src/client.rs`, `src/server.rs` of **opencord-server** (one test hook), `tests/private.rs`

**Interfaces:**
- Consumes: Task 11.
- Produces:
  - **`payload`:**
    - `build(message_id: [u8; 16], recipient: &[u8; 32], slip: Option<[u8; 64]>, body: proto::private_payload::Body, now_ms) -> Vec<u8>`;
    - `parse(bytes: &[u8]) -> Result<Parsed, PrivateError>`. It checks: a 16-byte id, a 32-byte recipient, a slip of 0 or 64 bytes, and a body present. Text passes `validation::message_content`; an edit's text too, and its id is 16 bytes; a delete's id is 16 bytes; a move has a 16-byte server id and a name of 1–100 characters. Task 13 uses `parse`.
  - **`outbox::send(service, peer: [u8; 32], body: Body, options: SendOptions) -> Result<Option<PrivateMessage>, CoreError>`**: the one path for every payload.
    - `SendOptions { relay: Option<String> }` sends through that server key instead of the contact's relay;
    - the result is the stored message for text, `None` otherwise.
  - **`outbox::retry(service, message_id: [u8; 16])`** and **`outbox::resend_unsent(service, server_key)`**.
  - **`Client` methods:**
    - `pub async fn private_send(&self, peer: &str, content: String) -> Result<PrivateMessage, CoreError>`;
    - `pub async fn private_edit(&self, peer: &str, message_id: &str, content: String) -> Result<(), CoreError>`;
    - `pub async fn private_delete(&self, peer: &str, message_id: &str) -> Result<(), CoreError>`;
    - `pub async fn private_retry(&self, message_id: &str) -> Result<(), CoreError>`.
  - **Server test hook** (`ServerHandle`, "Intended for tests"): `pub async fn insert_private_device_unchecked(&self, user_id: i64, keys: proto::DeviceKeys, fallback: proto::Prekey)`. It stands in for a dishonest server.

`send`, in this order (spec §10.4):
1. **Relay:** `options.relay`, else the contact's `relay_server_id` mapped to a server key. It must be connected, with `private_messages`, and the peer a member there. Otherwise `CoreError::InvalidInput` "You don't share a connected server with them right now.".
2. **Text only:** check the content. Insert the message (`id` random, `state` sending, `sent_at` = now, body sealed with label `"messages.body"` and the id as row). Emit `MessageAdded`.
3. **Devices:** the cached lists for (server, peer) and (server, self), or `FetchDevices`.
   - Keep only devices whose signature verifies against that person's public key (the contact's key, and this identity's); drop this device.
   - Store newly verified ones in `peer_devices`.
   - **The cache:** a map (server key, user id) → (devices, fetched at). An entry is replaced after 10 minutes, removed on `DEVICE_LIST_CHANGED` for that server, and removed with its server in `before_remove`.
4. **Keys:** for devices without a session, `ClaimKeys`. Keep bundles whose prekey verifies against that device's Ed25519 key with the bundle's `fallback` flag; skip the rest.
5. **Encrypt under the device lock:**
   - load each target's `PeerSessions`, add outbound sessions from the claimed prekeys, and encrypt the payload once per device. The payload's `slip` is `identity.slip_for(peer)`, this person's permission for the peer; the request's `slip` (next line) is the contact's `their_slip`, the peer's permission for this person;
   - build `SendPrivateMessage { recipient_id, send_id: message id, ciphertexts, slip: their_slip }`;
   - in one transaction, save every changed `PeerSessions` and, for text, set `outbox` (sealed, label `"messages.outbox"`) and `outbox_server_id`;
   - commit, then release the lock.
6. **Send**, then:
   - **`PrivateMessageSent`:** for text, `state` sent and `sent_at` the server's time, with the outbox cleared; set the contact's conversation to `Accepted` and `hidden` false, and update `last_activity_at`. Emit `MessageChanged` and `ContactChanged`.
   - **`DeviceListChanged`:** drop both cache entries and go back to step 3 once, with the same message id. A second one: the message `failed`.
   - **`NotConnected` or `Timeout`:** leave it `sending`, with its outbox.
   - **Any other error:** `failed`, outbox kept; return the error.
   - Edits and deletes change the local row only after the server accepts them, then emit `MessageChanged` or `MessageDeleted`. Their errors are returned.
7. **`retry`:** an outbox whose server is connected is resent as is. Otherwise, encrypt again from step 3 for the current relay.
8. **`resend_unsent(server)`**, after each catch-up: every `sending` row whose outbox belongs to that server. Over 24 h old → `failed` and emitted; else resend its outbox.

- [ ] **Step 1: Write the failing tests:**
  - `a_message_reaches_every_other_device_of_both_people`: A has two devices (two clients, one identity), B one. A1 sends → `private_pending_for(B, B1)` holds 1 and `(A, A2)` holds 1. A1's message turns `sent` with the server's time.
  - `a_device_added_meanwhile_gets_the_message_after_the_refusal`: A sends once (devices cached). B opens a second device. A sends again → it succeeds, and B2's queue has it.
  - `an_unsent_message_goes_out_once_after_reconnecting`. Send while the server is stopped (`shutdown`, then a new one on the same port and data). Before the restart, the message is `sending`; after it, B1's queue holds exactly 1.
  - `a_send_retried_after_a_lost_answer_arrives_once`: `private_retry` on a message the server already stored (resend its saved request) → B1's queue still holds 1.
  - `a_device_the_identity_did_not_sign_is_never_used`: `insert_private_device_unchecked` adds a forged device to B. A's send fails (the server keeps answering `DEVICE_LIST_CHANGED`), and the forged device's queue stays empty.
  - `sessions_are_saved_before_sending`: B1's two consecutive copies decode as different Olm messages. Using B's store directly (`opencord_core::private`), the second decrypts after the first. This is a real ratchet step, not a reused key.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, core clippy and `app/tool/check_platforms.sh`. Expect: pass.
- [ ] **Step 5: Commit** `feat(core): encrypt and send private messages`.

**Checks (for Gemini):** as Task 11.

### Task 13: Receiving

**Owner:** Claude. **Score:** 5 (decryption, duplicates, crash safety).

**Files:**
- Create: `src/private/inbox.rs`
- Modify: `src/private/service.rs` (the worker hands `PrivateMessage` events here), `opencord-server/src/server.rs` (one test hook), `tests/private.rs`

**Interfaces:**
- Consumes: Tasks 11 and 12.
- Produces:
  - the worker's handling of `PrivateMessage` events;
  - **server test hook** `pub async fn requeue_private_event(&self, user_id: i64, device_id: [u8; 16], event: proto::PendingEvent)`: stores the event under its own id and pushes it, as a redelivery would.

Each `PrivateMessage` (spec §10.5):
1. **Repeat:** as Task 11.
2. **Sender device:**
   - The `User.public_key` must be 32 bytes, else acknowledge and log.
   - Find (public key, `sender_device_id`) in `peer_devices`. If it's missing, `FetchDevices` for the sender through this server, verify, and store.
   - Still unknown or unverifiable: an undecryptable line (step 6) and acknowledge.
3. **Decrypt** under the device lock (`PeerSessions::decrypt`):
   - `Duplicate` → record the event and acknowledge, showing nothing;
   - `Failed` → an undecryptable line, and a session reset (below).
4. **Parse** (`payload::parse`). The conversation's peer:
   - the sender's key when the sender is someone else, and then `recipient_public_key` must be this identity;
   - `recipient_public_key` when the sender is this identity (a copy from one of its other devices).

   A failure → an undecryptable line.
5. **Slip:** when the sender is someone else and `verify_slip(sender, me, slip)` holds, store it as `their_slip`.
6. **Apply and save, in one transaction**: the sessions, the device if the account changed, the change below, the contact, and the event record. Then emit.
   - **Text** from someone else: the contact by `incoming_message`.
     - `Drop`: nothing but the record.
     - `Request` or `Conversation`: insert the message (`state` received, `sent_at` = the event's `sent_at_ms`). Set the conversation to `Request` if it was `None`; set `last_activity_at`; clear `hidden`.
   - **Text** that's a copy from one's own device: insert it as outgoing `sent`, and set the conversation to `Accepted`.
   - **Edit / delete:** only of a message by the same author (the peer's for theirs, this identity's for copies). Otherwise ignore it.
   - **Moved:**
     - from someone else: insert a `Moved` card (body = server name, `server_id` = hex);
     - a copy from one's own device: set `relay_server_id` and insert a `Divider` at that point.
   - **Request accepted:** the contact's conversation becomes `Accepted` on this side too (they accepted, so it's mutual).
   - **Session reset:** nothing to show; the new session is already saved.
   - **Undecryptable line:** a `kind = Undecryptable` message, `outgoing` false, with a random 16-byte id. The event record in the same transaction keeps it from being added twice.
7. **Acknowledge** (batched).

**Session reset:** after a `Failed` decrypt from a known device, at most once per peer device per hour, claim a key for that device, create a fresh outbound session, and send a `SessionReset` payload to that one device through the same server. The once-an-hour map is keyed by the device's Curve25519 key. An entry leaves when it's over an hour old, pruned on each insert.

- [ ] **Step 1: Write the failing tests:**
  - `a_message_reaches_the_other_person_and_the_senders_other_device`: B gets `MessageAdded` with the text; A2 gets it as outgoing, in the conversation with B.
  - `messages_wait_for_an_offline_device`: three while B is closed → they arrive in order after B reopens.
  - `a_message_delivered_twice_shows_once`. Snapshot B1's queued event (`private_pending_for`) while B is closed, then let B receive it. `requeue_private_event` the snapshot → B acknowledges it again, and shows no second message and no undecryptable line.
  - `a_crash_before_saving_decrypts_on_redelivery`:
    - hold `BEGIN EXCLUSIVE` on B's database from a second connection, so B's save fails after decrypting;
    - B neither acknowledges nor shows anything;
    - release the lock and make B reconnect (`drop_user_connections`) → the message shows, once.
  - `edits_and_deletes_reach_every_device`: and an edit by the wrong author changes nothing.
  - `a_stranger_starts_a_message_request`: C (no relation) → B's contact is `Request`.
  - `a_broken_session_recovers_with_a_reset`:
    - with B closed, delete B's `sessions` rows for A's device straight in its database;
    - A's next message → an undecryptable line on B, and a reset;
    - A's message after that decrypts on B.
  - `a_message_from_a_forged_device_is_not_believed`: a device for A inserted with `insert_private_device_unchecked` sends nothing that B shows as A's text. It shows as an undecryptable line.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** all of `tests/private.rs`, core clippy and `app/tool/check_platforms.sh`. Expect: pass.
- [ ] **Step 5: Commit** `feat(core): receive and decrypt private messages`.

**Checks (for Gemini):** as Task 11.

### Task 14: Friends, requests, blocks and privacy

**Owner:** Gemini. **Score:** 3.
**Review focus:** what each action changes locally only after the server agreed; errors returned to the caller; the block sync's user ids per server.

**Files:**
- Create: `crates/opencord-core/src/private/actions.rs`
- Modify: `src/private/mod.rs`, `src/client.rs`, `tests/private.rs`

**Interfaces:**
- Consumes: Tasks 10–13 (`PrivateService`, `MemberIndex`, `outbox::send`, the contact rules).
- Produces, as `Client` methods, each returning `Result<_, CoreError>`:
  - `friend_request_send(&self, server_key: &str, user_id: i64)`: the peer comes from the member index. `SendFriendRequest` carries `identity.slip_for(peer)`. On success: `after_my_request(server id, request id)`, save, emit `ContactChanged`.
  - `friend_request_answer(&self, peer: &str, accept: bool)`: the contact must be `Incoming`. Its request's server must be connected, else `CoreError::InvalidInput` "Couldn't answer: <server name> isn't connected right now.". On success: `after_my_answer`, save, emit.
  - `friend_request_cancel(&self, peer: &str)`: the same, with `CancelFriendRequest` and `after_my_cancel`.
  - `friend_remove(&self, peer: &str)`: `RemoveFriend` through every connected shared server with private messages. If at least one succeeded, the contact becomes `None` and is saved and emitted; otherwise the first error is returned.
  - `message_request_accept(&self, peer: &str)`: the conversation must be `Request`. Send a `RequestAccepted` payload (`outbox::send`); on success the conversation becomes `Accepted`, then save and emit.
  - `message_request_ignore(&self, peer: &str)`: delete the conversation's messages. If the contact has no friend state, no request and no block, delete it (emit `ContactRemoved`); else set the conversation to `None` (emit `ContactChanged`).
  - `private_block(&self, peer: &str)`:
    - an incoming request is declined, and an outgoing one cancelled;
    - friends get `friend_remove`;
    - then `after_block`, save, emit;
    - then `UpdatePrivacy` to every connected server with private messages where the peer is a member.

    The contact is created from the member index if it doesn't exist. A failed request or removal along the way is logged, and the block still happens.
  - `private_unblock(&self, peer: &str)`: unblocked, saved, emitted, and the same `UpdatePrivacy` sync.
  - `privacy_settings(&self) -> PrivacySettingsData`.
  - `set_privacy_settings(&self, settings: PrivacySettingsData)`: save, then `UpdatePrivacy` to every connected server with private messages. A failure on one server is logged; the next registration repeats it.
- Also: each `MemberJoin` of a blocked contact sends `UpdatePrivacy` to that server again (in `on_member_event`).

- [ ] **Step 1: Write the failing tests** in `tests/private.rs`:
  - `friends_are_made_through_a_shared_server`: A sends; B gets the request and accepts. Both, and A's second device, end `Friends`.
  - `a_declined_request_quietly_disappears`: A is back to `None` with no error; B's contact has `declines` 1, which shows as no spam yet.
  - `crossed_requests_become_a_friendship`: A→B on server X and B→A on server Y, both pending → both `Friends`. Both servers end with no request (`private_counts`).
  - `the_third_decline_sends_the_next_request_to_spam`.
  - `closed_friend_requests_give_their_error`: `CoreError::Server { code: ErrorCode::FriendRequestsClosed, .. }`.
  - `closed_message_requests_refuse_strangers_but_not_friends`.
  - `an_accepted_message_request_lets_them_write_after_requests_close`: C writes to B. B accepts, then closes message requests. C writes again and it arrives, thanks to B's slip.
  - `ignoring_a_message_request_deletes_it`.
  - `blocking_ends_everything_and_refuses_them`: A and B are friends; A blocks B.
    - Both stop being friends.
    - B's message gets `Undeliverable`, and so does B's friend request.
    - A sees nothing.
  - `unblocking_lets_them_write_again`.
  - `removing_a_friend_reaches_both_sides`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** Every local change happens after the server's success, except blocking, which never waits for anyone.
- [ ] **Step 4: Run** `tests/private.rs` and core clippy. Expect: pass.
- [ ] **Step 5: Commit** `feat(core): friends, message requests, blocks and privacy`.

### Task 15: Conversations, relays and typing

**Owner:** Gemini. **Score:** 3.
**Review focus:** a move changes nothing unless the note was accepted; Move too checks the server; hidden conversations come back.

**Files:**
- Modify: `src/private/actions.rs`, `src/client.rs`, `tests/private.rs`

**Interfaces:**
- Consumes: Task 14's module and Task 12's `outbox::send`.
- Produces, as `Client` methods:
  - `private_open_conversation(&self, server_key: &str, user_id: i64) -> Result<PrivateContact, CoreError>`:
    - the peer comes from the member index; a missing contact is created with the member's name;
    - a usable relay (§8) is kept. Without one, it becomes the friendship's server (`friend_server_id`) if that is usable, else `server_key`. From a profile the app passes the profile's server; from the Friends page, the first shared server in rail order;
    - `hidden` becomes false. Save and emit.
  - `private_shared_servers(&self, peer: &str) -> Vec<SharedServer>`, in the order servers were added.
  - `private_move(&self, peer: &str, server_key: &str) -> Result<(), CoreError>`:
    - `outbox::send` a `RelayMoved { server_id, server_name }` with `SendOptions { relay: Some(server_key) }`;
    - on success, set `relay_server_id` and insert a `Divider` (outgoing, `sent` at the server's time), then emit `MessageAdded` and `ContactChanged`;
    - on error, change nothing and return it.
  - `private_move_too(&self, peer: &str, message_id: &str) -> Result<(), CoreError>`:
    - the message must be a `Moved` card from that peer;
    - its server must be saved and connected on this device, and the peer a member there; otherwise `CoreError::InvalidInput` "This device isn't on <server name>.".
    - Then set the relay, insert a `Divider`, emit.
  - `private_typing(&self, peer: &str) -> Result<(), CoreError>`: `StartPrivateTyping { recipient_id, slip: their_slip }` through the relay. The app throttles it to every 8 s.
  - `private_mark_read(&self, peer: &str)`: `last_read_at` = the newest message's `sent_at`.
  - `private_set_muted(&self, peer: &str, muted: bool)` and `private_hide(&self, peer: &str)`: save, emit.
- **The first divider:** when a conversation's first text message is sent or arrives, insert a `Divider` for its relay just before it.

- [ ] **Step 1: Write the failing tests:**
  - `a_conversation_starts_on_the_profiles_server_with_a_divider`.
  - `moving_a_chat_takes_effect_for_the_mover_and_offers_the_other`: A and B share X (the relay) and Y. A moves to Y:
    - A's relay is Y and A has a divider;
    - B gets a `Moved` card with Y's server key;
    - B's `private_move_too` → B's relay is Y, with a divider;
    - B's next message travels through Y (Y's pending count for A rises).
  - `moving_to_a_server_the_other_person_isnt_on_is_refused`: an error, and A's relay is unchanged.
  - `move_too_needs_the_server_on_this_device`.
  - `typing_reaches_the_other_person_but_not_through_a_block`.
  - `shared_servers_list_only_servers_with_both_people`.
  - `a_hidden_conversation_comes_back_with_the_next_message`.
  - `reading_clears_the_unread_count`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `tests/private.rs` and core clippy. Expect: pass.
- [ ] **Step 5: Commit** `feat(core): private conversations, relay moves and typing`.

**Milestone E3 stop:** full checks, then D64 from `git diff`, then stop and report.

# Milestone E4: The app

Read `GEMINI.md` → "What the user sees" and "Reuse and conventions" before starting; most of what the review checks here is there.

For every widget: the app's own widgets only (`OcButton`, `OcIconButton`, `OcSwitch`, `ChoiceChips`, `OcDialog`, `showOcMenu`, `confirmAction`, `showOcToast`, `Hoverable`, `SectionLabel`, `ErrorBox`), `OcText` styles, `context.oc` colors, monochrome.

### Task 16: The Dart-facing core API and the key store

**Owner:** Gemini. **Score:** 2.

**Files:**
- Create: `crates/opencord-core/src/api/private.rs`, `app/lib/core/rust/private_key_store.dart`
- Modify: `crates/opencord-core/src/api/{mod,types}.rs`, `src/convert.rs`, `app/lib/core/rust/core_api.dart`, `app/test/support/{fake_core,fixtures}.dart`, the generated bindings

**Interfaces:**
- Consumes: the `Client` methods of Tasks 11–15.
- Produces:
  - **`api/private.rs`**, one function per `Client` method, in the style of `api/attachments.rs`: `on_runtime` for async calls; ids, peers and server keys as `String`.
    - `private_open(key: Vec<u8>) -> Result<PrivateOpened, CoreError>`: not 32 bytes → `CoreError::InvalidInput`.
    - `private_event_stream(sink: StreamSink<PrivateEvent>) -> Result<(), CoreError>`: forwards `client.private_events()` into the sink on the runtime. Opening it again replaces the earlier stream. Nothing is kept while none is open; the app reloads after subscribing.
    - `private_contacts`, `private_messages`, `private_send`, `private_edit`, `private_delete`, `private_retry`;
    - `friend_request_send`, `friend_request_answer`, `friend_request_cancel`, `friend_remove`;
    - `message_request_accept`, `message_request_ignore`;
    - `private_block`, `private_unblock`, `privacy_settings`, `set_privacy_settings`;
    - `private_open_conversation`, `private_shared_servers`, `private_move`, `private_move_too`;
    - `private_typing`, `private_mark_read`, `private_set_muted`, `private_hide`.
  - **`ReadySnapshot.private_messages: bool`**, set from the proto `Ready` in `convert::ready`, so the app knows which servers relay.
  - **`CoreApi`** (`core_api.dart`): one method per function above plus `Stream<core.PrivateEvent> privateEventStream()`, and their implementations over the generated functions.
  - **`PrivateKeyStore`** (`private_key_store.dart`): the same as `MediaCacheKeyStore`, with key names `opencord.private.key` and `opencord.<profile>.private.key`.

- [ ] **Step 1:** Add the API functions and the `ReadySnapshot` field, then regenerate the bindings.
- [ ] **Step 2:** Add the `CoreApi` methods and their implementations. Update `test/support/fixtures.dart`'s `snapshot(...)` with `privateMessages: true`, and give `FakeCoreApi` whatever the existing tests now call.
- [ ] **Step 3:** Write `PrivateKeyStore`, with a doc comment saying what the key protects.
- [ ] **Step 4: Run** core clippy, `cd app && flutter analyze && flutter test`. Expect: everything passes as before. This task adds no behavior of its own; Task 17 tests it.
- [ ] **Step 5: Commit** `feat(core): private messages API for the app`.

### Task 17: Models, repository, mock, state and navigation

**Owner:** Gemini. **Score:** 3.
**Review focus:** events reach each listener once (`_route`); load after subscribing; the mock answers like the core; the name, presence and unread rules.

**Files:**
- Create: `app/lib/core/model/private.dart`, `app/lib/core/rust/private_mapping.dart`, `app/lib/core/mock/mock_private.dart`, `app/lib/core/providers/private_state.dart`, `app/test/unit/private/{private_mapping,private_state,mock_private}_test.dart`
- Modify: `core/repository/{repository,events}.dart`, `core/rust/rust_repository.dart`, `core/mock/mock_repository.dart`, `core/providers/providers.dart`, `core/model/server.dart` (or wherever the Ready snapshot is mapped: `privateMessages`), `features/shell/navigation.dart`, `app_start.dart`

**Interfaces:**
- Consumes: Task 16.
- Produces:
  - **Models** (`core/model/private.dart`, immutable, with `copyWith` where state changes):
    - enums `FriendStatus { none, outgoing, incoming, friends }`, `ConversationStatus { none, request, accepted }`, `PrivateMessageKind { text, divider, moved, undecryptable }`, `PrivateSendState { received, sending, sent, failed }`;
    - `PrivateContact` (`peer`, `fingerprint`, `name`, `friend`, `requestSpam`, `blocked`, `conversation`, `relayServerKey`, `lastActivity` as `DateTime?`, `unread`, `muted`, `hidden`, `preview`);
    - `PrivateMessage` (`id`, `peer`, `outgoing`, `kind`, `content`, `serverKey`, `sentAt`, `editedAt`, `state`);
    - `PrivacySettings` (`allowFriendRequests`, `allowMessageRequests`).
  - **`OpencordRepository`** methods (mirroring the core; `RustRepository` calls through `_call`):
    - `privateContacts`, `privateMessages(peer, {before, limit = 50})`;
    - `sendPrivateMessage`, `editPrivateMessage`, `deletePrivateMessage`, `retryPrivateMessage`;
    - `sendFriendRequest(serverKey, userId)`, `answerFriendRequest(peer, {required bool accept})`, `cancelFriendRequest`, `removeFriend`;
    - `acceptMessageRequest`, `ignoreMessageRequest`;
    - `blockUser`, `unblockUser`, `privacySettings`, `savePrivacySettings`;
    - `openConversation(serverKey, userId)`, `moveConversation(peer, serverKey)`, `moveConversationToo(peer, messageId)`;
    - `sendPrivateTyping`, `markPrivateRead`, `setPrivateMuted(peer, {required bool muted})`, `hideConversation`.
  - **Repository state:**
    - `RepoException? get privateUnavailable`, `bool get privateStartedFresh` and `void acknowledgePrivateStartedFresh()`.
    - The app finds shared servers from its own state (below), so `private_shared_servers` isn't on the repository.
  - **`RepoEvent`s:**
    - `PrivateContactChanged(contact)`, `PrivateContactRemoved(peer)`;
    - `PrivateMessageAdded(message)`, `PrivateMessageChanged(message)`, `PrivateMessageDeleted(peer, messageId)`;
    - `PrivateTypingStarted(peer)`, `FriendRequestArrived(peer, spam)`;
    - `PrivateStateChanged()`: opened, started fresh, or became unavailable.
  - **Providers** (`private_state.dart`):
    - `privateContactsProvider`: a `Notifier` over `{byPeer, loaded, error}` that loads in `build` and applies events.
    - `privateConversationProvider(peer)`: a family over `{messages oldest-first, loading, hasMore, error}`, with `loadOlder()`; ordered by (`sentAt`, `id`).
    - `privateTypingProvider`: peer → until, 8 s after each event. An entry leaves when it expires, pruned on every change and by a timer, as `typingProvider` does.
    - `contactNameProvider(peer) → String`: spec §7.5, the relay server's member name, then the first shared server in rail order, then `contact.name`, then the fingerprint.
    - `contactAvatarProvider(peer) → ({String serverKey, int userId})?`: the same order.
    - `contactPresenceProvider(peer)`: spec §7.5.
    - `sharedServersProvider(peer) → List<SharedServerView>`: servers in rail order where this device has the data, it's connected, `privateMessages` is set, and a member has `user.publicKeyHex == peer`. `SharedServerView` is `{serverKey, name, peerUserId}`.
    - `privateUnreadProvider → int`: `unread` of contacts that are accepted, not muted and not blocked, plus incoming requests that aren't spam.
  - **`navigation.dart`:**
    - `Selection` gains `home` (`bool`, default `false`) and `peer` (`String?`), saved in the same JSON under `home` and `peer`;
    - `openHome()`, `openFriends()` (peer cleared) and `openConversation(String peer)`; `openServer` and `openChannel` set `home` to `false`;
    - `homeShownProvider = Provider<bool>`: `home`, or the server list is empty;
    - `currentPeerProvider = Provider<String?>`: the selection's peer, if it's a contact that isn't blocked.

Behavior:
- **`_route`** applies the private events to the notifiers. A conversation that hasn't been loaded (`ref.exists` is false) isn't created by an event.
- **`unreadTotalProvider` and `mentionTotalProvider`** both add `privateUnreadProvider`.
- **Startup** (`app_start.dart`): when not in mock mode, load the key with `PrivateKeyStore(profile: profile).load()` next to `_startMediaCache`. A `PlatformException` leaves the key `null`, logged. Pass the key to `RustRepository`.
- **`RustRepository`:**
  - `start()` also subscribes to `privateEventStream()`;
  - after `identityLoad` in `_use`, it opens with `privateOpen(key)` (unawaited, with its own `on core.CoreError` catch), then emits `PrivateStateChanged`;
  - a `null` key gives `privateUnavailable` = "Private messages need the system keyring, which couldn't be used." (`RepoErrorKind.other`).
- **`MockRepository`** (`mock_private.dart`):
  - **Data, all greyscale, made from mock members:**
    - 3 friends (2 online);
    - 1 incoming request and 1 spam request;
    - 1 outgoing request;
    - 2 message requests with a message each;
    - 1 blocked person with no shared server and the last seen name "Old Friend";
    - 2 accepted conversations of about 6 messages, one with a divider at the top and a move card.
  - **Behavior:** every action changes the mock world and emits the same events as the core.
    - A send emits `PrivateMessageAdded` (sending), then after 300 ms (immediately with `simulateLife: false`) `PrivateMessageChanged` (sent) and `PrivateContactChanged`.
    - `sendFriendRequest` to the mock member named "Quiet Quinn" throws `RepoException(RepoErrorKind.friendRequestsClosed, ...)`.

- [ ] **Step 1: Write the failing tests:**
  - `private_mapping_test.dart`: each enum value, and nulls, map both ways they're used.
  - `private_state_test.dart`, on `MockRepository(simulateLife: false)`:
    - contacts load once events are routed;
    - a conversation orders by time, then id, and `loadOlder` adds older ones in front;
    - typing expires after 8 s (a fake clock);
    - the name rule, case by case;
    - the presence rule;
    - `privateUnreadProvider` leaves out muted, blocked, spam and requests;
    - `homeShownProvider` is true with no servers;
    - the selection's `home` and `peer` survive a reload.
  - `mock_private_test.dart`: a send turns `sent`; accepting a request makes friends; blocking ends a friendship; "Quiet Quinn" throws `friendRequestsClosed`.
- [ ] **Step 2: Run** `cd app && flutter test test/unit/private` and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `flutter analyze && flutter test`. Expect: pass.
- [ ] **Step 5: Commit** `feat(app): private messages state, repository and mock`.

### Task 18: The Friends page and error dialogs

**Owner:** Gemini. **Score:** 2.

**Files:**
- Create: `app/lib/features/home/{friends_page,friend_rows,private_errors}.dart`, `app/test/widget/home/friends_page_test.dart`

**Interfaces:**
- Consumes: Task 17.
- Produces:
  - `FriendsPage` (the main area when no conversation is open).
  - In `private_errors.dart`:
    - `Future<void> showPrivateFailure(BuildContext context, RepoException error, {required String name, String? server, PrivateAction action = PrivateAction.message})`, with `enum PrivateAction { message, friendRequest, answer, move }`;
    - `confirmBlock(context, name)` and `confirmRemoveFriend(context, name)` over `confirmAction`.

`showPrivateFailure` shows an `OcDialog` with the title and text of spec §6.7 for the private kinds:
- `friendRequest` uses the friend request texts; `move` uses "Couldn't move the chat: *name* can't get private messages through *server*.";
- `conflict` on a friend request picks the sentence by the contact's state;
- other kinds keep the app's existing handling (`showOcToast(context, error.message)`).

The page:
- **Header:** 60 px, the title "Friends", and `ChoiceChips` tabs: Online · All · Pending · Message requests · Blocked. The tab is remembered under the key-value store key `ui.friendsTab`.
- **Each tab:** a `SectionLabel` with its count ("ONLINE — 3"), then the rows of spec §6.2.
- **Empty texts, exactly:** "No one's online right now.", "You haven't added any friends yet.", "No pending requests.", "No message requests.", "You haven't blocked anyone.".
- **Pending:**
  - incoming requests (Accept ✓ `OcIcons.check`, Decline ✗ `OcIcons.close`);
  - outgoing requests (Cancel ✗);
  - then a "Spam — n" row that expands. It starts collapsed every time.
- **Rows:**
  - a friend row opens the conversation (`openConversation` on the repository with the first shared server, then `navigation.openConversation`);
  - the ⋯ menu has Remove friend and Block, each confirmed;
  - a message request row shows `preview` and opens the conversation;
  - Blocked rows show `contactNameProvider` and Unblock.
- **Every action** awaits, checks `mounted`, and shows `showPrivateFailure` on `RepoException`.

- [ ] **Step 1: Write the failing widget tests** (with `MockApp`):
  - `each_tab_lists_its_people_with_a_count`;
  - `empty_tabs_say_so`;
  - `accepting_and_declining_update_pending`;
  - `cancelling_an_outgoing_request_removes_it`;
  - `spam_requests_wait_in_a_collapsed_section`;
  - `removing_a_friend_and_blocking_ask_first`;
  - `unblocking_moves_them_out_of_blocked`;
  - `a_failed_action_shows_its_dialog`: answering while the request's server is disconnected.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `flutter analyze && flutter test`. Expect: pass.
- [ ] **Step 5: Commit** `feat(app): the Friends page`.

### Task 19: The conversation view

**Owner:** Gemini. **Score:** 3.
**Review focus:** reuse of the chat rows; the composer's keys; every disabled state renders its reason; Move too only when it can work.

**Files:**
- Create: `app/lib/features/home/{private_chat,private_header,private_message_list,private_composer,relay_divider,move_card,request_bar,private_message_adapter}.dart`, `app/test/widget/home/private_chat_test.dart`

**Interfaces:**
- Consumes: Tasks 17 and 18.
- Produces:
  - `PrivateChat({required String peer})`: the main area for a conversation.
  - `Message asChatMessage(PrivateMessage m)` (`private_message_adapter.dart`):
    - `id` is the first 8 bytes of the hex id read as a signed 64-bit integer; `channelId: 0`;
    - `authorId` is `-1` for outgoing and `-2` for the peer;
    - `sendState` is mapped, `attachments` empty.

    It's used only to feed `MessageItem`, `MessageLine` and `MessageBubble` / `CompactMessage` (chosen by `MessageDensity`, as the channel list does). The bubbles get the relay's server key, or `''`; with no attachments they never use it.

The view, top to bottom:
- **Header** (`private_header.dart`):
  - the avatar with presence, the name, and the presence label;
  - a lock `OcIconButton`, with the tooltip from spec §6.3. It opens a small `OcDialog` with the identity key fingerprint in `OcText.mono` and a Copy button, as the profile's identity section shows it;
  - the relay chip, "via *Server*" with ▾. It opens `showOcMenu` with `sharedServersProvider(peer)`, the current relay checked. Picking another calls `moveConversation`; a failure goes to `showPrivateFailure(action: move)`.
  - The chip shows the warning style, and the composer the reason, when the relay isn't usable.
  - When the contact's relay is set but `relayServerKey` is null, a line under the header says: "This chat goes through *Server*, which this device hasn't joined."
- **The list** (`private_message_list.dart`):
  - newest at the bottom; `loadOlder` when scrolled near the top; a load failure with a retry, like the channel list's;
  - **message actions:** Edit and Delete on one's own text (Delete confirmed as in channels), Copy text on all;
  - **a failed message:** Retry (`retryPrivateMessage`) and Delete (`deletePrivateMessage`, local).
- **Divider** (`relay_divider.dart`): a wavy line (`CustomPainter`, a sine wave in `colors.textMuted`) on each side of the server's icon (`ServerAvatar`, 16 px) and its name in `OcText.small`; the name opens the server when this device has it. (Changed from a hatched band on 2026-10-24 at the owner's request.)
- **Move card** (`move_card.dart`): "*Name* moved this chat to *Server*".
  - **Move too** appears only when the card's `serverKey` is saved and connected on this device, and the peer is a member there (from the app's state); it calls `moveConversationToo`.
  - Otherwise the card reads "*Name* moved this chat to *Server*, a server this device isn't on."
- **Undecryptable line:** centered, muted, `OcIcons.lock` and "This message couldn't be decrypted.".
- **Typing:** `TypingDots` and "*Name* is typing…" while `privateTypingProvider` has the peer.
- **Bottom:** one of the following.
  - Request mode (conversation `request`): `RequestBar` with "*Name* wants to message you." and Accept / Ignore / Block. Ignore asks first, then deletes the conversation and opens Friends.
  - Blocked: "You blocked *name*." and Unblock.
  - No usable relay: "You don't share a connected server with *name* right now.". No shared server at all: "You no longer share a server with *name*.".
  - Otherwise `PrivateComposer`.
- **`PrivateComposer`:**
  - the channel composer's keys: Enter sends, Shift+Enter adds a line, ↑ in an empty field edits your last message, Esc cancels editing; no sending while composing with an IME;
  - `ComposerContextBar` while editing; a counter near 4,000 characters, counted in runes;
  - no attach, mic, commands or slash suggestions;
  - typing calls `sendPrivateTyping` at most every 8 s while there's text.
- **Read state:** while the chat is shown and the window focused, `markPrivateRead`, debounced 500 ms, again on each incoming message.

- [ ] **Step 1: Write the failing widget tests:**
  - `messages_show_in_order_under_the_first_divider`;
  - `own_messages_can_be_edited_and_deleted`;
  - `a_failed_message_retries`;
  - `move_too_appears_only_when_it_can_work`;
  - `the_request_bar_accepts_ignores_and_blocks`;
  - `each_disabled_state_says_why` (blocked, no usable relay, no shared server);
  - `the_relay_chip_lists_shared_servers_and_moves`;
  - `typing_shows_and_goes_away`;
  - `enter_sends_and_shift_enter_adds_a_line`;
  - `the_lock_shows_their_fingerprint`;
  - `opening_the_chat_marks_it_read`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `flutter analyze && flutter test`. Expect: pass, `monochrome_test.dart` included.
- [ ] **Step 5: Commit** `feat(app): private conversations`.

### Task 20: The rail button, the home view and its sidebar

**Owner:** Gemini. **Score:** 3.
**Review focus:** the rail's states; the shell switching; keyboard shortcuts in the home view; nothing lost when the Settings button goes.

**Files:**
- Create: `app/lib/features/home/{home_sidebar,home_main}.dart`, `app/test/widget/home/home_shell_test.dart`
- Modify: `features/servers/server_rail.dart`, `features/shell/desktop_shell.dart`, `features/shell/shortcuts.dart` (if Alt+↑/↓ need guarding), existing shell tests that assert the old top button

**Interfaces:**
- Consumes: Tasks 17–19.
- Produces: `HomeSidebar`, `HomeMain`, and the rail's home entry.

The changes:
- **Rail** (`server_rail.dart`):
  - Replace the top `OcIconButton(icon: OcIcons.menu, tooltip: 'Settings', …)` with a home entry. It's a `RailItemView` with a new optional `glyph` (`IconData?`, drawn centered instead of initials, in the initials' color), `name: 'Private'`, tooltip "Private messages".
  - It's selected while `homeShownProvider` is true. `mentions` is `privateUnreadProvider`, shown with the existing count badge. A tap calls `openHome()`. It has no menu.
  - A server entry is selected only when it's the current server and the home view isn't shown.
  - The divider under the top entry and the macOS traffic-light spacer stay.
- **Shell** (`desktop_shell.dart`), while `homeShownProvider`:
  - the sidebar column shows `HomeSidebar` and the main column `HomeMain`;
  - the member panel and its toggle are hidden;
  - Alt+↑/↓ channel navigation does nothing.
- **`HomeSidebar`:**
  - a 60 px header "Private messages";
  - a **Friends** row (`OcIcons.group`) with the incoming non-spam count; a **Message requests** row (`OcIcons.mail`) with its count;
  - `SectionLabel('DIRECT MESSAGES')`, then the conversations: accepted, not hidden, not blocked, newest first;
  - each conversation row:
    - avatar with presence (`MemberAvatar` through `contactAvatarProvider`, else the initials `Avatar`), the name, and an unread badge (bold when muted, with no count);
    - a ✕ on hover (`OcIconButton`, tooltip "Close conversation") that calls `hideConversation`;
    - a right-click menu: Mute notifications / Unmute notifications, Close conversation, — , Block (confirmed);
  - the existing `UserPanel` at the bottom.
- **`HomeMain`:**
  - `privateUnavailable` set → an `ErrorBox` with its message;
  - otherwise, with no servers and no contacts → `NoServersView`;
  - otherwise, `currentPeerProvider` set → `PrivateChat`; else `FriendsPage`.
  - When `privateStartedFresh`, show the spec §11 sentence once in an `OcDialog`, then call `acknowledgePrivateStartedFresh()`.

- [ ] **Step 1: Write the failing widget tests:**
  - `the_rail_button_opens_the_home_view_and_shows_its_badge`;
  - `servers_are_not_selected_while_home_is_open`;
  - `settings_still_open_from_the_user_panel_and_the_shortcut`;
  - `conversations_are_listed_newest_first_with_unread_badges`;
  - `closing_a_conversation_hides_it_until_its_next_message`;
  - `the_home_view_is_remembered`;
  - `with_no_servers_the_home_view_shows_the_join_choices`;
  - `a_fresh_start_is_explained_once`.

  Update existing shell tests that looked for the old Settings rail button, and only those assertions. Name each one in the ledger.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `flutter analyze && flutter test`. Expect: pass, the goldens included. If a sidebar or rail golden changes because the top button changed, regenerate only that golden with `flutter test --update-goldens <file>`, look at the image, and name it in the ledger.
- [ ] **Step 5: Commit** `feat(app): the private messages button and home view`.

### Task 21: Profiles, the member menu and the Privacy page

**Owner:** Gemini. **Score:** 2.

**Files:**
- Create: `app/lib/features/settings/user/privacy_page.dart`, `app/test/widget/members/profile_private_test.dart`, `app/test/widget/settings/privacy_page_test.dart`
- Modify: `features/members/member_profile.dart`, `features/members/member_actions.dart`, `features/settings/user_settings.dart`

**Interfaces:**
- Consumes: Tasks 17 and 18.

The changes:
- **Profile** (`member_profile.dart`):
  - **Message:** the placeholder (`Tooltip(message: 'Direct messages are coming in a later phase', …)`) becomes `OcButton.primary(label: 'Message')`. It calls `openConversation(serverKey, userId)`, closes the dialog, then calls `openConversation(peer)` on the navigation.
  - **The friend button**, by the contact's state:
    - "Add friend" (`OcIcons.personAdd`) → `sendFriendRequest`;
    - "Request sent" → `cancelFriendRequest`, then the toast "Friend request cancelled";
    - "Accept request" → `answerFriendRequest(accept: true)`;
    - "Friends" with `OcIcons.check`, disabled.
  - **A ⋯ menu** with Block (confirmed) or Unblock.
  - None of these show on your own profile.
  - When the server doesn't relay private messages, or `privateUnavailable` is set, Message and the friend button are disabled, with the tooltip "This server doesn't relay private messages." or that error.
  - Failures go to `showPrivateFailure` with the right `PrivateAction`.
- **Member menu** (`member_actions.dart`): Message, the friend action, and Block / Unblock, not on yourself.
- **Privacy page:**
  - `SettingsPage(id: 'privacy', label: 'Privacy', icon: OcIcons.lock, group: 'You', builder: (_) => const PrivacyPage())`, after `identity`;
  - two `OcSwitch` rows with spec §6.5's labels and lines;
  - the save bar through `SaveBarChanges`. The saved values are loaded from `privacySettings()` and kept in a field; `saveChanges` calls `savePrivacySettings` and throws `RepoException` on failure;
  - an `ErrorBox` when `privateUnavailable` is set.
- Grep for "coming in a later phase" and remove the private messages one; the others stay.

- [ ] **Step 1: Write the failing widget tests:**
  - `profile_buttons_follow_the_friend_state` (each of the four states, and their actions);
  - `your_own_profile_has_no_private_buttons`;
  - `blocking_from_a_profile_asks_first`;
  - `message_opens_the_conversation`;
  - `privacy_changes_wait_for_the_save_bar` (toggle → the bar → Save sends both values; Discard restores);
  - `a_failed_privacy_save_is_shown`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `flutter analyze && flutter test`. Expect: pass.
- [ ] **Step 5: Commit** `feat(app): friend and message buttons, and privacy settings`.

### Task 22: Notifications, badges and links

**Owner:** Gemini. **Score:** 3.
**Review focus:** what does and doesn't notify (spec §6.6); one listener per event.

**Files:**
- Modify: `features/desktop/notification_binding.dart`, `features/desktop/notification_policy.dart`, `features/links/app_links.dart`, `features/shell/desktop_shell.dart` (opening the new links)
- Create: `app/test/unit/desktop/private_notifications_test.dart`, `app/test/unit/links/private_link_test.dart`

**Interfaces:**
- Consumes: Tasks 17–21.
- Produces:
  - `notifyPrivate({required AppSettings settings, required bool windowFocused, required bool chatOpen, required PresenceStatus presence}) -> NotifyAs`, following the channel rules: do not disturb gives none; a focused window with the chat open gives none.
  - Links: `PrivateAppLink(peer)` for `opencord://private/<64 hex>` and `FriendsAppLink()` for `opencord://friends`.

What notifies:
- **`PrivateMessageAdded`:** incoming text in an accepted conversation that isn't muted and isn't from a blocked contact. The title is the name; the body is the content, previewed as channel messages are (200 characters, the preview setting); the link is the private link; the sound is `settings.messageSound`. It also asks for attention (`setUrgent`) while the window is unfocused, like a mention.
- **`FriendRequestArrived` with `spam: false`:** "*Name* sent you a friend request", linking to `opencord://friends`.
- **Nothing else:** requests, spam, answers, own copies, dividers, cards.

The binding listens to the repository's events, as it does for `MessageCreated`. `openAppLink` handles the new links with `openConversation(peer)` and `openFriends()`.

- [ ] **Step 1: Write the failing tests:**
  - `private_messages_notify_only_in_accepted_unmuted_conversations` (requests, muted, blocked and own copies don't);
  - `do_not_disturb_and_an_open_chat_silence_them`;
  - `friend_requests_notify_unless_spam`;
  - `the_window_title_counts_private_unread`;
  - `private_and_friends_links_parse_and_open`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `flutter analyze && flutter test`. Expect: pass.
- [ ] **Step 5: Commit** `feat(app): private message notifications and links`.

**Milestone E4 stop:** full checks, then D65 from `git diff`, then stop and report.

---

# Milestone E5: Hardening and docs

### Task 23: Security pass

**Owner:** Claude. **Score:** 5.

**Files:** whatever the findings touch, plus `crates/opencord-core/tests/private.rs` and `crates/opencord-server/tests/private.rs`.

Go through `git diff beta..HEAD` against each item. Fix what fails, with a test that would have caught it, and list every item with its result in the report.
1. **Logs:** no `tracing` call in the private modules (server and core) or the app's private code prints a key, pickle, slip, ciphertext, plaintext or name.
2. **Server limits:**
   - every private handler checks `enabled()`, the session's device where the plan says, membership, shapes, and rates;
   - no answer can pass 1 MiB: 10 devices; 20 bundles; a pending page at 786,432 bytes plus envelope.
3. **Client checks:** device signatures are checked on every `FetchDevices` answer and before decrypting; prekey signatures on every claim; slips before they're stored. The forged-device tests exist and pass.
4. **Duplicates and replays** never show anything; tests pin both.
5. **At rest:** add `the_database_file_holds_no_plaintext`. After a conversation with a known sentence and a contact named "Zebulon Quartz", read the raw bytes of `private/<hex>.db` and its WAL: neither string appears.
6. **Maps:** the device-list cache, acknowledgement batches, the reset map, the member index, the server's privacy cache and the app's typing map each lose their entries as their task says. Add a test for any without one.
7. **Blocks don't show:** a blocked sender gets `UNDELIVERABLE` for messages and friend requests, and typing gets success.
8. **`SessionConfig::version_1()`** everywhere, and nothing hand-written in place of vodozemac.
9. **Licences:** the check in `GEMINI.md`, for both new dependencies, on every target.

- [ ] **Step 1:** Work through the list. Each fix gets its failing test first.
- [ ] **Step 2: Run** the full checks. Expect: pass.
- [ ] **Step 3: Commit** `fix: private messages security pass` (or, with no fixes, `test: private messages security pass`).

**Checks (for Gemini):** the full checks.

### Task 24: Docs and checks by hand

**Owner:** Gemini. **Score:** 1.

**Files:**
- Create: `docs/private-messages.md`
- Modify: `docs/protocol.md`, `README.md`, `docs/plans/desktop-ui.md` (§4.1's first line)

**`docs/private-messages.md`**, for people who use Opencord and people who run servers. Every number and name comes from the code:
- what's encrypted and how (vodozemac, a key per message, devices signed by the identity);
- what servers see and can and can't do (spec §10.9);
- relay servers and moving a chat;
- offline delivery and its limits (the four settings, the cap and expiry, a device only getting messages through servers it has joined, new devices not getting history);
- friends, message requests, spam, blocks and the privacy settings;
- the local database (where it is, what's sealed, the keychain key, a fresh start);
- the `[private_messages]` config section;
- **"Checking it by hand"**, steps on two machines or two profiles:
  - a friend request with one side offline;
  - messages both ways with one side offline;
  - a second device of one identity;
  - moving a chat and Move too;
  - message requests on and off;
  - blocking;
  - restarting both apps mid-conversation.

**`docs/protocol.md`:**
- the 14 requests in the requests table, with who may send them;
- the two events in the audience table;
- the five error codes;
- a "Private messages" section: registration, pending events and acknowledgements, paging, the order of checks for messages and friend requests, permission slips, the signing payloads' bytes, and the payload format.

**Also:**
- `README.md`: a line under the features.
- `docs/plans/desktop-ui.md` §4.1: "Top: the Private messages button (opens the home view). Settings open from the user panel's cog."

- [ ] **Step 1:** Write the docs from the code (`GEMINI.md` → "Write docs from the code"), then grep each name you used.
- [ ] **Step 2: Commit** `docs: private messages`.

**Milestone E5 stop:** full checks, then D66 from `git diff` for Tasks 23 and 24, then stop and report. The branch then waits for the user's review; nothing is merged into `beta` without them.
