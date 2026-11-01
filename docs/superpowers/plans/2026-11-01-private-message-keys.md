# Private Message Keys Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Private messages trust devices by a per-person chat key, kept apart from the identity key, checked by a 4×4 grid of words; old chats start fresh.

**Architecture:** `opencord-common` gets the v2 device signature and the words. The server checks v2 signatures, accepts withheld devices in a send, and drops old state in a migration. The core (`crates/opencord-core/src/private/`) gains a chat key, own-device approval (an Olm-carried `ChatKeyShare`), and per-contact pinned keys that decide which devices get messages. The Flutter app shows the words, lock states, banners and the approval flow.

**Tech Stack:** Rust (ed25519-dalek, sha2, vodozemac 0.11.1, sqlx/SQLite, prost), flutter_rust_bridge 2.13.0, Flutter + Riverpod.

**Spec:** `docs/superpowers/specs/2026-11-01-private-message-keys-design.md` (read it first; section numbers below are its).

## Global Constraints

- Every `cargo`, `flutter`, `dart`, `sqlx` and `flutter_rust_bridge_codegen` command starts with `source .superpowers/sdd/media/env.sh &&`. `CARGO_INCREMENTAL=0`. Check `df -h /` before full test runs; never `cargo clean`.
- Desktop only. The phone app (`mobile/`, `crates/opencord-mobile`) gets no private-message features, but must still build and pass `.github/check.sh` (exhaustive matches on new core enums there need arms).
- The identity key is used for nothing in device trust or words. Permission slips stay as they are.
- Context strings, exactly: `"opencord-device-v2"` (18 bytes), `"opencord-words-v1"`, `"opencord-own-words-v1"`.
- Words: SHA-512(context || lower key || higher key), first 176 bits, 11 bits per word, MSB first, BIP-39 English list.
- Field numbers: `DeviceKeys.chat_key = 5`, `SendPrivateMessage.withheld = 5` (repeated `DeviceRef`), `Ready.private_messages_v2 = 23`, `PrivatePayload` body `ChatKeyShare chat_key_share = 16` (`bytes secret = 1`).
- Copy is the spec's, word for word (§7, §8, §9.3, §10).
- Server SQL changes need `.sqlx` regenerated (README "Changing database queries"); bridge API changes need `flutter_rust_bridge_codegen generate` from `app/` and the generated files committed.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Branch `feat/private-message-keys`; one PR into `beta`, held open with the bug batch.

## Review Focus

1. **A device re-registering with a new chat key** (after approval) must replace its old listing everywhere, and peers must then trust it without a banner: test in Task 4 (`an_approved_device_is_trusted_by_peers_without_a_banner`).
2. **A message arriving from a held device before the user answers the banner** shows the "keys you haven't accepted" line and is not decrypted into the chat: Task 5 (`a_message_from_held_keys_is_not_shown_as_text`).
3. **Both of the peer's key sets listed at once** (old devices not yet expired, new key on a reinstall): banner offers the key on most devices, old-key devices keep getting messages until accepted: Task 5 (`old_keys_keep_working_until_the_new_ones_are_accepted`).
4. **The app restarting between a share arriving and They match**: the share must still be waiting (sealed, in the database), not lost: Task 4 (`a_waiting_share_survives_a_restart`).
5. **Sending when every listed device is held** must be refused before anything is encrypted or saved as sending: Task 5 (`sending_with_only_held_devices_is_refused`).

---

### Task 1: Words and v2 device signatures in `opencord-common`

**Files:**
- Create: `crates/opencord-common/src/words.rs`, `crates/opencord-common/src/words/english.txt`
- Modify: `crates/opencord-common/src/lib.rs`, `crates/opencord-common/src/private.rs`

**Interfaces:**
- Produces:
  - `opencord_common::words::chat_words(a: &[u8; 32], b: &[u8; 32]) -> [&'static str; 16]`
  - `opencord_common::words::pairing_words(a: &[u8; 32], b: &[u8; 32]) -> [&'static str; 16]`
  - `opencord_common::private::device_payload_v2(device_id: &[u8; 16], curve25519: &[u8; 32], ed25519: &[u8; 32]) -> [u8; 98]`
  - `opencord_common::private::sign_device_v2(chat_key: &SigningKey, device_id, curve25519, ed25519) -> [u8; 64]`
  - `opencord_common::private::verify_device_v2(chat_key: &[u8], device_id: &[u8], curve25519: &[u8], ed25519: &[u8], signature: &[u8]) -> bool` (any wrong length → false; `verify_strict`)
  - The v1 `device_payload`/`sign_device`/`verify_device` stay until Task 3 removes them.

- [ ] **Step 1: Add the word list.** Download `https://raw.githubusercontent.com/bitcoin/bips/master/bip-0039/english.txt` into `crates/opencord-common/src/words/english.txt`. Check: 2048 lines, first `abandon`, last `zoo`, all distinct. Load it with `include_str!` and split on `'\n'` into a `LazyLock<[&str; 2048]>` (or `std::sync::OnceLock`); a header comment in `words.rs` names the source.

- [ ] **Step 2: Write the failing tests** in `words.rs`:
  - `words_are_the_same_in_either_order`: `chat_words(&[1;32], &[2;32]) == chat_words(&[2;32], &[1;32])`.
  - `another_key_gives_other_words`: `chat_words(&[1;32], &[2;32]) != chat_words(&[1;32], &[3;32])`.
  - `pairing_words_differ_from_chat_words`: same keys, `pairing_words != chat_words`.
  - `the_words_are_pinned`: `chat_words(&[1;32], &[2;32])` equals a 16-word vector computed once by the implementation and pasted into the test, so a change to the algorithm can't pass unnoticed. Also assert `words[0..4]` came from bits 0–43 by checking a hand-computed index: the first word is `WORDS[(digest[0] as usize) << 3 | (digest[1] >> 5) as usize]`.
  - in `private.rs`: `v2_device_signatures_verify_only_against_their_chat_key` (signs with key A; verifies with A; fails with B; fails with a flipped byte in each of device_id, curve25519, ed25519) and extend `payload_layouts_are_fixed` with the v2 layout (`&payload[..18] == b"opencord-device-v2"`, then id, curve, ed).

- [ ] **Step 3: Run them to see them fail.** `source .superpowers/sdd/media/env.sh && cargo test -p opencord-common words private` → compile errors for the missing functions.

- [ ] **Step 4: Implement** the functions above. The 11-bit split reads the digest as a big-endian bit string.

- [ ] **Step 5: Run the tests to see them pass**, then `cargo clippy -p opencord-common --all-targets -- -D warnings`.

- [ ] **Step 6: Commit** `feat(common): chat key device signatures and the 4x4 words`.

---

### Task 2: Protocol and server

**Files:**
- Modify: `proto/opencord/v1/private.proto`, `proto/opencord/v1/gateway.proto`, `crates/opencord-proto/tests/private.rs`
- Modify: `crates/opencord-server/src/handlers/private.rs` (`register_device`, `check_device_keys`, `check_targets`, `send_private_message`), `crates/opencord-server/src/db/private.rs` (`DeviceRow` gains `chat_key`), `crates/opencord-server/src/gateway/identify.rs:375`
- Create: `crates/opencord-server/migrations/0013_private_message_keys.sql`
- Modify: `crates/opencord-server/tests/common/mod.rs` (`TestDevice` gains `chat: SigningKey`; its keys are signed with `sign_device_v2`), `crates/opencord-server/tests/private.rs`
- Modify: `.sqlx/` (regenerated)
- Modify (only to compile): core's `proto::DeviceKeys { .. }` and `proto::SendPrivateMessage { .. }` literals get `chat_key: Vec::new()` / `withheld: Vec::new()`; Task 3 fills them.

**Interfaces:**
- Produces (proto):
  - `DeviceKeys.chat_key` (5, 32 bytes); `signature` now "by the chat key over opencord-device-v2 || device_id || curve25519 || ed25519".
  - `SendPrivateMessage.withheld` (5): `repeated DeviceRef`.
  - `Ready.private_messages_v2` (23): `bool`.
  - `message ChatKeyShare { bytes secret = 1; }`, `PrivatePayload.body.chat_key_share = 16`.
- Server behaviour: `register_device` checks `chat_key` is 32 bytes and `verify_device_v2(chat_key, …)`, else `INVALID_ARGUMENT` "the device's signature does not match its chat key". Ready sends `private_messages = false`, `private_messages_v2 = state.private.enabled()`.

- [ ] **Step 1: Proto changes** as above, with comments in the files' style. Extend `field_numbers_are_the_planned_ones` with the four new tags, and `pending_events_and_payloads_round_trip` with a `ChatKeyShare` body.

- [ ] **Step 2: Write the failing server tests** in `crates/opencord-server/tests/private.rs`:
  - rename `a_device_signed_by_someone_else_is_refused` → `a_device_signed_by_another_chat_key_is_refused`: keys signed by chat key A, `chat_key` field B → `INVALID_ARGUMENT`.
  - `an_identity_signature_is_no_longer_enough`: v1 signature by the identity key, `chat_key` = identity public key, signature over the v1 payload → refused.
  - `withheld_devices_count_as_covered`: Bob has devices 2 and 3; Alice sends copies to 2 and withholds 3 → `PrivateMessageSent`; only device 2 gets a pending event.
  - `a_device_both_sent_and_withheld_is_refused`: copy to 2, withheld 2 and 3 → `DEVICE_LIST_CHANGED`.
  - `a_send_that_misses_a_device_is_still_refused`: copy to 2, nothing for 3 → `DEVICE_LIST_CHANGED` (the existing `a_stale_device_set_is_refused` may already cover it; keep one).
  - `ready_says_whether_private_messages_are_on` asserts `private_messages == false` and `private_messages_v2 == true`, and both false when disabled.
  - `the_migration_keeps_friends_and_drops_devices`: on a database migrated to 0012 with a device, a pending event and a friendship, running 0013 leaves the friendship and no device or pending row. (Use the migration test pattern already in the server's tests; if none exists, apply the SQL files with `sqlx::migrate!` up to 0012 via a `Migrator` filtered by version, insert rows, then run the rest.)

- [ ] **Step 3: Run them to see them fail.** `cargo test -p opencord-server --test private`.

- [ ] **Step 4: Implement.**
  - Migration 0013: `DELETE FROM private_pending; DELETE FROM private_one_time_keys; DELETE FROM private_devices; ALTER TABLE private_devices ADD COLUMN chat_key BLOB NOT NULL DEFAULT x'';` (the table is empty, the default only satisfies SQLite), plus `private_recent_sends` emptied too.
  - `check_targets(conn, caller, recipient, device_id, targets, withheld)`: `given ∩ withheld` must be empty and `given ∪ withheld == expected`. A recipient with devices that are all withheld is still accepted (the app decides; Task 5 refuses before that point).
  - `DeviceRow.chat_key` stored and listed back in `DeviceKeys`.
  - Regenerate `.sqlx` (README commands, `DATABASE_URL="sqlite://$PWD/target/sqlx-dev.db"`).

- [ ] **Step 5: Run** `cargo test -p opencord-server -p opencord-proto -p opencord-common` → PASS; `cargo clippy` on the server crates. Core's private tests now fail against the new server; that's expected until Task 3 (don't run them yet).

- [ ] **Step 6: Commit** `feat(server): chat key device signatures, withheld devices, fresh private state`.

---

### Task 3: The core's chat key, fresh database and registration

**Files:**
- Create: `crates/opencord-core/migrations/0002_private_message_keys.sql`, `crates/opencord-core/src/private/chat_key.rs`
- Modify: `crates/opencord-core/src/private/{mod.rs,device.rs,service.rs,outbox.rs,inbox.rs,store.rs}`, `crates/opencord-core/src/convert.rs:446`, `crates/opencord-core/src/identity.rs` (remove `sign_device`), `crates/opencord-common/src/private.rs` (remove v1 device functions), `crates/opencord-core/src/api/types.rs` (`PrivateOpened.keys_reset`)
- Test: `crates/opencord-core/src/private/chat_key.rs` (unit), `crates/opencord-core/tests/private.rs`

**Interfaces:**
- Consumes: Task 1's `sign_device_v2`, `verify_device_v2`; Task 2's proto fields.
- Produces:
  - `pub struct ChatKey` with `async fn load_or_create(store: &PrivateStore, now_ms: i64) -> Result<ChatKey, PrivateError>`, `fn public(&self) -> [u8; 32]`, `fn sign_device(&self, device_id: &[u8;16], curve25519: &[u8;32], ed25519: &[u8;32]) -> [u8; 64]`, `fn secret(&self) -> [u8; 32]` (only for `ChatKeyShare`), `async fn replace(&mut self, conn: &mut SqliteConnection, sealer: &Sealer, secret: [u8; 32]) -> Result<(), PrivateError>`.
  - `Open::chat_key(&self) -> &tokio::sync::Mutex<ChatKey>` (alongside `Open::device()`).
  - `Device::keys(&self, chat_key: &ChatKey) -> proto::DeviceKeys` (fills `chat_key` and the v2 signature).
  - `outbox::VerifiedDevice` gains `chat_key: [u8; 32]`. Interim rule in `outbox::verified`: keep a listed device when `verify_device_v2` passes against its own `chat_key` (Task 5 replaces this with pinning).
  - `PrivateOpened { started_fresh: bool, keys_reset: bool }`.
  - `SharedServer.private_messages` and `ServerSnapshot.private_messages` now come from `ready.private_messages_v2`.

- [ ] **Step 1: Write the failing tests.**
  - unit `a_chat_key_is_made_once_and_kept_sealed`: two `load_or_create` calls on one store return the same `public()`; the raw database bytes don't contain `secret()` (use the existing `raw_bytes`/`contains` approach from `tests/private.rs` or read the column and check it isn't 32 plain bytes).
  - unit `replacing_the_chat_key_changes_its_signatures`.
  - integration `devices_register_with_their_chat_key`: after opening, `FetchDevices` for self shows `chat_key == client.chat_key_public()` (add a `#[cfg(any(test, feature = "testing"))]`-style accessor following how existing tests reach internals; if tests reach state through the public `Client` API only, add `Client::private_chat_key_for_test()` behind the crate's existing test feature).
  - integration `an_old_database_is_reset_once`: open with a database created by migration 0001 only, with a message and a friend; after opening, `keys_reset == true`, no messages, the friend kept; opening again gives `keys_reset == false`.
  - Delete `a_device_the_identity_did_not_sign_is_never_used` and `a_message_from_a_forged_device_is_not_believed`; Task 5 replaces them.

- [ ] **Step 2: Run** `cargo test -p opencord_core --test private` and the unit tests → fail.

- [ ] **Step 3: Implement.**
  - Migration 0002: `DELETE FROM sessions; DELETE FROM peer_devices; DELETE FROM device; DELETE FROM messages; DELETE FROM received_events; UPDATE contacts SET conversation = 0, relay_server_id = NULL, last_activity_at = NULL, last_read_at = 0, hidden = 0;` then `CREATE TABLE chat_key (id INTEGER PRIMARY KEY CHECK (id = 1), public BLOB NOT NULL, secret BLOB NOT NULL /* sealed */, created_at INTEGER NOT NULL);` `ALTER TABLE peer_devices ADD COLUMN chat_key BLOB NOT NULL DEFAULT x'';` and `INSERT INTO settings (key, value) VALUES ('keys_reset', '1')`. A brand-new database gets the same setting, so `PrivateStore::open` must clear it for a database created fresh by this open (no notice on a first install): set `keys_reset` only when the migration ran on a database that already had migration 0001 applied before this open (check `_sqlx_migrations` before migrating).
  - `keys_reset` is read and deleted by the service when it opens, and returned in `PrivateOpened`.
  - Seal the secret with the store's `Sealer` under label `"chat_key.secret"` and row id `[1]` (match `seal.rs`'s label/row API).
  - `service.rs::register` uses `device.keys(&chat_key)`.
  - `convert.rs` reads `private_messages_v2`.
  - Remove v1 `device_payload`/`sign_device`/`verify_device` and `Identity::sign_device`, and their tests.

- [ ] **Step 4: Run** the whole core test suite: `cargo test -p opencord_core` → PASS (multi-device tests pass under the interim rule).

- [ ] **Step 5: Commit** `feat(core): a chat key per person signs this device; old private state is reset`.

---

### Task 4: Approving your own devices

**Files:**
- Create: `crates/opencord-core/src/private/own_devices.rs`
- Create: `crates/opencord-core/migrations/0003_own_devices.sql`
- Modify: `crates/opencord-core/src/private/{mod.rs,outbox.rs,inbox.rs,service.rs,payload.rs}`, `crates/opencord-core/src/client.rs`, `crates/opencord-core/src/api/types.rs`
- Test: `crates/opencord-core/tests/private.rs`

**Interfaces:**
- Consumes: `ChatKey` (Task 3), `pairing_words` (Task 1), `ChatKeyShare` (Task 2).
- Produces (core API types):
  - `pub struct OwnDevice { pub device_id: String /* hex */, pub state: OwnDeviceState }`
  - `pub enum OwnDeviceState { Other, Offering }` (`Other`: listed for you with another chat key; `Offering`: it sent this device a share waiting for They match). Devices with your chat key aren't listed; "Not mine" devices aren't either.
  - `PrivateEvent::OwnDevicesChanged { devices: Vec<OwnDevice>, has_chats: bool }`
  - `Client` methods (and the matching `PrivateService` functions in `own_devices.rs`):
    - `private_own_devices(&self) -> Result<(Vec<OwnDevice>, bool /* has_chats */), CoreError>`
    - `private_pairing_words(&self, device_id: &str) -> Result<Vec<String>, CoreError>` (16 words from this device's and that device's Ed25519 keys)
    - `private_approve_device(&self, device_id: &str) -> Result<(), CoreError>` (sends `ChatKeyShare` to that one device through any connected server listing both)
    - `private_accept_keys_from(&self, device_id: &str, matched: bool) -> Result<(), CoreError>` (matched: adopt the waiting share, re-register on every server, add a "Your keys changed." divider to every conversation; not matched: drop the share)
    - `private_reject_device(&self, device_id: &str) -> Result<(), CoreError>`
  - Own copies: `outbox::devices` sends to own devices whose `chat_key` equals this device's and withholds the rest (`SendPrivateMessage.withheld`); `inbox` treats a copy from an own device with another chat key as undecryptable ("a message from keys you haven't accepted") unless it's a `ChatKeyShare`.
  - `PrivateMessageKind::KeysChanged` (stored kind 4; `outgoing = true` means "Your keys changed.").
  - Test helper in `tests/private.rs`: `async fn approve(old: &TestClient, new: &TestClient)` doing approve on `old`, waiting for `Offering` on `new`, then accept with `matched = true`.

- [ ] **Step 1: Write the failing tests.**
  - `a_second_device_with_another_key_gets_no_copies`: Alice's device 2 (same identity, fresh data dir) is listed; Alice sends Bob a text from device 1; device 2 gets nothing displayable; `private_own_devices` on device 1 lists device 2 as `Other`.
  - `approving_shares_the_key_and_copies_flow_both_ways`: after `approve(a1, a2)`, `a2`'s chat key equals `a1`'s, a text from either shows on the other.
  - `an_approved_device_is_trusted_by_peers_without_a_banner` (Review Focus 1): Bob has talked to Alice before `a2` is approved; after approval Bob's next send reaches `a2`, and Bob's conversation has no `KeysChanged` message (Task 5's pinning must keep this passing: write it now, it passes under the interim rule and guards Task 5).
  - `a_share_is_ignored_before_they_match`: `a2` gets the share; without `accept`, its chat key is unchanged and it's `Offering`.
  - `a_waiting_share_survives_a_restart` (Review Focus 4): share arrives, `a2` restarts, still `Offering`, accept works.
  - `a_share_from_someone_else_is_ignored`: Bob sends Alice's device a `ChatKeyShare` (craft through the test proxy as `a_message_from_a_forged_device_is_not_believed` did) → no `Offering`, key unchanged.
  - `not_mine_hides_a_device`: `private_reject_device` → not listed again, even after it re-registers.
  - Update `three_devices` and any other helper that gives one person two devices to call `approve`.

- [ ] **Step 2: Run** → fail.

- [ ] **Step 3: Implement.**
  - Migration 0003: `CREATE TABLE own_devices (device_id BLOB PRIMARY KEY, ed25519 BLOB NOT NULL, state INTEGER NOT NULL /* 0 other, 1 rejected */, share BLOB /* sealed waiting ChatKeyShare secret */, share_from_ed25519 BLOB, seen_at INTEGER NOT NULL);`
  - Own devices are learnt from the same `FetchDevices` responses the outbox and inbox already make for `self_id`, and on registration (fetch self once per server after `RegisterDevice`). Emit `OwnDevicesChanged` when the set changes.
  - `has_chats` = any row in `messages` of kind text.
  - Adopting: replace the chat key (`ChatKey::replace`) and keep the Olm sessions (they belong to device keys, which don't change). Re-register on every connected server with the new signature (same device id, so the server upserts).
  - The share's Olm message goes through the normal outbox path with a body of `ChatKeyShare`, but to only that one device (`withheld` = everyone else).

- [ ] **Step 4: Run** `cargo test -p opencord_core` → PASS.

- [ ] **Step 5: Commit** `feat(core): approve your own devices by comparing words`.

---

### Task 5: Trusting other people's keys

**Files:**
- Create: `crates/opencord-core/src/private/trust.rs`, `crates/opencord-core/migrations/0004_contact_keys.sql`
- Modify: `crates/opencord-core/src/private/{mod.rs,outbox.rs,inbox.rs,contacts.rs,service.rs}`, `crates/opencord-core/src/client.rs`, `crates/opencord-core/src/api/types.rs`
- Test: `crates/opencord-core/tests/private.rs`, unit tests in `trust.rs`

**Interfaces:**
- Consumes: `VerifiedDevice.chat_key` (Task 3), `chat_words` (Task 1), `KeysChanged` kind (Task 4).
- Produces:
  - `trust::classify(pinned: Option<[u8;32]>, listed: &[VerifiedDevice]) -> Classified` where `pub struct Classified { pub pin: Option<[u8;32]> /* key to pin now on first contact */, pub trusted: Vec<VerifiedDevice>, pub held: Vec<VerifiedDevice>, pub candidate: Option<[u8;32]> /* the other key on most held devices, ties lowest bytes */ }`. First contact (`pinned == None`): pin the key on the most devices, ties lowest bytes.
  - `PrivateContact` loses `fingerprint` and gains `verification: PrivateVerification` (`Unverified | Verified | Mismatch`), `new_keys: bool` (a candidate exists), `reachable: bool` (at least one trusted device was listed at the last fetch).
  - `PrivateMessageKind::NewDevice` (stored kind 5): "*Name* signed in on a new device."
  - `ErrorCode::KeysNotAccepted` (core-side only, mapped in `api/types.rs` like the others).
  - `Client` methods:
    - `private_words(&self, peer: &str, new_keys: bool) -> Result<Vec<String>, CoreError>` (`new_keys`: words with the candidate instead of the pinned key)
    - `private_verify(&self, peer: &str, new_keys: bool, matched: bool) -> Result<(), CoreError>` (matched + new_keys: pin the candidate, verified, add `KeysChanged`; matched: verified; not matched: `Mismatch`)
    - `private_accept_new_keys(&self, peer: &str) -> Result<(), CoreError>` (pin the candidate, unverified, add `KeysChanged`)
  - Outbox: encrypt to trusted devices only, list the held ones in `withheld`; with no trusted device of the peer, refuse with `KeysNotAccepted` before encrypting or saving the message.
  - Inbox: a message from a device of the peer not signed by the pinned key → the undecryptable line, text "a message from keys you haven't accepted"; nothing decrypted is shown.
  - Verified state is stored as the pair (my chat key, their pinned key) at They match; it's `Verified` only while both still equal. Changing my own key (Task 4 adopt) therefore unverifies everything with no extra code.

- [ ] **Step 1: Write the failing tests.**
  - unit `first_contact_pins_the_key_on_most_devices`, `ties_pin_the_lowest_key`, `devices_with_another_key_are_held_and_offered`, `a_device_with_the_pinned_key_is_trusted`.
  - `a_stolen_identity_gets_nothing` (replaces the deleted identity test): Carol's client imports Alice's identity (not approved); Bob sends to Alice; Carol shows nothing; Bob's contact for Alice has `new_keys == true`; Alice's `a1` gets the message.
  - `a_message_from_held_keys_is_not_shown_as_text` (Review Focus 2).
  - `old_keys_keep_working_until_the_new_ones_are_accepted` (Review Focus 3): Alice has `a1` (pinned) and a reinstalled `a3` with a new key; Bob's send reaches `a1` only; after `private_accept_new_keys`, it reaches `a3` only and Bob's chat has one `KeysChanged`.
  - `sending_with_only_held_devices_is_refused` (Review Focus 5): only `a3` listed → `KeysNotAccepted`, no message row stored.
  - `they_match_verifies_until_a_key_changes`: verify → `Verified`; Alice adopts a new key → Bob's contact becomes `Unverified` once Bob accepts it.
  - `they_dont_match_marks_a_mismatch_and_they_match_clears_it`.
  - `both_sides_see_the_same_words`: `alice.private_words(bob)` == `bob.private_words(alice)`, 16 entries.
  - `a_new_device_with_the_pinned_key_shows_a_quiet_divider`: after `approve(a1, a2)`, Bob's chat gets one `NewDevice` and stays `Verified`.

- [ ] **Step 2: Run** → fail.

- [ ] **Step 3: Implement.** Migration 0004: `CREATE TABLE contact_keys (public_key BLOB PRIMARY KEY /* the contact */, pinned BLOB NOT NULL, candidate BLOB, verified_mine BLOB, verified_theirs BLOB, mismatch INTEGER NOT NULL DEFAULT 0, reachable INTEGER NOT NULL DEFAULT 1);` `peer_devices.chat_key` (Task 3) tells which known device is new under the same key for `NewDevice`. Contact changes emit the existing `ContactChanged`.

- [ ] **Step 4: Run** `cargo test -p opencord_core` and `cargo test -p opencord-mobile` → PASS; clippy for the client crates.

- [ ] **Step 5: Commit** `feat(core): pin each person's chat key and hold the rest`.

---

### Task 6: The bridge and the app's repository

**Files:**
- Modify: `crates/opencord-core/src/api/private.rs` (new `pub async fn`s), `crates/opencord-core/src/frb_generated.rs`, `app/lib/src/rust/**` (generated)
- Modify: `app/lib/core/model/private.dart`, `app/lib/core/repository/repository.dart`, `app/lib/core/repository/events.dart`, `app/lib/core/rust/rust_repository.dart`, `app/lib/core/rust/core_api.dart`, `app/lib/core/mock/mock_private.dart`, `app/lib/core/providers/private_state.dart`
- Test: `app/test/unit/private/chat_keys_test.dart` (new, against the mock repository)

**Interfaces:**
- Consumes: Tasks 4–5 `Client` methods.
- Produces (bridge, `api/private.rs`): `private_words(peer, new_keys) -> Vec<String>`, `private_verify(peer, new_keys, matched)`, `private_accept_new_keys(peer)`, `private_own_devices() -> OwnDevices { devices: Vec<OwnDevice>, has_chats: bool }`, `private_pairing_words(device_id) -> Vec<String>`, `private_approve_device(device_id)`, `private_accept_keys_from(device_id, matched)`, `private_reject_device(device_id)`.
- Produces (Dart):
  - `enum PrivateVerification { unverified, verified, mismatch }`; `PrivateContact` drops `fingerprint`, gains `verification`, `newKeys`, `reachable`.
  - `PrivateMessageKind` gains `keysChanged`, `newDevice`.
  - `enum OwnDeviceState { other, offering }`, `class OwnDevice { String deviceId; OwnDeviceState state; }`.
  - `Repository`: `Future<List<String>> privateWords(String peer, {bool newKeys = false})`, `Future<void> verifyPrivate(String peer, {required bool matched, bool newKeys = false})`, `Future<void> acceptNewPrivateKeys(String peer)`, `Future<({List<OwnDevice> devices, bool hasChats})> ownDevices()`, `Future<List<String>> pairingWords(String deviceId)`, `Future<void> approveOwnDevice(String deviceId)`, `Future<void> acceptKeysFrom(String deviceId, {required bool matched})`, `Future<void> rejectOwnDevice(String deviceId)`, `bool get privateKeysReset`, `void acknowledgePrivateKeysReset()`.
  - `RepoErrorKind.keysNotAccepted`.
  - `final class OwnDevicesChanged extends RepoEvent { final List<OwnDevice> devices; final bool hasChats; }`.
  - `ownDevicesProvider` in `private_state.dart` (current list + hasChats, updated by the event).
  - The mock gives Mira a verified chat, Kai `newKeys: true`, and one `OwnDevice(other)`, so widget tests and the mock app show every state.

- [ ] **Step 1:** Add the bridge functions; run `cd app && flutter_rust_bridge_codegen generate` (with the env prefix).
- [ ] **Step 2: Write failing Dart unit tests** for the mock: `verifyPrivate(matched: true)` turns Mira's contact `verified` and emits `PrivateContactChanged`; `acceptNewPrivateKeys('kai')` clears `newKeys` and adds a `keysChanged` message; `approveOwnDevice` then `acceptKeysFrom` empties `ownDevices()`.
- [ ] **Step 3: Run** `flutter test test/unit` → fail; implement Dart model/repository/mock/rust repository; run → PASS; `flutter analyze` clean (fix every former `fingerprint` use; `private_header.dart` is rewritten in Task 7, a temporary removal of the Copy row is fine here).
- [ ] **Step 4: Commit** `feat(app): bridge and repository for chat keys and words`.

---

### Task 7: What people see

**Files:**
- Create: `app/lib/features/home/encryption_dialog.dart` (the §9.3 dialog and a reusable `WordGrid` widget), `app/lib/features/home/key_banners.dart` (the §8.2 and §9.3 banners), `app/lib/features/home/own_device_notice.dart` (§7.1 notices and the §7.2 dialogs)
- Modify: `app/lib/features/home/private_header.dart` (lock states, opens the dialog), `app/lib/features/home/private_chat.dart` (banners above the composer; composer disabled when `!reachable`), `app/lib/features/home/private_message_list.dart` / `private_message_adapter.dart` (`keysChanged` and `newDevice` dividers, the "keys you haven't accepted" line), `app/lib/features/home/home_main.dart` (own-device notice at the top of the home view), the place that shows `privateStartedFresh` (show `privateKeysReset` the same way, once)
- Test: `app/test/widget/home/private_chat_test.dart`, `app/test/widget/home/encryption_dialog_test.dart` (new), `app/test/widget/home/friends_page_test.dart` or `home_shell_test.dart` for the own-device notice

**Interfaces:**
- Consumes: Task 6's repository, models and providers.
- Produces: `WordGrid({required List<String> words})` (4 rows × 4, read left to right; each cell selectable text in the theme's mono style), `showEncryptionDialog(BuildContext, {required PrivateContact contact, bool newKeys = false})`.

- [ ] **Step 1: Write failing widget tests:**
  - `the dialog shows 16 words in 4 rows` (find 16 word texts; 4 `Row`s inside `WordGrid`).
  - `They match turns the lock green` (after tapping, the header lock's tooltip is "Verified").
  - `They don't match shows the red banner` with the spec's text.
  - `a key change shows the banner` with "Compare words" and "Use new keys anyway"; `Use new keys anyway` hides it and shows the divider "Kai's keys changed."
  - `the composer is disabled when nothing is reachable`.
  - `the home view offers to approve another device of yours` (Approve opens a dialog with a `WordGrid`; They match calls `approveOwnDevice`) and `a device without chats asks to be approved elsewhere` with "Keep new keys".
  - `an offered key asks for They match` (state `offering` → dialog with the grid, They match calls `acceptKeysFrom(matched: true)`).
  - `the old chats notice shows once`.
- [ ] **Step 2: Run** `flutter test test/widget/home` → fail.
- [ ] **Step 3: Implement** with the existing dialog, banner and button widgets (follow `request_bar.dart` and `move_card.dart` for banners, `private_header.dart`'s current dialog for the dialog shell). Copy exactly as the spec.
- [ ] **Step 4: Run** `flutter test` and `flutter analyze` → PASS.
- [ ] **Step 5: Commit** `feat(app): words, lock states, key banners and device approval`.

---

### Task 8: Docs, full checks, pull request

**Files:**
- Modify: `docs/private-messages.md` (Encryption and security, Identity and device verification, What servers see, Local database, a new "Comparing words" section, the by-hand scenarios: add §11's by-hand checks), `docs/decisions.md` (append `## D84: Private message keys apart from the identity key (2026-11-01)` in the file's style: Why, what changed, Verified, Owed)

- [ ] **Step 1:** Update the docs. In the by-hand section, add: compare words between Alice and Bob; a third profile with Alice's imported identity gets nothing and Alice sees the notice; approving it from Alice's first profile makes copies arrive.
- [ ] **Step 2:** `df -h /`, then `.github/check.sh` → everything passes (report any failure, related or not).
- [ ] **Step 3:** Push and `gh pr create --base beta` with what changed, why, how it was tested, and what wasn't (Windows/macOS by hand; the by-hand scenarios if not run). Body ends with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
- [ ] **Step 4:** Follow CI (`gh run watch`), fix failures on the branch (re-run check.sh before pushing), review `gh pr diff`. Don't merge: the owner merges the batch.
