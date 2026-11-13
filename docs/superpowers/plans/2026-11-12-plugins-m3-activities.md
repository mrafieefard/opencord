# Plugins, Milestone 3: Activities — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A plugin registers an activity; a member in a voice call starts it from the call bar; everyone in the call sees its tile and can open the plugin's own screen in it, small or large, until the activity ends.

**Architecture:**
- **The server** keeps two things in memory beside the channel types of milestone 2: the **activities** running plugins registered, and the **activity sessions** running in calls. A session belongs to one voice channel and ends by a member, by the plugin, when the plugin stops, or when the call empties.
- **A view's place** is now a plugin channel *or* an activity session. Everything milestone 2 built for views (client files, events both ways, limits, uploads, images, hot reload) is reused unchanged; only who may open one, and when it closes, differ.
- **`opencord-core`** opens views by place, and its client VM gains the call: `app.call` and the events `call_members_change` and `speaking_change`. The call's members and speakers come **from the renderer as an input**, because the app already tracks both; the core keeps no second copy.
- **The desktop app** adds the Activities button and picker to the call bar, an activity tile to the call's stage, and draws an opened activity with the same `PluginArea` and `opencord_blocks` as a plugin channel.

**Tech Stack:** as milestone 2 (Rust 2024, mlua 0.11 with Luau, axum 0.8, sqlx 0.9, prost; Flutter with Riverpod 3 and flutter_rust_bridge 2.13). **No new dependencies.**

**Spec:** `docs/superpowers/specs/2026-11-07-plugins-design.md`. Read §2, §5, §8.1, §8.5, §9.6, §10.3–§10.4, §11.4–§11.7, §12.5, §15, §20–§22 before Task 1, and the sections a task names before that task. "§" below means the spec. Milestone 2's plan, `docs/superpowers/plans/2026-11-09-plugins-m2-channels.md`, holds the view wire format this plan adds to. If the spec and this plan disagree, or a step can't be done as written, stop and ask the user.

## Global Constraints

- **Branch and pull request:** `feat/plugins-activities`, made by Claude from `feat/plugins-channels` (milestone 2); its first commit holds this plan. After Task 2, push it and open **one draft pull request into `beta`** so CI runs (its diff includes milestone 2 until #50 is merged; that's expected). Push at the end of every stage. It stays a draft until the Stage E review passes, then `gh pr ready`. Never merge. Never push to `beta`, `canary` or `main`.
- **Before Task 1:** `git switch feat/plugins-activities`; `git status --short` shows nothing tracked as changed; `git log -1 --format=%s` is `docs: plugins milestone 3 plan`; `df -h /` shows at least 10 GB free. If any fails, stop and ask the user.
- **Scope:** `crates/opencord-common/` (the permission bit), `crates/opencord-proto/`, `proto/`, `crates/opencord-server/`, `crates/opencord-plugins/` (limits, the definitions file), `crates/opencord-core/`, `crates/opencord-mobile/` (only to keep it compiling when the core's types grow), `app/`, `examples/plugins/` (new `paint/`), `docs/`. Nothing else. The phone app (`mobile/`) is milestone 4: don't touch it.
- **Not in M3:** the phone; activities outside a voice call; a tile map block, sprite sheets or other new blocks; unread dots; message components. The phone sends no `"activities"` feature, so it hears of no activity and no session.
- **The word "activity" is already used three times in the app** (`ActivityState` for unread counts, `PresenceState.activities` for status lines, `VoiceDockActivity` for the dock's idle timer). Everything this plan adds is named `PluginActivity…` or `ActivitySession…`, never plain `Activity…`.
- **Ids in Lua are decimal strings,** both sides. **Errors raised into Lua** are `"<CODE>: <message>"` strings made through `api::function`, `api::async_function` and `raise`, never `mlua::Error::runtime` or `lua.create_function`. Grep that must come back empty in new and changed API files: `rg -n "create_function|Error::runtime" crates/opencord-server/src/plugins/api/activity.rs crates/opencord-server/src/plugins/api/ui.rs crates/opencord-core/src/views`.
- **Numbers from Lua** are checked for being finite before use.
- **Limits** (new constants in `opencord_plugins::limits`):

  | Limit | Constant | Value |
  |---|---|---|
  | Activities per plugin | `ACTIVITIES_PER_PLUGIN` | 20 |
  | Activity sessions in one call | `ACTIVITY_SESSIONS_PER_CALL` | 4 |
  | Ids in one `call` input (members, and speakers) | `CALL_INPUT_MAX_IDS` | 1,000 |
  | An activity's name / description | (the channel type's constants in `api/ui.rs`) | 60 / 200 characters |

  An opened activity counts toward milestone 2's 4 open views per session.
- **Words the user sees** are copied exactly from the task that gives them.
- **Never log** view event data, file contents or tokens. Log ids, sizes and error kinds.
- **Generated files, never edited by hand:** `app/lib/src/rust/**`, `crates/opencord-core/src/frb_generated.rs`, `.sqlx/*.json`, `*.freezed.dart`.
- **The decisions entry (D96) is Claude's,** written at the Stage E review. Don't write one.

### Additions to the view wire format

Milestone 2's plan defines the format ("The view wire format"). M3 adds exactly this:

| Where | Addition |
|---|---|
| Input `call` (renderer → core) | `{"kind":"call","members":["<user id>",…],"speaking":["<user id>",…]}`. Both lists are always present, each sorted by id and without repeats. Sent only to a view whose place is an activity session: once when the view opens, then whenever either list changes, at most once every 100 ms (the newest lists win). |
| Op `closed` | two more reasons: `activity_ended`, `left_call` |
| The place (`init`'s `ctx.place`, `app.place`) | `{ kind = "activity", session_id = "<id>", channel_id = "<voice channel id>", activity = "<activity key>" }`; a channel's stays `{ kind = "channel", channel_id }` |

## Working Rules (read before Task 1)

`GEMINI.md` applies to everything here. Read "How to write code here" before each stage. The M1 plan's rules 1–13 and the M2 plan's rules 14–20 still hold.

### Environment and commands

As in the M2 plan's "Environment" and "Commands" (every `cargo`, `flutter`, `dart`, `sqlx` and `flutter_rust_bridge_codegen` command starts with `source .superpowers/sdd/media/env.sh &&`; never `cargo clean`). Added for this plan:

| What | Command, from the repository root |
|---|---|
| Activity tests on the server | `cargo test -p opencord-server --test plugins activities` |
| The paint example's tests | `cargo test -p opencord-server --test plugins paint` and `cargo test -p opencord_core --lib views::paint` |
| Core view tests against a real server | `cargo test -p opencord_core --test views` |

### Task owners

Every task has an **Owner** and a **Score** (1–5: how much a mistake costs and how hard it is to see). **For this milestone only, the user moved the line: every task scored 3, 4 or 5 is Claude's; Gemini implements only tasks scored 1 or 2.**

- **Owner: Gemini.** You implement it.
- **Owner: Claude.** Don't implement it. Hand the task to Claude with `delegate_task`, naming the task number and this plan's path, and wait for its report. Don't edit files or run git while it works: you share the working copy. Then do the task's **Gemini, after the report** steps if it has any, run the task's **Checks** yourself, and record it in the ledger with the report's commits. A check Claude's report says it couldn't run is yours to run before the task is recorded.
- **Claude, when handed a task:** each task is written to be done from its own section, the Global Constraints and the code it names. Write the tests first, run them, and **paste the failing line in your report** before the code that makes them pass. Finish with the commit the task gives. Report the files, commits, checks with their last lines, every mutation you ran (file:line, old → new, the test that failed), every call you made that the plan doesn't give, and anything left undone or not run. If `source` or an environment prefix is refused in your session, run cargo with `--config build.incremental=false --config profile.dev.debug=0` instead. **Never run or imitate `flutter_rust_bridge_codegen`:** where a task needs it, the task says which step is Gemini's.

| Task | Part | Score | Owner |
|---|---|---|---|
| 1 | Protocol | 2 | Gemini |
| 2 | The `START_ACTIVITIES` permission | 3 | Claude |
| 3 | Registered activities | 4 | Claude |
| 4 | Activity sessions | 5 | Claude |
| 5 | Views in an activity session | 5 | Claude |
| 6 | The client VM: the activity place and the call | 4 | Claude |
| 7 | The core's types and bindings | 2 | Gemini |
| 8 | The core: starting, ending and opening by place | 4 | Claude |
| 9 | The app's model, repository, mock and providers | 3 | Claude |
| 10 | The plugin area and its view, by place | 4 | Claude |
| 11 | The Activities button and picker | 3 | Claude |
| 12 | Activity tiles on the call's stage | 4 | Claude |
| 13 | The paint example and its end-to-end tests | 3 | Claude |
| 14 | Docs | 2 | Gemini |

### Rules reviews will enforce (added to rules 1–20)

21. **A mutation line gives `file:line`, the old text → the new text, and the pasted failing output.** Without all three it isn't claimed. A test that "can't fail" is the most common finding of every review so far.
22. **A test with several people lists each person and the one road they prove,** in a comment above it: "A starts it (the starter's road). B is in the call and never opened it (hears the session, gets no view event). C can see the channel but isn't in the call (sees the tile, can't open)". No person plays two roles the test tells apart.
23. **Every argument of a call has a named source.** No placeholder ids, no `'{}'` where a real value exists, no branch keyed on a field its caller never sets.
24. **Tests feed what the real sender sends.** A renderer or VM test builds its inputs and batches in the shapes of the wire format's tables (the M2 plan's, plus "Additions" above), copied field by field; a server test drives the gateway request. A hand-made shape the sender never produces proves nothing.
25. **What a control shows is what it does,** listed per screen in its task: each control, and the request or state it changes.
26. **A stage's end is a ledger entry** holding the last lines of `.github/check.sh` verbatim, and `gh pr checks` pasted **after no check is pending**, with every failure read (`gh run view <id> --log-failed`) and fixed.
27. **The pull request's description is written from `git diff --stat` and grep,** never from memory: every route, request, function and file it names is found by `rg` first.

### Progress ledger

Keep `.superpowers/sdd/plugins-m3/progress.md` (git-ignored; never `git add -f`), one entry per task in the M1 plan's format, its first line `# SDD ledger — plan: docs/superpowers/plans/2026-11-12-plugins-m3-activities.md`.

### When a step keeps failing

`GEMINI.md` → "When you're stuck: ask Claude", with question files in `.superpowers/sdd/plugins-m3/questions/`.

### Stages and their reviews

| Stage | Tasks | At its end |
|---|---|---|
| A — Protocol and permission | 1–2 | Full checks; push; open the draft pull request; read its checks |
| B — Server | 3–5 | Full checks; push |
| C — Core | 6–8 | Full checks; push; **Claude reviews stages B and C** |
| D — Desktop | 9–12 | Full checks; push |
| E — The example and docs | 13–14 | Full checks; push; **Claude reviews stages D and E**; then stop and report to the user |

A review is a bridge task, "Review stages <X> and <Y>, round <r> of docs/superpowers/plans/2026-11-12-plugins-m3-activities.md". Its report ends with `Round <r>: PASS` or `FAIL`. On FAIL, hand each finding in a task Claude owned back to Claude as a new task ("Fix findings <n…> of the stage <X> review, round <r>"), fix the findings in your own tasks yourself, run the full checks, push, and hand over the next round. Stop and ask the user after three failed rounds, or when a finding needs the user's decision.

**The reviewer, each round:** reads the whole diff of its stages against the plan; runs the tests; runs at least one mutation per pinned Review Focus test and one per task; renders every screen Stage D added, in dark and light, at the normal and the narrowest width, and looks at each at full size; reads `~/.claude-bridge/history.jsonl` and the question files for where the work struggled.

## Review Focus

The inputs most likely to hurt real people while ordinary tests pass. Each has a pinned test in the task named.

1. **The last person leaves the call while someone still has the activity open on screen** (they leave together, the connection drops, or they're moved or disconnected by a moderator). Expected: the session ends once, its views close with "This activity ended", the tile goes for everyone who could see it, and the plugin hears one `activity_end`. Pinned in Task 4 (`the_call_emptying_ends_its_sessions`, `a_dropped_last_member_ends_the_session_after_the_grace`) and Task 5 (`ending_a_session_closes_its_views_once`).
2. **Someone opens an activity without being in its call** (they can see the channel, or left a moment ago, or send the session's id by hand). Expected: refused, and a view already open closes when its member leaves the call. Pinned in Task 5 (`only_someone_in_the_call_opens_it`, `leaving_the_call_closes_ones_view`).
3. **The plugin stops, crashes or drops the activity while sessions run.** Expected: a restart keeps sessions whose activity the new VM registers again, and the new VM hears each `activity_start` once; a stop, a turn-off, an uninstall, or a restart that no longer registers the activity ends them. Pinned in Task 4 (`sessions_survive_a_restart_and_the_new_vm_hears_them`, `stopping_ends_sessions`, `a_restart_without_the_activity_ends_its_sessions`).
4. **Two members start the same activity in the same call at the same moment,** or a fifth activity is started. Expected: one session, which both get back; the fifth is refused with `PLUGIN_LIMIT`. Pinned in Task 4 (`starting_twice_gives_the_running_session`, `a_fifth_activity_is_refused`).
5. **The viewer goes to another channel and comes back while an activity is open.** Expected: the same view, with its state, told `pause` and then `resume`; nothing reopens and no state is lost. Pinned in Task 12 (`an open activity survives leaving the voice page`).

Also pinned, outside the five: an app that doesn't list `"activities"` hears of no activity and no session (Task 3, Task 4); the migration gives existing servers `START_ACTIVITIES` and changes no other bit (Task 2).

## Claude's calls in this plan (for the user to check)

1. **`START_ACTIVITIES` is bit 33,** checked in the voice channel, so a channel overwrite can turn it off for one call.
2. **Ending someone else's activity needs `MANAGE_CHANNELS` in that voice channel** (§15.4 says "an admin"). The starter always can.
3. **Starting an activity that already runs in the call gives back the running session** instead of an error, so two members pressing at once both land on the one tile.
4. **At most 4 activities per call.**
5. **Everyone who can see the voice channel hears of its sessions** (as with screen shares), but only members in the call can open one.
6. **A plugin's restart keeps its sessions** when the new VM registers the activity again; the new VM hears `activity_start` for each. This matches what milestone 2 does for views.
7. **The call's members and speakers reach a client script from the renderer** (the `call` input), not from a second copy in the core. The phone's screens will send the same input in milestone 4.
8. **`app.call`** (`{ channel_id, members, speaking }`) is added beside the two events, so a script can read the call when it starts instead of waiting for the first change.
9. **The Activities button shows only when the server has at least one registered activity** and the member may start one here. A server without plugins gets no extra button.
10. **An opened activity stays open while its viewer is in the call,** also when they look at another channel; it's told `pause` while it isn't drawn (another page, or the small strip in focus mode) and `resume` when it is again.
11. **Ending an activity asks first** ("End … for everyone?"), since it ends it for the whole call.
12. **The paint example keeps its drawing in memory,** for the session only, at most 300 strokes of 100 points; it's a sample, and says so.

## File Map

| File | Task |
|---|---|
| `proto/opencord/v1/{plugins,requests,events,gateway}.proto`, `crates/opencord-proto/tests/plugins.rs`, `docs/protocol.md` | 1 |
| `crates/opencord-common/src/permissions.rs`, `crates/opencord-server/migrations/0017_start_activities.sql`, `crates/opencord-server/src/plugins/events.rs` (permission names), `crates/opencord-plugins/api/opencord.d.luau` | 2 |
| `crates/opencord-plugins/src/limits.rs` | 3, 4, 6 |
| `crates/opencord-server/src/plugins/activities.rs` (new), `src/plugins/api/ui.rs`, `src/plugins/runtime.rs`, `src/plugins/mod.rs`, `src/gateway/{session,identify}.rs` | 3 |
| `crates/opencord-server/src/plugins/activities.rs`, `src/handlers/activities.rs` (new), `src/handlers/mod.rs`, `src/plugins/api/activity.rs` (new), `src/plugins/view_calls.rs`, `src/voice/mod.rs` | 4 |
| `crates/opencord-server/src/plugins/views.rs`, `src/handlers/views.rs`, `src/plugins/api/ui.rs` | 5 |
| `crates/opencord-server/tests/plugins/activities.rs` (new), `tests/plugins.rs` | 3–5 |
| `crates/opencord-core/src/views/{mod,runtime,context,api_app}.rs`, `src/views/call.rs` (new), `src/views/call_tests.rs` (new) | 6 |
| `crates/opencord-core/src/api/types.rs`, `src/convert.rs`, `crates/opencord-mobile/src/{convert,events}.rs`, the generated bindings | 7 |
| `crates/opencord-core/src/views/client.rs`, `src/client.rs`, `src/api/views.rs`, `tests/views.rs` | 8 |
| `app/lib/core/model/plugins.dart`, `core/model/permissions.dart`, `core/model/snapshot.dart`, `core/repository/{repository,events}.dart`, `core/rust/{core_api,core_mapping,rust_repository}.dart`, `core/mock/*`, `core/providers/{server_state,activity_sessions}.dart`, `features/settings/server/permission_info.dart` | 9 |
| `app/lib/core/providers/plugin_views.dart`, `features/plugins/plugin_area.dart`, `features/chat/chat_area.dart` | 10 |
| `app/lib/features/voice/voice_dock.dart`, `features/plugins/activity_picker.dart` (new) | 11 |
| `app/lib/features/voice/{voice_layout,voice_view,voice_focus}.dart`, `features/plugins/activity_tile.dart` (new), `core/providers/activity_sessions.dart` | 12 |
| `examples/plugins/paint/`, `crates/opencord-server/tests/plugins/paint.rs`, `crates/opencord-core/src/views/tests/paint.rs` | 13 |
| `docs/plugins.md`, `docs/protocol.md`, `.superpowers/sdd/plugins-m3/pr_body.md` | 14 |

---

## Stage A — Protocol and permission

### Task 1: Protocol

**Owner:** Gemini. **Score:** 2 (exact blocks to copy).

**Files:** `proto/opencord/v1/{plugins,requests,events,gateway}.proto`, `crates/opencord-proto/tests/plugins.rs`, `docs/protocol.md`; plus the smallest arm in each exhaustive `match` that stops compiling.

- [ ] **Step 1: Write the failing test** `activities_round_trip` in `crates/opencord-proto/tests/plugins.rs`: build one of each new message below with every field set to a non-default value, encode, decode, compare. Comment: "fails if a field number or name below changes".
- [ ] **Step 2: Run it:** `cargo test -p opencord-proto --test plugins` fails to compile (`ActivitySession` not found). Paste the line.
- [ ] **Step 3: Add to `plugins.proto`,** after `PluginChannelTypesUpdate`:

```proto
// An activity a running plugin registered (§15.1).
message PluginActivity {
  string plugin_id = 1;
  // [a-z0-9_], 1 to 40.
  string activity_key = 2;
  string name = 3;
  string description = 4;
  // A path under the plugin's assets/, or empty.
  string icon = 5;
  string plugin_name = 6;
}

// One activity running in one call (§15.2).
message ActivitySession {
  int64 id = 1;
  // The voice channel.
  int64 channel_id = 2;
  string plugin_id = 3;
  string activity_key = 4;
  // As registered when it started.
  string name = 5;
  string icon = 6;
  string plugin_name = 7;
  int64 starter_id = 8;
  int64 started_at_ms = 9;
  // Who has it open, by user id, ascending.
  repeated int64 viewer_ids = 10;
}

// -> activity_session. The caller is in this voice channel and holds
// START_ACTIVITIES there; the session lists "activities". An activity
// already running in the call answers with its session. At most 4 in a call.
message StartActivity {
  int64 channel_id = 1;
  string plugin_id = 2;
  string activity_key = 3;
}

// -> ack. The starter, or MANAGE_CHANNELS in the session's channel.
message EndActivity {
  int64 session_id = 1;
}

enum ActivityEndReason {
  ACTIVITY_END_REASON_UNSPECIFIED = 0;
  // A member ended it.
  ACTIVITY_END_REASON_ENDED = 1;
  // The plugin ended it.
  ACTIVITY_END_REASON_PLUGIN = 2;
  ACTIVITY_END_REASON_CALL_EMPTY = 3;
  ACTIVITY_END_REASON_PLUGIN_STOPPED = 4;
  // The plugin runs without the activity.
  ACTIVITY_END_REASON_UNAVAILABLE = 5;
}

// To sessions that list "activities": the whole list.
message PluginActivitiesUpdate {
  repeated PluginActivity activities = 1;
}

// To sessions that list "activities" and can see the session's channel.
message ActivitySessionCreate {
  ActivitySession session = 1;
}
message ActivitySessionUpdate {
  ActivitySession session = 1;
}
message ActivitySessionDelete {
  int64 session_id = 1;
  int64 channel_id = 2;
  ActivityEndReason reason = 3;
}
```

  In the same file: `ViewPlace`'s oneof gains `int64 activity_session_id = 2;` and its comment becomes "Where a view is open: a plugin channel, or an activity session in a call."; `ViewCloseReason` gains `VIEW_CLOSE_REASON_ACTIVITY_ENDED = 5;` and `VIEW_CLOSE_REASON_LEFT_CALL = 6;`; `OpenView`'s comment gains "For an activity session: the caller is in its call, and the session lists \"activities\"."
- [ ] **Step 4: Number them.** `requests.proto`: `StartActivity start_activity = 228;`, `EndActivity end_activity = 229;` in the request oneof; `ActivitySession activity_session = 51;` in the response oneof. `events.proto`: `PluginActivitiesUpdate plugin_activities_update = 140;`, `ActivitySessionCreate activity_session_create = 141;`, `ActivitySessionUpdate activity_session_update = 142;`, `ActivitySessionDelete activity_session_delete = 143;`. `gateway.proto`: `Ready` gains `repeated PluginActivity plugin_activities = 26;` and `repeated ActivitySession activity_sessions = 27;`; the comment on `Identify.features` lists `"activities"` too. Before saving, check with `rg -n "= (228|229|51|140|141|142|143|26|27);" proto/opencord/v1/` that none of these numbers is taken in its message; if one is, stop and ask.
- [ ] **Step 5: Make the workspace compile with the smallest arm,** as milestone 2's protocol task did, and list each arm in the ledger as a deviation: `crates/opencord-server/src/handlers/mod.rs` answers `ApiError::invalid_argument("not implemented")` for the two requests (Task 4 replaces it); `handlers/views.rs` answers the same for `ViewPlace`'s new kind (Task 5); `crates/opencord-core/src/convert.rs` returns `None` for the four events (Task 7); the server's `plugins/events.rs` maps the four events to `None` for good (plugins never observe them), with `events_tests.rs`'s count raised by 4. `Ready` literals get empty lists.
- [ ] **Step 6: `docs/protocol.md`:** add the requests, the response, the four events, the two `Ready` fields, the feature name and the new close reasons, each copied from the proto. Grep every name you wrote.
- [ ] **Step 7: Checks:** `cargo test -p opencord-proto --test plugins`; `cargo check --workspace --all-targets`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo fmt --all -- --check`.
- [ ] **Step 8: Commit** `feat(proto): activities and activity sessions`.

### Task 2: The `START_ACTIVITIES` permission

**Owner:** Claude. **Score:** 3 (a migration over every server's roles; a bit shared by three languages). Read §8.5.

**Files:** `crates/opencord-common/src/permissions.rs`, `crates/opencord-server/migrations/0017_start_activities.sql`, `crates/opencord-server/src/plugins/events.rs`, `crates/opencord-plugins/api/opencord.d.luau`, `docs/protocol.md`.

**Interfaces — produces:**
- `Permissions::START_ACTIVITIES = 1 << 33`, documented `/// Start an activity in a call one is in (design 2026-11-07 §8.5).`, and part of `Permissions::DEFAULT_EVERYONE`.
- Migration `0017_start_activities.sql`: `UPDATE roles SET permissions = permissions | 8589934592` with the same `WHERE` as `0002_voice.sql` (the role named by `server_meta`'s `everyone_role_id`). Nothing else.
- The name `"start_activities"` in `plugins/events.rs`'s permission names table and in the definitions file's permission name union; the bit's row in `docs/protocol.md`'s permissions table.

- [ ] **Step 1: Write failing tests:**
  - in `permissions.rs`'s tests: `start_activities_is_bit_33_and_on_for_everyone` (`bits() == 1 << 33`; `DEFAULT_EVERYONE` contains it). Fails if the bit moves or leaves the default.
  - in the server's migration tests (beside milestone 2's test of the channels table rebuild, `the_rebuild_keeps_channels_overwrites_and_messages`; find it with `rg -n the_rebuild_keeps crates/opencord-server`): `the_upgrade_gives_everyone_start_activities_and_nothing_else`: a database migrated to 0016 holding @everyone with `VIEW_CHANNEL | CONNECT` and a second role with `SPEAK`; after 0017, @everyone has exactly those plus bit 33 and the second role is unchanged. Fails if the `WHERE` is dropped (the second role gains the bit) or the constant is another bit.
  - in `tests/plugins/guild.rs` or wherever `permissions_to_names` is tested: the name `start_activities` comes back for the bit.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.** After editing a migration, touch the test file (sqlx's `migrate!` has no build script). If a `.sqlx` file changes, say so in the report: Gemini regenerates it.
- [ ] **Step 4: Checks:** `cargo test -p opencord-common`; the migration test by name; `cargo test -p opencord-server --test plugins definitions`; clippy on the three crates; `cargo fmt --all -- --check`.
- [ ] **Step 5: Commit** `feat(server): the START_ACTIVITIES permission`.

**At the end of stage A:** full checks, push, open the draft pull request, read its checks (rule 26).

---

## Stage B — Server

All three tasks add to `crates/opencord-server/src/plugins/activities.rs` and to `tests/plugins/activities.rs`. Each test drives the gateway request or the plugin's Lua, never a registry method (rule 15). Every state change here is made **under the server's write lock and announced before it's released**, as `channel_types.rs` does, since `Ready` is built under it too.

### Task 3: Registered activities

**Owner:** Claude. **Score:** 4 (start, restart and stop each change the list at a different moment). Read §15.1 and `plugins/channel_types.rs`, which this mirrors.

**Interfaces — produces:**
- `opencord_plugins::limits::ACTIVITIES_PER_PLUGIN: usize = 20`.
- `gateway::session::ACTIVITIES: &str = "activities"`.
- `plugins::activities::Activities` on `state.plugins.activities`, holding each plugin's registered activities with the same three moments as `ChannelTypes`: what a starting VM registers is held aside and replaces the plugin's list when its start finishes; a running plugin registers straight into its list; the entry leaves when the plugin stops, is turned off, fails to start or is uninstalled. Nothing is kept across server restarts. Public for later tasks: `list() -> Vec<proto::PluginActivity>` (by plugin id, then key), `get(plugin_id, key) -> Option<Registered>` (the live entry: `name`, `description`, `icon`, `plugin_name`), and the hooks `begin`, `started`, `stopped` called beside the channel types' at **every** site that calls those (find them: `rg -n "channel_types::(started|stopped)|channel_types\.begin" crates/opencord-server/src`).
- Lua: `ui.register_activity(key, spec)`. Needs the `register_activities` permission (`PERMISSION_DENIED` through `env.require`). `key` follows `valid_type_key`; `spec` is `{ name, description?, icon? }` with the channel type's rules (reuse `channel_type`'s checks through one shared helper, not a copy). Registering a key again replaces it. The 21st raises `PLUGIN_LIMIT: at most 20 activities`. Logs the action line `registered activity <key>`.
- `Ready.plugin_activities` and the event `PluginActivitiesUpdate` (the whole list, when it changed), both only for sessions with the `ACTIVITIES` feature.
- The definitions file gains `ui.register_activity` and its spec type, copied from the function that reads it.

Task 4 adds what happens to sessions when the list changes. This task adds nothing for sessions: no stub and no dead function.

- [ ] **Step 1: Write failing tests** in `tests/plugins/activities.rs` (new; add `mod activities;` to `crates/opencord-server/tests/plugins.rs`):
  - `a_registered_activity_reaches_apps_that_ask`: one session with `["plugin_users","plugin_channels","activities"]`, one without `"activities"`. The plugin registers `paint`; the first hears `PluginActivitiesUpdate` with it and finds it in a fresh `Ready`; the second hears nothing and its `Ready` has none. Fails if the feature check is dropped.
  - `registering_needs_the_permission`: without `register_activities`, the plugin's log shows `PERMISSION_DENIED`.
  - `a_bad_activity_is_refused` (a key with a capital, an empty name, an icon outside `assets/`, an icon not in the package): each `INVALID_ARGUMENT`.
  - `the_21st_activity_is_too_many_but_a_replacement_is_not`.
  - `a_restart_that_registers_the_same_activities_changes_nothing` (no `PluginActivitiesUpdate` during the restart) and `stopping_takes_its_activities_away` (the update comes, without them).
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Checks:** `cargo test -p opencord-server --test plugins activities`; `--test plugins definitions`; `--test plugins channels` (the shared helper); clippy; fmt.
- [ ] **Step 5: Commit** `feat(server): plugins register activities`.

### Task 4: Activity sessions

**Owner:** Claude. **Score:** 5 (sessions end from five directions, some of them while the plugin or the call is going away). Read §15.2, §15.4, §9.6, and `voice/mod.rs` (`announce`, `reconcile`, `expire_detached`), `voice/streams.rs` (how a screen share ends when its owner leaves), `plugins/view_calls.rs` (how a VM hears each view once).

**Interfaces — consumes:** Task 3's `Activities::get` and hooks; Task 1's messages.

**Interfaces — produces:**
- `limits::ACTIVITY_SESSIONS_PER_CALL: usize = 4`.
- In `plugins/activities.rs`: the sessions, by id, in memory. `Session { id, channel_id, plugin_id, activity_key, name, icon, plugin_name, starter_id, started_at_ms }`; `to_proto(&self, viewer_ids)`; and for Task 5: `session(id) -> Option<Session>`, `sessions_in(channel_id)`.
- **`StartActivity`** (`handlers/activities.rs`, through `handlers::handle`, so the request rate limit applies). In this order, under the write lock: the session lists `ACTIVITIES` (else `NOT_FOUND` "channel", as an app that never heard of activities); the caller's voice state is in `channel_id` (else `INVALID_ARGUMENT` "join the call to start an activity"); the caller holds `START_ACTIVITIES` in that channel (`require_channel`, so overwrites count); the plugin runs, has a client entry, and `Activities::get` finds the key (else `PLUGIN_UNAVAILABLE` "this activity is unavailable"); a session of the same plugin and key in this call is **returned as it is**; the call already has 4 → `PLUGIN_LIMIT` "this call already has 4 activities". Then the session is added, announced and returned.
- **`EndActivity`:** the session exists and the caller can see its channel (else `NOT_FOUND` "activity session"); the caller is its starter or holds `MANAGE_CHANNELS` in its channel (else `FORBIDDEN`). Ends it with reason `ENDED`.
- **A session ends** — one function, `end(state, session_id, reason)`, used by every road, which removes it, announces `ActivitySessionDelete`, and tells the plugin; ending a session that's already gone does nothing:
  - `EndActivity` → `ENDED`;
  - `activity.finish(session)` from Lua → `PLUGIN`;
  - its call empties → `CALL_EMPTY`: in `voice::announce`, for each channel in `channels` that now has no voice state. This one hook covers leaving, being moved or disconnected, losing access (`reconcile`), a deleted channel, and a dropped connection after its grace (`expire_detached`);
  - its plugin stops, is turned off, fails to start or is uninstalled → `PLUGIN_STOPPED` (Task 3's `stopped` hook);
  - its plugin runs without the activity: a restart's start finished without registering it → `UNAVAILABLE` (Task 3's `started` hook).
  - Nothing else ends one: not the starter leaving while others stay, not the starter losing the permission, not a plugin restart that registers the activity again.
- **Who hears of sessions:** sessions that list `ACTIVITIES` and whose user can see the session's channel: `Ready.activity_sessions`, `ActivitySessionCreate`, `ActivitySessionDelete`. A member who gains or loses sight of the channel gets a create or a delete with it, where `visibility.rs` sends the channel itself. `viewer_ids` is empty until Task 5.
- **What the plugin hears,** through `opencord.on`:
  - `activity_start`, with `{ session, starter, call }`: `session = { id, activity, channel_id, starter_id, started_at_ms }`, `starter` a user table (`events::user_table`), `call = { channel_id, member_ids }` (the call's members now, ascending).
  - `activity_end`, with `{ session, reason }`, `reason` one of `"ended"`, `"plugin"`, `"call_empty"`, `"plugin_stopping"`, `"unavailable"`.
  - A VM hears each session's start once, and its end only if it heard the start: a VM that starts while sessions run hears `activity_start` for each when its start finishes (the pattern of `views_heard` in `view_calls.rs`; its set leaves entries on the end, and with the VM). As the plugin stops, its sessions' `activity_end` handlers run within `stop`'s grace, after its views' `view_close`.
- **Lua:** a new global `activity` (`plugins/api/activity.rs`, installed from `api/mod.rs`): `activity.finish(session)` (not `activity.end`: `end` is a Luau keyword; the user's call at the stage B and C review) takes a session table or its id string, needs `register_activities`, ends only the plugin's own session, and returns `true` if it was running, `false` if not. Logs the action line `ended activity session <id>`. The global is read-only like `opencord`.
- The definitions file gains `activity.finish`, the session type and both events' argument types, each field copied from the function that builds it.

- [ ] **Step 1: Write failing tests** in `tests/plugins/activities.rs`. Each lists its people (rule 22).
  - `starting_needs_the_call_and_the_permission`: A in the call with the permission starts it; B in the call with an overwrite denying `START_ACTIVITIES` in this channel is refused `FORBIDDEN`; C not in the call is refused `INVALID_ARGUMENT`; D whose app doesn't list `"activities"` is refused `NOT_FOUND`.
  - `a_started_activity_reaches_who_can_see_the_channel`: A starts; B (in the call) and C (sees the channel, not in the call) each hear `ActivitySessionCreate`; D (can't see the channel) and E (no `"activities"`) hear nothing; the plugin's log shows its `activity_start` handler ran with the starter's id and both call members.
  - `starting_twice_gives_the_running_session` (two starts, sent without waiting for the first answer: the same id twice, one `ActivitySessionCreate`, one `activity_start`).
  - `a_fifth_activity_is_refused` (`PLUGIN_LIMIT`; a fifth in **another** call is fine).
  - `an_unknown_or_stopped_activity_is_unavailable`.
  - `only_the_starter_or_a_channel_manager_ends_it`: B (in the call, neither) is refused `FORBIDDEN`; the starter ends one; a member with `MANAGE_CHANNELS` ends another; each end is one `ActivitySessionDelete` with `ENDED` and one `activity_end` with `"ended"`.
  - `the_plugin_ends_its_own_session` (`activity.finish` → `PLUGIN`, returns true, then false) and `a_plugin_cant_end_anothers_session` (a second plugin's `activity.finish` with that id returns false, and the session still runs).
  - `the_call_emptying_ends_its_sessions`: two members, one leaves (the session stays), the other leaves (`CALL_EMPTY`, once).
  - `a_dropped_last_member_ends_the_session_after_the_grace`: the only member's connection drops; before the voice grace the session is still there, after it it's gone. Use the test server's clock or its expiry hook as the voice tests do; no `sleep` longer than a second.
  - `deleting_the_channel_ends_its_sessions`.
  - `stopping_ends_sessions`: with a session running, the plugin is stopped; apps hear `PLUGIN_STOPPED`; the plugin's log shows its `activity_end` ran with `"plugin_stopping"`.
  - `sessions_survive_a_restart_and_the_new_vm_hears_them`: restart the plugin; no `ActivitySessionDelete` is sent; the log shows `activity_start` a second time, once.
  - `a_restart_without_the_activity_ends_its_sessions`: a dev push or an update whose script no longer registers `paint` → `UNAVAILABLE`.
  - `sessions_are_in_ready_only_for_apps_that_ask`.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.** Never hold the voice states' guard while announcing (see `Audience::Voice`'s note in `state.rs`); read who's in a call, drop the guard, then act.
- [ ] **Step 4: Mutations to run and report (rule 21):** drop the `ACTIVITIES` feature check in the announce; make `end` skip `activity_end` when the reason is `CALL_EMPTY`; let a restart end every session; return a new session instead of the running one; remove the starter check. Each must fail the test named above and no other.
- [ ] **Step 5: Checks:** `cargo test -p opencord-server --test plugins activities`; `--test plugins views`; `--test plugins definitions`; `cargo test -p opencord-server --test streams` and the voice tests (`rg -l "voice" crates/opencord-server/tests/*.rs`); clippy; fmt.
- [ ] **Step 6: Commit** `feat(server): activity sessions in calls`.

### Task 5: Views in an activity session

**Owner:** Claude. **Score:** 5 (who may open a view, and that none outlives its right to be open). Read §15.3, `plugins/views.rs` and `handlers/views.rs` whole.

**Interfaces — consumes:** Task 4's `session`, `sessions_in`, `end`.

**Interfaces — produces:**
- `ViewInfo` gains `activity: Option<i64>` (the session's id); for such a view `channel_id` is the session's voice channel. Every place that builds a `ViewInfo` sets it.
- **`OpenView` with `activity_session_id`,** under the write lock, in this order: the session lists `ACTIVITIES` (else `NOT_FOUND` "activity session"); the activity session exists (same error); the caller's voice state is in its channel (else `FORBIDDEN` "join the call to open this activity"); the plugin runs with a client entry (else `PLUGIN_UNAVAILABLE`); the 4-views-per-session limit. `ViewOpened` is built as for a channel. A channel place keeps every check it has; a place with neither kind stays `INVALID_ARGUMENT`.
- **Such a view closes** (besides everything that already closes a view: `CloseView`, its gateway session ending, the plugin stopping, lost sight of the channel):
  - when its session ends → `ViewClosed` with `ACTIVITY_ENDED`, and the plugin's `view_close` reason `"activity_ended"`. `end` closes the views **before** it delivers `activity_end`, so a plugin hears every `view_close` of a session before its `activity_end`;
  - when its member's voice state leaves the session's channel → `LEFT_CALL`, plugin reason `"left_call"` (the hook in `voice::announce` that Task 4 added, for each channel left).
- **`viewer_ids`:** the users with at least one open view of the session, ascending. Whenever that set changes, `ActivitySessionUpdate` goes to the audience of Task 4's create. A session's end sends no update first, only the delete.
- **What plugins see:** a view's `place` is `{ kind = "activity", session_id, channel_id, activity }` for these and gains `kind = "channel"` for channel views (an added field; nothing else of it changes). `ui.views(place?)` and `ui.send_place(place, name, data)` take `{ session_id = "…" }` (that session's views) or `{ channel_id = "…" }` (that plugin channel's views, as now; it never matches an activity's view). A table with neither raises `INVALID_ARGUMENT: place must be a table such as { channel_id = "…" } or { session_id = "…" }`.
- The definitions file's `View`, `Place` and `view_close` reason types, copied from `ViewInfo::to_json` and `Closed::plugin_reason`.

- [ ] **Step 1: Write failing tests** in `tests/plugins/activities.rs`, each listing its people:
  - `only_someone_in_the_call_opens_it`: A (in the call) opens it; B (sees the channel, not in the call) is refused `FORBIDDEN`; C (in the call, app without `"activities"`) is refused `NOT_FOUND`; a made-up session id is `NOT_FOUND`.
  - `opening_tells_the_plugin_where`: the plugin's `view_open` logs `view.place.kind`, `session_id`, `channel_id`, `activity`; all four are right.
  - `viewers_are_counted_by_person`: A opens it from two gateway sessions (one `ActivitySessionUpdate`, `viewer_ids == [A]`), B opens it (`[A, B]`), A closes one view (no update), A closes the other (`[B]`).
  - `leaving_the_call_closes_ones_view`: A and B both have it open; A leaves the call; A's app hears `ViewClosed` `LEFT_CALL`, the plugin `view_close` with `"left_call"`; B's view still gets the plugin's events.
  - `ending_a_session_closes_its_views_once`: A and B have it open; the starter ends it; each hears one `ViewClosed` `ACTIVITY_ENDED`; the plugin's log shows two `view_close` lines, then one `activity_end`, in that order.
  - `the_call_emptying_closes_the_last_view`: the last member leaves with the view open: the plugin hears that view close once (not once for leaving and again for the end) and then `activity_end`.
  - `events_reach_a_sessions_views_only`: two sessions of the same plugin in two calls; `ui.send_place({ session_id })` reaches only the first's views, and `ui.send_place({ channel_id = <that voice channel> })` reaches none; `ui.views({ session_id })` lists only the first's.
  - `an_activity_view_counts_toward_the_four`.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Mutations to run and report:** remove the in-the-call check from `OpenView`; skip the close on leaving; deliver `activity_end` before the views close; count `viewer_ids` by view instead of by person; let `{ channel_id }` match activity views.
- [ ] **Step 5: Checks:** `cargo test -p opencord-server --test plugins` (all of it: `views`, `uploads`, `tickets` and `dev` build `ViewInfo`s or places); clippy; fmt.
- [ ] **Step 6: Commit** `feat(server): views in activity sessions`.

**At the end of stage B:** full checks, push (rule 26).

---

## Stage C — Core

### Task 6: The client VM: the activity place and the call

**Owner:** Claude. **Score:** 4 (a new input into the per-view thread; scripts read what it sets). Read §11.4, §11.5, and `views/{mod,runtime,context,api_app}.rs`.

Nothing here touches `crates/opencord-core/src/api/`, so no bindings change.

**Interfaces — produces:**
- `views::Place` gains `Activity { session_id: String, channel_id: String, activity: String }` (serde tag `"activity"`), which `context::place_table` and `init`'s `ctx.place` show as in "Additions to the view wire format".
- `limits::CALL_INPUT_MAX_IDS: usize = 1_000`.
- The input `call` (`views/call.rs`): `{"kind":"call","members":[…],"speaking":[…]}`. An id that isn't a string of digits, or a list longer than the limit, makes the whole input ignored with one `tracing::warn!` naming the view and the count, never the ids. A view whose place is a channel ignores the input.
- `app.call`: `nil` for a channel's view. For an activity's: a read-only table `{ channel_id, members, speaking }`, the two lists read-only arrays of id strings, both empty until the first `call` input, replaced by each input.
- `app.listen` takes two more names: `call_members_change`, called with `{ members = {…}, joined = {…}, left = {…} }`, and `speaking_change`, called with `{ speaking = {…} }`. Each runs only when its list differs from the one before, after `app.call` holds the new lists; an input that changes both runs `call_members_change` first. Listening to either from a channel's view is allowed and never fires.
- The definitions file gains `app.call`, the two listener names with their argument types, and the activity place; `views/definitions_tests.rs` covers them.

- [ ] **Step 1: Write failing tests** in `views/call_tests.rs` (wired like the other `*_tests.rs`), with inputs in the exact wire shape (rule 24):
  - `an_activity_view_reads_its_place` (`ctx.place` and `app.place` both hold the four fields).
  - `the_first_call_input_fills_app_call_and_tells_listeners` (members `["1","2"]`, speaking `[]`: `call_members_change` gets `joined = {"1","2"}`, `left = {}`; `speaking_change` doesn't run).
  - `only_what_changed_is_told` (the same members with speaking `["2"]`: only `speaking_change`; the same input again: neither).
  - `someone_leaving_is_in_left` (`["1","2"]` → `["2"]`).
  - `a_bad_call_input_changes_nothing` (an id `"x1"`; a number instead of a string; 1,001 ids): `app.call` keeps what it had and no listener runs.
  - `a_channel_view_has_no_call` (`app.call == nil`; a `call` input does nothing).
  - `app_call_cant_be_changed` (assigning to `app.call.members[1]` and to `app.call.speaking` raises).
  - `a_listener_that_sets_state_redraws_once` (one batch for an input that runs both listeners).
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Mutations to run and report:** tell `speaking_change` on every input; fill `app.call` after the listeners run; accept the input for a channel's view.
- [ ] **Step 5: Checks:** `cargo test -p opencord_core --lib views`; `cargo clippy -p opencord_core --all-targets -- -D warnings`; fmt.
- [ ] **Step 6: Commit** `feat(core): the call in a view's client script`.

### Task 7: The core's types and bindings

**Owner:** Gemini. **Score:** 2 (types to copy, a mapping, and the generator). You run the generator; Claude can't.

**Files:** `crates/opencord-core/src/api/types.rs`, `src/convert.rs`, `crates/opencord-mobile/src/{convert,events}.rs`, the generated bindings, and the Dart lines that stop compiling.

- [ ] **Step 1: Write the failing test** in `convert.rs`'s tests, `activities_and_sessions_convert`: a `Ready` with one `PluginActivity` and one `ActivitySession` gives a snapshot holding both, every field equal; each of the four proto events gives its payload below; `ActivityEndReason` maps name for name, and an unknown number maps to `Ended`. Fails if a field is dropped or two are swapped: set every field to a different value.
- [ ] **Step 2: Run it** (`cargo test -p opencord_core --lib convert`); paste the compile failure.
- [ ] **Step 3: Add to `api/types.rs`,** exactly:

```rust
/// An activity a running plugin registered (design 2026-11-07 §15.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginActivity {
    pub plugin_id: String,
    pub activity_key: String,
    pub name: String,
    pub description: String,
    /// A path under the plugin's `assets/`, or empty.
    pub icon: String,
    pub plugin_name: String,
}

/// One activity running in one call (§15.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivitySession {
    pub id: i64,
    /// The voice channel.
    pub channel_id: i64,
    pub plugin_id: String,
    pub activity_key: String,
    pub name: String,
    pub icon: String,
    pub plugin_name: String,
    pub starter_id: i64,
    pub started_at_ms: i64,
    /// Who has it open, ascending.
    pub viewer_ids: Vec<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityEndReason {
    Ended,
    Plugin,
    CallEmpty,
    PluginStopped,
    Unavailable,
}
```

  `ReadySnapshot` gains `pub plugin_activities: Vec<PluginActivity>` and `pub activity_sessions: Vec<ActivitySession>`. `CoreEventPayload` gains `PluginActivitiesUpdate(Vec<PluginActivity>)`, `ActivitySessionCreate(ActivitySession)`, `ActivitySessionUpdate(ActivitySession)` and `ActivitySessionDelete { session_id: i64, channel_id: i64, reason: ActivityEndReason }`.
- [ ] **Step 4: Map them in `convert.rs`** (`ready()`, the event conversion), replacing Task 1's `None` arm. A create or update without its `session` is `None`.
- [ ] **Step 5: Keep `opencord-mobile` compiling:** its event conversion returns `None` for the four payloads, and its snapshot ignores the two lists. The phone gets nothing new.
- [ ] **Step 6: Regenerate:** `cd app && flutter_rust_bridge_codegen generate`. Then make `flutter analyze` clean with the smallest change: in `rust_repository.dart`'s event switch the four new payloads map to `null`, each with the comment `// Task 9 maps this.`; test fixtures that build a `ReadySnapshot` get the two empty lists. List these in the ledger as deviations Task 9 removes.
- [ ] **Step 7: Checks:** `cargo test -p opencord_core --lib convert`; `cargo test -p opencord-mobile`; `cargo clippy -p opencord_core -p opencord-mobile --all-targets -- -D warnings`; `cargo fmt --all -- --check`; `cd app && flutter analyze && flutter test`.
- [ ] **Step 8: Commit** `feat(core): activities and sessions in the app's types`.

### Task 8: The core: starting, ending and opening by place

**Owner:** Claude. **Score:** 4 (a view must reopen in the right place on a new connection, and close for the new reasons). Read `views/client.rs` whole and `crates/opencord-core/tests/views.rs`.

**Interfaces — consumes:** Task 6's `Place::Activity`; Task 7's `ActivitySession`.

**Interfaces — produces:**
- In `views/client.rs`: a view remembers its place as `enum At { Channel(i64), Activity { session_id: i64, channel_id: i64, activity_key: String } }` in place of `Open.channel_id`; `open` takes it; `open_on_server` and `reopen` send the matching `ViewPlace`; the VM's `Place` is built from it. `CloseReason` (in `opencord_plugins::view`) gains `ActivityEnded` and `LeftCall`, and `on_closed` maps the two new wire reasons to them. A view that can't be opened again on a new session because its session is gone (`NOT_FOUND`) ends with the `closed` op `activity_ended`, not an error.
- In `client.rs`: `Client::start_activity(key, channel_id, plugin_id, activity_key) -> Result<ActivitySession, CoreError>`, `Client::end_activity(key, session_id) -> Result<(), CoreError>`, `Client::view_open_activity(key, session_id, channel_id, activity_key, context_json) -> Result<String, CoreError>`. `PluginActivitiesUpdate` forgets the server's remembered assets, as the channel types' update does.
- In `api/views.rs`, the same three as bridge functions: `start_activity`, `end_activity`, `view_open_activity`, with the argument names above (`server_key` for `key`).
- The desktop's features: nothing here; Task 9 adds `"activities"` in Dart.

**Gemini, after the report:** run `cd app && flutter_rust_bridge_codegen generate`, then `flutter analyze` (nothing should need changing), and commit the generated files as `chore(app): regenerate bindings for activities`.

- [ ] **Step 1: Write failing tests** in `tests/views.rs`, against the real server, with a test plugin that registers `paint` and whose client script draws `app.place.kind` and the number of events it got. The app under test asks for `["plugin_users","plugin_channels","activities"]`.
  - `starting_and_opening_an_activity_draws_it`: join the voice channel, `start_activity`, `view_open_activity`; the first batch shows `activity`; the app hears `ActivitySessionCreate`, then `ActivitySessionUpdate` with its own id in `viewer_ids`.
  - `a_press_in_an_activity_reaches_its_plugin_and_back` (the client sends, the server answers with `ui.send_place({ session_id })`, the view redraws).
  - `ending_the_activity_closes_the_view_with_its_reason` (the last batch is `closed` with `activity_ended`).
  - `leaving_the_call_closes_the_view` (`left_call`).
  - `a_new_session_reopens_the_activity_keeping_state`: drop the connection so a new session starts; the view keeps its count, and the plugin hears `view_open` again with the same `session_id`. Join voice again first if the new session needs it; say in the report what the server required.
  - `an_activity_gone_while_away_closes_the_view`: while disconnected, the session ends; on the new session the view's last batch is `closed` `activity_ended`.
  - `opening_without_the_call_is_refused` (`CoreError::Server` with the server's code).
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Mutations to run and report:** make `reopen` send the channel place for an activity's view; map `ACTIVITY_ENDED` to `PluginStopped`.
- [ ] **Step 5: Checks:** `cargo test -p opencord_core --lib views`; `cargo test -p opencord_core --test views --test client`; `cargo test -p opencord-plugins`; clippy on the core and the plugins crate; fmt. `flutter analyze` is Gemini's, after the generator.
- [ ] **Step 6: Commit** `feat(core): activities over the network`.

**At the end of stage C:** full checks, push, then hand over the review of stages B and C.

---

## Stage D — Desktop

For every task here: use the app's own widgets (`showPopover`, `OcMenu`, `OcButton`, `OcIcons`, `OcText`, `context.oc`, the dialog the app already uses to confirm deleting a channel), never Material's `showDialog`, `PopupMenuButton`, `InkWell` or `Icons.*`. Every new screen is rendered in dark and light, at the normal and the narrowest width, and looked at in full size (rule 20); say in the report which you rendered, or that Flutter couldn't run in your session, in which case Gemini renders them and looks. A widget test names the road its event takes (rule 16).

### Task 9: The app's model, repository, mock and providers

**Owner:** Claude. **Score:** 3 (state that every screen after it reads; a load must merge with events that arrived meanwhile). Read `core/providers/stream_state.dart` (sessions follow its shape), `core/providers/server_state.dart` (`pluginChannelTypes`) and `core/rust/rust_repository.dart`'s event switch.

**Interfaces — produces:**
- `core/model/plugins.dart`: `PluginActivity` (`pluginId`, `activityKey`, `name`, `description`, `icon`, `pluginName`), `ActivitySession` (`id`, `channelId`, `pluginId`, `activityKey`, `name`, `icon`, `pluginName`, `starterId`, `startedAt`, `viewerIds`), `enum ActivityEndReason { ended, plugin, callEmpty, pluginStopped, unavailable }`, all immutable with `==`.
- `Permissions.startActivities = Permissions(1 << 33)`, and a row in `permission_info.dart`'s voice group, after screen share: label `Start activities`, description `Start a plugin's activity in a voice call they're in`.
- `ReadySnapshot` and `ServerData` gain `pluginActivities`; events `PluginActivitiesUpdated(serverKey, activities)`, `ActivitySessionStarted(serverKey, session)`, `ActivitySessionChanged(serverKey, session)`, `ActivitySessionEnded(serverKey, sessionId, channelId, reason)`; the snapshot gains `activitySessions` (by id).
- `core/providers/activity_sessions.dart`: `activitySessionsProvider` (family by server key → `Map<int, ActivitySession>`), fed by `Ready`, the three session events and `ChannelDeleted`, wired where `streamsProvider` is fed.
- `OpencordRepository`: `Future<ActivitySession> startActivity(String serverKey, int channelId, String pluginId, String activityKey)`, `Future<void> endActivity(String serverKey, int sessionId)`, `Future<String> openActivityView(String serverKey, ActivitySession session, String contextJson)` (its `channelId` and `activityKey` come from the session given, rule 23). `RepoCapabilities` gains `activities` (true in both repositories).
- `RustRepository`: the three calls; the four events mapped (removing Task 7's `null`s and comments); `desktopFeatures` becomes `['plugin_users', 'plugin_channels', 'activities']`.
- The mock: one registered activity in `mock_world.dart` (`paint`, "Paint together", "A shared whiteboard", plugin "Paint"); `startActivity` adds a session and emits the started event, twice for the same activity returns the same session; `endActivity` emits the ended event; `openActivityView` gives a view that draws a counter like the mock's channel view, emits its first batch only after the caller could listen (as the channel's does), and adds the viewer to `viewerIds` with a changed event; leaving voice or the session ending closes it with the real reason. Fakes answer like the real thing.

- [ ] **Step 1: Write failing tests:** `test/unit/rust/core_mapping_test.dart` (each new type maps field for field, every field a different value); `test/unit/rust/rust_repository_test.dart` (`the desktop asks for activities before it connects`; each of the four core events becomes its repo event); `test/unit/providers/activity_sessions_test.dart` (Ready, started, changed, ended, channel deleted; a `Ready` after events replaces the map); and, beside the mock's existing tests (`rg -l MockRepository app/test/unit`), the mock's start-twice, end and view-close reasons.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Checks:** `cd app && dart format lib test && flutter analyze && flutter test`.
- [ ] **Step 5: Commit** `feat(app): activities and sessions in the app's model`.

### Task 10: The plugin area and its view, by place

**Owner:** Claude. **Score:** 4 (moving a widget's key breaks what depended on it; plugin channels must work exactly as before). Read `core/providers/plugin_views.dart`, `features/plugins/plugin_area.dart`, `features/plugins/blocks_host.dart`, and every test that names `PluginArea` or `pluginViewProvider` (`rg -ln "PluginArea|pluginViewProvider" app/test`).

**Interfaces — produces:**
- `sealed class ViewPlaceRef` in `plugin_views.dart` with `ChannelPlace(String server, int channel)` and `ActivityPlace(String server, ActivitySession session)`, equal by server and channel, or server and session **id** (a session whose `viewerIds` changed is the same place). `pluginViewProvider` is a family by `ViewPlaceRef`; a channel opens with `openView`, an activity with `openActivityView`. Everything else it does (early outputs, pending inputs, closing on dispose, the error on a failed open) is unchanged.
- `PluginArea` takes `place` in place of `serverKey` and `channelId`. The plugin id for its host comes from the channel or from the session. The unavailable states of §14.4 apply to a channel place only. Its closed messages gain `'activity_ended' => 'This activity ended'`; `left_call` shows nothing (the tile is going). The key handling, the lifecycle inputs, the error state with **Reload** and the spinner are unchanged.
- `PluginArea` takes `bool paused` (default false): when it turns true the view is sent `{'kind':'lifecycle','event':'pause'}`, when false again `resume`; the window being hidden still pauses, and a view is resumed only when neither holds.
- `chat_area.dart` passes a `ChannelPlace`.

- [ ] **Step 1: Write failing tests:** update the existing plugin area tests to the new constructor first (they must fail to compile, then pass unchanged in what they assert). Add `test/widget/plugins/plugin_area_place_test.dart`: `an activity place opens its session's view` (road: mock repository → `pluginViewProvider(ActivityPlace)` → `PluginArea`; the counter is drawn); `a session whose viewers changed keeps its view` (the provider isn't rebuilt: the view id is the same); `an ended activity shows its message`; `paused sends pause once and resume once` (and `the window hiding while paused sends nothing more`).
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Mutations to run and report:** compare `ActivityPlace` by the whole session; drop the `paused` guard so a hidden window resumes a paused view.
- [ ] **Step 5: Checks:** `cd app && flutter analyze && flutter test`; `cd app/packages/opencord_blocks && flutter test`.
- [ ] **Step 6: Commit** `refactor(app): a plugin view opens by place`.

### Task 11: The Activities button and picker

**Owner:** Claude. **Score:** 3 (a new control in the call bar every caller sees). Read §15.2, `features/voice/voice_dock.dart` (the soundboard button is the pattern, with the dock's `hold`/`release` while its popover is open) and `features/soundboard/soundboard_picker.dart`.

**Interfaces — consumes:** Task 9's `ServerData.pluginActivities`, `activitySessionsProvider`, `startActivity`, `Permissions.startActivities`. **Produces:** `showActivityPicker(context, {required Rect anchor})` in `features/plugins/activity_picker.dart`; `voiceFocusProvider` isn't touched (Task 12 focuses the tile).

**What's shown, and what each control does (rule 25):**
- **The button:** in the dock after the soundboard button, icon `OcIcons.extension`, tooltip `Activities`. Shown when all hold: `capabilities.activities`; the server has at least one registered activity; the member holds `startActivities` in the call's channel (`permissionsIn`). Pressing opens the picker above it.
- **The picker:** a popover 320 wide, at most 360 tall, scrolling. Header `Activities`. One row per registered activity, by plugin name then name: its icon (the plugin's asset through `pluginAssetFileProvider`, or `OcIcons.extension` when it has none or it fails to load), 32 square with radius 8; the name; under it the description, or the plugin's name when the description is empty; one line each, ellipsis.
  - A row whose activity already runs in this call shows `Running` at its right in the secondary text colour. Pressing any row calls `startActivity` and closes the picker; the server answers with the running session where there is one.
  - While the call has 4 sessions, rows that aren't running are disabled, and a footer line reads `This call already has 4 activities.`
- **When starting fails,** a toast: `pluginUnavailable` → `This activity isn't available right now.`; `pluginLimit` → `This call already has 4 activities.`; `forbidden` → `You don't have permission to start activities here.`; anything else → the error's message. `RepoErrorKind` has neither plugin kind yet: add `pluginUnavailable` and `pluginLimit`, mapped from the core's error codes where the other kinds are mapped (`rg -n "RepoErrorKind.forbidden" app/lib/core/rust`).

- [ ] **Step 1: Write failing tests** in `test/widget/plugins/activity_picker_test.dart` (road: mock world → `serverProvider`/`activitySessionsProvider` → dock → picker): `the button shows only with an activity and the permission` (three cases: no activities, no permission, both); `pressing a row starts it in the call's channel` (the mock got the call's channel id, the plugin id and the key of **that** row: use two activities); `a running activity says Running`; `a full call disables the rest`; `a refused start says why` (each of the three kinds); `the dock stays while the picker is open`.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement,** then render the dock and the picker (with one, with six activities, with one running, at the cap) in dark and light at the normal and the narrowest width, and look.
- [ ] **Step 4: Checks:** `cd app && flutter analyze && flutter test`.
- [ ] **Step 5: Commit** `feat(app): start an activity from the call bar`.

### Task 12: Activity tiles on the call's stage

**Owner:** Claude. **Score:** 4 (the stage's layout, focus and video requests all key on the tile's id, which changes shape here). Read §15.2–§15.4, §12.5, `features/voice/{voice_layout,voice_view,voice_focus,voice_tile}.dart` whole, and their tests (`rg -ln "VoiceTileId|voiceTiles|voiceFocusProvider" app`).

**Interfaces — consumes:** Tasks 9–11. **Produces:**
- `VoiceTileId` becomes `({int userId, bool screen, int? session})`; a person's or a screen's tile has `session: null`, an activity's has `userId: 0, screen: false` and its session id. Every place that builds or matches one is updated (the list above). `voiceTiles` takes the call's sessions and puts their tiles **after** every person and screen, oldest session first; with `videoOnly` they stay.
- `openActivitiesProvider` in `core/providers/activity_sessions.dart`: the ids of the sessions this device has opened, emptied when the call's server or channel changes and pruned when a session ends. While an id is in it, its `pluginViewProvider(ActivityPlace)` is kept alive (the notifier listens to it), whatever page is shown.
- `ActivityTile` in `features/plugins/activity_tile.dart`, placed by `_Stage` for a session's tile:
  - **Not opened:** the session's icon (40 square, radius 10, same fallback as the picker), its name, `Started by <name>`, and who has it open: `No one has it open`, `1 person has it open`, `<n> people have it open`. A primary button `Open`. Pressing `Open`, or the tile, adds the id to `openActivitiesProvider`.
  - **Opened, in the grid or large:** a bar 28 high across the top (icon 16, name, then at the right: a button `Make large` / `Back to grid` that toggles focus, and a button `Close`), and under it `PluginArea(place: ActivityPlace(…))`, clipped to the tile. `Close` removes the id (the view closes; the session goes on). Modals of the plugin cover only the area under the bar (§12.5).
  - **Opened, in focus mode's strip** (`small`): only the icon and the name; `PluginArea` isn't built and the view is paused. Pressing the tile focuses it.
  - **Secondary press, any state:** an `OcMenu` with `Open` or `Close`, and, for the starter or a member with `manageChannels` in the channel, `End activity`. That asks, in the app's confirm dialog: title `End <name> for everyone?`, body `It closes for everyone in the call.`, buttons `End activity` and `Cancel`; confirming calls `endActivity`. A refusal shows the error's message as a toast.
- **Pause and resume:** an opened view is `paused` whenever its `PluginArea` isn't on screen: the strip, another page, another server. Off the voice page the provider is alive but no `PluginArea` exists, so `openActivitiesProvider` sends the `pause` and the `resume` itself for the views it keeps; exactly one `pause` per time hidden and one `resume` per time shown again, whichever of the two is responsible.
- **The call input:** for each opened view, `{'kind':'call','members':[…],'speaking':[…]}` (ids as decimal strings, each list sorted, no repeats) is sent once the view is open and then when either list changes, at most once every 100 ms with the newest lists, from `voiceProvider`'s participants of the call's channel and `speakingProvider`. It's sent from the provider side, so it goes on while the page is elsewhere.
- A session that ends removes its tile; if it was focused, the stage returns to the grid.

- [ ] **Step 1: Write failing tests.** Update `test/unit/voice/voice_layout_test.dart` and the voice tests that build tile ids first. Then `test/widget/plugins/activity_tile_test.dart` and `test/widget/voice/voice_activity_stage_test.dart`, each naming its road (mock repository → providers → `VoiceView`):
  - `a session adds a tile after the people`; `its tile says who started it and how many have it open` (0, 1 and 3).
  - `Open draws the plugin's screen in the tile` (the mock's counter appears; pressing its button redraws: the event reached the mock and came back).
  - `Make large focuses it and the strip shows the others`; `in the strip an opened activity is only its name`.
  - `Close closes my view and leaves the tile` (the mock saw `closeView`; the session is still in the provider).
  - `End activity asks, then ends it for everyone` (Cancel ends nothing; the menu item is absent for someone who is neither the starter nor a channel manager).
  - `an open activity survives leaving the voice page`: open it, press its counter twice, go to a text channel, come back: the same view id, the count still 2, and the mock got exactly one `pause` then one `resume`.
  - `the call input follows who is in the call and who speaks`: on open the mock's view got one `call` input with both lists; a member joining sends another with them in `members`; ten speaking changes within 100 ms send one more input, with the last set.
  - `leaving the call closes the opened activities` and `an ended activity's tile goes, and focus with it`.
  - Goldens of the tile not opened, opened, and small, in dark and light.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement,** then render the stage with one and with three activities among five people, in the grid and in focus mode, in dark and light, at the normal and the narrowest width, and look at each at full size.
- [ ] **Step 4: Mutations to run and report:** let the page leaving dispose the view; send `call` only once; put activity tiles before people; show `End activity` to everyone; skip the `resume`.
- [ ] **Step 5: Checks:** `cd app && flutter analyze && flutter test`.
- [ ] **Step 6: Commit** `feat(app): activity tiles in a call`.

**At the end of stage D:** full checks, push (rule 26).

---

## Stage E — The example and docs

### Task 13: The paint example and its end-to-end tests

**Owner:** Claude. **Score:** 3 (the sample plugin authors copy; milestone 2's example agreed with its own test and not with the renderer). Read §12.6, §13, the canvas's and the `stroke` event's shapes in the M2 plan's wire format, `crates/opencord-plugins/src/blocks.rs` for every block and prop you use, `examples/plugins/tickets/` and its two test files.

**Files:** `examples/plugins/paint/{manifest.lua,server/main.luau,client/main.luau,assets/paint.png}`, `crates/opencord-server/tests/plugins/paint.rs`, `crates/opencord-core/src/views/tests/paint.rs`.

**What it does:**
- Manifest: id `paint`, name `Paint`, permission `register_activities` only, `api = 1`. It registers `paint` ("Paint together", "A shared whiteboard", `assets/paint.png`; a 64×64 PNG you draw with a few lines of code and commit, under 5 KB).
- **One board per session,** in the server script's memory, gone when the session ends (and with a restart; a comment says so and points to `db` for plugins that must keep data). At most 300 strokes; the oldest go first.
- **The board is 1600 × 1000 units** whatever the tile's size. The client draws it scaled by `min(width / 1600, height / 1000)`, centred, and turns pointer points back into board units, so everyone sees the same picture at any tile size. The canvas fills the whole area and the tools float over it (a `stack`), so the canvas's size is the view's (`app.size`, and `resize`).
- **Drawing:** the canvas's `ink` in the chosen colour and width. When a stroke is done, the client thins it to at most 100 points, rounds them to whole board units, keeps it as a path item whose id is the event's `stroke` id (so the renderer's own ink is replaced without a flicker), and sends `stroke` with `{ id, color, width, points }`. Handler arguments have the wire format's shapes exactly: `on_stroke` gets `{ stroke, points, done }` with `points` flat `[x, y, pressure, …]`.
- **The server** checks every field of a `stroke` (an id of 1 to 64 characters; a colour from the palette of five; a finite width from 1 to 24; 2 to 200 finite numbers from 0 to 1600 and 0 to 1000) and ignores a bad one with one warning in its log; stores it; and sends it to the session's views with `ui.send_place({ session_id = … })`. The sender gets it too and skips an id it has.
- **Someone opening later** gets the board in `strokes` events of 30 strokes each. **Clear** (a button; anyone in the call) empties the board for everyone.
- **The tools:** five colour buttons, three widths, `Clear`, and a line reading who is drawing with you from `app.call.members` (`Just you`, `You and 1 other`, `You and <n> others`), updated by `call_members_change`.

- [ ] **Step 1: Write failing tests.**
  - `tests/plugins/paint.rs`, against the real server, each listing its people: `a_stroke_reaches_the_others_in_the_call` (A draws; B, in the call with it open, gets it; C, in the call without it open, and D, in another call running paint too, get nothing); `someone_opening_later_gets_the_board` (65 strokes arrive as events of 30, 30 and 5); `a_bad_stroke_is_dropped` (a colour off the palette, a `NaN` width, 201 numbers, a point at 5000: none stored, none sent, one warning each); `clear_empties_it_for_everyone`; `the_board_goes_with_the_session` (end it, start it again: empty); `the_301st_stroke_drops_the_first`.
  - `views/tests/paint.rs`, running the example's real client script in the VM with inputs in the wire's shapes (rule 24): `a_finished_stroke_becomes_an_item_and_is_sent` (feed `stroke` events with `done = false` then `true`; one `draw` op with an item of that id, and one `app.send("stroke")` whose points are board units for a 800 × 500 view, so half the pointer's numbers doubled); `a_long_stroke_is_thinned_to_100_points`; `a_stroke_from_someone_else_is_drawn_once` (the same server message twice: one item); `resizing_keeps_the_picture` (after a `context` input of another size, the items' points changed by the new scale); `the_line_says_who_is_here` (a `call` input with three members: `You and 2 others`).
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Write the plugin.** No `type(x) == "table" and x.value or x` hedges: each handler takes the one shape the wire format gives.
- [ ] **Step 4: Mutations to run and report:** send the stroke in view pixels instead of board units; drop the server's colour check; send to `ui.send_all` instead of the session.
- [ ] **Step 5: Checks:** `cargo test -p opencord-server --test plugins paint`; `cargo test -p opencord_core --lib views::paint`; `cargo test -p opencord-server --test plugins example` (the examples' package test, if it lists them); clippy; fmt.
- [ ] **Step 6: Commit** `feat(examples): a shared paint activity`.

**Owed by hand, said in the report and in the pull request:** nobody has drawn in a real call on two desktops. That check is the user's.

### Task 14: Docs

**Owner:** Gemini. **Score:** 2. Read `GEMINI.md` → "Write docs from the code, not from memory" first.

**Files:** `docs/plugins.md`, `docs/protocol.md` (check only), `.superpowers/sdd/plugins-m3/pr_body.md`.

- [ ] **Step 1: `docs/plugins.md` gains a section "Activities"** after "Client Scripts and Custom Channels", with, in this order: what an activity is and who can start one; `ui.register_activity` and the `register_activities` permission; the events `activity_start` and `activity_end` with every field and every reason; `activity.finish`; the place of a view in an activity and `ui.views` / `ui.send_place` with `{ session_id }`; `app.call` and the two `app.listen` names with their arguments; when a session ends; when a view is paused; the limits of this plan's table; the paint example, with its path.
  - **Copy every function, field, reason and number from `crates/opencord-plugins/api/opencord.d.luau` and the code,** not from this plan: open the definition, copy the shape. For each name you write, run `rg -n "<name>" crates examples` and keep it only if it's found.
  - The permissions table gains `START_ACTIVITIES` for members; "Example Plugins" gains paint; "Runtime Limits and Quotas" gains the three limits.
- [ ] **Step 2: Read `docs/protocol.md` against the proto files** once more, now that every task is in: each of Task 1's names is there and matches.
- [ ] **Step 3: Write `pr_body.md`** under rule 27: what changed per stage, from `git diff --stat feat/plugins-channels...HEAD` and grep; how it was tested, with the test files; **what couldn't be tested** (Windows and macOS by hand; two real desktops in one call; the phone, which gets nothing); every commit outside this plan's scope; the calls from "Claude's calls in this plan" by number.
- [ ] **Step 4: Checks:** `rg` for every name in the three files comes back with a hit; `cargo test -p opencord-server --test plugins definitions`.
- [ ] **Step 5: Commit** `docs: activities for plugin authors`, then `gh pr edit --body-file .superpowers/sdd/plugins-m3/pr_body.md`.

**At the end of stage E:** full checks, push, read the pull request's checks to the end (rule 26), then hand over the review of stages D and E. After it passes: `gh pr ready`, write the head commit's hash into `.superpowers/sdd/plugins-m3/m3-finished` as the very last step, and report to the user. Don't start milestone 4: the user releases it.
