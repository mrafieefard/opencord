# Phone P0: The Foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The phone knows roles, server-wide permissions and presence, shows a server's members, and has the pieces every later phone plan builds on: a settings screen kit, a square photo picker, and the server button that opens a server's settings.

**Architecture:**
- **The facade carries what the core already has.** `ReadySnapshot` holds `roles`, `presences` and `server_permissions`; the facade drops them today, along with `RoleCreate/Update/Delete`, `PresenceUpdate` and the `server_permissions` half of `PermissionsUpdate` (`crates/opencord-mobile/src/convert.rs:228`, `events.rs:540`). This plan maps them.
- **The store keeps them per server,** beside the snapshot's members, and selectors answer "may I" questions the way the desktop's `ServerData.can` does.
- **The server button is the owner's design** (design §6.1). On iOS it takes the tab bar's search slot while a server is in front. On Android it's a header button.

**Tech Stack:** the phone app (Expo 58, React Native 0.88, uniffi through `ubrn` 0.31.0-6, zustand, jest) and Rust 2024 for `crates/opencord-mobile`. **No new dependencies:** `expo-image-picker` 58 is already installed.

**Spec:** `docs/superpowers/specs/2026-11-15-phone-parity-design.md`, §3 (P0), §4, §5 and §6. Read all of §6 before Task 6. The phone app's own design is `docs/superpowers/specs/2026-10-26-mobile-client-design.md` (§6 state, §9 visual design); `docs/mobile.md` says how to build and run it.

## Global Constraints

- **Branch:** `feat/phone-foundation`, made from `feat/plugins-phone` (PR #52) unless the owner answered design §9 Q3 otherwise; its first commit holds this plan and the design. Open a **draft pull request into `beta`** at the end of Stage A. Its description says the diff includes plugins milestones 1–4 until #46–#52 merge. Never merge; never push to `beta`, `canary` or `main`.
- **Before Task 1:** `git status --short -uno` prints nothing; `df -h /` shows at least 20 GB free; `source mobile/scripts/env.sh && cd mobile && yarn core:android && scripts/check-bindings.sh` passes; `adb devices` shows `emulator-5554`.
- **Scope:** `crates/opencord-mobile/`, `mobile/src/`, `mobile/packages/core-native` (generated only), `docs/`. Nothing in `crates/opencord-core`, `crates/opencord-server`, `proto/` or `app/` changes. If a task seems to need that, stop and ask.
- **Ids are decimal strings** in JavaScript; the facade converts. Permission bits never cross as numbers: they cross as named flags, as `ChannelAccess` does.
- **Words the user sees** are copied exactly from the task that gives them; where the desktop already says it, the phone says the same. Words with no desktop source are *new on the phone*; in this plan: `You no longer have access to this.`, `Choose a photo`, `Take a photo`, `Remove photo`, `Opencord can't use the camera. Allow it in Settings.`, `Open on a computer`, `N members · M online`.
- **Never log** names, message text or tokens. Log ids and error kinds.
- **Generated, never edited by hand:** `mobile/packages/core-native/{src,cpp}/generated`, `mobile/android`, `mobile/ios`.
- **Build commands** start with `source mobile/scripts/env.sh &&`. `CARGO_INCREMENTAL=0` is set by it; never `cargo clean`.
- **Numbers follow the facade's rule** (`crates/opencord-mobile/src/types.rs:1-3`): ids are decimal strings; times (ms since the epoch), sizes, durations, positions and counters cross as `f64` through `convert::number` and `convert::unsigned`; a draft id comes back through `convert::draft_id`. No `u64` or `i64` field in a record JavaScript reads, whatever the core's type: a `u64` becomes a JavaScript `bigint` and breaks every comparison.

## Working Rules

`CLAUDE.md` applies, and so do `GEMINI.md`'s "How to write code here" and "Reports and the ledger". **Working Rules 1–10 of `docs/superpowers/plans/2026-11-13-plugins-m4-phone.md` apply word for word** (test first; mutation lines; tests feed what the real sender sends; every argument has a named source; several-people tests list each person; what a control shows is what it does; screens are looked at in dark, light, phone and `--tablet`; stage ends paste `check.sh` and `gh pr checks`; what wasn't run isn't claimed; the PR description comes from `git diff --stat`). Read them before Task 1.

**Ledger:** `.superpowers/sdd/phone-p0/progress.md` (git-ignored), one entry per task.

### Lessons from earlier reviews (every phone plan)

These come from the review findings of the phone apps plan and the plugins milestones. Each is a rule a review checks.

1. **Screens are judged, not only taken.** Each screen's task names what must be measurably true (what stays visible with the keyboard up, row heights, where the time sits). The reviewer renders every screen in both themes and the tablet size and tries every gesture by hand; a screenshot that shows a blank or a red banner is a failure, reported. Metro's watcher misses edits: restart it with `-c` before screenshots, and launch twice after a restart.
2. **The app's own controls only:** `mobile/src/ui` (its `Row`, `Menu`, `Switch`, `Slider`, `Text`, `confirm`, toasts) and react-native-gesture-handler's touchables inside gesture rows, never React Native's `Pressable` there. No `as` casts to quiet the types; no system font in inputs.
3. **Errors are never swallowed:** no empty `catch`, no `void promise` without `.catch(report)`, no `try/catch` "for tests". A failure the user caused is a toast with the server's words; one they didn't is `report()`ed.
4. **Load, then merge.** A screen that loads a list subscribes first and merges what arrived during the load, never replaces it. After any `await` on the core, re-read state before writing it back.
5. **A page closed by an event** (P0's lost-permission rule) lists "my own request in flight" as a case: the event can come before the reply, and the reply's error must still show.
6. **Every item under "Produces" gets a test,** not only those in Step 1, and every Review Focus line gets its own test and its mutation (`file:line old → new`, with the failing output pasted).
7. **Every control is tested twice in a row,** checking the effect (the call made, the state changed), not the icon.
8. **A record change is three changes:** the Rust record, the TypeScript type in `core/api.ts`, and the fake. A test feeds the fake the shape the Rust side serialises.
9. **Native config is checked live.** Anything registered in `app.config.ts` or a config plugin (schemes, permissions, background modes) is proven on the emulator, not by a unit test.
10. **The D-entry** names only identifiers found with `rg` in `git diff`, and every count is counted, not remembered. For Claude's tasks, describe Claude's own reports.
11. **A new dependency** passes `yarn licenses:check` (`mobile/scripts/licenses.js`, MIT, Apache-2.0, BSD, ISC, Zlib, 0BSD only) in the task that adds it.
12. **Through the bridge** (if the owner chose Gemini plus the bridge): the bridge refuses `source`, `gh` and `.github/check.sh`. A Claude task ends after `cargo test`, `cargo clippy` and `cargo fmt --check` run with `--config` flags; **the bindings step (`yarn core:android`, `check-bindings.sh`), screenshots and the commit are Gemini's**, as the next step of the same task.


**Rulings:** where the plan is wrong or unclear, make the smallest change that satisfies the design, write `Task <N>: Ruling: <what> — <why> — <cost if wrong>` in the ledger, and go on.

### Task owners

Scores follow the memory's rule (4–5 Claude, 3 Gemini with the review focus named, 1–2 Gemini). If the owner chose one builder (design §9 Q2), ignore this column.

| Task | Score | Owner |
|---|---|---|
| 1 Facade: roles, server access, presence | 4 | Claude |
| 2 The JavaScript core and the fake | 2 | Gemini |
| 3 The store and its selectors | 3 | Gemini (focus: events applied before and after Ready; a role deleted while a member holds it) |
| 4 The settings kit | 2 | Gemini |
| 5 The square photo picker | 2 | Gemini |
| 6 The members list and a member's profile | 3 | Gemini (focus: grouping, offline at the end, plugin users) |
| 7 The server button and its sheet | 3 | Gemini (focus: the iOS tab slot while a server is or isn't in front) |
| 8 Live check, docs, D-entry, PR | 2 | Gemini |

### Commands

| What | Command, from the repository root, after `source mobile/scripts/env.sh &&` |
|---|---|
| The facade's tests | `cargo test -p opencord-mobile` |
| The core for the emulator, and the bindings | `cd mobile && yarn core:android` |
| The bindings are current | `mobile/scripts/check-bindings.sh` |
| The phone's JavaScript checks | `cd mobile && yarn typecheck && yarn lint && yarn test` |
| One jest file | `cd mobile && yarn test <path>` |
| The app on the emulator, fake world | `cd mobile && yarn android:fake` |
| Screenshots | `mobile/scripts/screens.sh` (add `--tablet`, `--light`) |
| Everything CI runs on Linux | `.github/check.sh` |

### Stages

| Stage | Tasks | At its end |
|---|---|---|
| A — The phone knows roles | 1–3 | Full checks; push; open the draft PR; read its checks |
| B — The pieces | 4–7 | Full checks; push; **review of A–B** (a fresh Opus context, as the M4 plan's "Stages and their reviews" says) |
| C — Live and docs | 8 | Full checks; push; review round; report to the owner |

## Review Focus

1. **Events arrive while a server is seeding** (a `RoleUpdate` or `PresenceUpdate` before the first Ready's recent activity is fetched). Expected: they're held with the other events and applied in order after it, never lost and never applied to a missing snapshot. Pinned in Task 3: `role and presence events wait for seeding`.
2. **A role is deleted, or a member loses it, while the members list or the server sheet is open.** Expected: the member moves to the right group, the role's colour goes, and a page the user may no longer open closes with the toast `You no longer have access to this.` Pinned in Task 3 (`deleting a role takes it off every member`) and Task 7 (`a row goes when its permission goes`).
3. **The administrator bit.** Expected: `ADMINISTRATOR` grants every flag, exactly as `convert::access` does for channels. (The server already sends the owner every bit, `crates/opencord-server/src/guild.rs:637`, so the phone adds no owner rule of its own, as the desktop's `ServerData.can` adds none.) Pinned in Task 1: `administrator_grants_every_server_flag`.
4. **The iOS search slot outside a server.** Expected: on the Servers tab it's still Search and filters servers; only while a server's screen is in front does it become the server button, and leaving the server brings Search back. Pinned in Task 7: `the tab slot is search outside a server`.
5. **A big server** (2,000 members). Expected: the members list scrolls without dropped frames (FlashList), and grouping runs once per change, not per row. Pinned in Task 6: `grouping 2000 members is one pass` (counts calls to the grouping function).

---

## Stage A — The phone knows roles

### Task 1: The facade's roles, server access and presence

**Files:**
- Modify: `crates/opencord-mobile/src/types.rs`, `src/convert.rs`, `src/events.rs`, `src/api/servers.rs`
- Test: in the same files' `#[cfg(test)]` modules, as the crate does today

**Interfaces:**
- Produces (uniffi records, all fields named as below; JavaScript sees them camelCased):
  - `RoleRecord { id: String, name: String, color: u32, position: i32, hoist: bool, mentionable: bool, max_upload_bytes: Option<f64>, plugin_id: Option<String>, access: ServerAccess }`
  - `ServerAccess { manage_server, manage_channels, manage_roles, kick_members, ban_members, create_invite, manage_nicknames, change_nickname, manage_messages, mention_everyone, manage_plugins, manage_soundboard, music_manage, mute_members, deafen_members, move_members, priority_speaker, video, screenshare, administrator: bool }`: the server-wide flags. A role's `access` is that role's own bits; the snapshot's is the user's.
  - `enum PresenceStatus { Online, Idle, Dnd, Offline }` and `PresenceRecord { user_id: String, status: PresenceStatus }`
  - `ServerSnapshot` gains `roles: Vec<RoleRecord>`, `presences: Vec<PresenceRecord>`, `server_access: ServerAccess`.
  - `MobileEvent` gains `RoleUpserted { server_key, role: RoleRecord }`, `RoleDeleted { server_key, role_id: String }`, `PresenceChanged { server_key, presence: PresenceRecord }`; `AccessChanged` gains `server_access: ServerAccess`.
  - `pub async fn presence_update(server_key: String, status: PresenceStatus) -> Result<(), MobileError>` in `api/servers.rs`, calling `Client::update_presence`.
- `RoleCreate`, `RoleUpdate`, `RoleDelete` and `PresenceUpdate` leave the dropped list in `core_event`.

- [ ] **Step 1: Write the failing tests**
  - `administrator_grants_every_server_flag`: `convert::server_access(Permissions::ADMINISTRATOR.bits())` has every field true.
  - `server_access_follows_the_permission_bits`: `KICK_MEMBERS | MANAGE_ROLES` gives exactly `kick_members` and `manage_roles` true.
  - `a_role_crosses_with_its_bits`: a `core::Role` with `permissions = BAN_MEMBERS` becomes a `RoleRecord` with `id` as a decimal string and `access.ban_members` true, everything else false.
  - `the_snapshot_carries_roles_presences_and_server_access`: a `ReadySnapshot` with two roles, one presence and `server_permissions = MANAGE_SERVER` converts with all three.
  - `role_and_presence_events_reach_javascript`: feed `RoleCreate`, `RoleDelete { role_id: 7 }` and `PresenceUpdate` through `core_event`; each comes out as its `MobileEvent` with ids as strings.
  - `a_permissions_update_carries_server_access`: `PermissionsUpdate { server_permissions: MANAGE_CHANNELS, .. }` gives `AccessChanged` with `server_access.manage_channels` true.
- [ ] **Step 2:** `cargo test -p opencord-mobile` fails on each; paste the lines in the ledger.
- [ ] **Step 3:** implement `pub(crate) fn server_access(bits: i64) -> ServerAccess` in `convert.rs`, with `ADMINISTRATOR` granting every flag, like `access` above it. "Is the owner" (needed by the Hosting row only) is `snapshot.ownerId === snapshot.selfUser.id` in JavaScript, as the desktop's `isOwner`.
- [ ] **Step 4:** `cargo test -p opencord-mobile` passes; `cargo clippy -p opencord-mobile --all-targets -- -D warnings` is clean.
- [ ] **Step 5:** `cd mobile && yarn core:android && scripts/check-bindings.sh`, then commit `feat(mobile): roles, server permissions and presence reach the phone`.

### Task 2: The JavaScript core and the fake

**Files:**
- Modify: `mobile/src/core/api.ts`, `mobile/src/core/native.ts`, `mobile/src/core/fake/FakeCore.ts`, `mobile/src/core/fake/world.ts`
- Test: `mobile/src/core/__tests__/fakeCore.test.ts`

**Interfaces:**
- Consumes: Task 1's records and `presenceUpdate`.
- Produces: `CoreApi.presenceUpdate(serverKey: string, status: PresenceStatus): Promise<void>`. The fake world gains two roles on its first server (`Moderators`, hoisted, colour `0x5865f2`, `kick_members` and `manage_messages`; `Owner`, hoisted, `administrator`), presences for every member (mixed, at least one of each status), and makes the fake's own user the server's owner on that server only, so every admin page shows there and none shows on the second server.

- [ ] **Step 1:** test `the fake sends presence changes as events`: `presenceUpdate(server, 'Idle')` emits `PresenceChanged` for the fake's own user.
- [ ] **Step 2:** test `the fake world has roles and an owner on its first server only`.
- [ ] **Step 3:** run them (fail), implement, run (pass).
- [ ] **Step 4:** `yarn typecheck && yarn lint && yarn test`; commit `feat(mobile): the core API and the fake know roles and presence`.

### Task 3: The store and its selectors

**Files:**
- Modify: `mobile/src/state/types.ts`, `mobile/src/state/events.ts`, `mobile/src/state/selectors.ts`, `mobile/src/state/actions/profile.ts`
- Test: `mobile/src/state/__tests__/roles.test.ts` (new)

**Interfaces:**
- Consumes: Task 1's events; the snapshot's `roles`, `presences`, `serverAccess`.
- Produces:
  - `useServerAccess(serverKey: string): ServerAccess | null`
  - `useRoles(serverKey: string): RoleRecord[]` (sorted by `position`, highest first)
  - `usePresence(serverKey: string, userId: string): PresenceStatus` (`'Offline'` when unknown)
  - `useMemberColor(serverKey: string, userId: string): number | null`: the colour of the member's highest role with a non-zero colour, as the desktop does.
  - `serverSettingsRows(access: ServerAccess, voiceEnabled: boolean): ServerRowKey[]`, a pure function in `selectors.ts`, giving the design §6.1 rows in order. `ServerRowKey` is `'overview' | 'voice' | 'commands' | 'soundboard' | 'privateMessages' | 'channels' | 'roles' | 'plugins' | 'hosting' | 'uploads' | 'members' | 'invites' | 'bans'`. The permissions per row are exactly `canOpenServerSettings`'s in `app/lib/features/settings/server_settings.dart:25-160` (Overview, Voice & video when voice is on, Commands, Private messages and Upload settings need `manage_server`; Soundboard needs `manage_server` or `manage_soundboard`; Channels `manage_channels`; Roles `manage_roles`; Plugins `manage_plugins`; Hosting (the desktop's Domains & network, owner only, and Music, `manage_server` or `music_manage`) shows when either would; Members needs kick, ban or `manage_roles`; Invites `manage_server`; Bans `ban_members`).
  - `setPresence(serverKey, status)` in `actions/profile.ts`.
- [ ] **Step 1: Write the failing tests**
  - `role and presence events wait for seeding`: with `seeding: true`, a `RoleUpserted` and a `PresenceChanged` go to `held` and apply after seeding ends, in order.
  - `deleting a role takes it off every member`: `RoleDeleted` removes the role and its id from each member's `roleIds`.
  - `access changes replace server access`.
  - `member colour comes from the highest coloured role`.
  - `serverSettingsRows matches the desktop` as a table test over at least: a plain member (no rows), `manage_channels` only (`channels`), `ban_members` only (`members`, `bans`), the owner (every row), voice off (no `voice`).
- [ ] **Step 2:** run (fail), implement, run (pass).
- [ ] **Step 3:** commit `feat(mobile): the store keeps roles, presence and server permissions`. Then the stage end (see Stages).

## Stage B — The pieces

### Task 4: The settings kit

**Files:**
- Create: `mobile/src/ui/settings/SettingsList.tsx`, `SettingsSection.tsx`, `SettingsRow.tsx`, `SettingsRow.ios.tsx`, `SettingsRow.android.tsx`, `SettingsRow.types.ts`, `useSaveButton.ts`, `index.ts`
- Modify: `mobile/src/ui/index.ts`, `mobile/src/app/dev/gallery.tsx`
- Test: `mobile/src/ui/settings/__tests__/SettingsRow.test.tsx`

**Interfaces:**
- Produces:
  - `SettingsList({ children })`: a scroll view with the platform's grouped background (`colors.sidebar` on iOS, `colors.background` on Android), keyboard-aware (`react-native-keyboard-controller`'s `KeyboardAwareScrollView`, already a dependency).
  - `SettingsSection({ title?: string, footer?: string, children })`: iOS inset rounded group with an uppercase footnote title; Android a section header in `labelLarge` and full-width rows.
  - `SettingsRow` props, one of: `{ kind: 'link', label, value?, onPress, icon? }`, `{ kind: 'switch', label, detail?, value, onChange, disabled? }`, `{ kind: 'slider', label, value, min, max, step?, format(v) => string, onChange, onDone }`, `{ kind: 'choice', label, value, options: {key, label}[], onChange }` (opens the app's `Menu`), `{ kind: 'text', label, value, placeholder?, maxLength, onChange, multiline? }`, `{ kind: 'destructive', label, onPress }`.
  - `useSaveButton({ dirty: boolean, saving: boolean, onSave: () => Promise<void> })`: puts `Save` in the native header's right side, disabled unless `dirty && !saving`, and shows a spinner while saving.
- Use the existing `mobile/src/ui` `Switch`, `Slider`, `Menu`, `Text` and theme tokens; no new colours.
- [ ] **Step 1:** tests: `a link row calls onPress`, `a switch row shows its value and calls onChange`, `a choice row opens the menu with each option`, `a destructive row is drawn in the danger colour`, `save is disabled until something changed`.
- [ ] **Step 2:** run (fail), implement, run (pass).
- [ ] **Step 3:** add a `Settings kit` page to `app/dev/gallery.tsx` with one of each row in two sections; screenshot it (dark, light, phone, tablet; on Android here, iOS compiled by CI) and fix what looks wrong before committing. Compare against iOS Settings and Android Settings: row height 44 pt iOS / 56 dp Android, value text in `textMuted`, chevrons only on link rows.
- [ ] **Step 4:** commit `feat(mobile): a settings list kit`.

### Task 5: The square photo picker

**Files:**
- Create: `mobile/src/features/media/pickPhoto.ts`
- Test: `mobile/src/features/media/__tests__/pickPhoto.test.ts`

**Interfaces:**
- Produces: `pickPhoto(options: { title: string, allowRemove: boolean }): Promise<{ kind: 'picked', file: PickedFile } | { kind: 'removed' } | { kind: 'cancelled' }>`. It shows the app's `Menu` as an action sheet with `Choose a photo`, `Take a photo` and, when `allowRemove`, `Remove photo` (destructive); then `expo-image-picker` with `allowsEditing: true`, `aspect: [1, 1]`, `quality: 0.9`, `mediaTypes: ['images']`. The camera asks for permission first; refusal shows the toast `Opencord can't use the camera. Allow it in Settings.`
- [ ] **Step 1:** tests (mock `expo-image-picker`): `a picked photo comes back as a file`, `cancelling comes back as cancelled`, `remove is offered only when allowed`, `a refused camera shows the toast`.
- [ ] **Step 2:** run (fail), implement, run (pass); commit `feat(mobile): a square photo picker`.

### Task 6: The members list and a member's profile

**Files:**
- Create: `mobile/src/features/members/MembersSheet.tsx`, `MemberRow.tsx`, `groupMembers.ts`, `MemberProfile.tsx`, `PresenceDot.tsx`
- Modify: `mobile/src/ui/Avatar.tsx` (an optional `presence` prop drawing `PresenceDot` at the bottom right), `mobile/src/features/voice/ParticipantSheet.tsx` (its header becomes `MemberProfile`), `mobile/src/features/chat/ChatPane.tsx` (a people button in the header), `mobile/src/features/server/TabletPanes.tsx` (the list as a third column at ≥ 1000 pt wide), `mobile/src/app/server/[serverKey]/members.tsx` (new route, a sheet)
- Test: `mobile/src/features/members/__tests__/groupMembers.test.ts`, `__tests__/MembersSheet.test.tsx`

**Interfaces:**
- Consumes: Task 3's selectors.
- Produces: `groupMembers(members, roles, presences, voice, query): MemberSection[]`, a port of `memberSections` in `app/lib/features/members/member_sections.dart:58-150`: while you're in voice, `In this voice channel` first; then one section per hoisted role (highest first) with the online members whose highest hoisted role it is; then `Online`; then `Offline`, collapsed when longer than `offlineCollapseAt` = 30. Headers read `NAME — N` (e.g. `MAINTAINER — 2`). A row's second line is `In voice · <channel>` when they're in voice. Empty sections are left out; `query` keeps names containing it. Port `app/test/unit/members/member_sections_test.dart`.
- `MemberProfile({ serverKey, userId })`, as the desktop's `app/lib/features/members/member_profile.dart`: avatar at 72 with presence, the name in the role colour, `In voice · <channel>` when in voice, `Roles` as chips (`No roles` when none), `Member since`, `Identity key` with the fingerprint in mono and `Copy fingerprint` (toast `Fingerprint copied`), and `This member is not on the server any more.` when they left. Later plans add buttons below it (P3: `Message`, `Add friend`; P4: roles, kick, ban; P5: voice moderation).
- Member rows: 52 pt, avatar 32 with presence, name in the role colour, an offline row at 50 % opacity, as the desktop.
- [ ] **Step 1:** tests: the ported `member_sections_test.dart` cases, `grouping 2000 members is one pass`, `tapping a row opens the profile`.
- [ ] **Step 2:** run (fail), implement, run (pass).
- [ ] **Step 3:** screenshots: the sheet from the chat header (phone), the column (tablet), a profile. Dark and light. Reached by the real button. Commit `feat(mobile): the members list and member profiles`.

### Task 7: The server button and its sheet

**Files:**
- Create: `mobile/src/features/serverMenu/ServerSheet.tsx`, `serverRows.ts`, `mobile/src/app/server/[serverKey]/settings/_layout.tsx` (a stack inside a form sheet), `settings/index.tsx`
- Modify: `mobile/src/app/(tabs)/_layout.tsx`, `mobile/src/features/server/ChannelListHeader.tsx` (the Android button, and on both systems a tap on the server's name, which is the only way in while a chat is in front and the tab bar is hidden), `mobile/src/features/servers/ServerRow.tsx` (a `Settings` item in the long-press menu), `mobile/src/state/actions/ui.ts` (`serverInFront: string | null`), `mobile/src/ui/icons.ts` (`serverMenu: { ios: 'ellipsis', android: 'more_vert' }`)
- Test: `mobile/src/features/serverMenu/__tests__/ServerSheet.test.tsx`, `__tests__/tabSlot.test.tsx`

**Interfaces:**
- Consumes: `serverSettingsRows` (Task 3), the settings kit (Task 4).
- Produces: `registerServerPage(key: ServerRowKey, page: { label: string, route: string })` in `serverRows.ts`. A row shows only when its key is in `serverSettingsRows(...)` **and** a page is registered for it, so this plan's sheet shows only the rows that exist, and each later plan registers its own. `hosting` is registered here as a non-tappable row whose value is `Open on a computer` (design §3).
- **The iOS slot** (`(tabs)/_layout.tsx`): while `serverInFront` is set, the `search` trigger gets `disabled`, the `serverMenu` icon, the label `Server`, and a `tabPress` listener that opens `server/[serverKey]/settings`. `disabled` makes the navigator emit `tabPress` without switching tabs (`expo-router/build/native-tabs/types.d.ts:462`). Outside a server it's Search as today. If the slot can't be driven this way on the device, write a ruling and float a `GlassIconButton` (`ui/GlassIconButton.ios.tsx`) at the same position, beside the tab bar.
- **The sheet** (design §6.1, sketch there): the header (server icon 40, name, `N members · M online`), then `Invite people` (registered by P2), `Mark as read` (`useServerRowActions(row).markRead`, the same file), then the admin rows, then `Leave server` (destructive; reuse `useServerRowActions(row).confirmLeave` from `features/servers/serverRowParts.tsx`, which already says `Leave <name>?` / `You can come back with an invite.`). Medium detent; a pushed page grows it to large.
- [ ] **Step 1:** tests: `the tab slot is search outside a server`, `the tab slot opens the server sheet inside a server`, `the server's name opens the sheet while a chat is in front`, `rows follow permissions` (fake world: first server shows each registered row, second shows none), `a row goes when its permission goes` (an `AccessChanged` while the sheet is open removes it, and an open page for it closes with the toast `You no longer have access to this.`), `leave asks first`.
- [ ] **Step 2:** run (fail), implement, run (pass).
- [ ] **Step 3:** screenshots on Android (header button) and the sheet on phone and tablet; the iOS slot is compiled by CI and listed as an owed iPhone check. Commit `feat(mobile): the server button and its sheet`. Then the stage end and review.

## Stage C — Live and docs

### Task 8: Live check, docs, D-entry and the pull request

- [ ] **Step 1:** against a local server (`docs/mobile.md`, "A server for the emulator"), with the desktop app in the same server: change a role's colour and permissions on the desktop; the phone's member list and server sheet follow within a second. Set yourself idle on the phone; the desktop shows it. Ledger the screenshots.
- [ ] **Step 2:** `docs/mobile.md`: the members list, the server button (iOS slot, Android header), and the hand checks table gains rows for both on iPhone. Move the stale items out of "Not there yet" (the soundboard, voice messages and commands exist).
- [ ] **Step 3:** the next D-number in `docs/decisions.md`: the server button's slot, settings saving on touch (design §6.2), member grouping. Written from `git diff`.
- [ ] **Step 4:** `.github/check.sh`; push; PR description from `git diff --stat` (what changed, why, how tested, iOS not run); `gh pr checks` until green; review round; report to the owner.
