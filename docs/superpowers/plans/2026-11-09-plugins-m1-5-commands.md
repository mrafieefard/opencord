# Plugins Milestone 1.5: Commands, Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. The owner decided that Claude builds all of this milestone, so tasks have no scores or owners.

**Goal:** A plugin's slash command is a function that can take its time, no command failure is silent, plugins can share names, and the `/` popup belongs to the floating UI on desktop and exists at all on the phone.

**Architecture:** The server keeps one registry per source and resolves `name + source`. A command call becomes a small state machine owned by a new `plugins/command_calls.rs` (waiting → thinking → done), and deferred commands live in `commands/thinking.rs` with their own expiry. Desktop and phone each get one popup built from a pure "groups and rows" model, so filtering, keyboard order and the rail are tested without a screen.

**Tech Stack:** Rust (tokio, mlua Luau, sqlx SQLite, prost), Flutter/Riverpod through flutter_rust_bridge, React Native (Expo, Reanimated, FlashList, uniffi-bindgen-react-native).

**Spec:** `docs/superpowers/specs/2026-11-07-plugins-design.md` §26 (with §10.3, §10.4, §20, §22 item 1.5, §25 items 21–29), and for the look `docs/superpowers/specs/2026-11-04-floating-ui-design.md` §4, §4.3, §5.12, §5.17.

## Global Constraints

- Branch `feat/plugins-commands`, from `feat/plugins`. The pull request goes into `feat/plugins` while #46 is open, else `beta`. The owner merges, with a merge commit. Nothing is pushed straight to `beta`, `canary` or `main`.
- Every `cargo`, `flutter`, `dart`, `flutter_rust_bridge_codegen` command starts with `source .superpowers/sdd/media/env.sh &&`. Every `yarn`, `npx`, `ubrn`, `adb`, Gradle command starts with `source mobile/scripts/env.sh &&`.
- `df -h /` before full checks (under 10 G free at the start of this milestone: delete `target/debug/incremental` and superseded binaries when low). Never `cargo clean` without asking.
- Exact strings, from §26: `"INVALID_ARGUMENT: a command needs a function"`, `"INVALID_ARGUMENT: no event called {name}"`, `"INVALID_ARGUMENT: already answered"`, `"PLUGIN_LIMIT: too many commands waiting"`, `"TIMEOUT: too late to reply"`, `"{plugin name} is thinking…"`, `"{plugin name} didn't answer"`, `"{plugin name} ran into an error"`, `"/roll answered after 4.2 s, too late"`, `"Two commands are called /roll. Pick one from the list."`, `"No commands match /xyz"`, `"Only you can see this"`.
- Numbers: 3 s to answer or defer, 15 min to reply after a defer, 1 s for autocomplete, 100 thinking per plugin, 100 commands per plugin, 5,000 rows must scroll smoothly.
- The popup and list never change the message list's size or scroll offset, and the phone's composer never moves.
- Motion: layout never overshoots, closing is faster than opening, hover is plain 120 ms `easeOut`, nothing grows from below 0.96; reduced motion removes animation.
- Older apps keep working: a request without `source`, and a stored text that reads sensibly without the new state.
- `.github/check.sh` passes before anything is pushed. The Windows, macOS and real-phone checks that can't be run here are listed in the pull request.

## Review Focus

These are not spelled out by §26's tests. Each has its test in the task that owns the code.

1. The server stops or crashes while a command is thinking: after the restart the message says "{plugin} didn't answer" and doesn't think forever (Task 5).
2. A moderator deletes the thinking message: the later `cmd:reply` neither brings it back nor raises, and the plugin's log says so (Task 5).
3. A plugin is removed or its commands change while the popup is open or its command is being written: the list updates, the selection stays on a real row, and running it gives a plain error (Tasks 9, 10, 14).
4. Music isn't set up here and a plugin has `/play`, typed without picking: the plugin's runs, not Opencord's unusable one (Task 3).
5. The connection drops during an only-you *thinking*: the note doesn't spin forever on desktop or phone (Tasks 11, 15).

---

## Stage 0: Prototype

### Task 1: A clickable prototype, approved before anything is built

**Files:**
- Create: `docs/superpowers/specs/2026-11-09-command-popup-prototype.html` (the local authority once approved, as the floating UI's was)

- [ ] **Step 1: Build one HTML page** with the real tokens from `app/lib/ui/theme/oc_colors.dart`, `oc_metrics.dart`, the phone's `mobile/src/ui/theme`, in dark and light. Desktop pane: the composer float over a chat, typing `/` opens the popup; rail, sticky groups, filter, ↑↓, Ctrl+↑↓, Enter, Esc, hover, the empty row, a dimmed row, `pop` open and instant close, the option list for `/play` with artwork, the command being written with its chip, a *thinking* and a *no answer* message. Phone pane: attach menu with Commands, the list with the horizontal source row, chips, a private reply. Controls for 1, 2 and 50 sources and for reduced motion.
- [ ] **Step 2: Settle the numbers it decides**: the popup's radius, row height, height cap, gap above the float, rail width; the phone's open and close timing. Write them in this plan under "Prototype results".
- [ ] **Step 3: Publish it for the owner and STOP.** No code in Tasks 2 onward starts until the owner approves. Rework from their notes until they do.
- [ ] **Step 4: Commit** `docs: the command popup's prototype`.

**Prototype results:** *(filled in at Step 2)*

---

## Stage A: Server

### Task 2: Protocol, limits and storage

**Files:**
- Modify: `proto/opencord/v1/commands.proto`, `proto/opencord/v1/models.proto:97`, `crates/opencord-plugins/src/limits.rs`, `crates/opencord-server/src/db/messages.rs`, `crates/opencord-server/src/handlers/messages.rs:287`, `crates/opencord-server/src/server.rs:130`, `docs/protocol.md`
- Create: `crates/opencord-server/migrations/0015_command_state.sql`
- Test: `crates/opencord-server/src/db/messages.rs` (tests module)

**Interfaces:**
- Produces, proto: `string source = 5` on `ExecuteCommand`; `string source = 5` on `AutocompleteCommand`; `string source = 5` on `UpdateCommandSettings`; `enum CommandState { COMMAND_STATE_ANSWERED = 0; COMMAND_STATE_THINKING = 1; COMMAND_STATE_NO_ANSWER = 2; }`; `CommandState state = 3` on `MessageInteraction`; `CommandState state = 4` on `EphemeralMessage`.
- Produces, limits: `COMMAND_THINKING_TIME = 15 min`, `COMMAND_THINKING_MAX: usize = 100`.
- Produces, db: `pub struct CommandRun<'a> { name: &'a str, user_id: i64, state: i32 }`; `insert_result(conn, id, channel_id, author_id, content, run: Option<CommandRun<'_>>)`; `finish_thinking(conn, id: i64, content: &str, state: i32) -> Result<bool, sqlx::Error>` (false when the message is gone or no longer thinking; leaves `edited_at` alone); `sweep_thinking(conn) -> Result<u64, sqlx::Error>`. `MessageRow` gains `interaction_user_id: Option<i64>`, `command_state: i64`.

- [ ] **Step 1: Write the failing tests** `finish_thinking_replaces_the_text_without_marking_it_edited`, `finish_thinking_returns_false_for_a_deleted_message`, `finish_thinking_returns_false_when_already_finished`, `sweep_thinking_turns_every_thinking_message_into_no_answer` (asserts content `"Dice Bot didn't answer"` for a plugin user named Dice Bot, state 2, an answered message untouched).
- [ ] **Step 2: Run** `cargo test -p opencord-server --lib db::messages` and see them fail to compile.
- [ ] **Step 3: Add the proto fields and limits.** Write `0015_command_state.sql`: `ALTER TABLE messages ADD COLUMN interaction_user_id INTEGER;`, `ALTER TABLE messages ADD COLUMN command_state INTEGER NOT NULL DEFAULT 0;`, and `CREATE INDEX messages_thinking ON messages (id) WHERE command_state = 1;` so the sweep stays fast on a large table.
- [ ] **Step 4: Implement the db functions.** `sweep_thinking` sets the author's display name from `users` into the text. `message_from_row` (`handlers/messages.rs:287`) uses `interaction_user_id` when present and falls back to `author_id` for older rows: today a plugin's reply loaded from history names the plugin as whoever ran it. Call `sweep_thinking` next to `state.commands.load` in `server.rs`, before plugins start.
- [ ] **Step 5: Refresh offline query data** with `cargo sqlx prepare --workspace`, then run the tests and see them pass.
- [ ] **Step 6: Commit** `feat(commands): command source and state in the protocol and messages`.

### Task 3: The same name in more than one place

**Files:**
- Modify: `crates/opencord-server/src/commands/mod.rs`, `crates/opencord-server/src/handlers/commands.rs`, `crates/opencord-server/src/plugins/api/commands.rs:207-219`, `crates/opencord-server/src/plugins/install.rs`, `crates/opencord-server/src/plugins/mod.rs`
- Test: `crates/opencord-server/tests/plugins/commands.rs`, `crates/opencord-server/src/commands/mod.rs`

**Interfaces:**
- Consumes: Task 2's `source` fields.
- Produces: `Commands.plugins: RwLock<HashMap<(String, String), PluginCommand>>` keyed by (plugin id, name); `find_plugin(&self, plugin_id: &str, name: &str) -> Option<PluginCommand>`; `plugins_named(&self, name: &str) -> Vec<PluginCommand>`; `pub fn settings_key(feature: &str, name: &str) -> String` (`plugin:<id>/<name>` for a plugin's feature, else `name`); `remove_settings_for_plugin(settings: CommandSettings, plugin_id: &str) -> CommandSettings`; `PluginHandle.installed_at: i64`.

- [ ] **Step 1: Write the failing tests**, replacing `a_taken_name_is_refused`:
  - `two_plugins_may_use_one_name_and_each_runs_its_own`
  - `a_plugin_may_use_a_builtin_name_and_no_source_runs_the_builtin`
  - `no_source_runs_the_only_plugin_with_the_name`
  - `no_source_with_two_plugins_asks_to_pick` (message exactly `"Two commands are called /roll. Pick one from the list."`, code `COMMAND_UNKNOWN`)
  - `an_unknown_source_is_command_unknown`
  - `no_source_prefers_a_plugin_when_the_builtin_feature_is_not_set_up` (Review Focus 4)
  - `settings_are_kept_per_source` (turning off one plugin's `/roll` leaves the other's on; a built-in keeps its plain key)
  - `uninstall_removes_only_that_plugins_settings` and `stopping_keeps_them`
  - `plugins_are_listed_in_install_order`
- [ ] **Step 2: Run** `cargo test -p opencord-server --test plugins commands::` and see them fail.
- [ ] **Step 3: Implement.** Drop both "already taken" checks. Replace `usable(ctx, name, channel_id)` with `usable(ctx, name, source, channel_id)`: `plugin:<id>` looks up that plugin; empty looks for a built-in when `feature_ready` or when no plugin has the name, then the plugins; any other source is `COMMAND_UNKNOWN`. Settings go through `settings_key` in `usable`, `available`, `all` and `update_settings`. Sort `plugin_commands` by (`installed_at`, name), with `installed_at` read from each handle. `uninstall` saves `remove_settings_for_plugin` and announces. M1 is unreleased, so plain plugin keys need no migration.
- [ ] **Step 4: Run the tests** and see them pass.
- [ ] **Step 5: Commit** `feat(commands): the same command name in more than one source`.

### Task 4: A function per command

**Files:**
- Create: `crates/opencord-server/src/plugins/command_calls.rs` (the `cmd` and `ac` tables, moved out of `runtime.rs`, which is past the 800-line limit)
- Modify: `crates/opencord-server/src/plugins/api/commands.rs`, `plugins/api/plugin.rs`, `plugins/runtime.rs:529-650,709-872`, `plugins/mod.rs:57`, `handlers/commands.rs`, `crates/opencord-plugins/api/opencord.d.luau`, `examples/plugins/welcome-bot/server/main.luau`
- Test: `crates/opencord-server/tests/plugins/commands.rs`, `crates/opencord-server/tests/plugins/example.rs`

**Interfaces:**
- Produces: registry key `opencord.commands`, name → `{ run = fn, options = { [option] = fn } }`, exported as `api::commands::COMMANDS`; `PluginEvent.command: Option<oneshot::Sender<CommandAnswer>>`; `pub enum CommandAnswer { Reply { content: String, only_you: bool }, Defer { only_you: bool, started: oneshot::Sender<Result<ThinkingId, ApiError>> }, Failed }`; `KNOWN_EVENTS: &[&str]` in `api/plugin.rs`; `command_calls::command_table(...)`, `command_calls::autocomplete_table(...)`.

- [ ] **Step 1: Write the failing tests**, moving every M1 test in `tests/plugins/commands.rs` to the new form:
  - `a_command_runs_its_own_function`
  - `register_without_a_function_raises` (exact message)
  - `option_autocomplete_true_raises` (the M1 form)
  - `registering_a_name_again_replaces_spec_and_function`
  - `an_option_autocomplete_function_gets_its_own_calls`
  - `on_command_raises_no_event_called` and `on_a_typo_raises_no_event_called` (`"INVALID_ARGUMENT: no event called message_creat"`)
  - `every_event_in_the_design_table_is_accepted`
  - `a_function_that_errors_answers_at_once` (under 1 s, `"{name} ran into an error"`, the error in the plugin's log)
  - `an_invalid_reply_raises_in_the_function`
- [ ] **Step 2: Run** `cargo test -p opencord-server --test plugins commands::` and see them fail.
- [ ] **Step 3: Implement `commands.register(spec, fn)`.** Check the function first. Store it with the options' functions in the registry; `autocomplete` on the spec is `true` when the option has a function, and anything but a function or nil raises `"INVALID_ARGUMENT: option autocomplete must be a function"`. `unregister` clears the registry entry.
- [ ] **Step 4: Implement `opencord.on`'s check.** `KNOWN_EVENTS` is every name in §10.4's table except `hooks.on` handlers and the two commands events, M2's included, so M2 needn't touch it. Only `on` checks; `off` keeps returning false. Change any test that listens to a made-up event name.
- [ ] **Step 5: Implement dispatch.** `dispatch` gets a `command` and a `command_autocomplete` branch, shaped like the `hook` branch, that look the function up in `COMMANDS` and call it with the table from `command_calls`. On a handler error, `failed(...)` runs as before and the call's state answers `CommandAnswer::Failed`. Remove both branches from `event_table`.
- [ ] **Step 6: Update `handlers/commands.rs`** to deliver with `command: Some(tx)` and `Failed` → `PLUGIN_UNAVAILABLE "{name} ran into an error"`. Validate reply content in `cmd:reply`, which raises instead of the old "sent an invalid reply". Update `opencord.d.luau` and the example plugin; `definitions_match_the_api` keeps them honest.
- [ ] **Step 7: Run the tests** and see them pass.
- [ ] **Step 8: Commit** `feat(plugins): a function per command`.

### Task 5: "Is thinking…"

**Files:**
- Create: `crates/opencord-server/src/commands/thinking.rs`
- Modify: `crates/opencord-server/src/plugins/command_calls.rs`, `handlers/commands.rs`, `plugins/runtime.rs:335`, `commands/mod.rs`, `crates/opencord-plugins/api/opencord.d.luau`
- Test: `crates/opencord-server/tests/plugins/commands.rs`

**Interfaces:**
- Consumes: Task 2's db functions, Task 4's `CommandAnswer`.
- Produces: `pub struct ThinkingId(u64)`; `pub enum Place { Channel { message_id: i64 }, OnlyYou { session_id: String, nonce: String } }`; `thinking::start(state: &Arc<AppState>, ctx: StartThinking) -> Result<ThinkingId, ApiError>`; `thinking::answer(state, id, content) -> Result<(), TooLate>`; `thinking::give_up(state, id, text)`; `thinking::abandon(state, plugin_id)`; `Commands.thinking: Thinking`. `pub struct TooLate { command: String, waited: Duration }`.

- [ ] **Step 1: Write the failing tests:**
  - `defer_posts_a_thinking_message_and_the_reply_edits_it` (the request returns at once; interaction state 1 then 0; stored text `"Dice Bot is thinking…"`; `edited_at_ms` stays `None`)
  - `defer_only_you_sends_thinking_then_replaces_it_by_nonce`
  - `defer_decides_where_the_reply_goes` (`reply(x, {only_you=true})` after a public defer stays public)
  - `defer_goes_to_the_member_when_the_plugin_cannot_post`, `defer_follows_results_private`
  - `a_second_defer_or_a_defer_after_reply_raises`
  - `no_reply_in_time_becomes_no_answer` (limit shortened through the test server's plugin config)
  - `the_plugin_stopping_while_thinking_becomes_no_answer`
  - `an_error_after_defer_says_ran_into_an_error`
  - `the_101st_thinking_command_raises_plugin_limit`
  - `a_reply_after_the_message_was_deleted_logs_and_does_not_raise` (Review Focus 2)
  - `a_restart_turns_thinking_into_no_answer` (Review Focus 1, through a real server restart on the same database)
  - `history_returns_a_thinking_message_with_its_state`
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement `thinking.rs`.** Each entry keeps plugin id and name, command, channel, member, `Place`, start `Instant` and the `AbortHandle` of its expiry task. `start` enforces the cap, posts (public through `insert_result` with state 1, or `EphemeralMessage` with state 1), and spawns the expiry. `answer` and `give_up` remove the entry first, so exactly one of a reply, the expiry and a stop wins. Public finish: `finish_thinking`, then `MessageUpdate`. A false result logs a warning: "/roll's message was deleted before the reply". Only-you finish: an `EphemeralMessage` with the same nonce, skipped quietly if the session is gone.
- [ ] **Step 4: Implement `cmd:defer`** as an `async_function`: it sends `Defer`, awaits `started`, and moves the call's state to `Thinking(id)`, so a reply can't overtake its placeholder. After a defer `cmd:reply` calls `thinking::answer` on the server runtime through `env.on_server`.
- [ ] **Step 5: In `handlers/commands.rs`** handle `Defer`: pick the place by the M1 rule (`results_private`, `only_you`, or no permission to post → only you), call `thinking::start`, send the result back, and return `CommandResult`. Extract the reply-posting half of `execute` into `post_reply` so `execute` stays under 50 lines per function.
- [ ] **Step 6: Call `thinking::abandon`** where a stopped VM removes its commands (`runtime.rs:335`).
- [ ] **Step 7: Run the tests** and see them pass.
- [ ] **Step 8: Commit** `feat(plugins): cmd:defer and the thinking state`.

### Task 6: Late replies are never silent

**Files:**
- Modify: `crates/opencord-server/src/plugins/command_calls.rs`, `handlers/commands.rs`
- Test: `crates/opencord-server/tests/plugins/commands.rs`, `plugins/command_calls.rs` (tests module)

**Interfaces:**
- Produces: `fn waited_text(waited: Duration) -> String` (`"4.2 s"` under a minute, `"15 min 2 s"` above).

- [ ] **Step 1: Write the failing tests:** `a_reply_after_3_seconds_raises_and_logs` (exact `"TIMEOUT: too late to reply"`, log `"/roll answered after 3.1 s, too late"` by pattern), `a_defer_after_3_seconds_raises_too`, `a_reply_after_15_minutes_raises_and_logs`, `a_late_suggest_returns_false_and_logs_nothing`, `waited_text_reads_well`, `an_answer_arriving_as_the_wait_ends_is_used`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** The call keeps its start `Instant`. A failed `send` (the waiter has gone) raises and logs at `Warn`; `TooLate` from `thinking::answer` does the same. In `handlers/commands.rs` wait on `&mut rx`; after a timeout call `rx.close()` then `rx.try_recv()`, and use an answer that made it in time, so a plugin is never told it succeeded while the member saw "didn't answer".
- [ ] **Step 4: Run the tests** and see them pass.
- [ ] **Step 5: Commit** `feat(plugins): a late reply raises and is logged`.

---

## Stage B: Desktop

### Task 7: Source and state through the core

**Files:**
- Modify: `crates/opencord-core/src/commands.rs`, `crates/opencord-core/src/api/client.rs`, `crates/opencord-core/src/api/types.rs`, `crates/opencord-core/src/convert.rs`, `app/lib/core/model/commands.dart`, the message model in `app/lib/core/model/`, `app/lib/core/repository/repository.dart`, `app/lib/core/rust/rust_repository.dart`, `app/lib/core/rust/core_mapping.dart`, `app/lib/core/mock/mock_repository.dart`, generated frb files
- Test: `app/test/unit/chat/commands_model_test.dart` (create), core's tests

**Interfaces:**
- Produces, core: `source: String` after `name` on `command_execute` and `command_autocomplete`; `source` on `CommandSettingsChanges`; `CommandRunState { Answered, Thinking, NoAnswer }` on the ephemeral event and message interaction.
- Produces, Dart: `CommandSpec.source` (`feature` when it starts with `plugin:`, else `''`); `CommandSpec.pluginId`; `CommandList.find(String name, {String source = ''})` (exact source when given, else §26.5's order); `enum CommandRunState { answered, thinking, noAnswer }`; `EphemeralNote.state`; `repository.executeCommand(server, channel, name, options, {required nonce, source = ''})`; `autocompleteCommand(..., {source = ''})`.

- [ ] **Step 1: Write the failing tests** `source_is_the_plugin_feature_or_empty`, `find_prefers_opencord_without_a_source`, `find_with_a_source_finds_only_that_one`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement** through the layers and regenerate with `flutter_rust_bridge_codegen generate`.
- [ ] **Step 4: Run** `flutter test test/unit/chat` and `cargo test -p opencord_core`, and see them pass.
- [ ] **Step 5: Commit** `feat(core,app): command source and state`.

### Task 8: The popup's rows, without a screen

**Files:**
- Create: `app/lib/features/chat/popup/popup_rows.dart`
- Modify: `app/lib/features/chat/command_input.dart:39-47`
- Test: `app/test/unit/chat/popup_rows_test.dart`

**Interfaces:**
- Produces: `class PopupSource { String id; String name; String avatarHash; int? userId; }`; `class PopupGroup { PopupSource source; List<CommandSuggestion> rows; int firstIndex; }`; `List<PopupGroup> groupCommands(List<CommandSuggestion> rows, ServerData data)`; `int stepRow(int at, int by, int count)` (wraps); `int stepGroup(List<PopupGroup> groups, int at, int by)`; `double offsetOfRow(List<PopupGroup> groups, int index, {required double row, required double header})`; `int groupAtOffset(List<PopupGroup> groups, double offset, {required double row, required double header})`.

- [ ] **Step 1: Write the failing tests:** `opencord_comes_first_then_plugins_in_server_order`, `empty_groups_are_left_out`, `a_plugin_without_an_avatar_still_groups_under_its_name`, `same_name_stays_in_both_groups`, `stepRow_wraps`, `stepGroup_jumps_and_wraps`, `offset_and_group_are_inverse_across_5000_rows`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** Remove `maxSuggestions` for commands; `matchingCommands` still matches name and aliases and keeps the server's order. A source's name and avatar come from `data`'s plugin users; a plugin with no row left shows its id.
- [ ] **Step 4: Run the tests** and see them pass.
- [ ] **Step 5: Commit** `feat(app): group the / popup's commands by source`.

### Task 9: The popup

**Files:**
- Create: `app/lib/features/chat/popup/composer_popup.dart`, `app/lib/features/chat/popup/popup_rail.dart`, `app/lib/ui/widgets/pop_in.dart`
- Modify: `app/lib/features/chat/composer.dart:185-262,677-738`, `app/lib/ui/widgets/popup_route.dart`
- Delete: `app/lib/features/chat/suggestion_list.dart`
- Test: `app/test/widget/chat/composer_popup_test.dart`, `app/test/widget/chat/goldens/`, `app/test/widget/chat/commands_test.dart`

**Interfaces:**
- Consumes: Task 8.
- Produces: `class PopIn extends StatefulWidget` (`child`, `alignment`; runs `OcSpring.pop` with §4.3's table once on insert, nothing under reduced motion), shared with `OcPopupRoute`; `class ComposerPopup` (`width`, `maxHeight`, `groups`, or `rows` for `@ # :`, `selected`, `onHover`, `onPick`, `emptyText`); `scrollToRow`.

- [ ] **Step 1: Write the failing widget tests:**
  - `the_rail_shows_only_with_two_sources_or_more`
  - `clicking_a_rail_icon_scrolls_to_its_group`, `scrolling_moves_the_rail_highlight`
  - `arrows_cross_groups_and_wrap`, `ctrl_arrows_jump_groups`, `enter_and_tab_pick_escape_closes`, `the_selected_row_stays_in_view`
  - `a_header_sticks_while_its_commands_scroll`
  - `typing_filters_and_empty_groups_lose_their_rail_icon`, `nothing_matching_shows_the_empty_row_and_stays_open`
  - `5000_rows_build_only_what_is_visible`
  - `the_message_list_keeps_size_and_offset_while_the_popup_opens_filters_and_closes` (measured)
  - `the_bottom_edge_stays_put_while_filtering`
  - `the_popup_opens_on_pop_and_closes_at_once`, `reduced_motion_has_no_animation` (pinned against §4.3)
  - `long_or_right_to_left_descriptions_stay_on_one_line`
  - `at_mentions_channels_and_emoji_use_it_without_rail_or_groups`
  - `commands_changing_while_open_keep_a_real_selection` (Review Focus 3)
  - goldens in light and dark
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** Use `CustomScrollView` with one `SliverMainAxisGroup` per source: a pinned header and a `SliverFixedExtentList`. Keyboard moves scroll to `offsetOfRow`. Use `popupDecoration`, the follower anchored bottom-left to the composer float at offset `(0, -gap)`, its width the float's, its height `min(cap, room above the composer)`. Sizes come from Task 1's results.
- [ ] **Step 4: In `composer.dart`** route all four triggers through `ComposerPopup`, handle Ctrl+↑↓, and delete `SuggestionList`.
- [ ] **Step 5: Run the tests** and see them pass.
- [ ] **Step 6: Commit** `feat(app): the / popup with its plugin rail`.

### Task 10: The command being written

**Files:**
- Modify: `app/lib/features/chat/command_input.dart`
- Test: `app/test/widget/chat/commands_test.dart`, goldens

**Interfaces:**
- Consumes: `ComposerPopup`, `PopIn`, `CommandSpec.source`.

- [ ] **Step 1: Write the failing tests:** `the_command_draws_no_box_of_its_own`, `the_chip_shows_the_plugins_icon`, `autocomplete_uses_the_popups_surface_and_place`, `a_picked_commands_source_is_sent`, `autocomplete_sends_the_source`, `a_removed_command_gives_a_plain_error_and_keeps_the_text` (Review Focus 3). Keep every existing Tab, Enter, Esc and Backspace test.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** Remove the box at line 410 so the `Wrap` sits where the text field was, and keep the float at its 62 height for a line of options. Replace `_ChoiceList` with `ComposerPopup` (its 40-tall rows keep `/play`'s artwork), and pass `source` to run and autocomplete.
- [ ] **Step 4: Run the tests** and see them pass.
- [ ] **Step 5: Commit** `feat(app): the command being written sits in the composer float`.

### Task 11: Thinking in the chat and the Commands page

**Files:**
- Create: `app/lib/features/chat/thinking_dots.dart`
- Modify: `app/lib/features/chat/message_rows.dart`, `app/lib/features/chat/message_list.dart`, the bubble that draws a message's interaction, `app/lib/core/providers/commands_state.dart`, `app/lib/features/settings/server/commands_page.dart`
- Test: `app/test/widget/chat/commands_test.dart`, `app/test/unit/chat/message_rows_test.dart`

**Interfaces:**
- Consumes: `EphemeralNote.state`, `CommandSpec.source`.
- Produces: `EphemeralNotesNotifier` removes a *thinking* note after `thinkingLimit` plus 30 s, and all *thinking* notes when the server connection comes back.

- [ ] **Step 1: Write the failing tests:** `a_thinking_message_shows_three_fading_dots`, `dots_stand_still_under_reduced_motion`, `no_answer_shows_muted_with_its_icon`, `the_reply_replaces_a_thinking_note_by_nonce`, `an_edit_from_thinking_shows_no_edited_mark`, `a_thinking_note_goes_when_the_connection_returns` (Review Focus 5), `the_commands_page_groups_by_source_with_icon_and_name`, `turning_off_one_plugins_roll_leaves_the_other`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** The Commands page keys its pending changes by source and name, and groups by `source` instead of `_features`, keeping the Music footer.
- [ ] **Step 4: Run** `flutter analyze` and the tests, and see them pass.
- [ ] **Step 5: Commit** `feat(app): thinking in the chat, Commands page by source`.

---

## Stage C: Phone

The phone lists the server's commands. `/volume` and `/music` stay desktop's: they drive its music panel and its per-device volume.

### Task 12: Commands through the binding and the store

**Files:**
- Create: `crates/opencord-mobile/src/api/commands.rs`
- Modify: `crates/opencord-mobile/src/api/mod.rs`, `src/events.rs:472`, `src/types.rs`, `src/convert.rs`, `mobile/src/core/api.ts`, `mobile/src/core/native.ts`, `mobile/src/core/fake/FakeCore.ts`, `mobile/src/core/fake/world.ts`, `mobile/src/state/types.ts`, `mobile/src/state/events.ts`, `mobile/src/state/store.ts`
- Create: `mobile/src/state/actions/commands.ts`
- Test: `crates/opencord-mobile` tests, `mobile/src/state/__tests__/commands.test.ts`

**Interfaces:**
- Produces, binding: `CommandRecord`, `CommandOptionRecord`, `CommandChoiceRecord`, `CommandListRecord`, `EphemeralNoteRecord`, `CommandRunStateRecord`; snapshot `commands`; `MobileEvent::CommandsUpdated`, `MobileEvent::EphemeralReceived`; `MessageRecord.interaction { userId, command, state }`; `command_execute(server_key, channel_id, name, source, options, nonce)`, `command_autocomplete(server_key, channel_id, name, source, option, value)`.
- Produces, store: `ServerState.commands`, `ServerState.ephemeral: Record<string, EphemeralNoteRecord[]>`; `runCommand(...)`, `suggestForOption(...)`, `dismissEphemeral(...)`.

- [ ] **Step 1: Write the failing tests:** `commands_update_reaches_the_store`, `an_ephemeral_with_the_same_nonce_replaces_the_note`, `a_failed_run_becomes_an_only_you_note`, `reconnecting_clears_thinking_notes` (Review Focus 5), and a convert test in the facade for each record.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement** and regenerate with `ubrn`; `mobile/scripts/check-bindings.sh` must pass.
- [ ] **Step 4: Run** `yarn -s jest src/state` and `cargo test -p opencord-mobile`, and see them pass.
- [ ] **Step 5: Commit** `feat(mobile): commands through the binding and the store`.

### Task 13: The list and the attach menu

**Files:**
- Create: `mobile/src/features/chat/commands/commandRows.ts`, `CommandList.tsx`, `SourceRow.tsx`
- Modify: `mobile/src/features/chat/Composer.tsx:121-155,222`, `mobile/src/features/chat/ChatPane.tsx`
- Test: `mobile/src/features/chat/commands/__tests__/commandRows.test.ts`, `CommandList.test.tsx`, `mobile/src/features/chat/__tests__/Composer.test.tsx`

**Interfaces:**
- Consumes: Task 12's store.
- Produces: `groupCommands(list, query, ctx) -> CommandGroup[]`, `flatRows(groups)`, `commandUnavailable(spec, ctx)` (the desktop's rules); `<CommandList query groups onPick onDismiss />`.

- [ ] **Step 1: Write the failing tests:**
  - `commandRows`: `opencord_first_then_install_order`, `the_availability_rules_match_desktops`, `filter_by_name_and_empty_groups_drop`, `5000_rows_group_quickly`
  - `Composer`: `attach_shows_photo_file_and_commands`, `no_attach_permission_leaves_only_commands`, `no_commands_leaves_out_the_entry`, `nothing_usable_hides_the_button`, `editing_hides_the_button`, `commands_puts_a_slash_and_opens_the_list`, `typing_a_slash_opens_and_deleting_it_closes`
  - `CommandList`: `the_source_row_shows_only_with_two_sources`, `tapping_a_source_scrolls_to_it`, `scrolling_moves_the_highlight`, `never_taller_than_40_percent_of_the_room`, `opening_filtering_and_closing_keep_the_list_and_composer_in_place` (measured), `commands_changing_while_open_keep_working` (Review Focus 3), `reduced_motion_has_no_animation`
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** The list is absolutely positioned in `ChatPane` above the dock, using `useChatDock`'s `height` and `fieldGrowth`, so it adds nothing to `padding`. Use FlashList with sticky headers, Reanimated for the rise and fade at Task 1's timing, `expo-haptics` `selectionAsync` on pick, Android's `BackHandler` first, and close on keyboard dismiss.
- [ ] **Step 4: Run** `yarn -s typecheck` and the tests, and see them pass.
- [ ] **Step 5: Commit** `feat(mobile): the slash command list and its attach entry`.

### Task 14: Picking, options and running

**Files:**
- Create: `mobile/src/features/chat/commands/useCommandDraft.ts`, `CommandChips.tsx`
- Modify: `mobile/src/features/chat/Composer.tsx`
- Test: `mobile/src/features/chat/commands/__tests__/useCommandDraft.test.ts`, `CommandChips.test.tsx`

**Interfaces:**
- Consumes: Tasks 12, 13.
- Produces: `useCommandDraft(serverKey, channelId) -> { spec, source, values, active, pick, setValue, focusOption, clear, missing, run }`.

- [ ] **Step 1: Write the failing tests:** `picking_makes_a_chip_and_option_chips`, `required_options_are_marked`, `tapping_a_chip_edits_that_option`, `choice_boolean_and_autocomplete_open_their_list_in_the_same_spot`, `outdated_suggestions_are_dropped`, `send_runs_with_source_and_nonce`, `a_missing_required_option_focuses_it_and_does_not_send`, `backspace_on_empty_removes_the_chip`, `a_removed_command_gives_a_plain_note_and_keeps_the_draft` (Review Focus 3), `many_options_scroll_sideways`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.** The options row sits in the same card as `EditingBar` and `AttachmentChips` (`styles.card`), so the one-line slot and the list above it stay where they are.
- [ ] **Step 4: Run** typecheck and the tests, and see them pass.
- [ ] **Step 5: Commit** `feat(mobile): fill in and run a slash command`.

### Task 15: Replies on the phone

**Files:**
- Create: `mobile/src/features/chat/EphemeralNote.tsx`, `mobile/src/features/chat/ThinkingDots.tsx`
- Modify: `mobile/src/features/chat/items.ts`, `MessageList.tsx`, `MessageBubble.tsx`
- Test: `mobile/src/features/chat/__tests__/items.test.ts`, `mobile/src/features/chat/__tests__/MessageList.test.tsx`

- [ ] **Step 1: Write the failing tests:** `ephemeral_notes_come_after_the_messages`, `only_you_shows_its_label_and_dismisses`, `thinking_shows_the_plugin_and_dots`, `no_answer_shows_muted`, `a_reply_replaces_thinking_without_moving_the_list`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** typecheck and the tests, and see them pass.
- [ ] **Step 5: Commit** `feat(mobile): private replies and the thinking state`.

---

## Stage D: Finishing

### Task 16: Review, checks and the pull request

**Files:**
- Modify: `docs/decisions.md` (the next D number), `docs/protocol.md`

- [ ] **Step 1: Write the decision entry**: what changed, §25's calls as built, what was tested, what wasn't.
- [ ] **Step 2: Render and look.** Desktop popup at every breakpoint in light and dark against the approved prototype; phone on the emulator with the keyboard up. Fix what differs.
- [ ] **Step 3: Review the whole diff** against §26 with a fresh reviewer. Fix what is real.
- [ ] **Step 4: Run** `df -h /`, then `.github/check.sh`. Everything passes; a failure unrelated to this work is still reported.
- [ ] **Step 5: Push and open the pull request** with `gh pr create` into `feat/plugins` if #46 is still open, else `beta`. Its description says what changed, why and how it was tested. Owed by hand: Windows and macOS popup, keyboard, back gesture and haptics on real phones.
- [ ] **Step 6: Read every check** with `gh pr checks`, fix failures on the branch, re-run `.github/check.sh` before each push, and read `gh pr diff` once more. The owner merges.
