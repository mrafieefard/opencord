# Plugins, Milestone 2: Custom Channels — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A plugin registers a channel type; an admin makes a channel of it; each member who opens the channel sees the plugin's own UI, drawn natively by the desktop app from the plugin's client script, talking to the plugin's server script through named events.

**Architecture:**
- **`opencord-plugins`** (shared, MPL-2.0) gains the **block schema**, the **tree checks** and the **tree diff**, so the rules are one piece of Rust tested against fixture trees.
- **`opencord-core`** runs **one client VM per open view** on its own thread: it loads the plugin's client files (cached by content hash), runs `init`/`render`, diffs each new tree with the last and sends **JSON patches** to the renderer; the renderer only draws and reports input.
- **The server** keeps a registry of open views, carries events both ways with rate and byte limits, serves the plugin's client files and assets, its files (public, or by a per-view token), and images from allowed websites through its own fetcher.
- **`opencord_blocks`** (new Flutter package, `app/packages/opencord_blocks`) draws every block of §12 from the patches. It knows nothing of Riverpod, the repository or `flutter_rust_bridge`: the app gives it a host for icons, images, users and side effects, so the phone's embedded module (M4) can reuse it.

**Tech Stack:** Rust 2024, mlua 0.11 with Luau, axum 0.8, sqlx 0.9 (SQLite), prost, serde_json; Flutter with Riverpod 3 and flutter_rust_bridge 2.13; no new dependencies unless a task names one.

**Spec:** `docs/superpowers/specs/2026-11-07-plugins-design.md`. Read §1–§6, §8, §10.3–§10.4, §11–§14, §16, §19–§23 before Task 1, and the sections a task names before that task. "§" below means the spec. If the spec and this plan disagree, or a step can't be done as written, stop and ask the user.

## Global Constraints

- **Branch and pull request:** `feat/plugins-channels`, made by Claude from `feat/plugins-commands` (milestone 1.5); its first commit holds this plan. After Task 5, push it and open **one draft pull request into `beta`** so CI runs (its diff includes milestones 1 and 1.5 until #46 and #47 are merged; that's expected). Push at the end of every stage. It stays a draft until the Stage E review passes, then `gh pr ready`. Never merge. Never push to `beta`, `canary` or `main`.
- **Before Task 1:** `git switch feat/plugins-channels`; `git status --short` is empty; `git log -1 --format=%s` is `docs: plugins milestone 2 plan`; `df -h /` shows at least 10 GB free. If any fails, stop and ask the user.
- **Scope:** `crates/opencord-plugins/`, `crates/opencord-server/`, `crates/opencord-common/` (the channel kind), `crates/opencord-proto/`, `proto/`, `crates/opencord-core/`, `app/` including the new `app/packages/opencord_blocks/`, `crates/opencord-mobile/` (only to keep it compiling when the core's types grow; the phone gets nothing new), `crates/opencord-plugin-cli/` (the init template), `examples/plugins/` (new `tickets/`), `docs/`, `.github/check.sh` and `.github/workflows/cicd.yml` (the new package's tests only). Nothing else. The phone app (`mobile/`) is milestone 4: don't touch it.
- **Not in M2:** activities, `START_ACTIVITIES`, call events; the phone; message components; unread dots on plugin channels (§14.5); 3D; a tile map. `app.on_back` exists in the client API but nothing on desktop calls it yet (the phone's back gesture does, in M4).
- **Licences:** `opencord_blocks` is **MPL-2.0**, like the app. New dependencies must be MIT, Apache-2.0, BSD, ISC or Zlib on every target (`GEMINI.md`'s licence check).
- **Ids in Lua are decimal strings,** both sides. Every id from Lua must be a string of digits (`INVALID_ARGUMENT: <name> must be an id string`).
- **Errors raised into Lua** are `"<CODE>: <message>"` strings, as in M1, made through the API helpers (`api::function`, `raise`), never `mlua::Error::runtime` or `lua.create_function` directly. Grep that must come back empty in new files: `rg -n "create_function|Error::runtime" crates/opencord-core/src/views crates/opencord-server/src/plugins/api/ui.rs`.
- **Numbers from Lua** are checked for being finite before use (`NaN` and infinities raise `INVALID_ARGUMENT: <name> must be a finite number`); every duration, size and count from Lua has the limit this plan gives it.
- **Limits** (new constants in `opencord_plugins::limits` unless the server config gives them):

  | Limit | Value |
  |---|---|
  | Client VM memory / handler time / frame budget | 32 MiB / 50 ms / 8 ms |
  | Client script start (all of `require`'d files plus `init`) | 10 × 50 ms |
  | A render's tree | 20,000 blocks, 64 deep |
  | One string prop | 10,000 characters (`text`, `rich_text`: 20,000) |
  | Canvas items / points in one path | 20,000 / 10,000 |
  | A list's materialised range | 200 items |
  | Open views per session | 4 |
  | Events to a view | 60 a second **and** 512 KiB a second *(the byte limit is Claude's addition)* |
  | Events from a view | 30 a second **and** 128 KiB a second |
  | One event's data | 64 KiB of JSON |
  | Raw image bytes (`image { bytes }`) | 256 KiB |
  | Server-fetched image | 10 MiB, 40 megapixels; PNG, JPEG, WebP, GIF only |
  | Server image cache | 100 MiB per plugin, entries fresh for 1 hour |
  | File tokens | good for 1 hour |
  | Device storage | 1 MiB per plugin per server; keys at most 200 characters |
  | Client timers per view / shortest `every` | 100 / 0.1 s |
  | Channel types per plugin | 20 |
  | Quick dialog text | 2,000 characters |
  | View error reports | 1 per view every 10 s, at most 2,000 characters |

- **Words the user sees** are copied exactly from the task that gives them.
- **Never log** view event data, file contents, or tokens. Log ids, sizes and error kinds.
- **Generated files, never edited by hand:** `app/lib/src/rust/**`, `crates/opencord-core/src/frb_generated.rs`, `.sqlx/*.json`, `*.freezed.dart`.
- **The decisions entry (D95) is Claude's,** written at the Stage E review. Don't write one.

### The view wire format

One encoding serves `flutter_rust_bridge` now and the phone's platform channel in M4 (§24): **JSON text**. Every task that touches the core, the renderer or their tests uses exactly these shapes.

**A node** (core side, Task 1): `Node<H> { block: String, key: Option<String>, props: BTreeMap<String, serde_json::Value>, handlers: BTreeMap<String, H>, children: Vec<Node<H>> }`. `H` is `mlua::Function` in the core and `()` in tests. Handler names are the event names without `on_` (`press`, `change`).

**A batch** (core → renderer): `{"view": "<view id>", "seq": <u64>, "ops": [ … ]}`. Ops, tagged by `op`:

| `op` | Fields | Meaning |
|---|---|---|
| `create` | `id`, `block`, `key?`, `props`, `events` (names), `children` (ids) | A new node. Its children are created earlier in the same batch. |
| `props` | `id`, `set` (object), `unset` (names) | Changed props only |
| `events` | `id`, `events` | Its handler names changed |
| `children` | `id`, `children` | Its full child id list, in order, after adds, moves and removes |
| `drop` | `ids` | Nodes gone, whole subtrees listed; the renderer frees them |
| `roots` | `main`, `modals`: `[{"id", "name", "size", "sheet", "dismissible"}]` | What's drawn: the main tree, then named modals, bottom to top |
| `draw` | `id`, `set` (items), `remove` (item ids), `order?` (item ids) | A canvas's items changed |
| `dialog` | `id`, `kind` (`confirm`, `alert`, `prompt`), `text`, `default?` | A quick dialog |
| `frames` | `on` (bool) | Send `frame` inputs, or stop |
| `sound` | `id`, `asset`, `volume` | Play a sound from the plugin's assets |
| `copy` / `open_link` / `open_channel` | `text` / `url` / `channel_id` | Side effects; `open_link` is asked of the viewer first |
| `error` | `message`, `details?` | The view stopped; `details` (file, line, trace) only when allowed |
| `too_old` | | "Update Opencord to open this" (§11.7) |
| `closed` | `reason` (`plugin_stopped`, `lost_access`, `channel_deleted`, `unavailable`) | The server closed it |

**An input** (renderer → core), tagged by `kind`:

| `kind` | Fields |
|---|---|
| `event` | `node`, `event` (name without `on_`), `data` |
| `range` | `node`, `start`, `end` (a lazy list's visible items) |
| `dialog` | `id`, `result` (`true`/`false`, text, or `null`) |
| `modal_dismiss` | `name` |
| `frame` | `dt` (seconds), `seq` (the last batch seen) |
| `sound_end` | `id` |
| `context` | `width`, `height`, `theme` (`dark`/`light`), `input` (`mouse`/`touch`), `safe_area` `{top,right,bottom,left}`, `text_scale` |
| `lifecycle` | `event`: `pause`, `resume`, `focus`, `blur`, `connection_lost`, `connection_back` |
| `key` | `event` (`key_down`/`key_up`), `key`, `code`, `shift`, `ctrl`, `alt`, `meta`, `repeat` |

**Event data** for `event` inputs: presses `{}` (`secondary_press` `{x, y}`); `change` `{value}`; `submit` `{value}`; `focus`/`blur` `{}`; `scroll` `{offset, extent}`; `reach_start`/`reach_end` `{}`; `dismiss` `{}`; image `load` `{width, height}`, `error` `{message}`; `animation_end` `{}`; menu `select` `{value}`; picker `upload` `{name, size}`, `upload_error` `{message}`; canvas `pointer_down`/`_move`/`_up`/`_cancel` `{x, y, pointer, kind, buttons, pressure?}`, `wheel` `{x, y, dx, dy}`, `pan` `{x, y, dx, dy, phase}`, `scale` `{x, y, scale, phase}`, `stroke` `{stroke, points, done}` (`points` flat `[x, y, pressure, …]`). `phase` is `start`, `update` or `end`.

## Working Rules (read before Task 1)

`GEMINI.md` applies to everything here. Read "How to write code here" before each stage.

### Environment

Each shell command runs in a fresh shell. Start **every** `cargo`, `flutter`, `dart`, `sqlx` and `flutter_rust_bridge_codegen` command with `source .superpowers/sdd/media/env.sh &&`. Check `df -h /` before the full checks; below 10 GB free, follow `GEMINI.md`'s clean-up. Never run `cargo clean`.

### Commands

| What | Command, from the repository root |
|---|---|
| Format | `cargo fmt --all`; `cd app && dart format lib test`; `cd app/packages/opencord_blocks && dart format lib test` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Shared crate tests | `cargo test -p opencord-plugins` |
| Core view tests | `cargo test -p opencord_core --lib views` |
| One unit test | `cargo test -p <crate> --lib -- --exact <module::path::test_name>` |
| Server plugin tests | `cargo test -p opencord-server --test plugins` (one: add `-- --exact <module::test_name>`) |
| Renderer | `cd app/packages/opencord_blocks && flutter analyze && flutter test` |
| App | `cd app && flutter analyze && flutter test` |
| Before each push | `.github/check.sh` |
| Dart bindings, after any change under `crates/opencord-core/src/api/` | `cd app && flutter_rust_bridge_codegen generate` |
| sqlx data, after any server query or migration change | as in the M1 plan's Working Rules (`cargo sqlx prepare --workspace -- --all-targets` on a fresh dev database) |

### Task owners

Every task has an **Owner** and a **Score** (1–5: how much a mistake costs and how hard it is to see).

- **Owner: Gemini.** You implement it. Score 3 means read its **Review focus** line twice.
- **Owner: Claude.** Don't implement it. Hand the task to Claude with `delegate_task`, naming the task number and this plan's path, and wait for its report. Don't edit files or run git while it works: you share the working copy. Then run the task's **Checks** yourself and record it in the ledger with the report's commits.
- **Claude, when handed a task:** follow it exactly; each Claude task is written to be done from its own section plus the Global Constraints. Run its checks, finish with the commit it gives, then report the files, commits, checks and anything left undone.

| Task | Part | Score | Owner |
|---|---|---|---|
| 1 | Block schema and tree checks | 4 | Claude |
| 2 | The tree diff | 4 | Claude |
| 3 | The client VM | 5 | Claude |
| 4 | Modals, quick dialogs, lazy lists | 4 | Claude |
| 5 | Timers, frames, device storage, context | 4 | Claude |
| 6 | Protocol | 2 | Gemini |
| 7 | The plugin channel kind | 5 | Claude |
| 8 | Channel types and availability | 4 | Claude |
| 9 | Serving client files, assets and plugin files | 5 | Claude |
| 10 | Views on the server | 5 | Claude |
| 11 | Images from the internet | 5 | Claude |
| 12 | Uploads from viewers | 4 | Claude |
| 13 | Files in messages, plugin channels from Lua, server definitions | 3 | Gemini |
| 14 | Hot reload and view error reports | 3 | Gemini |
| 15 | The core: views over the network | 4 | Claude |
| 16 | `opencord_blocks`: patches, host, layout core | 4 | Claude |
| 17 | `opencord_blocks`: canvas | 4 | Claude |
| 18 | The app's model, repository and mock | 3 | Gemini |
| 19 | Blocks: layout and content | 3 | Gemini |
| 20 | Blocks: controls | 3 | Gemini |
| 21 | Blocks: lists and animation | 3 | Gemini |
| 22 | Blocks: overlays | 3 | Gemini |
| 23 | Desktop: plugin channels in the list, and creating them | 3 | Gemini |
| 24 | Desktop: the plugin area | 3 | Gemini |
| 25 | Client definitions, docs, the init template | 2 | Gemini |
| 26 | The ticket system example and its end-to-end tests | 2 | Gemini |

### Rules reviews will enforce

On top of `GEMINI.md`'s and the M1 plan's rules 1–13 (which still hold):

14. **Every test names, in a comment, the change to the code that makes it fail** ("fails if `close_views` skips lost access"). In the ledger, a "mutation" line has the pasted failing output of that change, or it isn't claimed.
15. **Tests drive the entry point that ships:** the gateway request, the HTTP route, the patch batch the renderer gets, the widget the user sees. A helper test doesn't count for the path that calls it.
16. **For an event that reaches a widget, the test names the road:** core output → repository stream → provider → widget. A fake whose events reach no provider proves nothing.
17. **Every in-memory map lists both restart and stop** (and session end, view close, plugin uninstall where they apply) as times its entries leave, and a test shows one of them.
18. **A text field's controller lives in a `State`,** is disposed, and is changed only when the value changed from outside.
19. **What a control shows is what it sends.** A default selection is sent if the user doesn't touch it.
20. **Every new screen and every block is rendered** in dark and light, at the normal and narrowest widths, and looked at in full (`GEMINI.md` → "What the user sees"). Goldens alone aren't looking.

### Progress ledger

Keep `.superpowers/sdd/plugins-m2/progress.md` (git-ignored; never `git add -f`), one entry per task in the M1 plan's format.

### When a step keeps failing

`GEMINI.md` → "When you're stuck: ask Claude", with question files in `.superpowers/sdd/plugins-m2/questions/`.

### Stages and their reviews

| Stage | Tasks | At its end |
|---|---|---|
| A — The view engine | 1–5 | Full checks; push; open the draft pull request; read its checks |
| B — Protocol | 6 | Full checks; push |
| C — Server | 7–14 | Full checks; push; **Claude reviews stage C** |
| D — Core and renderer | 15–22 | Full checks; push; **Claude reviews stage D** |
| E — Desktop and the example | 23–26 | Full checks; push; **Claude reviews stage E**; then stop and report to the user |

A review is a bridge task, "Review stage <X>, round <r> of docs/superpowers/plans/2026-11-09-plugins-m2-channels.md". Its report ends with `Stage <X> round <r>: PASS` or `FAIL`. On FAIL, fix every finding, run the full checks, push, and hand over the next round. Stop and ask the user after three failed rounds, or when a finding needs the user's decision.

## Review Focus

The inputs most likely to hurt real people while ordinary tests pass. Each has a pinned test in the task named.

1. **A member loses access to a plugin channel while its view is open** (a role removed, an overwrite added, kicked). Expected: the view closes at once with "You can no longer see this channel", and no event reaches it after. Pinned in Task 10 (`losing_access_closes_the_view`).
2. **The plugin restarts while people have it open** (a dev save, a crash). Expected: views stay open with their state, the new server VM hears `view_open` for each; a stop or a turn-off closes them with "This plugin stopped". Pinned in Task 10 (`views_survive_a_restart_and_the_new_vm_hears_them`, `stopping_closes_views`) and Task 14 (`a_dev_push_reloads_open_views_keeping_state`).
3. **A hostile or broken client script:** an endless `render`, a memory bomb, a tree of 100,000 blocks, a `NaN` width, a duplicate key. Expected: only that plugin area shows the error; chat, voice and the rest of the app keep working. Pinned in Task 1 (`a_nan_size_is_refused_with_its_path`, `too_many_blocks_is_refused`) and Task 3 (`an_endless_render_shows_the_error_and_the_app_goes_on`, `a_memory_bomb_stops_the_view_only`).
4. **The connection drops and comes back on a new session while a view is open.** Expected: the view keeps its state and reopens on the server; the plugin hears `view_open` again; nothing runs twice. Pinned in Task 15 (`a_new_session_reopens_open_views_keeping_state`).
5. **Reaching what isn't yours over HTTP:** a private plugin file by path, another member's file token, a server script through the assets route, `..` and percent-encoded paths, an image URL off the manifest's domains or resolving to a private address. Expected: refused, and nothing read from disk. Pinned in Task 9 (`server_scripts_are_never_served`, `a_private_file_needs_its_own_token`) and Task 11 (`a_domain_off_the_list_is_refused`, `a_name_resolving_to_a_private_address_is_refused`).

Also pinned, outside the five: the channels table rebuild losing nothing (Task 7, `the_rebuild_keeps_channels_overwrites_and_messages`).

## Claude's calls in this plan (for the user to check)

1. **JSON** is the patch and input encoding (§24 left it open).
2. **Handlers are addressed by node id and event name,** not a fresh id per redraw, so a tap that races a redraw still reaches the current function (§11.3 says "an id per redraw"; the effect it wants is the same).
3. **`app.listen(name, fn)`** receives the surroundings, lifecycle and keyboard events of §11.5 (`close`, `pause`, `resume`, `resize`, `theme_change`, `connection_change`, `focus`, `blur`, `key_down`, `key_up`); `app.on` stays for server events, so names can't clash.
4. **Too old vs. error:** a manifest `api` above the app's shows "Update Opencord to open this"; an unknown block in a plugin whose `api` the app supports is an error with the block's path (a typo), not "too old" (§11.3 and §11.7 read both ways).
5. **Views survive a server VM restart** (dev saves and crash restarts), and the new VM hears `view_open` for each; only stopping closes them.
6. **A named `ui` with both sides' functions** in the type definitions, each marked "Server only" or "Client only": luau-lsp has one set of globals per workspace.
7. **Client errors go to the plugin's log** (one per view every 10 s), so `opencord-plugin dev` shows them.
8. **Hot reload carries the state as JSON** between the old and new VM, so state must be plain data; functions in it are dropped, and the docs say so.
9. The **byte limits** on view events (512 KiB/s to, 128 KiB/s from).
10. **Prop names and the colour names** in Task 1's table (§24 left them open).

## File Map

### Shared (`crates/opencord-plugins/`)

| File | Task | Contents |
|---|---|---|
| `src/limits.rs` | 1 | the client constants above |
| `src/blocks.rs` | 1 | `BlockSpec`, `PropKind`, `BLOCKS`, colours |
| `src/view/{mod,tree}.rs` | 1 | `Node<H>`, `from_lua`, `TreeError` |
| `src/view/diff.rs`, `src/view/patch.rs` | 2 | `reconcile`, `Patch`, `Retained` |
| `tests/fixtures/patches/*.json` | 2 | fixture trees and their patches, replayed by Task 16 |

### Server (`crates/opencord-server/`)

| File | Task |
|---|---|
| `migrations/0016_plugin_channels.sql`, `src/db/channels.rs`, `src/guild.rs`, `src/handlers/channels.rs` | 7 |
| `src/plugins/channel_types.rs`, `src/plugins/api/ui.rs` (registration) | 8 |
| `src/plugins/assets.rs`, `src/plugins/http.rs` | 9 |
| `src/plugins/views.rs`, `src/handlers/views.rs`, `src/plugins/api/ui.rs` (views, sends), `src/plugins/api/files.rs` (`files.token`) | 10 |
| `src/plugins/images.rs` | 11 |
| `src/attachments/…` (view uploads), `src/plugins/api/files.rs` (`files.put` of an upload) | 12 |
| `src/plugins/api/messages.rs`, `src/plugins/api/guild.rs` | 13 |
| `src/plugins/dev.rs`, `src/handlers/views.rs` (error reports) | 14 |
| `tests/plugins/{channels,assets,views,images,uploads}.rs` | 7–14 |

### Core, app and elsewhere

| File | Task |
|---|---|
| `crates/opencord-core/src/views/{mod,runtime,api_app,api_ui,handlers}.rs` | 3 |
| `crates/opencord-core/src/views/{modals,dialogs,lists}.rs` | 4 |
| `crates/opencord-core/src/views/{timers,frames,storage,context}.rs` | 5 |
| `proto/opencord/v1/{plugins,models,requests,events,gateway}.proto`, `docs/protocol.md` | 6 |
| `crates/opencord-core/src/views/client.rs`, `src/api/views.rs`, `src/connection.rs` (features), `src/convert.rs` | 15 |
| `app/packages/opencord_blocks/` | 16, 17, 19–22 |
| `app/lib/core/model/{channel,plugins}.dart`, `core/repository/*`, `core/rust/*`, `core/mock/*`, `core/providers/plugin_views.dart` | 18 |
| `app/lib/features/channels/*`, `features/settings/server/channels_page.dart` | 23 |
| `app/lib/features/plugins/{plugin_area,blocks_host,plugin_icons}.dart`, `features/chat/chat_area.dart` | 24 |
| `crates/opencord-plugins/api/opencord.d.luau`, `docs/plugins.md`, `crates/opencord-plugin-cli/src/init.rs` | 13, 25 |
| `examples/plugins/tickets/`, `crates/opencord-server/tests/plugins/tickets.rs`, `crates/opencord-core/src/views/tests/tickets.rs` | 26 |

---

## Stage A — The view engine

### Task 1: Block schema and tree checks

**Owner:** Claude. **Score:** 4 (every hostile tree passes through here).

**Files:**
- Create: `crates/opencord-plugins/src/blocks.rs`, `src/view/mod.rs`, `src/view/tree.rs`
- Modify: `src/lib.rs`, `src/limits.rs`

**Interfaces:**
- Produces:
  - `pub enum PropKind { Number, Integer, Text, Bool, Color, Size, Insets, Choice(&'static [&'static str]), Options, Items, Function, Json }`. `Size` is a number, `"fill"` or `"fit"`; `Insets` a number or `{top, right, bottom, left}` or `{x, y}`; `Color` below; `Options` a list of `{value, label, icon?}`; `Items` canvas items (Task 2 diffs them); `Json` any plain data.
  - `pub enum Children { None, One, Many, Lazy }`.
  - `pub struct BlockSpec { pub name: &'static str, pub props: &'static [(&'static str, PropKind)], pub events: &'static [&'static str], pub children: Children }`; `pub const BLOCKS: &[BlockSpec]`; `pub fn block(name: &str) -> Option<&'static BlockSpec>`.
  - `pub const COLORS: &[&str]`: `text`, `text_secondary`, `text_muted`, `background`, `surface`, `elevated`, `border`, `accent`, `on_accent`, `selected`, `hover`, `danger`. A colour is one of these, `#rrggbb`, `#rrggbbaa`, or a gradient `{from, to, angle?}`.
  - `pub struct Node<H>` as in the wire format; `pub fn from_lua<H>(value: &mlua::Value, handler: impl Fn(mlua::Function) -> H) -> Result<Node<H>, TreeError>`. A block is a table whose metatable is the `UI_NODE` registry value Task 3's `ui.*` constructors set; anything else is `not a block`.
  - `pub struct TreeError { pub path: String, pub message: String }`, displayed `"{path}: {message}"`; the path names each step, `column > [2] row > [0] button`.
- The schema (common props on every block: `key`, `width`, `height`, `min_width`, `max_width`, `min_height`, `max_height` (Size), `flex` (Number), `padding` (Insets), `opacity`, `visible` (Bool), `tooltip`, `label` (Text), `animate`, `enter`, `exit` (Json, Task 21); containers `row`, `column`, `stack`, `grid`, `scroll`, `box`, `list` also take `background` (Color), `border` (`{width, color}`), `radius` (Number), `shadow` (`none`, `small`, `medium`, `large`); a child of a `stack` also takes `x`, `y`, `right`, `bottom`):

  | Block | Props | Events | Children |
  |---|---|---|---|
  | `row`, `column` | `gap`, `align` (`start`, `center`, `end`, `space_between`, `space_around`), `cross` (`start`, `center`, `end`, `stretch`), `wrap` | | Many |
  | `stack` | | | Many |
  | `grid` | `columns` (Integer), `min_column_width`, `gap` | | Many |
  | `scroll` | `direction` (`vertical`, `horizontal`) | `scroll`, `reach_start`, `reach_end` | One |
  | `box` | | `press`, `double_press`, `long_press`, `secondary_press`, `hover_enter`, `hover_exit` | One |
  | `spacer` | `size` | | None |
  | `divider` | `color`, `thickness` | | None |
  | `list` | `count` (Integer), `reverse`, `gap`, `item_height`, `item` (Function, not an event) | `scroll`, `reach_start`, `reach_end` | Lazy |
  | `text` | `text`, `size`, `weight` (`regular`, `medium`, `semibold`, `bold`), `italic`, `color`, `align` (`start`, `center`, `end`), `max_lines`, `selectable`, `font` | | None |
  | `rich_text` | `text` (message markdown) | | None |
  | `image` | one of `asset`, `file`, `file_token`, `url`, `attachment`, `bytes`; `fit` (`cover`, `contain`, `fill`, `none`), `radius`, `placeholder` (Color), `version` (Integer) | `load`, `error` | None |
  | `icon` | `name`, `size`, `color` | | None |
  | `user` | `id`, `size`, `show_name` | | None |
  | `badge` | `text`, `tone` (`neutral`, `accent`, `danger`) | | None |
  | `progress` | `value` (0–1, or absent for indeterminate), `style` (`bar`, `ring`) | | None |
  | `spinner` | `size` | | None |
  | `button` | `text`, `variant` (`primary`, `secondary`, `danger`, `plain`), `icon`, `loading`, `disabled` | `press`, `long_press` | None |
  | `icon_button` | `icon`, `disabled` | `press` | None |
  | `checkbox`, `switch` | `value` (Bool), `text`, `disabled` | `change` | None |
  | `radio` | `value` (Bool), `text`, `disabled` | `change` | None |
  | `slider` | `value`, `min`, `max`, `step`, `disabled` | `change`, `change_end` | None |
  | `select` | `value`, `options` (Options), `placeholder`, `disabled` | `change` | None |
  | `tabs`, `segmented` | `value`, `options` (Options) | `change` | None |
  | `text_input` | `value`, `placeholder`, `password`, `max_length`, `lines` (Integer, 1 default), `disabled` | `change`, `submit`, `focus`, `blur` | None |
  | `image_picker`, `file_picker` | `text`, `accept` (extensions, `file_picker` only) | `upload`, `upload_error` | None |
  | `modal` | `open`, `size` (`small`, `medium`, `large`, `full`), `sheet`, `dismissible`, `title` | `dismiss` | One |
  | `sheet` | `open`, `dismissible` | `dismiss` | One |
  | `menu` | `items` (Options; `danger` allowed per item) | `select` | One (the trigger) |
  | `tooltip` | `text` | | One |
  | `toast` | `text`, `open`, `duration` (seconds, 3 default) | `dismiss` | None |
  | `canvas` | `items` (Items), `ink` (`{color, width}`) | `pointer_down`, `pointer_move`, `pointer_up`, `pointer_cancel`, `wheel`, `pan`, `scale`, `stroke` | None |

  `button`'s `text` is §12.2's `label`, renamed so it doesn't clash with the common `label` (screen readers). Events are given in Lua as `on_<event>` (`on_press`).
- Checks, each an error with the path: unknown block; unknown prop (`unknown prop "colour"`); a prop of the wrong kind (`width must be a number, "fill" or "fit"`); non-finite numbers; a size outside 0–100,000; a font size outside 6–200; an `opacity` outside 0–1; a string over its limit; duplicate sibling keys (`two children have the key "a"`); children on a block that takes none, or more than one where it takes one; over 20,000 blocks (`the tree has more than 20000 blocks`) or 64 deep; an `image` with no source or two; `bytes` (a Lua string or `buffer`, sent as base64 in the prop) over 256 KiB; a `menu`, `select`, `tabs` or `segmented` with over 100 options.

- [ ] **Step 1: Write failing tests** in `view/tree.rs`, each building the tree with a stand-in `UI_NODE` metatable: `a_tree_converts_with_its_keys_props_and_handlers`, `an_unknown_block_is_refused_with_its_path`, `an_unknown_prop_is_refused`, `a_nan_size_is_refused_with_its_path`, `a_duplicate_key_is_refused`, `too_many_blocks_is_refused`, `too_deep_is_refused`, `a_plain_table_is_not_a_block`, `an_image_needs_exactly_one_source`, `colours_by_name_hex_and_gradient`, `every_block_in_the_design_table_is_in_the_schema` (the list of §12.2's block names, written out).
- [ ] **Step 2: Run** `cargo test -p opencord-plugins view::tree` and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** them: pass.
- [ ] **Step 5: Commit** `feat(plugins): the block schema and tree checks`.

**Checks:** `cargo test -p opencord-plugins`, clippy.

### Task 2: The tree diff

**Owner:** Claude. **Score:** 4 (a wrong diff draws a wrong screen everywhere).

**Files:** Create `crates/opencord-plugins/src/view/diff.rs`, `src/view/patch.rs`, `tests/fixtures/patches/*.json`, `tests/patches.rs`.

**Interfaces:**
- Consumes: Task 1's `Node<H>`.
- Produces:
  - `pub enum Patch` with one variant per op in the wire format, `#[serde(tag = "op", rename_all = "snake_case")]`; `pub struct Batch { pub view: String, pub seq: u64, pub ops: Vec<Patch> }`.
  - `pub struct Retained` (the last tree with ids, blocks, keys, props, event names, children; canvas items by id).
  - `pub fn reconcile<H>(prev: Option<&Retained>, next: Node<H>, ids: &mut IdSource) -> Reconciled<H>` with `pub struct Reconciled<H> { pub tree: Retained, pub ops: Vec<Patch>, pub handlers: Vec<(u64, String, H)> }`. `IdSource` hands out increasing `u64` from 1, never reused in a view.
- Algorithm (React's): siblings with keys match by key; siblings without keys match by position among the unkeyed; a match with a different block is replaced (new id, old subtree dropped). Ops come in this order: `create` (children before parents), `props`, `events`, `draw`, `children`, `drop`. A parent gets a `children` op only when its child id list changed. A canvas's `items` aren't sent as a prop: they're diffed by item `id` into `draw`; `order` only when the order changed. Identical trees give no ops.
- Fixtures: each file is `{"before": <tree or null>, "after": <tree>, "ops": [...]}` with trees in the wire format's node shape (no handlers; `events` as names). Write at least: `first_render`, `append`, `insert_middle_keyed`, `move_keyed`, `remove_with_subtree`, `prop_change`, `prop_unset`, `same_key_new_block`, `unkeyed_by_position`, `events_change`, `canvas_items`, `identical`. Task 16 replays them in Dart, so they're the contract between the two sides.

- [ ] **Step 1: Write failing tests:** `tests/patches.rs` reads every fixture and asserts `reconcile` gives exactly its `ops`; `applying_the_ops_rebuilds_the_tree` applies the ops with a small reference applier (in the test file) to the `before` document and compares with `after`, over every fixture **and** 1,000 random tree pairs from a seeded generator (a fixed `u64` seed, xorshift; no new dependency); `ids_are_never_reused`; `handlers_come_back_by_node_id_and_event`.
- [ ] **Step 2: Run** `cargo test -p opencord-plugins --test patches` and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** them: pass.
- [ ] **Step 5: Commit** `feat(plugins): the tree diff and its patches`.

**Checks:** `cargo test -p opencord-plugins`, clippy.

### Task 3: The client VM

**Owner:** Claude. **Score:** 5 (a thread per view, budgets, hostile scripts on the member's own device).

**Files:** Create `crates/opencord-core/src/views/{mod,runtime,api_app,api_ui,handlers}.rs`. Modify `crates/opencord-core/src/lib.rs`, `crates/opencord-plugins/src/limits.rs` (if Task 1 didn't add every client constant).

**Interfaces:**
- Consumes: `opencord_plugins::sandbox::{Vm, VmLimits, Budget, paused}`, Tasks 1–2.
- Produces:
  - `pub struct ViewSpec { pub view_id: String, pub plugin_id: String, pub entry: String, pub sources: BTreeMap<String, String>, pub context: ViewContext, pub viewer: Viewer, pub place: Place, pub show_errors: bool }`; `sources` maps `client/…` paths to source text.
  - `pub trait ViewSink: Send + Sync { fn batch(&self, json: String); fn send(&self, name: String, data_json: String); fn report_error(&self, text: String); }`.
  - `pub fn start(spec: ViewSpec, sink: Arc<dyn ViewSink>) -> ViewHandle`; `ViewHandle::input(&self, json: &str)`, `::server_message(&self, name: String, data_json: String)`, `::reload(&self, entry: String, sources: BTreeMap<String, String>)` (Task 15 uses it; keep state per §11.6 and Claude's call 8), `::close(&self)` (runs `close` listeners within the budget, then the thread ends; joining takes at most 1 s).
  - One OS thread per view with a current-thread tokio runtime; inputs on an unbounded channel processed in order; the VM never runs on the caller's thread.
- Lua (§11.2–§11.4): globals `init(ctx)`, `render(state)`, optional `migrate(old)`; `ui.<block>(props, children?)` for every block in `BLOCKS`, returning a table with the `UI_NODE` metatable; `app.set(fn)` (runs `fn(state)` on the live state table and marks the view dirty), `app.state` (read), `app.send(name, data)` (`json.encode`d; over 64 KiB raises `"PLUGIN_LIMIT: an event is at most 64 KiB"`; name `[a-z0-9_.:-]{1,64}`), `app.on(name, fn)`; `app.copy(text)` only while a press handler runs (`press`, `double_press`, `long_press`, `secondary_press`), else `"FORBIDDEN: app.copy works only while handling a press"`; `require` loads `client/<name>.luau`/`.lua` once; no `http`, `db`, `files`, `hooks`, `timer` (the server's), `io`, `os.execute`.
- Redraw (§11.3): after each input or server message, if anything called `app.set`, call `render(state)` once, convert (Task 1), reconcile (Task 2), store the handlers by `(node id, event)`, and send one batch. The first batch follows `init`. An input for a `(node, event)` with no handler is ignored.
- Every handler runs as an async Lua thread with the 50 ms budget; `init` plus every `require` has 500 ms. An error in `init` or `render`, or a tree error, sends an `error` op (`message` "This plugin hit an error"; `details` the Lua error and traceback only when `show_errors`) and the view stops running Lua; an error in any other handler is reported and the view goes on. Every error is passed to `report_error` (Task 15 rate-limits the sending).

- [ ] **Step 1: Write failing tests** in `views/runtime.rs` with a recording `ViewSink`:
  - `init_then_render_sends_the_first_tree`;
  - `set_in_a_handler_redraws_once` (two `app.set` calls, one batch);
  - `a_press_reaches_its_handler_by_node_and_event`;
  - `a_handler_from_an_old_redraw_is_gone`;
  - `an_endless_render_shows_the_error_and_the_app_goes_on` (the `error` op arrives within 1 s; a second view on another thread keeps answering);
  - `a_memory_bomb_stops_the_view_only`;
  - `the_view_has_no_network_files_or_server_api`;
  - `require_loads_client_files_only` (`require("../server/main")` fails);
  - `send_over_64_kib_raises`;
  - `copy_outside_a_press_raises_and_inside_sends_a_copy_op`;
  - `errors_hide_their_details_unless_allowed`;
  - `closing_runs_close_listeners_and_ends_the_thread`.
- [ ] **Step 2: Run** `cargo test -p opencord_core --lib views` and see them fail.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** them: pass.
- [ ] **Step 5: Commit** `feat(core): the client VM`.

**Checks:** `cargo test -p opencord_core --lib views`, clippy.

### Task 4: Modals, quick dialogs, lazy lists

**Owner:** Claude. **Score:** 4 (coroutines paused across inputs; state that must never leak between modals).

**Files:** Create `crates/opencord-core/src/views/{modals,dialogs,lists}.rs`. Modify `views/runtime.rs`, `views/api_app.rs`.

**Interfaces:**
- Lua (§11.4, §12.5): `app.modals[name] = fn(state, props)`; `app.modal.open(name, props) → handle` (`handle:close(result?)`, `handle:update(props)`); `app.modal.close(arg?)`: a string naming an open modal closes that one (its result `nil`), anything else closes the top one with `arg` as its result (so results that are strings go in a table; the docs say so), `app.modal.close_all()`, `app.modal.is_open(name)`, `app.modal.ask(name, props) → result` (pauses the handler until that modal closes; dismissing gives `nil`); `app.confirm(text) → bool`, `app.alert(text)`, `app.prompt(text, default?) → string?` (each pauses until the `dialog` input; text over 2,000 characters raises).
- Each redraw renders every open modal with `fn(state, props)` after the main tree, each diffed against its own last tree, and a `roots` op whenever the stack changed. Size defaults `medium`, `dismissible` true, `sheet` false; a `modal_dismiss` input for a non-dismissible modal is ignored. Opening a name that's already open moves it to the top with the new props.
- Lazy lists (`list` with `count` and `item(i)`): the core keeps each list's range, starting at items 0–29 (or the last 30 when `reverse`); a `range` input sets it (cut to 200 items) and redraws; items are rendered by calling `item(i)` (0-based) during `render`, keyed `#<i>` unless `item` gives a key; an error in `item` is a render error with the list's path.
- Maps and when entries leave: open modals (on close, `close_all`, and view close); paused dialogs (on their answer or view close, which ends their coroutine); list ranges (when the list's node is dropped).

- [ ] **Step 1: Write failing tests:** `opening_a_named_modal_adds_a_root_drawn_with_the_state`, `a_modal_redraws_when_the_state_changes`, `ask_returns_what_close_gave`, `dismissing_resolves_ask_with_nil`, `a_non_dismissible_modal_ignores_dismiss`, `confirm_pauses_the_handler_until_answered`, `an_answer_after_close_is_dropped`, `a_list_renders_only_its_range`, `a_range_over_200_is_cut`, `reverse_starts_at_the_end`, `a_dropped_list_forgets_its_range`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(core): modals, quick dialogs and lazy lists in views`.

**Checks:** `cargo test -p opencord_core --lib views`, clippy.

### Task 5: Timers, frames, device storage, context

**Owner:** Claude. **Score:** 4 (timers and frames on a real clock; files on the member's disk).

**Files:** Create `crates/opencord-core/src/views/{timers,frames,storage,context}.rs`. Modify `views/runtime.rs`, `views/api_app.rs`.

**Interfaces:**
- Lua: `app.after(seconds, fn)`, `app.every(seconds, fn)` → timer, `app.cancel(timer)`; at most 100 per view (`"PLUGIN_LIMIT: at most 100 timers"`); `every` at least 0.1 s; timers don't fire while paused (`pause` input) and resume after.
- `app.on_frame(fn(dt))`, `app.off_frame()`: sends `frames { on }`; each `frame` input runs `fn` with the 8 ms budget then redraws; a `frame` input whose `seq` is older than the last batch sent is skipped, so a slow renderer never queues frames. Paused while hidden. Over budget: reported, and frames stop (`frames { on = false }`).
- `app.storage.get(key)`, `.set(key, value)`, `.delete(key)`: plain JSON values in `<core data dir>/plugins/<server key hash>/<plugin id>/storage.json`, written aside and renamed; over 1 MiB raises `"PLUGIN_LIMIT: this plugin's device storage is full"`; keys 1–200 characters. `ViewSpec` gains `storage_dir: PathBuf`.
- Context (§11.4): `app.viewer` (`id`, `name`, `roles` ids), `app.place` (`{ kind = "channel", channel_id }`), `app.size`, `app.theme`, `app.platform` (`"desktop"`), `app.breakpoint` (from the width: under 600 `compact`, under 840 `medium`, else `expanded`), `app.input`, `app.safe_area`, `app.text_scale`; updated by `context` inputs.
- `app.listen(name, fn)` (Claude's call 3) for `close`, `pause`, `resume`, `resize` (`{width, height, breakpoint}`), `theme_change` (`theme`), `connection_change` (`{connected}`), `focus`, `blur`, `key_down`, `key_up` (the `key` input's fields); any other name raises `"INVALID_ARGUMENT: no view event called <name>"`. `app.on_back(fn)` is stored; nothing calls it in M2.
- `app.open_link(url)` (http and https only, at most 2,000 characters), `app.open_channel(id)`, `app.play_sound(asset, { volume?, on_end? })` (an `assets/` path; volume 0–1, default 1; `on_end` runs on the `sound_end` input): each sends its op.
- Maps and when entries leave: timers (fired `after`, `cancel`, view close), sounds waiting for `sound_end` (on it, or view close).

- [ ] **Step 1: Write failing tests:** `after_fires_once_and_every_repeats` (with tokio's paused clock), `timers_wait_while_paused`, `a_101st_timer_raises`, `a_nan_delay_raises`, `frames_turn_on_and_off`, `a_stale_frame_is_skipped`, `a_slow_frame_stops_frames`, `storage_round_trips_and_survives_a_new_view`, `full_storage_raises`, `breakpoints_follow_the_width` (599 → compact, 600 → medium, 840 → expanded), `listen_refuses_unknown_names`, `open_link_refuses_other_schemes`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(core): timers, frames, device storage and context in views`.

**Checks:** `cargo test -p opencord_core --lib views`, clippy. Then the stage's full checks, push, and open the draft pull request (Global Constraints).

---

## Stage B — Protocol

### Task 6: Protocol

**Owner:** Gemini. **Score:** 2.

**Files:** Modify `proto/opencord/v1/{plugins,models,requests,events,gateway}.proto`, `crates/opencord-proto/tests/plugins.rs`, `docs/protocol.md`, and every Rust struct literal the new fields break (fill them with `..Default::default()` or the field's default; no behaviour).

- [ ] **Step 1: Add, exactly as written:**

`models.proto`: in `ChannelKind`, `CHANNEL_KIND_PLUGIN = 4;`. In `Channel`:
```proto
  // Plugin channels only (design 2026-11-07 §14).
  optional string plugin_id = 11;
  optional string type_key = 12;
  PluginChannelState plugin_state = 13;
```
and after `Channel`:
```proto
// Whether a plugin channel can be opened (§14.4).
enum PluginChannelState {
  PLUGIN_CHANNEL_STATE_UNSPECIFIED = 0;
  PLUGIN_CHANNEL_STATE_AVAILABLE = 1;
  // Its plugin isn't running, or isn't installed.
  PLUGIN_CHANNEL_STATE_PLUGIN_STOPPED = 2;
  // Its plugin runs but no longer registers this type.
  PLUGIN_CHANNEL_STATE_TYPE_GONE = 3;
}
```

`plugins.proto`, at the end:
```proto
// A kind of custom channel a running plugin registered (§14.1).
message PluginChannelType {
  string plugin_id = 1;
  // [a-z0-9_], 1 to 40.
  string type_key = 2;
  string name = 3;
  string description = 4;
  // A path under the plugin's assets/, or empty.
  string icon = 5;
  string plugin_name = 6;
}

// A file of a plugin's package a view may load: under client/ or assets/.
message PluginFileRef {
  string path = 1;
  // Lowercase hex SHA-256.
  string sha256 = 2;
  uint64 size_bytes = 3;
}

// Where a view is open. Activity sessions join in milestone 3.
message ViewPlace {
  oneof kind {
    int64 channel_id = 1;
  }
}

// -> view_opened. VIEW_CHANNEL in an available plugin channel; the session
// lists "plugin_channels"; at most 4 open views per session.
message OpenView {
  ViewPlace place = 1;
  // "desktop" or "phone".
  string platform = 2;
}

message ViewOpened {
  int64 view_id = 1;
  string plugin_id = 2;
  // The manifest's api.
  uint32 api = 3;
  // The manifest's client entry, like "client/main.luau".
  string entry = 4;
  repeated PluginFileRef files = 5;
  // Errors show their file, line and trace: the viewer manages plugins or
  // holds a live dev token for this plugin.
  bool show_errors = 6;
}

// -> ack. One's own view.
message CloseView {
  int64 view_id = 1;
}

// -> ack. 30 a second and 128 KiB a second per view; data at most 64 KiB.
message SendViewEvent {
  int64 view_id = 1;
  // [a-z0-9_.:-], 1 to 64.
  string name = 2;
  string data_json = 3;
}

// -> ack. An error in a view's Lua, for the plugin's log: one per view
// every 10 seconds, at most 2,000 characters.
message ReportViewError {
  int64 view_id = 1;
  string text = 2;
}

// A server script's event to one view.
message ViewMessage {
  int64 view_id = 1;
  string name = 2;
  string data_json = 3;
}

enum ViewCloseReason {
  VIEW_CLOSE_REASON_UNSPECIFIED = 0;
  VIEW_CLOSE_REASON_PLUGIN_STOPPED = 1;
  VIEW_CLOSE_REASON_LOST_ACCESS = 2;
  VIEW_CLOSE_REASON_CHANNEL_DELETED = 3;
  VIEW_CLOSE_REASON_UNAVAILABLE = 4;
}

// The server closed a view.
message ViewClosed {
  int64 view_id = 1;
  ViewCloseReason reason = 2;
}

// A plugin's client files changed (an update or a dev push): open views
// load them again, keeping their state (§11.6).
message ViewScriptsUpdate {
  string plugin_id = 1;
  string entry = 2;
  repeated PluginFileRef files = 3;
}

message PluginChannelTypesUpdate {
  repeated PluginChannelType types = 1;
}
```

`requests.proto`: `CreateChannel` gains `optional string plugin_id = 5; optional string type_key = 6;` (both, for `CHANNEL_KIND_PLUGIN` only). `CreateUpload` gains
```proto
  // Instead of a channel: an upload for a plugin view's picker (§13.3).
  optional int64 view_id = 8;
```
`Request.kind` gains `OpenView open_view = 224; CloseView close_view = 225; SendViewEvent send_view_event = 226; ReportViewError report_view_error = 227;`. `Response.result` gains `ViewOpened view_opened = 50;`.

`events.proto`: `Event.kind` gains `ViewMessage view_message = 136; ViewClosed view_closed = 137; ViewScriptsUpdate view_scripts_update = 138; PluginChannelTypesUpdate plugin_channel_types_update = 139;`.

`gateway.proto`: `Ready` gains `repeated PluginChannelType plugin_channel_types = 25;`; the `features` comment lists `"plugin_channels"`.

- [ ] **Step 2: Write a round-trip test** in `crates/opencord-proto/tests/plugins.rs` for `OpenView`, `ViewOpened` and `ViewScriptsUpdate`, and run it.
- [ ] **Step 3: Build the workspace** (`cargo check --workspace --all-targets`) and fix the struct literals; `flutter_rust_bridge_codegen generate` if the core's API types changed (they shouldn't).
- [ ] **Step 4: `docs/protocol.md`:** the new requests, events and fields, one line each, in the plugins section.
- [ ] **Step 5: Commit** `feat(proto): plugin channels and views`.

---

## Stage C — Server

### Task 7: The plugin channel kind

**Owner:** Claude. **Score:** 5 (rebuilding the `channels` table; every older app must keep working).

**Files:** Create `crates/opencord-server/migrations/0016_plugin_channels.sql`, `tests/plugins/channels.rs`. Modify `crates/opencord-common/src/channel.rs`, `crates/opencord-server/src/{guild.rs, db/channels.rs, handlers/channels.rs, state.rs, gateway/identify.rs}`, `.sqlx/`.

**Interfaces:**
- `ChannelKind::Plugin` (`"plugin"` in the database). `channels` gains `plugin_id TEXT` and `type_key TEXT`, with `CHECK ((kind = 'plugin') = (plugin_id IS NOT NULL AND type_key IS NOT NULL))`. The old `CHECK (kind IN (…))` can't be altered in SQLite: the migration rebuilds the table by SQLite's documented twelve steps. It starts with `-- no-transaction`, turns foreign keys off for its connection, copies every row, recreates every index and trigger on `channels`, runs `PRAGMA foreign_key_check` (a row there fails the migration), and turns foreign keys back on. Nothing that references `channels` may lose a row (`ON DELETE CASCADE` must not fire).
- The guild's channel gains `plugin: Option<PluginChannelRef { plugin_id, type_key }>`; `to_proto` fills `plugin_id`, `type_key` and `plugin_state` (Task 8 computes the state; here it's `PLUGIN_STOPPED`).
- `CreateChannel` with `CHANNEL_KIND_PLUGIN` needs `plugin_id` and `type_key` (`INVALID_ARGUMENT` otherwise); Task 8 adds the registry check. A plugin channel holds no messages or voice: sending, typing, fetching history and joining it are refused like a category.
- **Older apps:** sessions that don't list `"plugin_channels"` never see a plugin channel: not in `Ready`, not in channel create/update/delete or reorder events, not in permission overwrite events for one. Add `Audience` filtering by feature where channel events are broadcast, and filter `Ready`'s channels. The core's `Identify` doesn't list the feature until Task 15, so today's apps see nothing new.

- [ ] **Step 1: Write failing tests:** `the_rebuild_keeps_channels_overwrites_and_messages` (a database at migration 15 with channels, overwrites, messages, attachments and read states, migrated; every row equal; `PRAGMA foreign_key_check` empty), `a_plugin_channel_needs_its_plugin_and_type`, `a_plugin_channel_takes_no_messages`, `an_older_app_never_sees_a_plugin_channel` (Ready and events, through the gateway, with and without the feature), `a_plugin_channel_round_trips_through_the_guild`.
- [ ] **Step 2: Run** them and see them fail.
- [ ] **Step 3: Implement**, regenerate the sqlx data.
- [ ] **Step 4: Run** them and the full server tests: pass.
- [ ] **Step 5: Commit** `feat(server): the plugin channel kind`.

**Checks:** `cargo test -p opencord-server --test plugins channels::`, `cargo test -p opencord-server`, clippy.

### Task 8: Channel types and availability

**Owner:** Claude. **Score:** 4 (state kept in step across plugin starts, stops and installs).

**Files:** Create `crates/opencord-server/src/plugins/channel_types.rs`, `src/plugins/api/ui.rs`. Modify `src/plugins/{mod,runtime,install}.rs`, `src/plugins/api/mod.rs`, `src/handlers/channels.rs`, `src/gateway/identify.rs`, `tests/plugins/channels.rs`.

**Interfaces:**
- Lua: `ui.register_channel_type(key, { name, description?, icon? })` with `create_channel_types`; `key` `^[a-z0-9_]{1,40}$`; `name` 1–60 characters, `description` up to 200, `icon` an existing `assets/` file ending `.png`, `.jpg`, `.jpeg` or `.webp`; registering a key again replaces it; over 20 raises `"PLUGIN_LIMIT: at most 20 channel types"`; logged as an action (`"registered channel type {key}"`).
- `ChannelTypes` registry keyed `(plugin_id, type_key)`. Entries leave when the plugin's VM stops **or restarts** (a restarted plugin registers again as it starts) and when it's uninstalled.
- `plugin_state(channel)`: `AVAILABLE` when the plugin runs and registered the type; `TYPE_GONE` when it runs without it; `PLUGIN_STOPPED` otherwise. Worked out again, and a `ChannelUpdate` sent to feature sessions for each plugin channel whose state changed, when: a plugin's start finishes, it stops or restarts, a type is registered, it's installed or uninstalled. A restart that registers the same types sends nothing.
- `PluginChannelTypesUpdate` to feature sessions whenever the list changes; `Ready.plugin_channel_types` lists them.
- `CreateChannel` with a plugin kind needs a registered `(plugin_id, type_key)`: else `INVALID_ARGUMENT: that channel type isn't available`. The owning plugin gets `plugin_channel_create` (`{ channel }`) and `plugin_channel_delete` (`{ channel_id }`), whatever its permissions.

- [ ] **Step 1: Write failing tests:** `a_registered_type_is_listed_to_feature_sessions`, `a_channel_of_an_unregistered_type_is_refused`, `stopping_the_plugin_makes_its_channels_unavailable_and_starting_brings_them_back` (state through events; the channel's id, position and overwrites unchanged), `a_version_without_the_type_shows_type_gone`, `a_restart_with_the_same_types_sends_nothing`, `the_plugin_hears_its_channels_created_and_deleted`, `a_bad_key_icon_or_name_raises`, `the_21st_type_raises`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(server): plugin channel types and availability`.

**Checks:** `cargo test -p opencord-server --test plugins channels::`, clippy.

### Task 9: Serving client files, assets and plugin files

**Owner:** Claude. **Score:** 5 (HTTP paths onto the disk; private data).

**Files:** Create `crates/opencord-server/src/plugins/assets.rs`, `tests/plugins/assets.rs`. Modify `src/plugins/http.rs`, `src/plugins/mod.rs`.

**Interfaces:**
- `pub fn client_files(handle) -> Vec<proto::PluginFileRef>`: every file under `client/` and `assets/` of the installed version, with its SHA-256 and size, worked out once per installed version.
- Routes, each needing the viewer's media token (as `/media/…` does), a member of the server:
  - `GET /plugins/{id}/assets/{*path}`: only `client/…` and `assets/…` of the installed package, by the package path rules; never `manifest.lua`, `server/…`, `SIGNATURE` or anything else; `ETag` the SHA-256; with `?v=<sha256>` matching, `Cache-Control: public, max-age=31536000, immutable`.
  - `GET /plugins/{id}/files/{*path}`: public plugin files only (§13.1); a private one is a 404, the same as a missing one.
  - `GET /plugins/{id}/files/t/{token}`: a file through a token.
- `FileTokens::issue(plugin_id, path, user_id) -> String`: a random 32-byte token (URL-safe base64) good for one hour, bound to the plugin, the path and the user; the route refuses it for any other user's media token. Tokens leave the map when they expire (a sweep every minute), when the plugin is uninstalled, and when the server restarts (memory only). Task 10 puts the Lua `files.token(path, view)` on top, since views arrive there.
- Content types from the extension, with `X-Content-Type-Options: nosniff`; anything not an image, a font (`.ttf`, `.otf`), audio (`.ogg`, `.mp3`, `.wav`) or Luau source is `application/octet-stream`.

- [ ] **Step 1: Write failing tests:** `client_files_list_hashes_and_sizes`, `server_scripts_are_never_served` (`server/main.luau`, `manifest.lua`, `SIGNATURE`, each a 404), `dotdot_and_encoded_paths_are_refused` (`..%2F`, `%2e%2e/`, `a//b`, `/etc/passwd`), `assets_need_a_media_token`, `a_public_file_is_served`, `a_private_file_needs_its_own_token` (by path: 404; a token issued for another member: 403 or 404), `a_token_expires_after_an_hour` (with the clock the plugins service already uses for tests), `a_versioned_asset_is_immutable`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(server): serve plugin client files, assets and files`.

**Checks:** `cargo test -p opencord-server --test plugins assets::`, clippy.

### Task 10: Views on the server

**Owner:** Claude. **Score:** 5 (access checks on a long-lived stream; rate and byte limits; restarts).

**Files:** Create `crates/opencord-server/src/plugins/views.rs`, `src/handlers/views.rs`, `tests/plugins/views.rs`. Modify `src/handlers/mod.rs`, `src/plugins/{mod,runtime,events}.rs`, `src/plugins/api/ui.rs`, `src/gateway/session.rs` (closing on session end), `src/plugins/api/plugin.rs` (`KNOWN_EVENTS` already lists `view_open`/`view_close`).

**Interfaces:**
- `Views` registry by view id: plugin id, user id, session id, place, platform, and its two token buckets (to and from). Entries leave on `CloseView`, on session end, and when the server closes the view.
- `OpenView`: the place is a plugin channel, `AVAILABLE`, the user has `VIEW_CHANNEL` there, the session lists `"plugin_channels"`, fewer than 4 views open on the session (`PLUGIN_LIMIT: too many open views`); answers `ViewOpened` (Task 9's `client_files`, the manifest's `api` and `client` entry; `show_errors` when the user holds `MANAGE_PLUGINS` or a live dev token for the plugin). A plugin without a `client` entry: `PLUGIN_UNAVAILABLE: this plugin has no screen`. The plugin gets `view_open` with `{ view }`.
- `CloseView` (own views only; another session's view is `NOT_FOUND`) and session end: the plugin gets `view_close` with `{ view, reason }` (`closed`, `disconnected`).
- The server closes a view, sending `ViewClosed` and `view_close`, when: the user loses `VIEW_CHANNEL` there (a role, member or overwrite change, kick, ban: `LOST_ACCESS`, plugin reason `lost_access`); the channel is deleted (`CHANNEL_DELETED`); the channel becomes unavailable (`UNAVAILABLE`); the plugin stops, is turned off or uninstalled (`PLUGIN_STOPPED`, plugin reason `plugin_stopping`). **A VM restart keeps views open** (Claude's call 5): once the new VM's start finishes, it gets `view_open` for each open view.
- `SendViewEvent`: own view; 30 a second and 128 KiB a second (else `RATE_LIMITED`); data valid JSON, at most 64 KiB; name `[a-z0-9_.:-]{1,64}`; delivered to the plugin's `ui.on(name, fn(view, data))` handlers through a `view_event` branch in `dispatch`. `ui.on` with a name outside that pattern raises.
- Lua: `files.token(path, view) → string` (§13.1), through Task 9's `FileTokens` for the view's user; the path must be an existing private file (`NOT_FOUND: no plugin file at <path>`).
- Lua (§10.3): `ui.views(place?)` → view tables `{ id, user, place, platform }` with a `send(name, data)` method; `ui.send_place(place, name, data)`, `ui.send_role(role_id, name, data)` (views whose user holds the role), `ui.send_all(name, data)`, each returning how many views it reached. `place` is `{ channel_id = "…" }`. Sending is 60 a second and 512 KiB a second per view: `view:send` over it raises `"RATE_LIMITED: too many events to this view"`; the broadcasts skip a limited view and log one warning a minute. Data over 64 KiB raises `"PLUGIN_LIMIT: an event is at most 64 KiB"`.

- [ ] **Step 1: Write failing tests** (gateway-level, two members):
  - `opening_a_view_answers_its_files_and_tells_the_plugin`;
  - `an_unavailable_or_hidden_channel_cant_be_opened`;
  - `a_fifth_view_on_a_session_is_refused`;
  - `events_go_both_ways` (client `SendViewEvent` → `ui.on`; `view:send` → `ViewMessage`);
  - `views_of_another_session_cant_be_used`;
  - `losing_access_closes_the_view` (Review Focus 1: an overwrite denying `VIEW_CHANNEL`; then a send to the view reaches nobody);
  - `deleting_the_channel_closes_its_views`;
  - `views_survive_a_restart_and_the_new_vm_hears_them` (Review Focus 2);
  - `stopping_closes_views` (Review Focus 2);
  - `the_31st_event_in_a_second_is_refused`, `bytes_over_the_limit_are_refused` (both directions);
  - `send_role_reaches_only_holders`;
  - `a_session_ending_closes_its_views`;
  - `a_file_token_works_for_its_views_user_only`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(server): plugin views`.

**Checks:** `cargo test -p opencord-server --test plugins views::`, clippy.

### Task 11: Images from the internet

**Owner:** Claude. **Score:** 5 (the server fetching for members; SSRF; decoding untrusted images).

**Files:** Create `crates/opencord-server/src/plugins/images.rs`, `tests/plugins/images.rs`. Modify `src/plugins/http.rs`.

**Interfaces:**
- `GET /plugins/{id}/images?url=<percent-encoded>` with a member's media token: the plugin holds `internet`; the URL is `https`, its host on the manifest's `domains` (the same matching as `http.request`); fetched with the plugin's `net.rs` client, so every address and redirect check applies; at most 10 MiB; the first bytes must be a PNG, JPEG, WebP or GIF signature (SVG and everything else refused); the header's width × height at most 40 megapixels (read the header without decoding the pixels).
- Served with the type found, `X-Content-Type-Options: nosniff`, and cached on disk at `data/plugins/<id>/image-cache/<sha256 of url>` for an hour; at most 100 MiB per plugin, oldest first out. Entries also leave on uninstall (the plugin's folder). Concurrent fetches of one URL wait for the first.
- Uncached fetches are rate limited per member: 20 every 10 s (`RATE_LIMITED`).

- [ ] **Step 1: Write failing tests** against a local fake image server allowed through the test net guard: `an_allowed_png_is_served_and_cached` (the fake counts one request for two members), `a_domain_off_the_list_is_refused`, `a_name_resolving_to_a_private_address_is_refused` (with the real guard), `an_svg_is_refused`, `too_many_pixels_is_refused`, `over_10_mib_is_refused`, `without_internet_the_route_refuses`, `the_cache_stays_under_its_cap`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(server): plugin images from allowed websites`.

**Checks:** `cargo test -p opencord-server --test plugins images::`, clippy.

### Task 12: Uploads from viewers

**Owner:** Claude. **Score:** 4 (member files entering a plugin's storage).

**Files:** Create `tests/plugins/uploads.rs`. Modify `crates/opencord-server/src/attachments/…` (where `CreateUpload` and finishing live), `src/plugins/api/files.rs`.

**Interfaces:**
- `CreateUpload` with `view_id` (and `channel_id` 0): the view is the session's; the plugin holds `accept_uploads`; the server's upload size limit and blocked extensions apply as for messages, and the upload counts as the member's. When it finishes, the plugin gets `file_uploaded` with `{ view, file = { id, name, size, content_type } }`.
- `files.put(path, file)` accepts that `file` table: it copies the upload's bytes into the plugin's files (quota as `files.put`) within an hour of the upload. After an hour, or once copied, the upload is deleted; use the existing clean-up of unsent uploads and say in the code comment which duration it uses (set view uploads to one hour if it differs).
- An upload for a view of a plugin without `accept_uploads`: `FORBIDDEN: this plugin doesn't take uploads`.

- [ ] **Step 1: Write failing tests:** `an_upload_reaches_the_plugin_and_can_be_kept`, `blocked_extensions_and_size_limits_apply`, `without_accept_uploads_it_is_refused`, `another_sessions_view_cant_take_an_upload`, `an_unkept_upload_is_deleted_after_an_hour`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(server): uploads from plugin views`.

**Checks:** `cargo test -p opencord-server --test plugins uploads::`, clippy.

### Task 13: Files in messages, plugin channels from Lua, server definitions

**Owner:** Gemini. **Score:** 3. **Review focus:** files in messages go through the same checks as a member's attachments (count, size, blocked extensions); every new Lua function raises with the exact strings below.

**Files:** Modify `crates/opencord-server/src/plugins/api/{messages,guild}.rs`, `crates/opencord-plugins/api/opencord.d.luau`, `tests/plugins/{messages,channels}.rs`.

**Interfaces:**
- `messages.send(channel, { content?, files = { "<path>", … } })` with `attach_files`: each path is a plugin file; the message gets them as attachments through the attachments code a member's upload uses (processing, thumbnails, storage), so the same limits apply: at most the server's attachment count, each under its size limit, no blocked extension. A missing file: `NOT_FOUND: no plugin file at <path>`. Without `attach_files`: the M1 permission error. Find the function that turns stored bytes into an attachment; if none takes bytes, ask Claude before writing one.
- `channels.create({ kind = "plugin", type_key, name, topic?, parent_id? })` with `manage_channels`: only the plugin's own registered types (`INVALID_ARGUMENT: that channel type isn't registered by this plugin`).
- `opencord.d.luau`, server side: `ui.register_channel_type`, `ui.on`, `ui.views`, `ui.send_place`, `ui.send_role`, `ui.send_all`, the `View` type (`id`, `user: User`, `place`, `platform`, `send`), `files.token`, `files.put` taking the upload table, `messages.send`'s `files`, and the events `view_open`, `view_close`, `file_uploaded`, `plugin_channel_create` and `plugin_channel_delete` (view events arrive through `ui.on`, not `opencord.on`), with their payload shapes **as the code builds them** (read `events.rs` and Task 10's payloads; don't invent fields). Each new function marked `--- Server only`.

- [ ] **Step 1: Write failing tests:** `a_plugin_sends_its_file_as_an_attachment`, `too_many_files_or_a_blocked_one_is_refused`, `a_missing_file_is_not_found`, `a_plugin_creates_a_channel_of_its_own_type`, `another_plugins_type_is_refused`. The definitions test (`definitions_match_the_api`) keeps passing with the new names.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(plugins): files in messages and plugin channels from Lua`.

### Task 14: Hot reload and view error reports

**Owner:** Gemini. **Score:** 3. **Review focus:** a dev push reaches only the sessions with a view of that plugin; error reports can't flood the log.

**Files:** Modify `crates/opencord-server/src/plugins/dev.rs`, `src/plugins/install.rs` (updates), `src/handlers/views.rs`, `tests/plugins/{dev,views}.rs`.

**Interfaces:**
- After a dev push or an update installs and the new VM's start finishes, every session with an open view of that plugin gets `ViewScriptsUpdate` (the new `client_files`, entry). Sessions without one get nothing.
- `ReportViewError`: own view; the plugin's log gets an Error line `"view error for {user name}: {text}"` (text cut to 2,000 characters); at most one per view every 10 s, others dropped without a reply error. The `last report` time lives in Task 10's view entry, so it leaves with it.

- [ ] **Step 1: Write failing tests:** `a_dev_push_tells_only_sessions_with_a_view` (two members, one with the view open), `an_update_tells_open_views`, `a_view_error_reaches_the_log`, `a_second_report_within_10_seconds_is_dropped`, `another_sessions_view_cant_report`. (The client half of `a_dev_push_reloads_open_views_keeping_state` is Task 15's.)
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(server): reload open views and log their errors`.

At the end of the stage: full checks, push, read the pull request's checks, then hand over the stage C review.

---

## Stage D — Core and renderer

### Task 15: The core: views over the network

**Owner:** Claude. **Score:** 4 (threads, streams across the bridge, reconnects).

**Files:** Create `crates/opencord-core/src/views/client.rs`, `src/api/views.rs`. Modify `src/connection.rs`, `src/convert.rs`, `src/api/{mod,types}.rs`, `src/api/client.rs`, `crates/opencord-mobile/src/convert.rs` (compiles; plugin channels never reach the phone), generated bindings, and the desktop's startup (`app/lib/main.dart` or wherever it first calls the core) to pass the feature.

**Interfaces:**
- `api/views.rs` (FRB): `view_output_stream(sink: StreamSink<ViewOutput>)` (one stream for every view; `ViewOutput { view_id: String, json: String }`); `view_open(server_key, channel_id: i64, context_json: String) -> Result<String, CoreError>` (the core's own view id, a fresh string per open, kept across server reopenings); `view_input(view_id, json)`; `view_close(view_id)`; `plugin_asset_file(server_key, plugin_id, path, sha256) -> Result<String, CoreError>` (a cached file path); `plugin_image_file(server_key, plugin_id, source_json) -> Result<String, CoreError>` (`asset`, `file`, `file_token`, `url`, `attachment` sources through the routes of Tasks 9 and 11 and the attachments route; `bytes` written to a temporary file); `view_upload(view_id, node_id: u64, path: String)` (uploads through `CreateUpload { view_id }` and sends the picker's `upload` or `upload_error` event to the view).
- Types: the core's `Channel` gains `plugin_id`, `type_key` and `plugin_state`; `ReadySnapshot` gains `plugin_channel_types`; a `PluginChannelTypesUpdate` event. **Features per app:** `Identify.features` comes from a new `set_plugin_features(features: Vec<String>)` the app calls before connecting, defaulting to `["plugin_users"]`; the desktop passes `["plugin_users", "plugin_channels"]`; the phone's facade doesn't call it, so phones never get plugin channels before M4.
- Opening: `OpenView`; a manifest `api` above `opencord_plugins::API_VERSION` sends `too_old` and stops; otherwise loads every `client/` file of `ViewOpened.files` from the cache (`<cache>/plugins/<sha256>`) or the assets route with `?v=`, checks each file's SHA-256 (a mismatch is an error), and starts the VM (Task 3) with the viewer, place, context and `show_errors`.
- Routing: `ViewMessage` → `server_message`; `ViewClosed` → a `closed` op with the reason, then the VM closes; `ViewScriptsUpdate` → load the new files, then `reload` (state kept); `app.send` → `SendViewEvent`; `report_error` → `ReportViewError`, at most one every 10 s per view.
- Connection: losing it sends a `connection_lost` lifecycle input and `connection_change { connected = false }`; a resumed session needs nothing more; **a new session reopens every open view** with `OpenView`, maps the new server view id to the same core view id, keeps the VM and its state, and sends `connection_back`. A reopening the server refuses closes the view with that error.
- Maps and when entries leave: open views (on `view_close`, `ViewClosed`, the server's removal, and the app closing the server); the server-to-core view id map (the same).

- [ ] **Step 1: Write failing tests** (against the test server, through the core's client, with the real VM):
  - `only_an_app_that_asks_gets_plugin_channels` (default features: none in `Ready`);
  - `opening_a_plugin_channel_draws_its_first_tree` (the plugin's client script sends a button; the stream carries a batch with it);
  - `a_press_reaches_the_server_and_its_answer_redraws`;
  - `client_files_load_once_by_hash` (a second open fetches nothing);
  - `a_changed_file_with_an_old_hash_is_refused`;
  - `a_too_new_api_sends_too_old`;
  - `a_new_session_reopens_open_views_keeping_state` (Review Focus 4: the counter state survives; the plugin's log shows two `view_open`; one press after reopening counts once);
  - `a_dev_push_reloads_open_views_keeping_state` (Review Focus 2's client half);
  - `server_closing_the_view_sends_closed`.
- [ ] **Steps 2–4** as before; regenerate the Dart bindings.
- [ ] **Step 5: Commit** `feat(core): plugin views over the network`.

**Checks:** `cargo test -p opencord_core --lib views`, the core's integration tests named above, clippy, `flutter analyze` in `app`.

### Task 16: `opencord_blocks`: patches, host, layout core

**Owner:** Claude. **Score:** 4 (the renderer's state machine; shared with the phone in M4).

**Files:** Create `app/packages/opencord_blocks/` (`pubspec.yaml` with Flutter only, `LICENSE` MPL-2.0, `lib/opencord_blocks.dart`, `lib/src/{controller,host,theme,node,view,common_props}.dart`, `lib/src/blocks/{row_column,box,unknown}.dart`, `test/`). Modify `app/pubspec.yaml` (path dependency), `.github/check.sh` and `.github/workflows/cicd.yml` (the package's `flutter analyze` and `flutter test`, beside the app's).

**Interfaces:**
- `class BlocksController extends ChangeNotifier`: `void applyBatch(String json)` (applies ops in order; a batch whose `seq` isn't above the last is ignored); `Stream<String> get inputs` (input JSON); `void send(Map<String, Object?> input)`; the node store (id → `BlockNode { block, key, props, events, children }`); `roots`, `dialogs`, `framesOn`, `error`, `tooOld`, `closed`.
- `abstract class BlocksHost`: `IconData? icon(String name)`; `Future<ImageProvider?> image(Map<String, Object?> source)`; `Widget user(String id, double size, bool showName)`; `Widget richText(String markdown, TextStyle style)`; `Future<String?> pickFile({required bool image, List<String> accept})`; `Future<bool> confirmLink(String url)`; `void copy(String text)`; `void openChannel(String id)`; `Future<void> playSound(String asset, double volume, VoidCallback onEnd)`; `Future<ByteData?> font(String file)` (a font from the plugin's `assets/fonts/`); `Future<void> upload(int nodeId, String path)` (a picker's file; the core answers with the block's `upload` or `upload_error` event).
- `class BlocksTheme { Map<String, Color> colors; TextStyle text; double radius; … }` with every colour name of Task 1; `Color? resolveColor(Object? value)` for names, hex and gradients (gradients become `Gradient`).
- `class BlocksView extends StatelessWidget` (`controller`, `host`, `theme`): draws the main root and, above it, the modal layer (Task 22), only inside its own box; reports `context` inputs on size, theme, text scale changes.
- Blocks here: `row`, `column` (Flex with `gap`, `align`, `cross`, `wrap`), `box` (one child, presses through `GestureDetector`, hovers through `MouseRegion`), the common and container props (sizes, `flex`, padding, opacity, `visible`, tooltip, semantics `label`, background, border, radius, shadow from the theme), and an `unknown` block that draws "Unknown block: {name}" in the error style.
- **Fixture replay:** `test/patches_test.dart` reads every file in `crates/opencord-plugins/tests/fixtures/patches/` (by a path relative to the package), applies `ops` to `before`, and compares the node store with `after`. Both sides of the wire agree or a test fails.

- [ ] **Step 1: Write failing tests:** the fixture replay; `a_stale_batch_is_ignored`; `a_press_sends_its_event_input`; `row_gap_align_and_wrap`; `common_props_size_padding_opacity_and_visible`; goldens of a sample tree in dark and light at 800 and 360 wide.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(blocks): the renderer's patches, host and layout core`.

**Checks:** the package's `flutter analyze` and `flutter test`; `.github/check.sh client` runs them.

### Task 17: `opencord_blocks`: canvas

**Owner:** Claude. **Score:** 4 (pointer streams, frame back-pressure, ink drawn without Lua).

**Files:** Create `app/packages/opencord_blocks/lib/src/blocks/canvas.dart`, `test/canvas_test.dart`, goldens.

**Interfaces:**
- Items (§12.6), each with `id`: `line` (`x1, y1, x2, y2`), `path` (`points` flat `[x, y, …]`, `closed`), `rect` (`x, y, w, h, radius`), `ellipse` (`x, y, w, h`), `text` (`x, y, text, size, color`), `image` (`x, y, w, h` plus one image source); each with `stroke` (`{color, width}`), `fill` (colour or gradient), `opacity`, `transform` (`{translate = {x, y}, rotate (degrees), scale}`). Drawn in `order` by one `CustomPainter` that repaints only on a `draw` op or an image load.
- Pointer events only for the handlers present, `pointer_move` at most 60 a second; `wheel`, `pan`, `scale` from Flutter's gesture recognisers.
- Ink (`ink = { color, width }`): the renderer draws the stroke under the pointer itself; `stroke` events carry the points every 50 ms or 64 points, and once with `done = true` at the end; the local stroke stays until a `draw` op adds an item with the stroke's id or 2 s pass.
- Frames: while `framesOn`, one `frame` input per vsync, but never a second before a batch newer than the one the last frame named has arrived.

- [ ] **Step 1: Write failing tests:** `items_draw_in_order`, `a_draw_op_repaints_only_its_canvas`, `pointer_moves_are_capped_at_60_a_second`, `ink_draws_locally_and_sends_batched_points`, `frames_wait_for_the_previous_batch`; goldens in dark and light.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(blocks): the canvas`.

### Task 18: The app's model, repository and mock

**Owner:** Gemini. **Score:** 3. **Review focus:** each view output reaches exactly one controller once; generated FRB types stay inside `core/rust/`.

**Files:** Modify `app/lib/core/model/channel.dart` (`ChannelKind.plugin`, `pluginId`, `typeKey`, `pluginState`), `app/lib/core/model/plugins.dart` (`PluginChannelType`), `core/repository/{repository,events}.dart`, `core/rust/{rust_repository,core_mapping,core_api}.dart`, `core/mock/{mock_repository,mock_world}.dart`, `core/providers/server_state.dart` (channel types), create `core/providers/plugin_views.dart`, tests.

**Interfaces:**
- `enum PluginChannelState { available, pluginStopped, typeGone }`; `Channel.isAvailablePlugin`.
- Repository: `Future<String> openView(String serverKey, int channelId, String contextJson)`; `void viewInput(String viewId, String json)`; `Future<void> closeView(String viewId)`; `Stream<ViewOutput> get viewOutputs` (`ViewOutput(viewId, json)`); `Future<String> pluginAssetFile(serverKey, pluginId, path, sha256)`; `Future<String?> pluginImageFile(serverKey, pluginId, Map<String, Object?> source)`; `Future<void> viewUpload(String viewId, int nodeId, String path)`; `ServerData.pluginChannelTypes`, updated by `PluginChannelTypesUpdated`.
- `pluginViewProvider` (family by server and channel): opens a view, owns a `BlocksController`, pipes the stream's batches for its view id into it and its `inputs` into `viewInput`, and closes the view when disposed.
- Mock: `MockRepository` gets `MockView` scripts: a Dart function from input JSON to batch JSON, keyed by plugin id, with one built in for the mock world's plugin channel (a counter: a column with a text and a button; a press redraws with the next count), so app tests run without Lua. The mock world gains a plugin channel `#tickets` (available) and a stopped one.

- [ ] **Step 1: Write failing tests:** `channel_kind_and_state_map_from_the_core`, `a_view_output_reaches_only_its_controller` (two views open: road core stream → provider → controller, named in the test), `closing_the_provider_closes_the_view`, `the_mock_counter_counts`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(app): plugin channels and views in the model and repository`.

### Task 19: Blocks: layout and content

**Owner:** Gemini. **Score:** 3. **Review focus:** long, right-to-left and empty text never overflow; images load through the host only.

**Files:** Create `app/packages/opencord_blocks/lib/src/blocks/{stack,grid,scroll,spacer_divider,text,rich_text,image,icon,user,badge_progress_spinner}.dart`, tests, goldens.

**Interfaces:** each block of Task 1's table in these groups, with its props and events as the wire format gives them:
- `stack` (children's `x`, `y`, `right`, `bottom` → `Positioned`; unpositioned children fill), `grid` (`columns` or `min_column_width`, `gap`), `scroll` (`direction`; `scroll` events at most 10 a second; `reach_start`/`reach_end` once per arrival within 48 px of the end), `spacer`, `divider`;
- `text` (weights map to Inter's 400, 500, 600, 700; `font` loads `assets/fonts/<file>` once per view through `host.font` and Flutter's `FontLoader`, falling back to Inter until it's loaded), `rich_text` (`host.richText`), `image` (`host.image`; `fit`, `radius`, `placeholder` colour while loading; `version` change reloads; animated GIF and WebP play; `load`/`error` events), `icon` (`host.icon`, a missing name draws nothing and logs once), `user` (`host.user`), `badge`, `progress` (bar and ring, indeterminate without `value`), `spinner` (the app's spinner look, drawn here).

- [ ] **Step 1: Write failing tests** per block, plus `a_very_long_word_wraps_inside_its_box`, `right_to_left_text_aligns_start`, `an_image_version_change_reloads`; goldens of every block in dark and light at 800 and 360 wide.
- [ ] **Steps 2–4** as before. Render and look.
- [ ] **Step 5: Commit** `feat(blocks): layout and content blocks`.

### Task 20: Blocks: controls

**Owner:** Gemini. **Score:** 3. **Review focus:** a text field keeps its caret while the plugin redraws; each control sends what it shows.

**Files:** Create `app/packages/opencord_blocks/lib/src/blocks/{button,toggles,slider,select,tabs,text_input,pickers}.dart`, tests, goldens.

**Interfaces:**
- `button` (`variant` primary filled with `accent`/`on_accent`, secondary `surface` with a border, danger `danger`, plain text only; `loading` shows a spinner and ignores presses; `disabled` dims), `icon_button`, `checkbox`, `switch`, `radio`, `slider` (`change` at most 20 a second while dragging, `change_end` once), `select` (a menu of `options` inside the plugin area), `tabs`, `segmented`.
- `text_input`: the controller lives in the block's `State` (Rule 18); while focused, a redraw changes the text only when the plugin's `value` differs from the value it last sent; `change` is sent at most every 100 ms with the latest text and once more after typing stops; `submit` on Enter (single line) or Ctrl+Enter (several lines); `password` obscures; `max_length` is enforced while typing.
- `image_picker`, `file_picker`: `host.pickFile`, then `host.upload(nodeId, path)`; the block shows a spinner until its `upload` or `upload_error` event comes back through the core.

- [ ] **Step 1: Write failing tests** per control, plus `typing_keeps_the_caret_across_redraws`, `a_plugin_value_change_replaces_the_text`, `change_is_throttled_and_the_last_text_is_sent`, `a_select_shows_and_sends_its_value`; goldens.
- [ ] **Steps 2–4** as before. Render and look.
- [ ] **Step 5: Commit** `feat(blocks): controls`.

### Task 21: Blocks: lists and animation

**Owner:** Gemini. **Score:** 3. **Review focus:** a reversed list stays pinned to the bottom as items arrive; range requests never ask past `count`.

**Files:** Create `app/packages/opencord_blocks/lib/src/blocks/list.dart`, `lib/src/animation.dart`, tests, goldens.

**Interfaces:**
- `list`: a lazy `ListView` over `count` items; the visible range plus 20 on each side is sent as a `range` input when it changes (at most 10 a second, never past `count`); items the core hasn't sent yet draw a blank of `item_height` (or 48); `reverse` keeps the newest at the bottom and stays there while new items arrive unless the user scrolled up; `reach_start`/`reach_end` as `scroll`'s.
- `animate = { duration, curve }` (`linear`, `ease`, `ease_in`, `ease_out`, `ease_in_out`, `spring`; duration 0–2,000 ms) animates size, position, opacity and colour changes with implicit animations; keyed children of `list`, `column` and `row` with `enter`/`exit` (`fade`, `slide_up`, `slide_down`, `scale`) animate in and out; `animation_end` once each. Reduced motion (`MediaQuery.disableAnimations`) turns all of them off.

- [ ] **Step 1: Write failing tests:** `only_the_visible_range_is_asked`, `a_range_never_passes_count`, `a_reversed_list_stays_at_the_bottom`, `scrolled_up_it_stays_put`, `an_animate_prop_animates_and_reduced_motion_does_not`, `enter_and_exit_run_once`.
- [ ] **Steps 2–4** as before.
- [ ] **Step 5: Commit** `feat(blocks): lazy lists and animation`.

### Task 22: Blocks: overlays

**Owner:** Gemini. **Score:** 3. **Review focus:** nothing a plugin draws leaves the plugin area; focus can't escape the top modal.

**Files:** Create `app/packages/opencord_blocks/lib/src/blocks/{modal_layer,modal,sheet,menu,tooltip,toast,dialogs}.dart`, tests, goldens.

**Interfaces:**
- The modal layer (§12.5) draws named modals from `roots` and in-tree `modal`/`sheet` blocks with `open = true`, over a dimmed backdrop **inside the plugin area's bounds** (a `Stack` in `BlocksView`, never the app's `Navigator` or `Overlay`). Sizes: small 360, medium 520, large 760 wide, full the whole area; at the `compact` breakpoint a `modal` shows as a sheet unless `sheet = false`. Dismissible ones close on a backdrop tap or Esc (`modal_dismiss` input for named, `dismiss` event for in-tree). Focus stays inside the top modal (`FocusScope` with traversal limited to it). Opening scales from 0.97 and fades in 180 ms ease-out (the package can't use the app's springs); closing is instant.
- `menu` (its child is the trigger; the list opens below it, inside the area), `tooltip`, `toast` (at the bottom of the area, `duration` then `dismiss`), and the quick dialogs from `dialog` ops: confirm (**OK** and **Cancel**), alert (**OK**), prompt (a field, **OK**, **Cancel**), each answering with a `dialog` input.

- [ ] **Step 1: Write failing tests:** `a_modal_stays_inside_the_area` (the area is a 400×300 box inside an 800×600 app; the modal's rect is inside it), `named_modals_stack_in_order`, `compact_turns_a_modal_into_a_sheet`, `esc_closes_only_dismissible_ones`, `focus_stays_in_the_top_modal`, `a_confirm_answers_true_and_false`, `a_toast_dismisses_after_its_duration`; goldens.
- [ ] **Steps 2–4** as before. Render and look.
- [ ] **Step 5: Commit** `feat(blocks): modals, sheets, menus, tooltips, toasts and dialogs`.

At the end of the stage: full checks, push, then hand over the stage D review.

---

## Stage E — Desktop and the example

### Task 23: Desktop: plugin channels in the list, and creating them

**Owner:** Gemini. **Score:** 3. **Review focus:** an unavailable channel can't be opened by any path (click, keyboard, quick switcher, link).

**Files:** Modify `app/lib/features/channels/{channel_row,channel_sidebar}.dart`, the create channel dialog (`showCreateChannel`), `features/settings/server/channels_page.dart`, `features/shell/navigation.dart`, tests.

**Interfaces:**
- A plugin channel's row: the type's icon (`pluginAssetFile` of `PluginChannelType.icon`) at the text channel icon's size, else `OcIcons.extension`; its name; no unread dot or mention count. Unavailable: greyed (`textMuted`), not openable; hovering shows **"This channel is unavailable"**, or for `typeGone` **"This plugin no longer provides this channel"**. Its menu keeps what other channels have (edit, delete for admins, copy link).
- Opening a plugin channel by any path (the row, the keyboard, the quick switcher, an `opencord://…/c/<id>` link) goes through one `canOpen` check in `navigation.dart`; an unavailable one shows the same message as a toast instead.
- The create channel dialog lists, after Text and Voice, each type in `pluginChannelTypes` as **"{type name}"** with **"{plugin name}"** under it and its icon, for members with `MANAGE_CHANNELS`; creating one sends `plugin_id` and `type_key`; the server's error shows in the dialog.
- Server settings → Channels lists plugin channels like the others.

- [ ] **Step 1: Write failing tests:** `a_plugin_channel_shows_its_type_icon`, `an_unavailable_channel_shows_why_and_doesnt_open` (row, keyboard and link), `a_type_gone_channel_says_so`, `create_channel_lists_plugin_types_and_sends_them`, `the_servers_refusal_shows_in_the_dialog`; goldens of the sidebar with available and unavailable plugin channels in dark and light.
- [ ] **Steps 2–4** as before. Render and look.
- [ ] **Step 5: Commit** `feat(app): plugin channels in the channel list`.

### Task 24: Desktop: the plugin area

**Owner:** Gemini. **Score:** 3. **Review focus:** the app's own shortcuts still work with a plugin area focused; leaving the channel closes the view; error details only for those allowed.

**Files:** Create `app/lib/features/plugins/{plugin_area,blocks_host,plugin_icons,blocks_theme}.dart`. Modify `app/lib/features/chat/chat_area.dart` (and where the header and composer are chosen), tests, goldens.

**Interfaces:**
- A plugin channel's main area is the channel header (name and topic, as a text channel's) and `PluginArea` in place of the messages and composer. States (§14.3):
  - **Loading:** the app's spinner in the area's middle;
  - **Running:** `BlocksView`;
  - **Error:** **"This plugin hit an error"**, a **Reload** button (closes and opens the view again), and, when the `error` op has `details`, the details in `OcText.mono`, selectable;
  - **Unavailable:** **"This channel is unavailable"** or **"This plugin no longer provides this channel"**;
  - **Too old:** **"Update Opencord to open this"**;
  - **Closed by the server:** by reason, **"This plugin stopped"**, **"You can no longer see this channel"**, **"This channel was deleted"**, **"This channel is unavailable"**.
- `blocks_host.dart` implements `BlocksHost`: icons from `plugin_icons.dart`, a map from these names to `OcIcons`: `add`, `arrow_back`, `arrow_forward`, `arrow_up`, `arrow_down`, `check`, `close`, `delete`, `edit`, `search`, `settings`, `info`, `warning`, `error`, `star`, `person`, `group`, `chat`, `send`, `image`, `link`, `lock`, `lock_open`, `refresh`, `play`, `pause`, `stop`, `music`, `mic`, `volume`, `schedule`, `history`, `pin`, `tag`, `copy`, `download`, `upload`, `more`, `menu`, `visibility`, `visibility_off`, `palette`, `extension` (and docs list the same); images through `pluginImageFile`/`pluginAssetFile` as `FileImage`; `user` with the app's avatar and name; `richText` with the message markdown view; `pickFile` with `file_selector`; `upload` with `viewUpload`; `font` with `pluginAssetFile`; `confirmLink` with an `OcDialog`, **"Open this link?"**, the URL, **Open** and **Cancel**, then `url_launcher`; `copy` to the clipboard with the toast **"Copied"**; `openChannel` through navigation; `playSound` through the core's local sound playback (the function the soundboard preview uses; if it can't take a file path, ask Claude).
- `blocks_theme.dart` builds `BlocksTheme` from `OcColors` (`danger` is `recording`, `background` is `chat`) and Inter.
- Input: the area takes focus on a click inside it; while focused, keys go to the view as `key` inputs **after** the app's own shortcuts (push-to-talk, mute, deafen, the global keys) have had them; type-to-focus doesn't run in a plugin channel. Window focus and visibility send `lifecycle` inputs; size, theme and text scale send `context`.
- Leaving the channel disposes the provider, which closes the view.

- [ ] **Step 1: Write failing tests** (MockApp with the mock counter): `opening_the_channel_shows_the_plugin_and_a_press_counts` (road: mock view → repository stream → provider → `BlocksView` → tap → `viewInput` → next batch), `every_state_shows_its_words`, `details_show_only_when_allowed`, `reload_reopens_the_view`, `leaving_the_channel_closes_the_view`, `push_to_talk_still_works_with_the_area_focused`, `open_link_asks_first`; goldens of every state in dark and light at the normal and narrowest widths.
- [ ] **Steps 2–4** as before. Render and look, including the counter at 360 wide.
- [ ] **Step 5: Commit** `feat(app): the plugin area`.

### Task 25: Client definitions, docs, the init template

**Owner:** Gemini. **Score:** 2.

**Files:** Modify `crates/opencord-plugins/api/opencord.d.luau`, `crates/opencord-server/tests/plugins/definitions.rs` (or a new core test for the client globals), `docs/plugins.md`, `crates/opencord-plugin-cli/src/init.rs` and its template files, `README.md` if it lists plugin features.

**Interfaces:**
- `opencord.d.luau`, client side (Claude's call 6): `app` (every function and field of Tasks 3–5 with its shape), `ui.<block>` for every block in `BLOCKS` with a props type listing that block's props and `on_<event>` handlers, `init`, `render`, `migrate`; marked `--- Client only`. A test compares the client VM's globals and `BLOCKS`' names and props with the definitions; the shapes of `app.viewer`, `app.place` and `app.size` are checked by hand against the code.
- `docs/plugins.md`: a "Client scripts and custom channels" section: the redraw model, the client API, every block with its props and events (a table per group; a test checks every block name in `BLOCKS` appears), images and their five sources, files and tokens, uploads, the limits above, the icon names of Task 24, hot reload and that state must be plain data (Claude's call 8). Fix the limits table's wrong rows (files in a zip 2,000; database 1 GB; files 5 GB; HTTP timeout 10 s; timers 1,000; handler time 100 ms).
- `opencord-plugin init` adds `client/main.luau` (the counter of §11.2 with a server echo), registers a channel type in `server/main.luau`, adds `create_channel_types` to the manifest, and sets `client` in it.

- [ ] **Steps:** write the definitions test (failing), update the definitions, docs and template, run `cargo test -p opencord-plugin-cli` and the definitions test, commit `docs(plugins): client scripts, blocks and custom channels`.

### Task 26: The ticket system example and its end-to-end tests

**Owner:** Gemini. **Score:** 2.

**Files:** Create `examples/plugins/tickets/` (`manifest.lua`, `server/main.luau`, `client/main.luau`, `assets/ticket.png`), `crates/opencord-server/tests/plugins/tickets.rs`, `crates/opencord-core/src/views/tests/tickets.rs`. Modify `docs/plugins.md` (link the sample), `.github` only if a new test binary needs listing.

**Interfaces:**
- The plugin: permissions `create_channel_types`, `send_messages`, `read_roles`; config `support_role` (`role`) and `log_channel` (`channel`); registers the type `tickets` (**"Tickets"**, "Open and track support tickets", `assets/ticket.png`); keeps tickets in its database (`id`, `channel_id`, `author_id`, `title`, `body`, `priority`, `state`, `created_at`, with replies in a second table), saved under the channel's id.
- The client: a list of the member's tickets (everyone's for the support role), newest first, with **"No tickets yet."** when empty; **New ticket** opens a named modal (title `text_input`, body with 4 lines, priority `segmented` low/normal/high, **Open ticket**); a ticket's detail with its replies, a reply field, and **Close ticket** for the support role; `compact` shows the list or the detail, `expanded` both side by side.
- The server: `ui.on("ticket.create" | "ticket.reply" | "ticket.close" | "ticket.list" | "ticket.open")`, checks the viewer's rights from `view.user`, never from the client's data; sends updates to the viewers who may see each ticket; posts **"Ticket #{id} opened by {name}: {title}"** in `log_channel` when set.
- `tickets.rs` (server, gateway level): install the zip, create a tickets channel, open a view as two members, create a ticket as one, the support member gets the update, the other member doesn't see someone else's ticket, closing by a non-support member is refused.
- `views/tests/tickets.rs` (core, the real client script against a scripted server feed): the first tree contains "No tickets yet."; pressing **New ticket** opens the modal root; submitting sends `ticket.create` with the typed values.

- [ ] **Steps:** write the two tests (failing), write the plugin, run both, add the sample to the docs, commit `feat(examples): the ticket system plugin`.

At the end of the stage: full checks, push, hand over the stage E review, then stop and report to the user.
