# Plugins, Milestone 4: The Phone — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Where this plan and that skill differ, this plan wins:** the ledger is where "Progress ledger" says, not under the plan's file name; reviews go in rounds as "Stages and their reviews" says, with a new review after each fix pass; and the ledger's folder is never deleted (it holds `gates.md` and `m4-finished`).
>
> **One builder.** The owner decided (2026-11-13) that this milestone has no Gemini and no bridge: one Claude Opus session builds every task, in order, in this working copy. There is no owners table and no `delegate_task`. Reviews are done by a fresh context, never by the builder (see "Stages and their reviews").

**Goal:** On the phone, a member opens a plugin channel and sees the plugin's own screen, and in a voice call starts, opens and ends activities, with the same plugins, scripts and blocks as the desktop.

**Architecture:**
- **The renderer is the desktop's.** A small Flutter module, `mobile/flutter_module`, wraps `opencord_blocks` and is embedded in the Expo app (design §18): an Android AAR and iOS frameworks, hosted by a new Expo native module, `mobile/modules/plugin-area`. One Flutter engine per open plugin area, spawned from one engine group.
- **One Rust core.** The Flutter module links no Rust. The client VM runs in `opencord-core` inside the React Native app, as every other part of the phone's core does. `crates/opencord-mobile` gains the view, activity and file calls the desktop's Flutter API already has.
- **Batches and inputs travel core → JavaScript → native module → Dart, and back.** JavaScript pulls a view's batches from the core as it pulls events, and hands each to the native module, which passes it to that area's engine over one binary `MethodChannel`. Inputs come back the same road. This is Claude's call 1: it departs from §18's wording, and Stage A measures it before anything is built on it.
- **React Native draws everything around the plugin's area** (§18): the channel row, the pane and its states (loading, error, unavailable, too old, closed), the Activities button and picker, the activity tile and the opened activity. The Flutter view draws only the plugin's blocks and their modals.
- **No server and no protocol change.** The phone starts listing `"plugin_channels"` and `"activities"` in `Identify.features`; the server already serves both.

**Tech Stack:** as the phone app (Expo 58, React Native 0.88, uniffi through `ubrn`, zustand, jest) and as milestone 3 (Rust 2024, `opencord-core`'s client VM; Flutter 3.44 with `opencord_blocks`). **New:** the Flutter engine in the phone app (add-to-app). **No new npm or pub dependencies** beyond what `flutter create -t module` generates; the Flutter module uses no Flutter plugins.

**Spec:** `docs/superpowers/specs/2026-11-07-plugins-design.md`. Read §2, §5, §11 (all), §12.1, §12.5, §13, §14.3–§14.4, §15, §16, §18, §21–§23 before Task 1, and the sections a task names before that task. "§" below means the spec. The view wire format is in the M2 plan (`docs/superpowers/plans/2026-11-09-plugins-m2-channels.md`, "The view wire format") plus the M3 plan's "Additions". The phone app's own design is `docs/superpowers/specs/2026-10-26-mobile-client-design.md`; `docs/mobile.md` says how to build and run it. If the spec and this plan disagree, the spec wins: ledger a ruling (see "Rulings") and go on.

## Global Constraints

- **Branch and pull request:** `feat/plugins-phone`, made by Claude from `feat/plugins-activities` (milestone 3, pull request #51); its first commit holds this plan. After Stage A's gates pass, push it and open **one draft pull request into `beta`** (its diff includes milestones 2 and 3 until #50 and #51 merge; that's expected). Push at the end of every stage. Never merge. Never push to `beta`, `canary` or `main`.
- **If #51 gets new commits** after this branch was made, merge `feat/plugins-activities` into this branch at the next stage end (a merge commit, never a rebase).
- **Before Task 1,** each a check; if one fails, stop and ask the owner:
  - `git switch feat/plugins-phone`; `git status --short -uno` prints nothing; `git log -1 --format=%s` is `docs: plugins milestone 4 plan`.
  - `df -h /` shows at least 25 GB free (the Flutter engine artifacts, an AAR per build mode and a release APK).
  - `source mobile/scripts/env.sh && flutter --version` prints 3.44.x, and `adb devices` shows `emulator-5554` (else `cd mobile && yarn emulator`).
  - `source mobile/scripts/env.sh && cd mobile && yarn core:android && scripts/check-bindings.sh` passes. Earlier milestones waved this check through because the machine had no Android build of the core; this milestone changes the facade, so it must be green from here on.
- **Scope:** `crates/opencord-mobile/`, `mobile/` (new: `flutter_module/`, `modules/plugin-area/`, `plugins/withPluginArea.ts`, `scripts/build-flutter.sh`, `src/features/plugins/`), `app/packages/` (a new shared package, Task 2; `opencord_blocks` only for a bug a phone test proves), `app/lib/` (only the three files Task 2 moves, replaced by re-exports), `app/test/` (one new test), `.github/` (the phone jobs and `check.sh`), `docs/`. Nothing in `crates/opencord-server`, `crates/opencord-core/src` or `proto/` changes; if a task seems to need that, stop and ask.
- **Not in M4:** installing or managing plugins on the phone (the Plugins tab stays desktop only); creating plugin channels on the phone; unread dots for plugin channels; new blocks; drag and drop into a plugin area.
- **The phone is judged on how it feels** (the owner, 2026-10-29): not a copy of the desktop. Every screen a task adds is looked at on the emulator in dark and light, on the phone and the tablet size, reached by its real button, never only by a deep link. Use the app's own controls (`mobile/src/ui`), its sheet, its menu, its toasts.
- **Words the user sees** are copied exactly from the task that gives them. Where the desktop already says it, the phone says the same words.
- **Ids in JavaScript are decimal strings,** as everywhere in the phone app; the facade converts.
- **Never log** view event data, file contents or tokens. Log ids, sizes and error kinds.
- **Generated or built, never edited by hand and never committed unless already tracked:** `mobile/packages/core-native/{src,cpp}/generated`, `mobile/android`, `mobile/ios`, `mobile/flutter_module/.android`, `.ios`, `build/`, the AAR repo and the iOS frameworks.
- **Build commands** start with `source mobile/scripts/env.sh &&` (it sources the Rust and Flutter environment too). Check `df -h /` before a release build. Never `cargo clean`, never `flutter clean` on `app/`.
- **Limits** (constants in `mobile/src/features/plugins/limits.ts`; and in Dart, `hostCallTimeout` in the module's `lib/src/channel.dart`: a host call waits 30 seconds for React Native, a file picker without limit):

  | Limit | Constant | Value |
  |---|---|---|
  | Plugin areas alive at once on one device | `AREAS_MAX` | 2 (one plugin channel, one activity) |
  | Batches pulled from the core per call | `VIEW_BATCH_MAX` | 64 |
  | `call` inputs to a view | `CALL_INPUT_EVERY_MS` | 100 |

### The plugin area channel

One `MethodChannel` per engine, named `dev.opencord/plugin_area`, `StandardMethodCodec`. Every JSON value below is a string holding JSON, never a decoded map, so the wire format's bytes pass through untouched.

| Direction | Method | Arguments | Meaning |
|---|---|---|---|
| native → Dart | `setup` | `{ "json": "<setup>" }` | Theme, insets and scale (below). Sent before the first batch, and again when any of it changes. |
| native → Dart | `batch` | `{ "json": "<batch>" }` | One batch of ops, exactly as the core gave it. |
| native → Dart | `hostReply` | `{ "id": int, "json": "<result>" }` | The answer to a `hostCall`: `{"ok":<value>}` or `{"error":"<kind>"}`. |
| native → Dart | `back` | none → `bool` | The system back button or gesture: true when the renderer closed a modal, a sheet or a dialog with it. |
| Dart → native | `ready` | none | The Dart side is listening. Batches pushed earlier are held natively and sent now, in order. |
| Dart → native | `input` | `{ "json": "<input>" }` | One input for the core, exactly as `BlocksController.inputs` gave it. |
| Dart → native | `status` | `{ "json": "<status>" }` | What React Native draws around the area (below). Sent on every change. |
| Dart → native | `hostCall` | `{ "id": int, "method": string, "json": "<args>" }` | A `BlocksHost` call React Native answers (Task 9's table). |

**Setup:** `{"theme":"dark"|"light","colors":{…},"text_scale":1.0,"safe_area":{"top":0,"right":0,"bottom":0,"left":0},"reduce_motion":false,"server_key":"…","plugin_id":"…"}`. `colors` holds the phone theme's values as `#RRGGBB` or `#AARRGGBB` under the twelve names `buildBlocksTheme` gives `BlocksTheme` on the desktop (`app/lib/features/plugins/blocks_theme.dart`): `text`, `text_secondary`, `text_muted`, `background`, `surface`, `elevated`, `border`, `accent`, `on_accent`, `selected`, `hover`, `danger`. The text style and the radius are that function's too (Inter, 14, radius 8).

**Status:** `{"state":"loading"|"running"|"error"|"too_old"|"closed","reason":"…","detail":"…","overlays":0}`. `loading` until the first `roots` op; `reason` is the `closed` op's reason; `detail` is the error text an `error` op carries for those allowed to see it (else empty); `overlays` counts open modals, sheets and dialogs.

## Working Rules (read before Task 1)

`CLAUDE.md` applies to everything here. `GEMINI.md`'s "How to write code here" and "Reports and the ledger" hold for this builder too: they are the project's review lessons, whoever builds.

### Rules reviews will enforce

1. **Test first.** Every test a task lists is written and run before the code that makes it pass, and its failing line is pasted in the ledger. A test that passes before its code exists is a finding about the test.
2. **A mutation line gives `file:line`, the old text → the new text, and the pasted failing output.** Without all three it isn't claimed.
3. **Tests feed what the real sender sends.** Batches in a JavaScript or Dart test are fixtures recorded from the real core (Task 6), copied whole. Inputs are in the wire format's shapes, field by field. A hand-made shape the sender never produces proves nothing.
4. **Every argument of a call has a named source.** No placeholder ids, no `'{}'` where a real value exists.
5. **A test with several people** lists each person and the one road they prove, in a comment above it.
6. **What a control shows is what it does,** listed per screen in its task: each control, and the request or state it changes.
7. **Screens are looked at.** For every screen a task adds: `mobile/scripts/screens.sh` in dark and light, phone and `--tablet`, each PNG opened at full size, and the ledger names each file and what was fixed after looking. A screenshot with a dev overlay over it isn't one.
8. **A stage's end is a ledger entry** with the last lines of `.github/check.sh` pasted verbatim, and `gh pr checks` pasted once no check is pending, every failure read (`gh run view <id> --log-failed`) and fixed on the branch.
9. **What wasn't run isn't claimed.** iOS is compiled by CI and never run here: every report and the pull request say so, per feature.
10. **The pull request's description is written from `git diff --stat` and `rg`,** never from memory, and lists every commit that isn't a task's own with why.

### Rulings

Where the plan is wrong, unclear or contradicts the spec, decide the smallest change that satisfies the spec, write `Task <N>: Ruling: <what> — <why> — <what it costs if wrong>` in the ledger, and go on. Stop and ask the owner only for: a Stage A gate that fails; something irreversible or outside this working copy that isn't in the plan; a change to the server, the core's source or the protocol; three failed review rounds.

**Add-to-app is the part of this plan most likely to be out of date.** Before Tasks 3–5, read Flutter's current add-to-app documentation for Android and iOS (`docs.flutter.dev/add-to-app`), and Expo's for native modules and config plugins. Where they differ from a step here, the documentation wins, with a ruling.

### Progress ledger

`.superpowers/sdd/plugins-m4/progress.md` (git-ignored; never `git add -f`), first line `# SDD ledger — plan: docs/superpowers/plans/2026-11-13-plugins-m4-phone.md`, one entry per task: status, commits, each test's failing then passing line, mutations, screenshots looked at, rulings, anything not run.

### Commands

| What | Command, from the repository root, after `source mobile/scripts/env.sh &&` |
|---|---|
| The facade's tests | `cargo test -p opencord-mobile` |
| The core for the emulator, and the bindings | `cd mobile && yarn core:android` |
| The bindings are current | `mobile/scripts/check-bindings.sh` |
| The Flutter module's checks | `cd mobile/flutter_module && flutter analyze && flutter test` |
| The shared package's checks | `cd app/packages/opencord_plugin_host && flutter analyze && flutter test` |
| The desktop still passes | `cd app && flutter analyze && flutter test` |
| The Flutter module for Android | `mobile/scripts/build-flutter.sh android` (add `--release` for a release APK) |
| The phone's JavaScript checks | `cd mobile && yarn typecheck && yarn lint && yarn test` |
| The app on the emulator, fake world | `cd mobile && yarn android:fake` |
| Everything CI runs on Linux | `.github/check.sh` |

### Stages and their reviews

| Stage | Tasks | At its end |
|---|---|---|
| A — The pipe, proven | 1–6 | The gates in Task 6, measured and pasted; full checks; push; open the draft pull request; read its checks (both phone jobs must build) |
| B — The phone knows plugins | 7–8 | Full checks; push |
| C — The renderer's host | 9–10 | Full checks; push; **review of stages A–C** |
| D — The screens | 11–15 | Full checks; push |
| E — Live checks and docs | 16–17 | Full checks; push; **review of stages D–E**; then stop and report to the owner |

**A review** is a fresh context (a new agent or a new session, on Opus), given: this plan, the spec, the stage's diff (`git diff <stage base>..HEAD`), the ledger, and this list of what to do. It reads the whole diff against the plan; runs the tests; runs at least one mutation per pinned Review Focus test and one per task; opens every screenshot the stage's tasks name; and ends with `Round <r>: PASS` or `FAIL` and numbered findings. The builder fixes each finding with a test that failed first, runs the full checks, pushes, and asks for the next round from another fresh context. Stop and ask the owner after three failed rounds.

**The decisions entry (D97)** is written by the builder in Task 17, after the last review passes, and checked by that review's next round.

## Review Focus

The inputs most likely to hurt real people while ordinary tests pass. Each has a pinned test in the task named.

1. **The viewer leaves a plugin's screen and comes back** (another channel, the app to the background and back, the call sheet closed and opened). Expected: an open activity is the same view with its state, told `pause` then `resume` once each; a plugin channel closes when left and opens fresh. Nothing is drawn into a dead engine, and no engine outlives its view. Pinned in Task 11 (`an area outlives its native view and is destroyed with its session`) and Task 15 (`an open activity survives leaving the call's pane`).
2. **Batches arrive before the area can take them** (before `viewOpen` returns its id, before Dart says `ready`), **or JavaScript reloads under open views.** Expected: no batch is lost or applied twice, and order holds; after a reload no engine and no view of the old start is left. Pinned in Task 7 (`early batches wait for the view's id`, `starting the pump destroys leftover areas before attaching`), Task 4 (`batches before ready are held in order`) and Task 1 (`attach_closes_views_left_by_an_older_generation`).
3. **The app connects to a server without the renderer** (the native module missing in a build, or an older server). Expected: the phone lists no plugin feature it can't draw, so it hears of no plugin channel and no activity; nothing crashes. Pinned in Task 8 (`features follow the renderer`).
4. **Back and the keyboard inside a plugin's screen.** Expected: back closes the plugin's top modal first and leaves the screen only when none is open; a focused text field stays above the keyboard; closing the keyboard doesn't close the screen. Pinned in Task 10 (`back closes the top overlay first`) and by hand on the emulator in Task 6 (gate G6) and Task 16.
5. **The call changes under an open activity** (the last person leaves, the viewer is moved or disconnected, the activity is ended by someone else, the plugin stops). Expected: the view closes with the desktop's words, the tile goes, and the pane returns to the call's grid without a stuck engine. Pinned in Task 15 (`an ended activity closes its screen and its tile goes`, `leaving the call closes the opened activity`).

Also pinned: an unavailable plugin channel can't be opened and says why (Task 12); a fifth activity and a refused start say the desktop's words (Task 14).

## Claude's calls in this plan (for the owner to check)

**The owner, 2026-11-13:** "start milestone 4 and decide best for the questions". So calls 1, 9 and 12 stand as written: batches go through JavaScript unless gate G2 or G3 fails; iOS is compiled by CI and its hand checks are owed; reviews are fresh contexts on Opus.

1. **Batches go through JavaScript,** not from native code straight to the Flutter view as §18 words it. The core is linked into the React Native app through generated JavaScript bindings; reaching it from Kotlin and Swift as well would need a second, hand-written native interface on both systems. The renderer's frame loop never queues frames, so a slower road lowers the frame rate instead of piling up. Stage A measures it (gate G2); if it fails, the builder stops and the owner chooses.
2. **One engine per open plugin area, at most two alive:** one plugin channel and one activity. Opening a second activity closes the first one's view (the activity itself goes on for everyone).
3. **On the phone an opened activity takes the call's whole stage,** with the people in a strip above it. It never draws inside the small grid tile as on the desktop: a phone's tile is about 180 points wide.
4. **React Native draws the states** (loading, error, too old, closed, unavailable), so they look like the rest of the phone app; the Flutter view is shown only while the plugin runs.
5. **`richText` on the phone is drawn by a smaller view** than the desktop's: the same parser (moved to a shared package, the desktop's files left as re-exports), without syntax-highlighted code blocks or hover actions.
6. **Uploads, link confirmation, copying, opening a channel, sounds and the file picker are answered by React Native,** with the phone's own picker, dialog and toast.
7. **No plugin management on the phone:** no Plugins tab, no installing, no creating plugin channels. An admin does that on the desktop.
8. **An unavailable plugin channel** shows in the list greyed, and tapping it shows the desktop's sentence as a toast.
9. **iOS is built by CI and never run in this plan.** The owner's rule of 2026-10-29 ("iOS before pushing") was made for the first phone branch; here the push is the only way to compile iOS at all. The pull request stays a draft until the last review passes, and its description lists every iOS check owed to a Mac and a real iPhone.
10. **The phone app grows by the Flutter engine,** about 5–7 MB per processor type in the download. Gate G4 records the real number.
11. **The engine is warmed up** when a server with plugin channel types or activities is opened and when a call is joined, as §18 says, and dropped when neither holds for 60 seconds.
12. **Reviews are fresh contexts started by the builder,** since nobody else is in this milestone. They run on Opus.
13. **§18 asks for keyboard, focus and back gestures to be checked on real phones early.** Here they are checked early on the emulator (gate G7) and on real phones only by the owner, from the hand checks Task 17 writes.

## File Map

| File | Task |
|---|---|
| `crates/opencord-mobile/src/api/plugins.rs` (new), `src/api/mod.rs`, `src/mobile.rs`, `src/types.rs`, `src/convert.rs`, `src/events.rs` | 1, 7 |
| `app/packages/opencord_plugin_host/` (new), `app/lib/features/chat/markdown.dart`, `app/lib/core/{fences,mention_tokens}.dart`, `app/test/unit/plugins/plugin_icon_names_test.dart`, `app/pubspec.yaml` | 2 |
| `mobile/flutter_module/` (new: `pubspec.yaml`, `lib/main.dart`, `lib/src/{area,channel,phone_host,status}.dart`, `test/`) | 3, 9, 10 |
| `mobile/modules/plugin-area/` (new: `android/`, `ios/`, `src/`, `expo-module.config.json`, `package.json`), `mobile/plugins/withPluginArea.ts`, `mobile/app.config.ts`, `mobile/package.json`, `mobile/scripts/build-flutter.sh` | 4, 5 |
| `.github/workflows/*.yml` (the two phone jobs, the Linux client job), `.github/check.sh` | 5 |
| `mobile/src/app/dev/plugin-area.tsx` (new), `.superpowers/sdd/plugins-m4/gates.md` | 6 |
| `mobile/src/core/{api,native}.ts`, `src/core/fake/{FakeCore,world}.ts`, `src/core/fake/fixtures/` (new), `src/features/plugins/{viewPump,viewSession,limits}.ts` (new) | 7 |
| `mobile/src/state/{types,events,store,selectors}.ts`, `src/state/actions/plugins.ts` (new), `src/core/boot.ts` | 8 |
| `mobile/src/features/plugins/{PluginArea,hostCalls,areaSetup}.tsx/.ts` (new) | 11 |
| `mobile/src/features/server/{ChannelRow,ChannelList,ContentPane}.tsx`, `src/features/plugins/PluginPane.tsx` (new) | 12 |
| `mobile/src/features/plugins/warmUp.ts` (new), `src/features/server/ServerScreen.tsx` | 13 |
| `mobile/src/features/voice/ControlBar.tsx`, `src/features/plugins/ActivityPicker.tsx` (new) | 14 |
| `mobile/src/features/voice/{VoicePane,tileGrid}.ts(x)`, `src/features/plugins/{ActivityTile,ActivityStage,callInput}.ts(x)` (new) | 15 |
| `docs/mobile.md`, `docs/plugins.md`, `docs/decisions.md`, `.superpowers/sdd/plugins-m4/pr_body.md` | 16, 17 |

---

## Stage A — The pipe, proven

Stage A builds the thinnest real version of every layer, on both systems, and measures it. Nothing in it is thrown away. If a gate in Task 6 fails, stop: the later stages assume this pipe.

### Task 1: The facade's view calls

Read `crates/opencord-core/src/api/views.rs` (the Flutter API: this task gives the phone the same calls), `crates/opencord-mobile/src/api/setup.rs` (`events_attach` and `next_events` are the pattern for pulling), `src/mobile.rs` and `src/error.rs`.

**Files:** `crates/opencord-mobile/src/api/plugins.rs` (new), `src/api/mod.rs`, `src/mobile.rs`, `src/types.rs`.

**Produces** (each `#[uniffi::export]`, errors as `MobileError`, ids as decimal strings parsed with `parse_id`):

```rust
pub fn set_plugin_features(features: Vec<String>) -> Result<(), MobileError>;
/// A new generation of the view stream; an older `next_view_outputs` returns empty at once.
pub fn view_outputs_attach() -> Result<u64, MobileError>;
/// Waits for at least one batch, then returns what is ready, up to `max` (1–256), in order.
pub async fn next_view_outputs(generation: u64, max: u32) -> Result<Vec<ViewOutputRecord>, MobileError>;
pub async fn view_open(server_key: String, channel_id: String, context_json: String) -> Result<String, MobileError>;
pub async fn view_open_activity(server_key: String, session_id: String, channel_id: String,
    activity_key: String, context_json: String) -> Result<String, MobileError>;
pub fn view_input(view_id: String, json: String) -> Result<(), MobileError>;
pub fn view_close(view_id: String) -> Result<(), MobileError>;
pub async fn plugin_asset_file(server_key: String, plugin_id: String, path: String) -> Result<String, MobileError>;
pub async fn plugin_image_file(server_key: String, plugin_id: String, source_json: String) -> Result<String, MobileError>;
pub async fn view_upload(view_id: String, node_id: u64, path: String) -> Result<(), MobileError>;

#[derive(uniffi::Record)] pub struct ViewOutputRecord { pub view_id: String, pub json: String }
```

`Mobile` holds the view stream the way it holds events: `view_outputs_attach` takes `client.view_outputs()` (which replaces any earlier receiver) and bumps a generation; a waiting `next_view_outputs` of an older generation returns an empty list.

**A JavaScript reload leaves views open in the core with nobody to draw them.** `Mobile` keeps the ids `view_open` and `view_open_activity` returned and `view_close` removed; `view_outputs_attach` closes every one still in that set before it returns, so a new JavaScript start begins with none.

- [ ] **Step 1: Write failing tests** in `src/api/plugins.rs` (`#[cfg(test)]`), against the crate's existing test support (`rg -n "cfg\(test\)" crates/opencord-mobile/src` for how others reach a `Mobile` without a server): `a_newer_attach_ends_the_older_wait` (the older call returns empty; the newer one doesn't); `attach_closes_views_left_by_an_older_generation` (the set of open ids is empty after it, and holds an id again after a new open: test the set's bookkeeping directly if a view can't be opened without a server); `a_bad_id_is_invalid_input` (`view_open` with `channel_id = "x"`); `features_are_checked_by_the_core` (33 characters → the core's error kind, unchanged).
- [ ] **Step 2: Run** `cargo test -p opencord-mobile plugins` and paste the failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, `cargo clippy -p opencord-mobile --tests`, then `cd mobile && yarn core:android && scripts/check-bindings.sh` and `yarn typecheck`.
- [ ] **Step 5: Commit** `feat(mobile-core): plugin views for the phone` (the generated bindings that are tracked go in the same commit).

### Task 2: A shared package for what both hosts need

The phone's Flutter module can't depend on the desktop app, yet needs what it owns: the plugin icon names, the markdown parser and the fonts. The parser moves to a package and the desktop's three files stay as one-line re-exports; the icon names and fonts are copied. No desktop import and no desktop golden changes.

**Files:** `app/packages/opencord_plugin_host/` (new: `pubspec.yaml`, `lib/opencord_plugin_host.dart`, `lib/src/{plugin_icon_code_points,markdown,fences,mention_tokens,markdown_text}.dart`, `assets/fonts/`, `test/` with a `flutter_test_config.dart` that loads the fonts as `app/test/flutter_test_config.dart` does), the three desktop files that move, `app/test/unit/plugins/plugin_icon_names_test.dart` (new), `app/pubspec.yaml`.

**Produces:**
- `pluginIconCodePoints` (`Map<String, int>`): each plugin icon name and its code point in Material Symbols Rounded, copied from the desktop's `pluginIcons` (`app/lib/features/plugins/plugin_icons.dart`), which **stays where it is and unchanged**: the desktop's tests load that font by family name, and an `IconData` from another package would draw as boxes in its goldens. A desktop test keeps the two lists the same.
- Copies of the desktop's font files (`app/assets/fonts/`: Material Symbols Rounded, Inter, JetBrains Mono) under the package's `assets/fonts/`, for the phone's module to declare. The package itself declares no font.
- `parseMarkdown` and its block and span types, moved with `fences.dart` and `mention_tokens.dart` (all three import only Flutter's foundation and each other; if one turns out to import more of the app, move that too or stop and rule). The three desktop files become `export 'package:opencord_plugin_host/…';`.
- `MarkdownText({required String markdown, required TextStyle style, required Color linkColor, required Color codeBackground, String? Function(int id)? userName, String? Function(int id)? channelName, void Function(String url)? onLink})`: a widget drawing the parser's blocks with plain Flutter text: paragraphs, bold, italic, strikethrough, inline code, fenced code (monospace on `codeBackground`, no highlighting), quotes, lists, links (tappable, `onLink`), and mentions by name. It is the phone's `richText`; the desktop keeps `MarkdownView`.

- [ ] **Step 1: Move the three parser files with `git mv`,** add the re-exports, copy the fonts and the code points, write `plugin_icon_names_test.dart` (`the phone's icon names are the desktop's`: same keys, and each code point equals the desktop's `IconData.codePoint`), and run `cd app && flutter analyze && flutter test`. Expected: every test passes and no golden changes (`git status --short app/test` shows nothing). This step has no failing test: it is a move, and the desktop's suite is its test.
- [ ] **Step 2: Write failing tests** in the package's `test/markdown_text_test.dart`: one per construct above (the text appears; a link's tap calls `onLink` with its url; a mention shows the name `userName` gives, and `@unknown` when it gives null), and goldens `markdown_text_{dark,light}.png` of one message holding every construct.
- [ ] **Step 3: Run them and paste the failures.**
- [ ] **Step 4: Implement `MarkdownText`,** open both goldens and look.
- [ ] **Step 5: Checks:** the package's and the desktop's (Commands).
- [ ] **Step 6: Commit** `refactor(app): the plugin host's icons and markdown parser in a shared package`.

### Task 3: The Flutter module, drawing batches

**Files:** `mobile/flutter_module/` (made with `flutter create -t module --org dev.opencord opencord_plugin_area`, then renamed to the folder above; commit what the template says to commit, ignore `.android`, `.ios`, `build`).

`pubspec.yaml` depends on `opencord_blocks` and `opencord_plugin_host` by path (`../../app/packages/…`), and on nothing else. It declares the families `MaterialSymbolsRounded`, `Inter` and `JetBrainsMono` from the shared package's files (`packages/opencord_plugin_host/assets/fonts/…`), with the weights `app/pubspec.yaml` declares, so text measures as on the desktop.

**Produces:**
- `lib/main.dart`: `main()` runs `PluginAreaApp`.
- `lib/src/channel.dart`: `AreaChannel`, the Dart end of "The plugin area channel": `Stream<String> batches`, `ValueListenable<AreaSetup?> setup`, `void sendInput(String json)`, `void sendStatus(AreaStatus status)`, `Future<Object?> hostCall(String method, Object? args)`, and it answers `back`. It calls `ready` once its handlers are set.
- `lib/src/area.dart`: `PluginAreaApp`: a `MaterialApp` as the renderer's own tests wrap it (`app/packages/opencord_blocks/test/support.dart:181`), no debug banner, a transparent background, brightness from `setup`; a `MediaQuery` over the view carrying `setup`'s text scale and safe area; one `BlocksController` fed by `batches`, its `inputs` sent with `sendInput`; `BlocksView(controller, host, theme)` with the theme built from `setup.colors` the way `buildBlocksTheme` does. Until `setup` arrives it draws nothing.
- `lib/src/status.dart`: `AreaStatus` and `statusOf(BlocksController)` (the table under "Status").
- In this task `host` is `PhoneBlocksHost` with `icon` (from `pluginIconCodePoints`) and `richText` (`MarkdownText`, ids shown as they are) real. Until Task 9 the other members answer what the renderer treats as nothing: `image` and `font` null, `user` the id as text, `pickFile` null, `confirmLink` false, and `copy`, `openChannel`, `upload` and `playSound` do nothing but log the method's name. Task 6's gates run on this, so nothing may throw.

- [ ] **Step 1: Write failing tests** (`test/area_test.dart`, with a `test/flutter_test_config.dart` loading the fonts), driving the channel through `TestDefaultBinaryMessengerBinding` with batches built by the renderer's own test helpers (`batch`, `create`, `roots` in `app/packages/opencord_blocks/test/support.dart`: copy the helpers into the module's `test/support.dart`), a text and a button: `nothing is drawn before setup`; `a batch after setup draws its text`; `pressing the button sends its input` (the `input` method is called with the wire format's `event` input for that node); `the status goes loading, then running`; `a closed op reports closed with its reason`; `an error op reports error`; `ready is sent once`.
- [ ] **Step 2: Run** `flutter test` in the module and paste the failures.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Checks:** the module's (Commands).
- [ ] **Step 5: Commit** `feat(mobile): a Flutter module that draws plugin areas`.

### Task 4: The native module, Android

Read `mobile/modules/voice-service/` whole (the app's one native module and native view: the pattern for the Gradle file, the module definition and the config file) and the add-to-app documentation for Android (engine groups; a Flutter view inside another app's view).

**Files:** `mobile/modules/plugin-area/` (`package.json` named `@opencord/plugin-area`, `expo-module.config.json`, `android/build.gradle`, `android/src/main/java/dev/opencord/pluginarea/{PluginAreaModule,PluginAreaView,Areas}.kt`, `src/{index.ts,PluginAreaView.tsx,PluginArea.types.ts}`), `mobile/scripts/build-flutter.sh`, `mobile/plugins/withPluginArea.ts`, `mobile/app.config.ts`, `mobile/package.json`.

**Produces, in JavaScript** (`@opencord/plugin-area`):

```ts
export const available: boolean;               // false when the native module isn't in the build
export function warmUp(): void;                // makes the engine group and one spare engine
export function coolDown(): void;              // drops the spare engine
export function createArea(areaId: string, setupJson: string): Promise<void>;  // resolves at Dart's `ready`
export function destroyArea(areaId: string): void;
export function destroyAllAreas(): void;        // every area an earlier JavaScript start left behind
export function setSetup(areaId: string, setupJson: string): void;
export function pushBatch(areaId: string, json: string): void;
export function hostReply(areaId: string, id: number, json: string): void;
export function back(areaId: string): Promise<boolean>;
export function addInputListener(l: (e: { areaId: string; json: string }) => void): Subscription;
export function addStatusListener(l: (e: { areaId: string; json: string }) => void): Subscription;
export function addHostCallListener(l: (e: { areaId: string; id: number; method: string; json: string }) => void): Subscription;
export function PluginAreaView(props: { areaId: string; style?: ViewStyle }): JSX.Element;
```

**How it works:**
- `Areas` (Kotlin, one per process) owns a `FlutterEngineGroup` and a map `areaId → engine + channel + held batches`. `createArea` takes the spare engine when there is one, else spawns one. `pushBatch` before Dart's `ready` appends to the held list; `ready` sends them in order and resolves `createArea`.
- **An area lives from `createArea` to `destroyArea`, whatever views exist.** `PluginAreaView` attaches a Flutter view to its area's engine when it is mounted and detaches when unmounted or given another `areaId`; the engine and its Dart state go on. At most one native view is attached to an area; a second one takes it over.
- The Flutter view is drawn so React Native's sheets, animations and rounded clips work over it (the texture render mode, transparent).
- `build-flutter.sh android [--release]` runs `flutter build aar` for the module (debug only without `--release`; debug, profile and release with it) into `mobile/flutter_module/build/host/outputs/repo`. `withPluginArea.ts` adds that folder and Flutter's engine repository to the Android project's repositories at prebuild; the module's `build.gradle` takes the AAR per build type.
- `available` is false, and every function a no-op, when the native module is missing (`requireOptionalNativeModule`), so jest and a build without Flutter still run.

- [ ] **Step 1: Write failing tests:** `mobile/plugins/__tests__/withPluginArea.test.ts` (the config gains both repositories exactly once when applied twice); `mobile/modules/plugin-area/src/__tests__/index.test.ts` (without the native module: `available` is false, each function returns without throwing, `createArea` rejects with `unavailable`).
- [ ] **Step 2: Run** `yarn test` and paste the failures.
- [ ] **Step 3: Implement** the script, the config plugin, the Kotlin and the TypeScript.
- [ ] **Step 4: Build and run:** `mobile/scripts/build-flutter.sh android`, then `cd mobile && npx expo prebuild -p android --no-install && yarn android:fake`. The app starts as before. Paste the last lines of each.
- [ ] **Step 5: Native test of the held batches** (`android/src/test/.../AreasTest.kt`, or, if the module has no JVM test setup and adding one costs more than this test, a ruling plus the same check done in Task 6's dev screen and pasted): `batches before ready are held in order` (three pushed, then `ready`: the channel got 1, 2, 3; a fourth goes straight through).
- [ ] **Step 6: Checks:** `cd mobile && yarn typecheck && yarn lint && yarn test`.
- [ ] **Step 7: Commit** `feat(mobile): a native module hosting plugin areas (Android)`.

### Task 5: The native module on iOS, and CI

Nothing here can be run on this machine. Write it from the add-to-app documentation for iOS and from `mobile/modules/voice-service/ios/`; CI's iOS job is its compiler.

**Files:** `mobile/modules/plugin-area/ios/{PluginAreaModule.swift,PluginAreaView.swift,Areas.swift,PluginArea.podspec}`, `expo-module.config.json`, `mobile/scripts/build-flutter.sh` (the `ios` case), `.github/workflows/` (the two phone jobs and the Linux client job), `.github/check.sh`.

- `Areas.swift` mirrors `Areas.kt`: a `FlutterEngineGroup`, areas by id, held batches, the same channel. `PluginAreaView` (an `ExpoView`) embeds the area's `FlutterViewController` as a child of the nearest view controller while mounted and removes it when unmounted, leaving the engine.
- `build-flutter.sh ios [--release]` runs `flutter build ios-framework` into `mobile/modules/plugin-area/ios/Frameworks/` (ignored by git), keeping one configuration: Release with the flag, Debug without. The podspec vendors what is there (`Flutter.xcframework`, `App.xcframework`).
- **CI:** both phone jobs install Flutter (`subosito/flutter-action@v2` with `env.FLUTTER_VERSION`, as the client jobs do) and run `mobile/scripts/build-flutter.sh android --release` / `ios --release` before `expo prebuild`. The Linux client job, and `check.sh`'s client part, run `flutter analyze` and `flutter test` in `mobile/flutter_module` and `app/packages/opencord_plugin_host`.
- **Licences:** find how the phone app lists licences (`mobile/scripts/licenses.js`, `docs/mobile.md`) and how the desktop does (`app/assets/licenses/`), and add the Flutter engine's to the phone's list the same way.

- [ ] **Step 1: Write the Swift, the podspec, the script's `ios` case and the workflow changes.**
- [ ] **Step 2: Run** `.github/check.sh` and paste its last lines.
- [ ] **Step 3: Commit** `feat(mobile): plugin areas on iOS; CI builds the Flutter module`. Don't push yet: Task 6 comes first.

### Task 6: The gates

A developer screen that opens one plugin channel through every real layer, and the measurements that decide whether the rest of the plan stands.

**Files:** `mobile/src/app/dev/plugin-area.tsx` (the app has a `dev` folder; follow what is there), `mobile/src/core/{api,native}.ts` (only the calls Task 1 added), `.superpowers/sdd/plugins-m4/gates.md`.

The screen takes a server key and a plugin channel id, sets the features, and wires by hand what Task 7 will wire properly: attach, pull batches, `createArea`, push, forward inputs, `viewOpen` with the first `context` input. It shows the status JSON as text above the area and has two developer switches: **Busy JS** (blocks the JavaScript thread for 30 ms out of every 50) and **Second area** (opens the same channel in a second area below).

**Set up:** a local server (`docs/plugins.md` says how to run one with dev mode) with `examples/plugins/tickets` and `examples/plugins/paint` installed and a plugin channel of the tickets type made from the desktop app or the test support; the emulator reaches it at `10.0.2.2`.

**Record real batches while here** (rule 3's fixtures): log each batch's JSON for one tickets session (open, press one button, open one modal, close it) to a file and save them as `mobile/src/core/fake/fixtures/tickets-<n>.json`, with the inputs that caused them. Tasks 7, 9 and 11 use them.

| Gate | Measured how | Passes when |
|---|---|---|
| G1 It draws and answers | Open the tickets channel; press a button that changes the screen | The change shows. Screenshot pasted in `gates.md`. |
| G2 The road is fast enough | In Dart, the time from `sendInput` of a press to the next `batch` applied, 100 presses, idle and with **Busy JS** on | Idle: median ≤ 25 ms, 95th ≤ 60 ms. Busy: 95th ≤ 150 ms. |
| G3 A canvas animates | A view with `app.on_frame` moving one rect (a ten-line client script pushed with `opencord-plugin dev`; not committed) | ≥ 30 frames a second idle, counted in Dart over 10 s. |
| G4 Start-up | First `createArea` to `ready`, cold and after `warmUp()`, 5 runs each | Cold ≤ 2,000 ms; warm ≤ 400 ms. |
| G5 Size | The release APK per processor type, before and after (`build-flutter.sh android --release`, then the release build) | Growth ≤ 12 MB each. The number goes in the pull request. |
| G6 Two areas | **Second area** on: both draw, both answer; memory before and after (`adb shell dumpsys meminfo dev.opencord.mobile`) | Both work; the growth is recorded. |
| G7 Keyboard and back | In tickets (it has three `ui.text_input`s and one `ui.modal`): focus a text field, type, close the keyboard; open the modal, press back | The field stays visible above the keyboard and the text reaches the plugin; back closes the modal, and a second back leaves the screen. |
| G8 Both systems build | Push; the pull request's checks | Phone (Android) and Phone (iOS) pass. |

- [ ] **Step 1: Build the screen** and record the fixtures.
- [ ] **Step 2: Measure G1–G7** on the emulator, writing each number and screenshot into `gates.md`.
- [ ] **Step 3: Commit** `feat(mobile): a developer screen for plugin areas`, push, open the draft pull request (title `feat: plugins on the phone (milestone 4)`), and read its checks to the end for G8.
- [ ] **Step 4: Decide.** Every gate passed: write `Stage A: gates passed` with the table in the ledger and go on. **Any gate failed: stop and report to the owner,** with the numbers, what was tried, and, for G2 or G3, an estimate of the alternative (a C interface on `opencord-mobile` that the native module calls directly, so batches skip JavaScript). Emulator numbers run slower than a phone's; say so, don't adjust the gates.

**At the end of stage A:** full checks; rule 8's ledger entry.

## Stage B — The phone knows plugins

### Task 7: The view pump, the view session, and the fake

**Files:** `mobile/src/core/{api,native}.ts`, `src/core/fake/{FakeCore,world}.ts`, `src/features/plugins/{viewPump,viewSession,limits}.ts`.

**Produces:**
- `CoreApi` gains every call of Task 1 (and Task 8's), named in camelCase as the rest.
- `startViewPump(core: CoreApi, area: AreaModule): ViewPump`, written after `startEventLoop` (`src/core/eventLoop.ts`): first `area.destroyAllAreas()` (areas a JavaScript reload left behind; the core closes their views on attach, Task 1), then it attaches, loops on `nextViewOutputs(generation, VIEW_BATCH_MAX)`, retries a failure with the same backoff, ends on an empty batch. `pump.listen(viewId, onBatch): () => void`; `pump.hold(): Hold` starts keeping every batch whose view has no listener until `hold.release(viewId | null)` hands the kept ones for that id to its listener in order and drops the rest.
- `class ViewSession` (one open view, the phone's `pluginViewProvider`): `new ViewSession({ core, pump, area: AreaModule, place: ViewPlace, setupJson })`, where `ViewPlace = { kind: 'channel'; serverKey; channelId } | { kind: 'activity'; serverKey; sessionId; channelId; activityKey }`. It creates the area, waits for Dart's first `context` input, opens the view with it (holding early batches as above), then forwards batches to `pushBatch` and inputs to `viewInput`. `session.status` (a zustand-readable snapshot of the last status), `session.viewId`, `session.send(input)` (for `call` and lifecycle inputs, which React Native originates), `session.close()` (closes the view, destroys the area, stops listening; safe to call twice and while opening).
- `FakeCore` implements the calls: `viewOpen` and `viewOpenActivity` return an id and play the recorded tickets fixtures: the opening batches at once, and an input is answered by the batches a recorded input caused when its `kind`, its `event` and its node id equal that recorded input's (an input matching none brings nothing); `pluginAssetFile` and `pluginImageFile` answer a bundled image; a new scenario value isn't needed. `world.ts` gains one plugin channel of the tickets type, one unavailable one, one registered activity (`Paint together`) and, in the default call, no session.

- [ ] **Step 1: Write failing tests** (`src/features/plugins/__tests__/viewPump.test.ts`, `viewSession.test.ts`, with `FakeCore` and a hand-written fake of the area module that records calls and lets the test fire `ready`, inputs and statuses): `batches reach their view's listener in order`; `early batches wait for the view's id` (two batches arrive before `viewOpen` resolves: both are pushed, once, in order, after it); `batches for another view are dropped on release`; `a failed open closes the area and reports error`; `inputs before the view is open are sent after it, in order`; `close while opening closes the view when it arrives`; `close is safe twice`; `a newer pump ends the older` (an empty batch); `starting the pump destroys leftover areas before attaching` (the fake area module saw `destroyAllAreas` before the fake core saw `viewOutputsAttach`).
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Mutations to run and report:** drop the hold (push early batches as they come); release before setting the listener; forget `destroyArea` in `close`.
- [ ] **Step 5: Checks:** the phone's JavaScript checks.
- [ ] **Step 6: Commit** `feat(mobile): plugin view sessions`.

### Task 8: Plugin channels, activities and sessions in the phone's state

Read `crates/opencord-mobile/src/{types,convert,events}.rs` around channels, and `mobile/src/state/{types,events,store,selectors}.ts`. Today `convert.rs` drops plugin channels and `events.rs` drops six plugin events (`rg -n "ChannelKind::Plugin|PluginChannelTypesUpdate|ActivitySession" crates/opencord-mobile/src`).

**Produces, in the facade:**
- `ChannelKind::Plugin`; `ChannelRecord` gains `plugin: Option<PluginChannelRecord>` with `plugin_id`, `type_key`, `state: PluginChannelState` (`Available`, `PluginStopped`, `TypeGone`). `convert::channel` no longer returns `None` for one.
- `PluginChannelTypeRecord`, `PluginActivityRecord`, `ActivitySessionRecord`: the fields of the core's `PluginChannelType`, `PluginActivity` and `ActivitySession` (`crates/opencord-core/src/api/types.rs`), ids as strings, `started_at_ms` as a number.
- `ServerSnapshot` gains `plugin_channel_types`, `plugin_activities`, `activity_sessions`.
- `MobileEvent` gains `PluginChannelTypesUpdated { server_key, types }`, `PluginActivitiesUpdated { server_key, activities }`, `ActivitySessionCreated { server_key, session }`, `ActivitySessionUpdated { server_key, session }`, `ActivitySessionDeleted { server_key, session_id, reason }` (the reason as the core's enum).
- `ChannelAccess` gains `start_activities` and `manage_channel` (from `START_ACTIVITIES` and `MANAGE_CHANNELS`).
- `pub async fn activity_start(server_key, channel_id, plugin_id, activity_key) -> Result<ActivitySessionRecord, MobileError>` and `pub async fn activity_end(server_key, session_id) -> Result<(), MobileError>`. `MobileError` keeps the core's `PluginLimit` and `PluginUnavailable` kinds apart (`rg -n "PluginLimit" crates/opencord-mobile/src/error.rs`; add them if they're folded into another kind).

**Produces, in JavaScript:**
- `ChannelItem` gains `{ kind: 'plugin'; channel; access; typeName: string; icon: string; state: 'available' | 'plugin_stopped' | 'type_gone' }` (name and icon from the server's channel types; the channel's own name when the type isn't registered). Sections place plugin channels by position among text and voice channels, as the desktop does.
- Selectors `pluginActivities(serverKey)`, `activitySessions(serverKey, channelId)` (oldest first), and `canStartActivities(serverKey, channelId)`.
- `src/state/actions/plugins.ts`: `startActivity`, `endActivity`.
- **Features:** at boot, before any connection, `core.setPluginFeatures(available ? ['plugin_users', 'plugin_channels', 'activities'] : ['plugin_users'])` (`src/core/boot.ts`), with `available` from `@opencord/plugin-area`; the fake core always gets all three.

- [ ] **Step 1: Write failing tests.** Rust (`convert.rs`, `events.rs` test modules): `a_plugin_channel_is_kept_with_its_state` (each of the three states); `the_six_plugin_events_reach_javascript` (one per event, field by field); `access_names_start_activities_and_manage_channel`. JavaScript (`src/state/__tests__/plugins.test.ts`, `src/core/__tests__/boot.test.ts`): `a plugin channel sits at its position among the others`; `a type update renames and re-icons its channels`; `a session is created, updated and deleted` (viewer ids follow the update; the delete removes only that session); `features follow the renderer` (`available` false → `['plugin_users']`, and it is called before the first `serverAdd`/connect).
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement,** then `yarn core:android && scripts/check-bindings.sh`.
- [ ] **Step 4: Checks:** `cargo test -p opencord-mobile`, clippy, and the phone's JavaScript checks.
- [ ] **Step 5: Commit** `feat(mobile): plugin channels, activities and sessions in the phone's state`.

**At the end of stage B:** full checks, push (rule 8).

## Stage C — The renderer's host

### Task 9: `PhoneBlocksHost`

Each `BlocksHost` member (`app/packages/opencord_blocks/lib/src/host.dart`), decided. Read the desktop's `app/lib/features/plugins/blocks_host.dart` for what each one does there.

| Member | Where it's answered | How |
|---|---|---|
| `icon(name)` | Dart | an `IconData` of family `MaterialSymbolsRounded` from `pluginIconCodePoints[name]`; null for a name it lacks |
| `richText(markdown, style)` | Dart | `MarkdownText`; names through the cached `names` host call; a link goes to `confirmLink` |
| `user(id, size, showName)` | Dart, data from React Native | host call `user` `{id}` → `{name, avatar_path}`; a round image, or the initial on the theme's avatar colour; cached per area |
| `image(source)` | React Native | host call `image` `{source}` → `{path}`; `attachmentFile` for `{attachment}`, else `pluginImageFile`; drawn with `FileImage` |
| `font(file)` | React Native | host call `asset` `{path: "assets/fonts/<file>"}` → `{path}`; the bytes are read in Dart |
| `pickFile({image, accept})` | React Native | host call `pick` → `{path}` or `{}`; the image picker for `image`, else the document picker; no timeout |
| `upload(nodeId, path)` | React Native | host call `upload` `{node_id, path}` → `viewUpload` |
| `confirmLink(url)` | React Native | host call `link` `{url}` → `{opened}`; the app's confirm dialog: title `Open this link?`, the url as its body, buttons `Open` and `Cancel`; then `Linking.openURL` |
| `copy(text)` | React Native | host call `copy` `{text}`; the clipboard, then the toast `Copied` |
| `openChannel(id)` | React Native | host call `open_channel` `{id}`; the app's channel navigation; an unknown or hidden channel shows the toast the app already uses for that |
| `playSound(asset, volume, onEnd)` | React Native | host call `sound` `{asset, volume}`, resolving when it ends: `pluginAssetFile`, played with `expo-audio`; `onEnd` runs on any outcome |

**Files:** `mobile/flutter_module/lib/src/phone_host.dart`, `test/phone_host_test.dart`. (React Native's side is Task 11.)

- [ ] **Step 1: Write failing tests,** each driving the channel and answering `hostCall` from the test: one per member (the method name and arguments sent are exactly the table's; the reply's value is used: the image's file is the one drawn, the user's name is the one shown); `a host call that errors draws the block's fallback, not an exception`; `a host call with no reply times out after hostCallTimeout, except pick`; `names and users are asked once per id`.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Checks:** the module's.
- [ ] **Step 5: Commit** `feat(mobile): the plugin area's host calls`.

### Task 10: Status, back, setup changes, and how it looks on a phone

**Files:** `mobile/flutter_module/lib/src/{area,status,channel}.dart`, `test/`.

- **Back:** `back` closes the top dialog, else the top modal or sheet when it is `dismissible` (§12.5), and answers true; with none open, or a top one that isn't dismissible, false. Find how the renderer closes one on Esc (`rg -n "escape|dismiss" app/packages/opencord_blocks/lib/src`) and use the same path, adding a method on `BlocksController` only if there is none.
- **Setup changes** (theme switched, text scale or safe area changed, the area resized or rotated) redraw without losing state; a size change sends the wire format's `context` input once per change.
- **Compact:** at a phone's width a `modal` shows as a sheet (§12.5); check it does, since no desktop width reached it.
- **Touch:** every block is usable by touch at a phone's size: targets at least 44 points high where the block is a control. Fix in `opencord_blocks` only what a failing test here proves, and run the desktop's goldens after.

- [ ] **Step 1: Write failing tests:** `back closes the top overlay first` (a modal over a sheet: two backs close both, the third answers false); `a modal that isn't dismissible keeps back` (answers false, stays open); `the status counts overlays`; `a new setup recolours without a new context input`; `a new size sends one context input`; `a modal is a sheet at 390 wide`; goldens of the tickets fixtures at 390×760 and 820×1180 (a tablet), dark and light, and of each block family (`rg -n "class .*Block" app/packages/opencord_blocks/lib/src/blocks` for the list; reuse the package's own test trees) at 390 wide.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement,** then open every golden at full size and fix what looks wrong on a phone.
- [ ] **Step 4: Checks:** the module's, `opencord_blocks`' and the desktop's.
- [ ] **Step 5: Commit** `feat(mobile): back, status and phone sizes in the plugin area`.

**At the end of stage C:** full checks, push, and the review of stages A–C.

## Stage D — The screens

Read `mobile/src/ui/` (the controls), `src/features/server/` and `src/features/voice/` whole before Task 11, and the M1.5 plan's phone tasks for how the last plugin feature was built on the phone (`docs/superpowers/plans/2026-11-09-plugins-m1-5-commands.md`).

### Task 11: `PluginArea`, the component

**Files:** `mobile/src/features/plugins/{PluginArea.tsx,hostCalls.ts,areaSetup.ts}`.

**Produces:**
- `areaSetup(theme, insets, textScale, reduceMotion, serverKey, pluginId): string`: the setup JSON, its colours from the phone's theme tokens.
- `answerHostCalls(session, deps)`: answers each host call of Task 9's table with the phone's own picker, dialog, toast, navigation and audio, and replies `{"error":"<kind>"}` on failure. A call for an area that is gone is dropped.
- `<PluginArea session paused onLeave />`: the native view over the session's area while `status.state` is `running`, and otherwise, drawn by React Native in the area's place:

  | State | Shows |
  |---|---|
  | `loading` | A spinner, centred |
  | `error` | `This plugin hit an error`, a button `Reload` (closes the session and opens a new one), and, when `detail` isn't empty, the detail in the monospace font under it, scrollable |
  | `too_old` | `Update Opencord to open this` |
  | `closed` with `plugin_stopped`, `lost_access`, `channel_deleted` or `unavailable` | `This channel is unavailable` |
  | `closed` with `activity_ended` | `This activity ended` |
  | `closed` with `left_call` | nothing: the caller unmounts it |
  | `closed`, anything else | `This plugin closed this screen`, and `Reload` |

  The reasons are the core's `CloseReason` as the `closed` op spells them (`rg -n "CloseReason" crates/opencord-core/src/views/client.rs crates/opencord-plugins/src/view.rs`): check each spelling there before using it; a reason the table doesn't name falls in the last row.
- **Pause and lifecycle:** `paused` true, or the app in the background, sends `{"kind":"lifecycle","event":"pause"}` through `session.send`, and `…"resume"` when both end; exactly one each per change (the desktop's `_tellPaused` in `app/lib/core/providers/plugin_views.dart` is the model). **Back:** while mounted and not paused, the Android back button asks `back(areaId)` first and runs `onLeave` only when it answers false; on iOS the pane's swipe-back is off while `status.overlays > 0`.
- **The keyboard:** the area's bottom inset in the setup follows the keyboard's height (`react-native-keyboard-controller`, as the chat composer does), so the plugin's focused field scrolls above it.

- [ ] **Step 1: Write failing tests** (`__tests__/PluginArea.test.tsx`, `hostCalls.test.ts`; the fake core, the fake area module, React Native Testing Library): one per row of the states table (the words appear; `Reload` closes one session and opens another); `the native view shows only while running`; `an area outlives its native view and is destroyed with its session` (unmount the component: `destroyArea` isn't called; `session.close()`: it is, once); `pause and resume are sent once per change` (paused, then background, then foreground, then unpaused: one `pause`, one `resume`); `back asks the area first`; one per host call (the phone's control was used and the reply has the table's shape; a cancelled picker replies `{}`); `a refused upload replies an error`.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Mutations:** show the native view while loading; call `destroyArea` on unmount; send `pause` on every render.
- [ ] **Step 5: Checks:** the phone's JavaScript checks.
- [ ] **Step 6: Commit** `feat(mobile): the plugin area and its states`.

### Task 12: Plugin channels

**Files:** `mobile/src/features/server/{ChannelRow,ChannelList,ContentPane}.tsx`, `src/features/plugins/PluginPane.tsx`, `src/features/server/useServerPanes.ts`.

**What's shown, and what each control does (rule 6):**
- **The row:** the type's icon (the plugin's asset through `pluginAssetFile`, 20 points, or the `extension` icon Task 14 adds to `mobile/src/ui/icons.ts` (add it here, since this task comes first) when it has none or it fails), then the channel's name; no unread dot ever (§14.5). An unavailable channel's row is at the app's disabled opacity. Tapping an available one opens it as any channel. Tapping an unavailable one opens nothing and shows a toast: `This channel is unavailable` for `plugin_stopped`, `This plugin no longer provides this channel` for `type_gone` (§14.4).
- **The pane** (`PluginPane`, `ContentPane`'s third kind): the app's pane header with the channel's name and the sidebar button, as `ChatPane`'s; under it `<PluginArea>` filling the rest, down to the safe area. One `ViewSession` per open pane, made when the pane mounts and closed when it unmounts (leaving the channel closes the view, §14.3). While the phone's sidebar covers the pane it is `paused`.
- A channel that becomes unavailable while open shows `PluginArea`'s closed state; one that becomes available again while its pane is open opens a fresh session.

- [ ] **Step 1: Write failing tests** (`src/features/server/__tests__/`, `src/features/plugins/__tests__/PluginPane.test.tsx`): `a plugin channel row shows its type's icon and no unread dot`; `tapping an unavailable channel says why and opens nothing` (both sentences); `opening one draws the plugin's screen` (the fixture's text appears through the fake area's status going `running`: assert the native view is mounted with that session's area id and the fake core got `viewOpen` with **that** channel's id, using two plugin channels); `leaving closes the view` (the fake core got `viewClose` with the id it returned); `the sidebar over the pane pauses it`; `a channel that comes back opens again`.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement,** then `yarn android:fake` and `screens.sh m4-12` for: the channel list with both fake plugin channels; the tickets channel open; its modal open; the unavailable toast. Dark and light, phone and tablet. Look at each.
- [ ] **Step 4: Checks.** **Step 5: Commit** `feat(mobile): plugin channels on the phone`.

### Task 13: Warming the engine

**Files:** `mobile/src/features/plugins/warmUp.ts`, `src/features/server/ServerScreen.tsx`, the voice join action.

`useEngineWarmUp()`: calls `warmUp()` when the open server has at least one plugin channel type or registered activity, or the device is in a call; calls `coolDown()` 60 seconds after neither holds. It never runs when `available` is false.

- [ ] **Step 1: Write failing tests** (fake timers): `opening a server with plugins warms up once`; `a server without plugins doesn't`; `joining a call warms up`; `it cools down 60 s after the last reason goes, and not before`; `a reason coming back cancels the cool-down`.
- [ ] **Step 2: Run, paste.** **Step 3: Implement.** **Step 4: Checks.** **Step 5: Commit** `feat(mobile): warm the plugin engine before it's needed`.

### Task 14: The Activities button and picker

Read §15.2, `mobile/src/features/voice/ControlBar.tsx` (the soundboard button is the pattern) and how the soundboard's sheet is opened.

**What's shown, and what each control does:**
- **The button:** in the call's control bar after the soundboard button, an `extension` icon (Material Symbols' `extension`, added to `mobile/src/ui/icons.ts` the way its other names are), accessibility label `Activities`, `testID="control-activities"`. Shown when all hold: the server has at least one registered activity; `canStartActivities` for the call's channel; `available`. Pressing opens the picker.
- **The picker:** the app's sheet, titled `Activities`. One row per registered activity, by plugin name then name: its icon (the plugin's asset, 40 points with radius 10, or the `extension` icon), the name, and under it the description, or the plugin's name when the description is empty; one line each. A row whose activity already runs in this call shows `Running` at its right. Pressing a row starts it (`startActivity` with the call's channel, the row's plugin id and key), closes the sheet, and opens the session it returns (Task 15), running or new.
  - While the call has 4 sessions, rows that aren't running are disabled, and a line under the list reads `This call already has 4 activities.`
- **When starting fails,** a toast: the plugin-unavailable kind → `This activity isn't available right now.`; the plugin-limit kind → `This call already has 4 activities.`; forbidden → `You don't have permission to start activities here.`; anything else → the error's message.

- [ ] **Step 1: Write failing tests** (`src/features/plugins/__tests__/ActivityPicker.test.tsx`, the control bar's test file): `the button shows only with an activity, the permission and the renderer` (four cases); `pressing a row starts it in the call's channel` (two activities: the fake core got the second's plugin id and key); `a running activity says Running`; `a full call disables the rest`; `a refused start says why` (the three kinds); `starting opens what came back`.
- [ ] **Step 2: Run, paste.**
- [ ] **Step 3: Implement,** then `screens.sh m4-14`: the control bar; the picker with one, with six, with one running, at the cap. Both themes, phone and tablet. Look.
- [ ] **Step 4: Checks.** **Step 5: Commit** `feat(mobile): start an activity from the call`.

### Task 15: Activity tiles and the opened activity

Read §15.2–§15.4, `mobile/src/features/voice/{VoicePane,tileGrid,ParticipantTile,CallSheet}.tsx` whole.

**Produces:**
- **State:** `openActivity: { serverKey, channelId, sessionId } | null` in the store, with its `ViewSession` kept beside it (not in the store): at most one. Opening another closes the first. It is cleared, and its session closed, when the device leaves the call, when the session is deleted, and when its view reports `closed`.
- **The tile** (`ActivityTile`), in the call's grid after every person, oldest session first; `tileGrid` counts them: the session's icon (40 points, radius 10, the same fallback), its name, `Started by <name>`, and `No one has it open` / `1 person has it open` / `<n> people have it open`; as a tile gets short the icon goes first, then the viewers line, then the starter (never smaller text). Tapping opens it. A long press opens the app's menu: `Open` or `Close`, and, for the starter or a member with `manage_channel` there, `End activity`.
- **The opened activity** (`ActivityStage`), in place of the grid, in the voice pane and in the call sheet alike (whichever is on screen draws it; the other, if mounted, doesn't): the people in one horizontal strip of small tiles above (56 points high, scrolling, the speaking ring kept); a bar with the icon, the name, a `⋯` button (the same menu, with `Close`) and a close button (accessibility label `Close`); under it `<PluginArea>` for the session, to the bottom safe area, above the control bar. `Close` closes this device's view; the tile returns.
- **End activity** asks in the app's confirm dialog: title `End <name> for everyone?`, body `It closes for everyone in the call.`, buttons `End activity` and `Cancel`; confirming calls `endActivity`; a refusal shows the error's message as a toast.
- **Pause:** the opened activity is `paused` whenever no `ActivityStage` for it is on screen (another channel, the call sheet closed, the sidebar over the pane), and resumed when one is again; its session and engine stay.
- **The call input** (`callInput.ts`): for the opened view, `{"kind":"call","members":[…],"speaking":[…]}` (ids as decimal strings, each list sorted as numbers, no repeats), sent with `session.send` once the view is open and then when either list changes, at most once every `CALL_INPUT_EVERY_MS` with the newest lists, from the call's members in the store. It goes on while paused.

- [ ] **Step 1: Write failing tests** (`src/features/plugins/__tests__/{ActivityTile,ActivityStage,callInput}.test.tsx`, `src/features/voice/__tests__/`), each naming its road (fake core → store → `VoicePane`): `a session adds a tile after the people`; `its tile says who started it and how many have it open` (0, 1 and 3); `tapping the tile opens the activity over the stage` (the fake core got `viewOpenActivity` with the session's id, the call's channel and the activity's key); `the people stay in a strip above it`; `Close closes my view and the tile returns` (`viewClose`; the session is still in the store); `opening a second activity closes the first`; `End activity asks, then ends it for everyone` (Cancel ends nothing; the item is absent for someone who is neither the starter nor a channel manager); `an open activity survives leaving the call's pane` (open it, go to a text channel, come back: the same view id and area id, exactly one `pause` then one `resume`, `destroyArea` never called); `the call input follows who is in the call and who speaks` (one on open with both lists; a member joining sends another; ten speaking changes within 100 ms send one more, with the last set); `leaving the call closes the opened activity`; `an ended activity closes its screen and its tile goes` (the session's delete: the stage shows `This activity ended` until dismissed or 3 s pass, then the grid); `the voice pane and the call sheet never both draw it`.
- [ ] **Step 2: Run them and paste the failures.**
- [ ] **Step 3: Implement,** then `screens.sh m4-15`: the grid with one and with three activities among five people; an activity opened, in the pane and in the call sheet; the long-press menu; the confirm dialog. Both themes, phone and tablet. Look at each at full size.
- [ ] **Step 4: Mutations:** destroy the session when the pane unmounts; send `call` only once; put activity tiles before people; show `End activity` to everyone; skip the `resume`; let both the pane and the sheet mount the area.
- [ ] **Step 5: Checks.** **Step 6: Commit** `feat(mobile): activities in a call on the phone`.

**At the end of stage D:** full checks, push (rule 8).

## Stage E — Live checks and docs

### Task 16: The real thing on the emulator

Everything so far ran on fixtures. This task runs the phone against a real server with the real example plugins, beside a desktop client, and fixes what it finds (each fix with a test that failed first).

**Set up** as in Task 6: the local server with `tickets` and `paint`; the phone emulator signed in as one member. **The second member is a second emulator** (the tablet one, started beside the phone's) when the machine holds both (`free -g` shows at least 6 GB free with one running); steps that say "the second member" are done there. If it can't hold both, or a step needs the desktop app (nobody is at its window in this plan), write that step in the ledger and in `docs/mobile.md`'s hand checks as owed to the owner, and don't claim it.

- [ ] **Step 1: Plugin channel.** On the phone: open the tickets channel; open a ticket from it; type in a field with the keyboard up; open and close its modal with its button and with back. The second member sees the ticket. Turn the plugin off, then on (by whatever the server's tools offer without a person at the desktop: `rg -n "stop|disable" crates/opencord-plugin-cli/src docs/plugins.md`; if only the desktop's Plugins tab can, this is an owed hand check): the phone's open channel shows `This channel is unavailable` and the row greys; on again, tapping opens it.
- [ ] **Step 1b: The host calls tickets doesn't use.** The tickets example has no image, user, rich text, file picker, upload, link, copy or sound. For each, push a client script of a few lines with `opencord-plugin dev` (not committed) that uses that one block or call, and see it work on the phone: an image from the plugin's assets; a `user`; `rich_text` with every construct; a file picker and its upload; a link (the confirm dialog, then the browser); copy (the toast); a sound.
- [ ] **Step 2: Activities.** Join the call on both. Start `Paint together` from the phone's picker: the second member sees the tile. Open it on both; draw on each (`adb shell input swipe` draws a stroke); the other sees every stroke. Close the call sheet and reopen: the drawing is there. Leave the phone's channel pane for a text channel and return. Have the second member end the activity: the phone says `This activity ended`. Start it again; leave the call on the phone with it open; rejoin; the tile is there and opens fresh. (Only one activity is registered by the examples, so opening a second while the first is open stays with Task 15's test.)
- [ ] **Step 3: Rotate the emulator and switch its theme and font size** with a plugin screen open: it redraws and keeps its state.
- [ ] **Step 4: Run the tablet emulator** through Steps 1 and 2's opening parts: the channel list beside the pane, the plugin's screen at tablet width.
- [ ] **Step 5: Write each step's result in the ledger,** with a screenshot per screen (`adb exec-out screencap -p`), each opened and looked at. A step that couldn't be done says why.
- [ ] **Step 6: Commit** each fix as its own commit.

### Task 17: Docs, D97 and the pull request

**Files:** `docs/mobile.md`, `docs/plugins.md`, `docs/decisions.md`, `.superpowers/sdd/plugins-m4/pr_body.md`.

- [ ] **Step 1: `docs/mobile.md`:** building with the Flutter module (`build-flutter.sh`, what it makes, what CI does), the plugin area's layers in one diagram of text (core → JavaScript → native module → Dart, with the file for each), and a **hand checks** section for real phones, Android and iPhone, taken from Task 16's steps plus: the keyboard over a plugin's text field; the back gesture and the iPhone's swipe-back with a modal open; a call from another app arriving while an activity is open; the app killed in the background and reopened on a plugin channel. Mark every one "not run" for the iPhone.
- [ ] **Step 2: `docs/plugins.md`:** a section "On the phone" for plugin authors: what is the same; `ctx.input` is `touch` and the compact breakpoint applies; modals become sheets; no hover; the activity opens over the whole stage; rich text has no highlighted code. Each sentence checked against the code (rule 10).
- [ ] **Step 3: D97** in `docs/decisions.md`, in the form of D96: the decision, one bullet per layer, the gates' real numbers from `gates.md`, the calls above as they ended up (with every ruling from the ledger), the pinned tests by name (each found with `rg` first), and **not tested here**, listing iOS in full.
- [ ] **Step 4: `pr_body.md`** under rule 10: what changed per stage; how it was tested; the gates' numbers and the app's size growth; what couldn't be tested (everything on iOS beyond compiling; real phones; a real call between two phones); every commit that isn't a task's own, with why. Then `gh pr edit --body-file …`.
- [ ] **Step 5: Commit** `docs: plugins on the phone`.

**At the end of stage E:** full checks, push, read the pull request's checks to the end (rule 8), then the review of stages D–E. After it passes: `gh pr ready`, write the head commit's hash into `.superpowers/sdd/plugins-m4/m4-finished` (the file holds only the hash) as the very last step, and report to the owner: the commits; every ruling, with what it costs if wrong; every deferred minor finding; every check with its result pasted; the gates; the screens looked at; and everything not run, iOS first. Never merge.
