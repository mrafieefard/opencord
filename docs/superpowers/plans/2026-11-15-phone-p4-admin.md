# Phone P4: Running a Server — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Someone with the right permissions runs their server from the phone: its name, description and icon; channels and their permissions; roles; members, invites and bans; voice, upload and private message settings; commands on and off; and the soundboard's sounds, uploaded with a trim.

**Architecture:**
- **Each desktop page becomes a phone page** pushed from the server sheet (P0's `registerServerPage`), with the same permission gate (P0's `serverSettingsRows`), the same fields and the same words. Layout is the phone's: P0's settings kit, saving on touch, text saved with the header's `Save` (design §6.2).
- **The facade gains every admin call** of `opencord_core::Client` that these pages use, and the settings the snapshot drops today (`ReadySnapshot.voice_settings` beyond the soundboard part, `upload_settings`, `private_settings`, `ServerInfo.open_join` and `everyone_role_id`).
- **Reordering is a drag** in the list (channels, roles), sent once when the finger lifts, as the desktop sends one `reorder_*` per drop.

**Tech Stack:** as P0. No new dependencies (the drag uses `react-native-reanimated` and `react-native-gesture-handler`, already in).

**Spec:** design §3 (P4), §6.1, §6.2, §4's anti-goal "Admin by accident". The desktop pages are the reference for every word and rule: `app/lib/features/settings/server/*.dart` (named per task), `app/lib/features/settings/server_settings.dart` (the gates), `permission_info.dart` (every permission's name and sentence), `overwrites.dart` (the three-state overwrite rule), `app/lib/features/soundboard/trim_editor.dart`.

## Global Constraints

- **Branch:** `feat/phone-admin`, from the last finished phone plan (P3, or P2 if P3 is still held). Draft PR into `beta` at Stage A's end, stacked.
- **Scope:** `crates/opencord-mobile/`, `mobile/src/`, `docs/`. No core, server, protocol or desktop change.
- **Words new on the phone** (no desktop source) are marked so where they're given; every other quoted word is the desktop's, from the file the task names. Where a sentence is given with `…`, copy the whole sentence from that file.
- **Every destructive action confirms** with the system dialog and the desktop's words: delete a channel, delete a role, kick, ban, revoke an invite, delete a sound, remove an overwrite. The button names the action (`Delete channel`, `Ban`), never `OK`.
- **The server decides.** The phone hides what the user may not do (P0's gates), but every refusal still shows the server's error as a toast and puts the value back. No page assumes success.
- **Not in P4:** hosting (domains and network, Lavalink, local songs, storage: design §3, desktop only); plugin channels and the Plugins page (P6).
- P0's Global Constraints from "Ids are decimal strings" on apply. Permission bits cross as named flags (P0's `ServerAccess`) on records the phone reads, and as `i64` only inside `PermissionSet` values the phone writes back (Task 1), never as JavaScript numbers above 2^53: `ADMINISTRATOR` is bit 63. **That's the one place a cheap shortcut breaks the server:** bits cross as decimal strings.
- **Numbers follow the facade's rule** (`crates/opencord-mobile/src/types.rs:1-3`): ids are decimal strings; times (ms since the epoch), sizes, durations, positions and counters cross as `f64` through `convert::number` and `convert::unsigned`; a draft id comes back through `convert::draft_id`. No `u64` or `i64` field in a record JavaScript reads, whatever the core's type: a `u64` becomes a JavaScript `bigint` and breaks every comparison.

## Working Rules

P0's apply word for word. **Ledger:** `.superpowers/sdd/phone-p4/progress.md`.

Scores (the owner, 2026-11-15): **0** no code; **1–3 Gemini**; **4–5 Claude**, handed over with claude-bridge `delegate_task`, each a cold session, so the task's section is self-contained. The rubric is P0's "Task owners". A Claude task runs first in its stage where its dependencies allow (memory: "my tasks come first"); the order is under the table.

| Task | Score | Owner | Why |
|---|---|---|---|
| 1 Facade: settings in the snapshot, and the permission sets | 4 | Claude | Security: permission bits up to bit 63 written back to the server; a lossy conversion can grant or strip `ADMINISTRATOR` |
| 2 Facade: the admin calls | 3 | Gemini | Many calls in one pattern. Focus: fields left out stay unchanged; `parent_id` `"0"`; draft ids through `convert::draft_id` |
| 3 The JavaScript core, the fake and `permissionInfo.ts` | 2 | Gemini | A fake and a port checked against every bitflags name |
| 4 Overview | 2 | Gemini | A page of fields with the desktop's words |
| 5 Voice & video, Upload settings, Private messages | 2 | Gemini | Pages of fields with the desktop's words |
| 6 Commands | 2 | Gemini | A grouped list of switches |
| 7 Channels: list, create, edit, delete, reorder | 3 | Gemini | A drag gesture and its two calls. Focus: a drop across categories; the list as it is now; losing access mid-edit |
| 8 Channel permissions | 4 | Claude | Security: allow and deny sets written to the server, and the three-state rule; a slip opens a private channel to everyone |
| 9 Roles: list, edit, permissions, reorder | 4 | Claude | Security: role permissions and the hierarchy, merged with remote edits while open |
| 10 Members, invites and bans | 3 | Gemini | Kick and ban are destructive, but the server re-checks rank; the rules are a port of `_may` with a case per branch. Focus: no actions on someone above you or the owner; reasons sent |
| 11 Soundboard sounds, with trim | 3 | Gemini | A waveform with two handles and a draft held by the core. Focus: the draft closed on every way out; the length limit |
| 12 Live checks, docs, D-entry, PR | 2 | Gemini | Running the checks and writing from `git diff` |

**Order:** Stage A: **1**, then 2, 3. Stage C: **8, 9** (they need only Stage A), then 7.

Stages: A = 1–3, B = 4–6, C = 7–9 with a review, D = 10–11 with a review, E = 12. Commands as P0.

## Review Focus

1. **The administrator bit and other high bits.** Expected: a role with `ADMINISTRATOR` (bit 63) or `START_ACTIVITIES` (bit 33) saved from the phone comes back from the server with the same bits. Pinned in Task 1 (`bit_63_round_trips_as_a_string`) and Task 9 (`saving a role keeps bits the page doesn't show`).
2. **Editing a role or channel someone else changes at the same time.** Expected: a `RoleUpserted` or `ChannelUpserted` while the page is open updates the untouched fields and keeps the user's unsaved text, as the desktop's draft does; saving sends only what the user changed. Pinned in Task 9: `a remote change keeps the local edit`.
3. **Acting on someone above you,** or on the owner. Expected: kick, ban and role changes aren't offered for members whose highest role is at or above yours, or for the owner, as the desktop's `member_actions.dart` decides; a server refusal still shows. Pinned in Task 10: `no actions on someone above you`.
4. **Losing a permission while its page is open.** Expected: P0's rule closes the page with `You no longer have access to this.`, and an unsaved edit is dropped, not sent. Pinned in Task 7: `losing manage channels closes the editor`.
5. **Reordering while someone adds or deletes a channel.** Expected: the drop sends positions for the list as it is now; a channel deleted mid-drag isn't sent; one added appears after. Pinned in Task 7: `a drop sends the list as it is now`.

---

## Stage A — The facade

### Task 1: Settings in the snapshot, and the permission sets

**Files:** modify `crates/opencord-mobile/src/types.rs`, `src/convert.rs`, `src/events.rs`.

**Interfaces (produces):**
- `PermissionSet { bits: String }`: an `i64` as a decimal string, with `pub fn permission_names(set: PermissionSet) -> Vec<String>` and `pub fn permission_set(names: Vec<String>) -> PermissionSet` exported, the names being the `bitflags` names in `crates/opencord-common/src/permissions.rs` (`VIEW_CHANNEL` …, `ADMINISTRATOR`). JavaScript never does bit arithmetic.
- `RoleRecord` (P0) gains `permissions: PermissionSet`. `ChannelRecord` gains `overwrites: Vec<OverwriteRecord>`, `bitrate: u32`, `user_limit: u32`, `text_in_voice: bool`, with `OverwriteRecord { target_kind: OverwriteTarget (Role | Member), target_id: String, allow: PermissionSet, deny: PermissionSet }`.
- `ServerSnapshot` gains `open_join: bool`, `everyone_role_id: String`, `voice_settings: VoiceSettingsRecord` (every field of the core's `VoiceSettings`, `afk_channel_id` as `Option<String>`), `upload_settings: UploadSettingsRecord { blocked_extensions, voice_message_max_seconds, max_file_bytes: f64, max_attachments }`, `private_settings: PrivateSettingsRecord { enabled, friends_only }`.
- Events: `ServerInfoChanged` gains `open_join` and `everyone_role_id`; new `VoiceSettingsChanged { server_key, settings }` sent beside the existing `SoundboardSettingsChanged` (which stays for the current screens); new `UploadSettingsChanged`; `PrivateSettingsUpdate` leaves the dropped list as `PrivateSettingsChanged`.
- [ ] Tests: `bit_63_round_trips_as_a_string`, `permission_names_are_the_bitflags_names`, `an_overwrite_crosses_with_both_sets`, `the_snapshot_carries_every_server_setting`, `settings_events_reach_javascript`, `parent_zero_leaves_the_category`. Run (fail), implement, run (pass); clippy; bindings; commit `feat(mobile): server settings and permission sets in the facade`.

### Task 2: The admin calls

**Files:** create `crates/opencord-mobile/src/api/admin.rs`; modify `src/api/mod.rs`, `src/convert.rs`.

**Interfaces (produces):** one export per `Client` call, same names, ids as strings, permission bits as `PermissionSet`, optional fields as `Option` (left out = unchanged, as the core's `*Changes`):
`update_server(server_key, ServerChangesRecord { name, description, open_join })`, `set_server_icon(server_key, source: Option<String>)`, `create_channel(server_key, kind, name, topic, parent_id)`, `update_channel(server_key, channel_id, ChannelChangesRecord { name, topic, parent_id: Option<String>, bitrate, user_limit, text_in_voice })`; `parent_id` `"0"` moves the channel out of its category, as the protocol says (`proto/opencord/v1/requests.proto:302`), `delete_channel`, `reorder_channels(server_key, Vec<ChannelPositionRecord { channel_id, position }>)` (the core's `ChannelPosition` has no parent: moving under another category is an `update_channel` with `parent_id`), `set_channel_overwrite(server_key, channel_id, OverwriteRecord)`, `delete_channel_overwrite(server_key, channel_id, target_kind, target_id)`, `create_role(server_key, name, color, permissions, hoist, mentionable, max_upload_bytes)`, `update_role(server_key, role_id, RoleChangesRecord)`, `delete_role`, `reorder_roles(server_key, role_ids)`, `add_member_role`, `remove_member_role`, `kick_member(server_key, user_id, reason)`, `ban_member`, `unban_member`, `fetch_bans → Vec<BanRecord>`, `update_voice_settings(server_key, VoiceSettingsChangesRecord)`, `update_upload_settings(server_key, blocked_extensions: Option<Vec<String>>)`, `update_private_settings(server_key, enabled, friends_only)`, `command_settings(server_key) → CommandListRecord`, `update_command_settings(server_key, CommandSettingsChangesRecord { source, name, enabled })` (the core's `CommandSettingsChanges { source, name, enabled: Option<bool> }`), `update_soundboard_sound`, `delete_soundboard_sound`, `soundboard_open_draft(path) → SoundDraftRecord { draft_id: f64, duration_ms: f64, waveform: Vec<f32> }` (the draft id as the voice-message drafts already cross it, `convert::draft_id`), `soundboard_preview_draft(SoundSelectionRecord)`, `soundboard_upload(server_key, selection, name, emoji)`, `soundboard_close_draft(draft_id)`.
- [ ] Tests: `changes_left_out_stay_none`, `a_draft_id_round_trips`, `a_ban_crosses_with_its_user_and_reason`. Run (fail), implement, run (pass); clippy; bindings; commit `feat(mobile): the admin calls in the facade`.

### Task 3: The JavaScript core, the fake, and the permission list

**Files:** `mobile/src/core/api.ts`, `native.ts`, `fake/FakeCore.ts`, `fake/world.ts`; create `mobile/src/features/admin/permissionInfo.ts`; tests `core/__tests__/fakeAdmin.test.ts`, `features/admin/__tests__/permissionInfo.test.ts`.

- **The fake** applies each admin call to its world and emits the matching event, so every page can be driven without a server; it refuses deleting the `@everyone` role and banning the owner with the server's errors.
- **`permissionInfo.ts`** is a port of `app/lib/features/settings/server/permission_info.dart`: the same groups, order, names and sentences (`Change nickname` / `Pick their own name on this server`, …), and which apply to text channels, voice channels and categories. Its test lists every bitflags name from Task 1's `permission_names` of all bits and asserts each is in exactly one group (or listed as hidden, as the desktop hides some).
- [ ] Run (fail), implement, run (pass); commit `feat(mobile): the core API, the fake and the permission list for admin pages`. Stage A's end.

## Stage B — The server's own settings

Each page: create `mobile/src/features/admin/<Page>.tsx` and its route under `mobile/src/app/server/[serverKey]/settings/`; register it in `serverRows.ts`; a test file `features/admin/__tests__/<Page>.test.tsx` with `each control does what it shows` (one assertion per control: the call and its arguments) and `a refusal puts the value back`; screenshots (dark, light, phone, tablet, reached from the server button).

### Task 4: Overview

Desktop: `overview_page.dart`. Sections:
- The icon at 80 with `Add a picture`, or `Change` and `Remove` when there's one (P0's `pickPhoto` → `set_server_icon`), and the desktop's footnote `PNG, JPEG, GIF or WebP, up to 8 MB. The middle square is …` (whole, from `app/lib/features/settings/picture_chooser.dart`).
- `Name` (`Server name`, required: `The server needs a name.`), `Description` (`What this server is about`), both with the header's `Save`.
- `Open to anyone with the address` / `Off: people need an invite to join` switch.
- `Address`: the server's address and fingerprint in mono with copy buttons, and the desktop's sentence starting `Share both: people compare the fingerprint when they add the` (whole).
- [ ] Tests as above; implement; screenshots; commit `feat(mobile): server overview settings`.

### Task 5: Voice & video, Upload settings, Private messages

Desktop: `voice_page.dart`, `uploads_page.dart`, `private_messages_page.dart`; one commit each.
- **Voice & video:** the fields `voice_page.dart` shows, with its labels, ranges and order (`Voice`: `Highest voice bitrate`; `Screen share`: `Highest resolution`, `Highest frame rate`, `Viewers per stream`; `Camera`: `Allow cameras`, `Cameras on at once, per channel`; `Idle`: `AFK channel`, `Move idle people after`), and the rows it marks as coming later shown the same way; the AFK channel as a choice of voice channels plus `None`; values as the desktop formats them (kbps, seconds, `Nx` fps). Shown only when the server has voice.
- **Upload settings:** `Blocked file types` with the desktop's footer (`Files with these extensions will be rejected when members try to upload them.`); each extension a row with a trailing remove (`Remove <ext>`, swipe on iOS); `Add extension` opens a text prompt (`Add extension, e.g. exe or sh`), validated with the desktop's rule and message (`Enter a valid extension (1 to 20 letters, numbers, or dots)`); `Block programs` adds the desktop's list at once.
- **Private messages:** `Private messages` switch with the desktop's sentence (whole), `Only between friends` with its sentence; the second disabled while the first is off.
- [ ] Tests per page; implement; screenshots; three commits `feat(mobile): voice and video settings`, `… upload settings`, `… private message settings`.

### Task 6: Commands

Desktop: `commands_page.dart`. A list grouped by source (the server's own, then each plugin, with P0's plugin icon), each command a switch row with its description; names that repeat across sources show the source under the name, as the desktop. Loaded with `command_settings` when the page opens and after each `CommandsUpdated`.
- [ ] Tests; implement; screenshots; commit `feat(mobile): commands on and off`. Stage B's end.

## Stage C — Channels and roles

### Task 7: Channels

Desktop: `channels_page.dart`, `channel_editor.dart`.
- **The list** is the channel tree as the channel list draws it, with a drag handle on each row (`Edit` mode on iOS, a long press on Android). A drop sends `update_channel` with the new `parent_id` first when the row landed under another category (`"0"` when it landed outside every category), then `reorder_channels` with positions for the whole list as it is now.
- **Create:** a `+` in the header with `Create channel` and `Create category` (the desktop's two buttons, `channels_page.dart`): kind (`Text`, `Voice`), `Channel name` or `Category name`, `Channel topic` (`Let everyone know what this channel is for`), the category (`No category`). Plugin channel types are P6. Empty list: `No channels yet.`
- **Edit** (a tap), with `channel_editor.dart`'s words and rules: `Channel name` (`The channel needs a name.`), `Channel topic`, `Category`; for voice `Bitrate` (with its footer `Higher bitrates sound clearer and use more bandwidth. This server …`, whole), `User limit` (`No limit`), `Text chat in this channel` / `People in voice can write messages here`; then a `Permissions` link (Task 8) and `Delete channel` or `Delete category` (destructive; the desktop's confirm words). Leave out what the desktop marks as coming later (its `Voice chat and push-to-talk take effect with later versions of …` rows), as the desktop shows them.
- [ ] Tests: `a drop sends the list as it is now`, `losing manage channels closes the editor`, `create sends kind, name and parent`, `delete asks first`. Implement; screenshots; commit `feat(mobile): channel settings`.

### Task 8: Channel permissions

Desktop: `overwrites.dart` and the permissions part of `channel_editor.dart`.
- **The page:** the overwrites as rows under `ROLES` and `MEMBERS` (`@everyone` first), `Add role or member` (a searchable picker: `Search roles and members`, `No role or member matches.`), the desktop's header sentence (`What everyone may do here, unless a role or member says …`, whole), and a tap opening one overwrite (`Allow or deny for <name> here. Inherit keeps …`, whole; `Reset to inherit`; `Remove <name>`): `permissionInfo.ts`'s groups for this channel kind, each permission a three-state control (`Allow`, `Inherit`, `Deny`) as a `Segmented` with the desktop's icons, saved on touch through `set_channel_overwrite` with the new `allow` and `deny` sets. `Remove` deletes the overwrite (`delete_channel_overwrite`).
- **The rule** (from `overwrites.dart`, ported as a pure function `applyOverwriteChoice(allow, deny, name, choice)`): allowing a permission removes it from deny and adds it to allow, and so on; an overwrite with both sets empty is removed rather than saved.
- [ ] Tests: port `app/test/unit/overwrites_test.dart` for `applyOverwriteChoice`; `an empty overwrite is removed`; `a choice saves on touch`. Implement; screenshots; commit `feat(mobile): channel permissions`.

### Task 9: Roles

Desktop: `roles_page.dart`.
- **The list:** roles by position with their colour and member count, drag to reorder (only roles below your highest; `reorder_roles` with the full order), `Create role` (a new `New role` with the desktop's default colour and permissions). `@everyone` is last and can't be dragged or deleted. Plugin roles (`plugin_id` set) show `Belongs to the plugin <name>. Its permissions follow the plugin…` (whole) and can't be edited. A role at or above your own shows `This role is at or above your own, so you can look but not …` (whole) and is read-only.
- **A role,** with `roles_page.dart`'s words and rules: `Role name` (`The role needs a name.`); the colour (the desktop's swatches, as a grid); `Display separately` / `Its members get their own section in the list`; `Allow anyone to @mention it`; `Maximum file size` (`No limit of its own`, the desktop's units and validation: `Enter a valid positive number`, `Must be at least 1 MB`, `Cannot exceed server maximum (<size>)`; on `@everyone` it's required: `A limit is required for @everyone`); `Copy role ID` (toast `Role ID copied`); then `Permissions`: every server-wide permission of `permissionInfo.ts` as switches, grouped, with `Administrator` behind its sentence (`Administrators have every permission in every channel, …`, whole). Saving sends only changed fields (`RoleChangesRecord`), and the permission set is the role's current set with only the touched names changed. `Delete role` (confirm `Delete <name>?` / `Everyone who has this role loses it.`).
- [ ] Tests: `saving a role keeps bits the page doesn't show`, `a remote change keeps the local edit`, `everyone can't be moved or deleted`, `only roles below yours can be dragged`. Implement; screenshots; commit `feat(mobile): roles`. Stage C's end and review.

## Stage D — People and sounds

### Task 10: Members, invites and bans

Desktop: `members_page.dart`, `invites_bans_pages.dart`, `app/lib/features/members/member_actions.dart`.

- **`mayActOn`** (create `mobile/src/features/members/mayActOn.ts`, test `__tests__/mayActOn.test.ts`): `mayActOn(actor: ActorContext, member: MemberContext): { kick: boolean, ban: boolean, roles: boolean }`, a port of `member_actions.dart`'s `_may` for these three: never on yourself or the owner; only on members whose highest role is below the actor's (the owner is above everyone); with `kick_members`, `ban_members` or `manage_roles`. A table test with a case for every branch of `_may` these three use, named after the branch. P5 adds the voice actions to the same function.
- **Members:** a search field (`Search N members`), each row with avatar, name, roles and `Joined`; a tap opens P0's `MemberProfile` with the actions this user may take: `Roles` (a checklist of assignable roles: `add_member_role`, `remove_member_role`), `Kick` and `Ban` (each asks for an optional reason in its confirm). The same actions appear on the profile wherever it opens. Empty search: `No members match "<query>".`
- **Invites:** `Links stop working when they expire or reach their limit.`; each invite with `Code`, `Made by`, `Uses`, `Expires`; `Create invite` opens P2's invite sheet; a row's menu `Copy link`, `Copy code`, `Revoke invite` (confirm `Revoke this invite?` with the desktop's sentence). Empty: `No invites yet. Create one to bring people in.`
- **Bans:** each ban with the user, the reason and who banned; `Unban` (confirm).
- [ ] Tests: `no actions on someone above you`, `kick asks for a reason and sends it`, `revoke asks first`, `unban removes the row`. Implement; screenshots; commit `feat(mobile): members, invites and bans`.

### Task 11: Soundboard sounds, with trim

Desktop: `soundboard_page.dart`, `app/lib/features/soundboard/trim_editor.dart`.
- **The page,** with `soundboard_page.dart`'s words: first the server's soundboard settings (`Soundboard enabled`, `Allow default sounds` / `The sounds that come with Opencord.`, `Allow sounds from other servers` / `For people with Use external sounds.`, `Cooldown between sounds, per person`, `Most sounds`; the `soundboard_*` and `max_sounds` fields of `update_voice_settings`, so Task 5's Voice & video page leaves them out); then `Sounds` with `Upload sound` (empty: `No sounds yet. Upload one from an audio file.`); a tap on one opens `Edit sound`: `Sound name`, the emoji (`Change emoji`, `Remove emoji`, P2's picker), `Volume`, `Preview`, `Delete sound` (confirm `Delete <name>?` / `It is removed from the soundboard for everyone.`).
- **Add:** pick an audio file (the music upload's `pickAudio.ts`), `soundboard_open_draft(path)`, then a trim screen: the waveform (`SoundDraft.waveform`) across the width with two handles for start and length (at most the server's limit, as the desktop clamps), `Play` previewing the selection (`soundboard_preview_draft`), `Normalize` and `Volume`, then `Name`, `Emoji` and `Upload`. The draft is closed (`soundboard_close_draft`) on upload, on cancel, on back, and when the app goes to the background for more than 60 s.
- [ ] Tests: `the draft is closed on every way out`, `the selection can't pass the limit`, `upload sends the selection`. Implement; screenshots; commit `feat(mobile): soundboard sounds with a trim editor`. Stage D's end and review.

## Stage E — Live checks

### Task 12: Live checks, docs, D-entry and the pull request

- [ ] Against a local server with the desktop open on the same server as another admin, do each page's main action on the phone and see it on the desktop, and the reverse for roles and channels (Review Focus 2). Ledger each with a screenshot.
- [ ] `docs/mobile.md` ("Running a server", "Not there yet" loses its admin row); the iPhone hand checks (the drag to reorder, the trim handles, the colour grid).
- [ ] The next D-number: permission sets as strings, drag-to-reorder, the trim screen. From `git diff`.
- [ ] `.github/check.sh`; push; PR description; `gh pr checks` green; final review round; report.
