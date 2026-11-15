# Phone P3: Private Messages and Friends — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

> **Hold:** don't start until the owner has answered design §9 Q7. The memory of 2026-11-01 says private messages stay off the phone until the owner says so.

**Goal:** On the phone you have friends and private chats with the desktop's end-to-end encryption: requests, blocking, chats that survive being offline, comparing words, and approving the phone as one of your own devices.

**Architecture:**
- **The core does all of it.** `opencord_core::Client` has the whole private API (`private_*`, `friend_*`, `message_request_*`, `privacy_settings`; `crates/opencord-core/src/client.rs`) and a separate event stream, `private_events()`. The facade crosses both. No server or protocol change.
- **The private database's master key** is 32 random bytes kept in the phone's keystore (expo-secure-store), as the desktop keeps it in the system keychain (`app/lib/core/rust/private_key_store.dart`, name `opencord.private.key`). `private_open(key)` opens the database at boot, after the identity.
- **The phone is a device of its own.** A phone that imported a desktop's identity is a new device with no chat key: the desktop shows `A device signed in as you with different keys.`, and the two compare 16 words (docs/private-messages.md, Scenario 3). A phone with a new identity makes its own chat key.
- **Screens are the phone's,** not the desktop's home page: a **Messages** tab (design §6.3), a chat that reuses the server chat's list, bubbles and composer through an adapter (as the desktop's `private_message_adapter.dart` does), and sheets for friends and keys.

**Tech Stack:** as P0. No new dependencies.

**Spec:** design §3 (P3), §6.3; `docs/private-messages.md` (read "Encryption and security", "Relay servers and moving a chat", "Friends, message requests, spam and blocks", "Local database and sealing at rest" before Task 1); `docs/superpowers/specs/2026-11-01-private-message-keys-design.md` (D84). The desktop screens are `app/lib/features/home/*.dart`; their words are the phone's.

## Global Constraints

- **Branch:** `feat/phone-private`, from `feat/phone-chat` (P2). Draft PR into `beta` at Stage A's end, stacked.
- **Scope:** `crates/opencord-mobile/`, `mobile/src/`, `docs/`. No core, server, protocol or desktop change: if one is needed, stop and ask.
- **Never log** message content, names, words, keys or the master key. The keystore entry is never backed up in plain text: `keychainAccessible: AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY` on iOS, and Android's encrypted shared preferences (expo-secure-store's default).
- **Losing the master key loses this device's chat history,** never the identity: the core starts fresh (`PrivateOpened.started_fresh`), and the phone shows the desktop's sentence (Task 2).
- P0's Global Constraints from "Ids are decimal strings" on apply. A `peer` is the core's string as it is (a public key); it isn't an id and isn't converted.
- **Numbers follow the facade's rule** (`crates/opencord-mobile/src/types.rs:1-3`): ids are decimal strings; times (ms since the epoch), sizes, durations, positions and counters cross as `f64` through `convert::number` and `convert::unsigned`; a draft id comes back through `convert::draft_id`. No `u64` or `i64` field in a record JavaScript reads, whatever the core's type: a `u64` becomes a JavaScript `bigint` and breaks every comparison.

## Working Rules

P0's apply word for word. **Ledger:** `.superpowers/sdd/phone-p3/progress.md`.

Scores (the owner, 2026-11-15): **0** no code; **1–3 Gemini**; **4–5 Claude**, handed over with claude-bridge `delegate_task`, each a cold session, so the task's section is self-contained. The rubric is P0's "Task owners". A Claude task runs first in its stage where its dependencies allow (memory: "my tasks come first"); the order is under the table.

| Task | Score | Owner | Why |
|---|---|---|---|
| 1 Facade: records and calls | 4 | Claude | The FFI for end-to-end encryption: the master key's bytes, key strings and words cross here; a wrong mapping can send to the wrong keys |
| 2 Facade: the private event stream, and opening at boot | 5 | Claude | A second event source merged into the generation-based queue (concurrency across JavaScript reloads) and the master key in the keystore (data loss if mishandled) |
| 3 The JavaScript core and the fake | 2 | Gemini | A fake world following the pattern |
| 4 The store | 3 | Gemini | Load-then-merge with events arriving during the load, testable on the fake. Focus: events before the first load; a message changing state twice |
| 5 The Messages tab | 2 | Gemini | A list with swipes and menus the servers list already has |
| 6 The private chat | 4 | Claude | Refactors the server chat's list and composer (keyboard and anti-jump rules that took two review rounds to get right) to take a second data source, without changing server chats |
| 7 Friends, and starting a chat from a profile | 2 | Gemini | Lists and buttons over Task 1's calls |
| 8 Keys: words, new keys, your own devices | 5 | Claude | Security: comparing words and approving devices decides who can read every chat; a wrong `keys` string or a missing block on sending leaks messages |
| 9 Privacy | 1 | Gemini | Two switches |
| 10 Live checks, docs, D-entry, PR | 4 | Claude | The encryption scenarios with two devices of one identity must be judged, not only run (Scenario 3 and 3b) |

**Order:** Stage A: **1, 2**, then 3, 4. Stage B: **6**, then 5, 7. Stage C: **8**, then 9. Stage D: **10**.

Stages: A = 1–4 (the phone knows private messages), B = 5–7 with a review, C = 8–9 with a review, D = 10. Commands as P0.

## Review Focus

1. **The phone imported a desktop's identity.** Expected: before approval it gets no messages and sends none, and the Messages tab says why with `Your other devices use different keys. Approve this device …` (the desktop's `own_device_notice.dart` sentence, whole) and **Compare words**; after both sides tap **They match**, new messages arrive on both. Pinned in Task 8: `an unapproved device shows the notice and sends nothing`, and by hand in Task 10 (Scenario 3 with the phone as `alice-laptop`).
2. **A private event arrives before the contacts are loaded** (the stream starts at `private_open`, the list loads after). Expected: nothing is lost; the store applies events on top of the loaded list in order. Pinned in Task 4: `events before the first load apply after it`.
3. **Sending while offline, then the app is killed.** Expected: the message shows as waiting, then sends after the next start (the core keeps the outbox); a failed one offers **Retry**. Pinned in Task 6: `a waiting message sends after restart` (fake) and by hand in Task 10 (Scenario 7).
4. **Their keys change while the chat is open.** Expected: the banner appears at once, the composer is disabled until **Compare words** or **Use new keys anyway**, and nothing is sent to the new key before that. Pinned in Task 8: `new keys block sending until chosen`.
5. **The keystore loses the master key** (the app reinstalled with its data restored, or the keystore cleared). Expected: the desktop's fresh-start sentence once, a working empty database, and the identity untouched. Pinned in Task 2: `a missing master key starts fresh and says so once`.

---

## Stage A — The phone knows private messages

### Task 1: Records and calls in the facade

**Files:** create `crates/opencord-mobile/src/api/private.rs`, `src/private_types.rs`; modify `src/api/mod.rs`, `src/lib.rs`, `src/convert.rs`.

**Interfaces (produces):** one uniffi record or enum per core type, with the same field names (`crates/opencord-core/src/api/types.rs:2274-2440`): `PrivateOpenedRecord { started_fresh, keys_reset }`, `PrivateContactRecord` (every field of `PrivateContact`, enums as uniffi enums, times as `f64` ms), `PrivateMessageRecord`, `OwnDeviceRecord`, `KeyWordsRecord { words: Vec<String>, keys: String }`, `SharedServerRecord` (`peer_user_id` as a decimal string), `PrivacySettingsRecord`. And one exported call per `Client` method, named the same with the same arguments (`private_open(key: Vec<u8>)`, which refuses any length but 32 with `MobileError::InvalidInput`; `private_contacts`, `private_messages(peer, before: Option<String>, limit: u32)`, `private_send`, `private_edit`, `private_delete`, `private_retry`, `friend_request_send(server_key, user_id: String)`, `friend_request_answer`, `friend_request_cancel`, `friend_remove`, `message_request_accept`, `message_request_ignore`, `private_block`, `private_unblock`, `privacy_settings`, `privacy_settings_set` (the core's `set_privacy_settings`), `private_open_conversation(server_key, user_id: String)`, `private_shared_servers`, `private_move`, `private_move_too`, `private_typing`, `private_mark_read`, `private_set_muted`, `private_hide`, `private_own_devices` (returns `OwnDevicesRecord { devices, has_chats }`), `private_pairing_words`, `private_approve_device`, `private_accept_keys_from`, `private_words`, `private_verify`, `private_accept_new_keys`, `private_reject_device`). A `limit` over 100 is clamped to 100.

- [ ] Tests: `every_private_enum_round_trips` (each variant of `FriendStatus`, `ConversationStatus`, `PrivateVerification`, `PrivateMessageKind`, `PrivateSendState`, `OwnDeviceState`), `a_short_master_key_is_refused`, `a_shared_server_user_id_is_a_string`. Run (fail), implement, run (pass); clippy; bindings; commit `feat(mobile): the private message calls in the facade`.

### Task 2: The private event stream, and opening at boot

**Files:** modify `crates/opencord-mobile/src/events.rs`, `src/api/private.rs`; create `mobile/src/core/privateKey.ts`; modify `mobile/src/core/boot.ts`.

**Interfaces (produces):**
- `MobileEvent::Private(PrivateEventRecord)` with `PrivateEventRecord` one variant per `PrivateEvent` (`OwnDevicesChanged { devices, has_chats }`, `ContactChanged`, `ContactRemoved { peer }`, `MessageAdded`, `MessageChanged`, `MessageDeleted { peer, message_id }`, `Typing { peer }`, `FriendRequestReceived { peer, spam }`).
- `private_open` takes the receiver from `Client::private_events()` once and feeds it into the same queue `next_events` drains, behind the same generation rule (`eventsAttach` starts a new generation; an old generation gets nothing). Read how the media and core sources are merged (`events.rs:230-260`, `Incoming`) and add a third, `Incoming::Private`. A second `private_open` in one process (a JavaScript reload) keeps the first receiver: test it.
- `privateKey.ts`: `loadOrCreatePrivateKey(): Promise<Uint8Array>` under the keystore name `opencord.private.key`; 32 bytes from `expo-crypto`'s `getRandomBytes` if it's a dependency, else `crypto.getRandomValues` (React Native 0.88 has it); ruling if neither.
- `boot.ts`: after the identity loads, `privateOpen(key)`. On `startedFresh` with a key that existed before, show once the desktop's sentence `Your private messages on this device couldn't be opened, so this device starts fresh. New messages will arrive normally.`
- [ ] Tests (Rust): `private_events_reach_javascript`, `a_reload_keeps_the_private_stream`, `an_old_generation_gets_no_private_events`. (jest): `a missing master key starts fresh and says so once`, `the key is made once and reused`. Run (fail), implement, run (pass); bindings; commit `feat(mobile): private events reach the phone, and its database opens at boot`.

### Task 3: The JavaScript core and the fake

**Files:** `mobile/src/core/api.ts`, `native.ts`, `fake/FakeCore.ts`, `fake/privateWorld.ts` (new); test `core/__tests__/fakePrivate.test.ts`.

- The fake private world: friends `Kai` (online, verified, a 60-message chat with an edited and a deleted message and one failed send), `Mira` (not verified, `new_keys: true`), an incoming request from `Rowan`, an outgoing to `Sol`, a spam request, a blocked `Vex`, a message request from `Ash`, and one other own device waiting (`OwnDevicesChanged`). `privateSend` echoes `MessageAdded` as `Sending`, then `MessageChanged` as `Sent` after 400 ms; Kai types back after 2 s.
- [ ] Tests: `the fake echoes a send through both states`, `the fake has every contact kind`. Run (fail), implement, run (pass); commit `feat(mobile): the core API and a fake private world`.

### Task 4: The store

**Files:** create `mobile/src/state/private.ts`, `state/actions/private.ts`; modify `state/store.ts`, `state/events.ts`, `state/selectors.ts`; test `state/__tests__/private.test.ts`.

**Interfaces (produces):** `usePrivateContacts(filter: 'chats' | 'friends' | 'pending' | 'blocked' | 'requests')`, `usePrivateMessages(peer)`, `usePrivateTyping(peer)`, `useOwnDevices()`, `usePrivateUnreadTotal()` (unmuted unread, for the tab badge); actions `loadPrivate()`, `loadOlder(peer)`, and one per call of Task 1 that changes state, each optimistic only where the core echoes an event (sending shows the core's `MessageAdded`, never a guess).
- Chats: contacts with a conversation that isn't hidden, newest `lastActivityMs` first. Requests: `ConversationStatus` of a message request, spam last.
- [ ] Tests: `events before the first load apply after it`, `a message changing state twice ends in the last`, `a hidden chat leaves the list and comes back with a new message`, `unread total skips muted`. Run (fail), implement, run (pass); commit `feat(mobile): the store keeps private chats`. Stage A's end.

## Stage B — Chats and friends

### Task 5: The Messages tab

**Files:** create `mobile/src/app/(tabs)/messages/_layout.tsx`, `index.tsx`, `[peer].tsx`, `friends.tsx`, `requests.tsx`; `mobile/src/features/private/ChatList.tsx`, `ChatRow.tsx`; modify `app/(tabs)/_layout.tsx` (the tab between Servers and You: label `Messages`, icon `bubble.left.and.bubble.right` / `chat`, badge `usePrivateUnreadTotal()`).

- **The list:** at the top, a `Message requests` row with its count when there are any, and the own-device notice when one is waiting (Task 8 draws it; here a slot). Then the chats: avatar, name, preview, time, unread badge, a bell-slash when muted. The header's right button opens Friends.
- **Swipes** like the servers list: leading `Read`, trailing `Close` (`private_hide`). **Long press:** `Mute notifications` / `Unmute notifications`, `Close conversation`, `Block` (confirm: title `Block <name>?`, the desktop's body from `home_sidebar.dart`, button `Block`).
- **Requests page:** the desktop's words (`requests_page.dart`): title `Message requests`, header `They wrote first. Nothing reaches your chats until you accept.`, empty `No one is waiting for an answer`, each row `Wants to message you` with `Accept` and `Decline`.
- [ ] Tests: `chats are newest first`, `the badge skips muted`, `each swipe and menu item does what it says`, `requests accept and decline`. Run (fail), implement, run (pass); screenshots (dark, light, phone, tablet: on the tablet the list and the chat side by side); commit `feat(mobile): the Messages tab`.

### Task 6: The private chat

**Files:** create `mobile/src/features/private/PrivateChat.tsx`, `privateAdapter.ts` (private messages → the rows `MessageList` draws, as `app/lib/features/home/private_message_adapter.dart`), `PrivateHeader.tsx`, `RequestBar.tsx`, `MoveCard.tsx`, `RelayDivider.tsx`; modify `features/chat/MessageList.tsx` and `Composer.tsx` only to take a data source and a send function as props, where they read a server channel today (no behaviour change for server chats: their tests must pass untouched).

- **The header:** avatar, name, the lock (verified: a shield) opening Task 8's sheet; a `⋯` menu with `Mute notifications`, `Block`, `Remove friend` (confirm with `Remove <name>?` / `Are you sure you want to remove <name> as a friend?` / `Remove friend`).
- **Sending states:** `Sending` (clock), `Sent`, `Failed` with a tap to `Retry` (`private_retry`), as the desktop's bubbles show them. Long press: `Copy text`, and on your own `Edit` and `Delete`. Typing sends `private_typing` (throttled as server chats do), and opening the chat marks it read.
- **Request bar** when the chat is a request: `Accept`, `Ignore` (confirm `Ignore request?` / `This removes the conversation.` / `Ignore`), `Block`.
- **Relay changes:** a divider where the server that carries the chat changed (`RelayDivider`, the desktop's words from `relay_divider.dart`), and the move card (`MoveCard`, its `Move too` button calls `private_move_too`) when the other side moved it.
- **Older messages** load as the list nears its top, 50 at a time, without the list jumping (the server chat's anti-jump rule holds).
- [ ] Tests: `a waiting message sends after restart`, `a failed message retries`, `edit and delete reach the core`, `opening marks read`, `server chats still pass` (the existing suites run unchanged). Run (fail), implement, run (pass); screenshots with the keyboard up; commit `feat(mobile): private chats`.

### Task 7: Friends, and starting a chat from a profile

**Files:** create `mobile/src/features/private/FriendsPage.tsx`; modify P0's `features/members/MemberProfile.tsx` (buttons `Message` and `Add friend` / `Pending` / `Friends`, for anyone but yourself and plugin users).

- **Friends page:** a segmented control `Online`, `All`, `Pending`, `Blocked`, with the desktop's section titles (`ONLINE — N`, `ALL — N`, `INCOMING — N`, `SENT — N`, `Spam — N`, `BLOCKED — N`) and empty text `No pending requests.`. Incoming rows: accept and decline; sent rows: cancel; blocked rows: unblock.
- **From a profile:** `Message` calls `private_open_conversation(server, user)` and opens the chat; it's hidden when the server has private messages off (`SharedServer.private_messages`), and when friends-only, shown only to friends. `Add friend` calls `friend_request_send`.
- [ ] Tests: `message opens the conversation`, `a server with private messages off shows no button`, `each friends section does what it says`. Run (fail), implement, run (pass); screenshots; commit `feat(mobile): friends, and messaging from a profile`. Stage B's end and review.

## Stage C — Keys and privacy

### Task 8: Keys: words, new keys, your own devices

**Files:** create `mobile/src/features/private/EncryptionSheet.tsx`, `WordGrid.tsx`, `KeyBanner.tsx`, `OwnDeviceNotice.tsx`; modify `PrivateChat.tsx`, `ChatList.tsx`.

- **The sheet** (`encryption_dialog.dart`, its words): title `End-to-end encryption`, the status (`Verified`, `Not verified`, `New keys`), `Compare these words with <name> in person or on a call.` and the rest of that sentence as the desktop has it, the 16 words in a 4×4 grid (mono, numbered, large enough to read aloud), `They match` and `They don't match`. `They match` calls `private_verify(peer, newKeys, true, words.keys)`; `They don't match` calls it with `false` and shows the desktop's warning.
- **New keys banner** in the chat (`key_banners.dart`): `Compare words` (opens the sheet with `new_keys: true`) and `Use new keys anyway` (`private_accept_new_keys`). The composer is disabled until one is chosen.
- **Your own devices** (`own_device_notice.dart`), at the top of the Messages tab:
  - On a device that **has** the chat key, for another device signed in as you: `A device signed in as you with different keys. Approve it …` (whole sentence), buttons `Approve` (opens the sheet with `private_pairing_words(device)`, title `Approve this device`, `Check that <device> shows the same words.`, `Only tap They match if both screens show these words.`, `They match gives it the keys to your chats.`; `They match` → `private_approve_device`) and `Not mine` (`private_reject_device`).
  - On **this** device when it has no chat key and another of yours does: `Your other devices use different keys. Approve this device …`, `Compare words` (`private_pairing_words`; `They match` → `private_accept_keys_from(device, true, keys)`), and `Keep new keys` (the desktop's, same call with `false`).
  - A device is named `Your device ABCD` (the first four characters of its id, upper case), as the desktop.
- [ ] Tests: `an unapproved device shows the notice and sends nothing`, `new keys block sending until chosen`, `they match sends the keys the words came from` (the `keys` string from the same `KeyWords`, never re-fetched), `not mine rejects`. Run (fail), implement, run (pass); screenshots of each state from the fake; commit `feat(mobile): comparing words and approving devices`.

### Task 9: Privacy

**Files:** create `mobile/src/features/you/PrivacyPage.tsx`, route `app/(tabs)/you/privacy.tsx`; modify `YouScreen.tsx`.

- Two switches with the desktop's words (`privacy_page.dart`): `Friend requests` / `Let people send you friend requests.`; `Message requests` / `Let people who aren't your friends message you.` Each saves on touch (`privacy_settings_set`).
- [ ] Test `each switch saves`; run (fail), implement, run (pass); screenshots; commit `feat(mobile): privacy settings`. Stage C's end and review.

## Stage D — Live checks

### Task 10: Live checks, docs, D-entry and the pull request

- [ ] Run `docs/private-messages.md` Scenarios 1, 2, 3 (the phone as the second device, both ways: phone first and desktop first), 3b, 4, 5, 6 and 7 with the phone as one side and the desktop as the other, against a local server. Ledger each step's result and a screenshot.
- [ ] `docs/mobile.md` (a "Private messages" section; "Not there yet" loses its row); `docs/private-messages.md` names the phone where it says "app" or "instance"; the iPhone hand checks (the keystore after a reinstall, Scenario 3 with an iPhone).
- [ ] The next D-number: the master key in the keystore, the Messages tab, the shared chat list. From `git diff`.
- [ ] `.github/check.sh`; push; PR description; `gh pr checks` green; final review round; report.
