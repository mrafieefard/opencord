# Mobile Client (Early Version) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. If your harness has neither skill, work through the tasks in order yourself and tick the boxes as you go.

**Goal:** Opencord on Android and iOS. It is a React Native app on the Rust client core, laid out like WhatsApp and Telegram:
- joined servers are in the main list;
- inside a server, the channel list shifts into a left sidebar when a chat opens;
- it has chat, voice, and the music player with its upload section and volume.

Video and screen sharing are the second plan.

**Architecture:**
- **`crates/opencord-mobile`** is a UniFFI facade over `opencord_core::client::Client`, the same client the Flutter API wraps. It gives JavaScript string ids, number times, named permission flags, and events pulled in batches.
- **`mobile/packages/core-native`** (`@opencord/core-native`) is the turbo-module that uniffi-bindgen-react-native generates and builds from that crate.
- **`mobile/`** is an Expo app:
  - Expo Router with native stack, tabs and sheets;
  - one Zustand store, fed by an event loop;
  - screens built from a small design system that copies the desktop's tokens;
  - a `FakeCore` for tests and screenshots.

**Tech Stack:**
- **Rust 2024:** UniFFI 0.31 (uniffi-bindgen-react-native **0.31.0-6**) and the existing `opencord-core`.
- **TypeScript:**
  - Expo SDK **58** (React Native 0.88, New Architecture, Hermes) and Expo Router;
  - Reanimated 4, Gesture Handler, FlashList v2, react-native-keyboard-controller;
  - `@expo/ui`, `@react-native-menu/menu`, `@react-native-segmented-control/segmented-control`;
  - Zustand;
  - Jest (`jest-expo`) with React Native Testing Library.

**Spec:** `docs/superpowers/specs/2026-10-26-mobile-client-design.md`.
- Read it fully before Task 1. The spec says *what* and *why*; this plan says *where* and *how*.
- Section numbers like "spec §8.5" point into it.
- If the two disagree, or a step is impossible as written, stop and ask the user. Do not improvise.

## Global Constraints

- **Platforms:**
  - **Android:** 8.0 (API 26, AAudio's minimum, which cpal needs) and newer, `arm64-v8a` and `x86_64`. Built and tested on the dev machine's `Pixel_7` emulator (API 33, x86_64).
  - **iOS:** at the deployment target Expo SDK 58 sets. Built in CI; run on the Mac in Task 22.
  - **Local builds** target `x86_64` only.
- **Branch:**
  - create `feat/mobile` from `canary` (`git switch -c feat/mobile origin/canary`) and commit there after every task;
  - each milestone ends in a pull request from `feat/mobile` into `canary` (`GEMINI.md`, "Every change is a pull request");
  - after the owner merges, run `git merge origin/canary` and carry on;
  - never commit to `beta`, `canary` or `main`.
- **Decisions:** one entry per milestone in `docs/decisions.md`, from the next free number (D75 today), for R0 to R5, in the existing style.
- **Names and paths:**

  | Item | Name | Location |
  |---|---|---|
  | Facade crate | `opencord-mobile` (library `opencord_mobile`) | `crates/opencord-mobile` |
  | App package | `opencord-mobile` (private) | `mobile/` |
  | Native package | `@opencord/core-native` | `mobile/packages/core-native` |
  | Voice service module | `@opencord/voice-service` | `mobile/modules/voice-service` |
- **App:**
  - id `dev.opencord.mobile` on both systems; display name **Opencord**;
  - deep-link scheme `opencord-app`; invite schemes `opencord` and `opencords`.
- **Versions:**
  - Expo SDK 58: `npx expo install` picks every Expo-managed package's version;
  - `uniffi-bindgen-react-native@0.31.0-6` and `@ubjs/core` at the version that release names;
  - `uniffi` pinned with `=` to the version ubrn 0.31.0-6 is built with (its `Cargo.lock`);
  - Node 26 (`mobile/.nvmrc`), Yarn 1.22 (classic workspaces), Java 17.
- **Storage:**
  - **SecureStore:** key `opencord.identity`, the base64 identity secret, with `keychainAccessible: AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY`.
  - **The core's settings:** `mobile.displayName`, `mobile.theme`, `mobile.musicListening` (JSON `{volume, muted, duck}`) and `mobile.read.<serverKey>` (JSON of channel id → message id).
- **Numbers:**

  | What | Value |
  |---|---|
  | Messages per page | 50 |
  | Recent-activity seed | 50 per channel, 4 fetches at once, 10 s time-out each |
  | Event batches | at most 256 |
  | Typing | sent at most every 8 s; shown for 10 s |
  | A closed channel keeps | its newest 100 messages |
  | Read positions | written at most once per 2 s per server |
  | Message text | 1–4,000 characters; counter from 3,900 |
  | Display name | 1–32 characters after trimming (the core's rule) |
  | Music search | 1–200 characters |
  | Volume | 0–200 % in 5 % steps |
  | Music defaults | volume 0.5, muted false, duck false |
  | Queued upload rows | fade within 3 s |
  | Toasts | 3 s |
  | Music position | refreshed every 500 ms |
  | Tablet layout | from 768 pt wide |
  | Phone sidebar | `min(86 % of width, 360 pt)` |
  | Tablet sidebar | 320 pt |
  | Message groups | same author within 5 minutes |
  | Bubble width | at most 78 % |
  | Image thumbnails | fit within 260 × 320 pt |
- **Look:** spec §9 exactly: colors, type, spacing, radii, icons, motion. No color literal outside `mobile/src/ui/theme/tokens.ts`.
- **Words:** every user-visible sentence in the spec and this plan is copied exactly.
- **Licences:**
  - the facade and the app are MPL-2.0, like `opencord-core` and `app/` (the crate's `license` field and `package.json`'s);
  - new dependencies must be MIT, Apache-2.0, BSD, ISC, Zlib or 0BSD;
  - MPL-2.0 is allowed only for UniFFI and uniffi-bindgen-react-native and their runtimes.
- **Never log** the identity secret, the backup text, or message content. Log ids, counts and error kinds.

## Working Rules (read before Task 1)

### Environment

Each shell command runs in a fresh shell.
- **Rust:** start every `cargo` command with `source .superpowers/sdd/media/env.sh &&`.
- **Mobile:** start every `yarn`, `npx`, `ubrn`, `adb`, `emulator` and Gradle command with `source mobile/scripts/env.sh &&`. Task 3 writes that file: the Android SDK, the NDK React Native 0.88 asks for, Java 17, and the Rust environment.
- **Disk:** check `df -h /` before native builds. Gradle, the emulator and the Rust Android target together take several GB.
- **Never** run `cargo clean`, or delete `~/.gradle`, `~/.android` or `/home/amin/sdk/android`, without asking.

### Commands

| What | Command (from the repository root unless it says `cd mobile`) |
|---|---|
| Facade lint | `cargo clippy -p opencord-mobile --all-targets -- -D warnings` |
| Facade tests | `cargo test -p opencord-mobile` (one: `-- --exact <path::test_name>`) |
| Build the core for the emulator and regenerate the bindings | `cd mobile && yarn core:android` |
| Regenerate the bindings only (host build) | `cd mobile && yarn core:bindings` |
| Bindings up to date? | `mobile/scripts/check-bindings.sh` |
| Type check, lint, tests | `cd mobile && yarn typecheck && yarn lint && yarn test` (one test: `yarn test -t '<name>'`) |
| Start an emulator | `cd mobile && yarn emulator` (phone) or `yarn emulator tablet` |
| Run on the emulator | `cd mobile && yarn android` (real core) or `yarn android:fake` (FakeCore) |
| Screenshots | `mobile/scripts/screens.sh <task> <name>=<route> …` (add `--tablet` for the tablet emulator) |
| Licences | `cd mobile && yarn licenses:check` |
| Everything CI runs on Linux | `.github/check.sh` (only the phone parts: `.github/check.sh mobile`) |
| A local server for the emulator | `cargo run -p opencord-server`; the emulator reaches it at `10.0.2.2:7710` |

### Generated files: never edit by hand

- **The bindings:** `mobile/packages/core-native/src/generated/**`, `mobile/packages/core-native/cpp/generated/**`, and the turbo-module glue ubrn writes. Task 3 lists the exact paths in `docs/mobile.md`. Regenerate them, and commit them with the change that caused it.
- **`mobile/android/**` and `mobile/ios/**`:** made by `expo prebuild` and ignored by git. Native settings go through `mobile/app.config.ts` and config plugins.
- **`mobile/assets/icon.png` and `adaptive-icon.png`:** made from `icon.svg` by `yarn icons`.

### Task owners

Every task has an **Owner** and a **Score** (1–5, how much a mistake costs: FFI, threads, platform audio, background work and event ordering score 4–5).

- **Owner: Gemini.** You implement it. Score 3 means read its **Review focus** line twice: that's what the review checks hardest.
- **Owner: Claude.** Don't implement it. Hand the task to Claude, naming the task number and this plan's path, and wait for its report. Don't edit files or run git while it works: you share the working copy.
  - Then read the report, run the task's **Checks** yourself, and record the task in the ledger with the report's commits.
  - Claude's tasks come first in their milestone, so yours build on them. Milestone R0 is Claude's alone.
- **Owner: Claude on the Mac** (Task 22). The user runs it in a Claude Code session on the Mac, from this section. Gemini waits for its commits, then merges and runs the Linux checks.
- **Claude, when handed a task:** follow the task exactly. Run its checks; finish with one commit per task using the message given, then report the files, commits, checks and anything left undone.

### Skills for UI work

- **Before every task that touches screens or components,** read `~/.claude/plugins/marketplaces/swmansion/skills/react-native-best-practices/SKILL.md`, then each sub-skill the task names, under `references/`. Follow their critical rules.
- **Where the plan and a skill differ,** the plan decides *what* is built and the skill decides *how* a library is used.
- **Check versions first.** Read the installed version of Reanimated and Gesture Handler (`node_modules/<pkg>/package.json`) before writing code for them, and use that version's API. The gestures skill has the v2 and v3 table.

### Rules reviews will enforce

`GEMINI.md` → "How to write code here" applies everywhere. Read it before each milestone. These come on top:

1. **Strict TypeScript.** No `any`, no `@ts-ignore` or `@ts-expect-error`, no non-null `!`. No `as` casts except narrowing a generated union by its `tag`.
2. **No swallowed errors.** No empty `catch`. Every promise is awaited, or handed to `report(promise, context)` (Task 4), which logs it and shows a toast with `describeError`. Production code never bends for a test.
3. **After any `await`, check again before writing.** While waiting, the open channel, the server, a session's version or the screen may have changed. Merge what came back with what arrived meanwhile (by id or version); never assign over it.
4. **Components read the store through the selector hooks** in `src/state/selectors.ts`. Objects and arrays built in a selector go through `useShallow` or a memoized selector. No component reads the whole store.
5. **Tokens only:**
  - colors from `tokens.ts` through `useTheme()`;
  - text through `<Text variant=…>`;
  - icons through `<Icon name=…>` with names from spec §9.4;
  - spacing from `space`.

  A color literal anywhere else fails `noColorLiterals.test.ts`.
6. **One touch system.** Inside lists, panes and sheets, everything pressable uses Gesture Handler's components (`Pressable` or `RectButton` in v2, `Touchable` in v3), per the gestures skill. Never React Native's `Pressable` or `Touchable*` in the same tree. Gesture callbacks call JavaScript only through `scheduleOnRN`.
7. **Animations** follow the animations skill:
  - CSS transitions and animations for changes driven by state;
  - shared values only for gesture- or scroll-driven motion;
  - never `runOnJS`;
  - durations shortened under Reduce Motion (`useReducedMotion`).
8. **Lists that can grow use FlashList.** Keys are ids. Rows are `React.memo` with stable props, and there is no `ScrollView` with `.map` over unbounded data.
9. **Native first.** Use the components spec §10 names. Don't draw a control that table lists.
10. **Accessibility:**
  - every icon-only button has an `accessibilityLabel`;
  - rows have a role;
  - touch targets are at least 44 pt;
  - text follows the system size up to 1.6× (`maxFontSizeMultiplier={1.6}` in `Text`).
11. **Tests:**
  - written first, and able to fail (`GEMINI.md`);
  - queries by role, label or text;
  - timers through Jest's fake timers;
  - `FakeCore` answers like the core: it echoes `MessageCreated` with the nonce, throws the core's error variants, and sends the events the core sends after each call. A test that needs a new fake behavior adds it to `FakeCore` the way the core behaves, with a test of its own in `fakeCore.test.ts`.
12. **Screenshots are part of done.** Each UI task captures the screens it lists in dark and light at phone width (and tablet where it says), looks at every one, fixes what looks wrong, and lists the files in the ledger. Passing tests alone don't finish a UI task.
13. **Rust in the facade:** no `unwrap` or `expect` outside tests; no std lock across `.await`; ids parsed before the first `.await`; every change to an exported item regenerates the bindings in the same commit (`yarn core:bindings`).
14. **Dependencies:**
  - added with `npx expo install <pkg>` when Expo knows the package, else `yarn add <pkg>@<exact version>`;
  - `yarn licenses:check` passes;
  - nothing this plan doesn't name, without asking the user.
15. **Platform files:** code only one system needs goes in `.ios.tsx` / `.android.tsx` files, or behind `Platform.OS` in one place, never scattered.

### Progress ledger

Keep `.superpowers/sdd/mobile/progress.md`. After each task, append:

```
## Task N — <title> (owner: <Gemini|Claude|Claude on the Mac>)
- Commit: <hash>
- Tests added: <test names>
- Commands run: <command> → <result summary>
- Screenshots: <paths, or "none: no UI">
- Deviations from the plan: <none | what and why>
- Left undone / doubts: <none | list>
```

What counts as a deviation is in `GEMINI.md` → "Reports and the ledger".

### When a step keeps failing

Follow `GEMINI.md` → "When you're stuck: ask Claude", with the question files in `.superpowers/sdd/mobile/questions/`.

### Milestone stops

At the end of each milestone (R0 … R5):
1. **Checks:** run `.github/check.sh` (everything, including `mobile`).
2. **Screenshots:** take every screen the milestone touched, in both themes at phone width, and at tablet width where its tasks say. Look at each one.
3. **The decisions entry:** write it from `git diff <first commit of the milestone>^..HEAD`, as `GEMINI.md` → "Write docs from the code" says:
   - include **Why**, a bullet per part, and **Tests**;
   - grep every name and count every number you write;
   - for Claude's tasks, describe what Claude's reports say.
4. **The pull request:** push `feat/mobile` and open one into `canary`, with the milestone report as its body. Read its checks and fix failures on the branch.
5. **Stop and report** to the user. Continue only when the user releases the next milestone.

## Review Focus

These are the inputs most likely to break for real people while no ordinary test would catch them. Each has a pinned test in the task named.

1. **A JavaScript reload, or the app coming back from the background, while events flow.**
   - Expected: no event lost or applied twice, and no crash.
   - Pinned: Task 1 (`a_new_attach_releases_the_waiting_call`) and Task 4 (`restarting the loop takes over the stream`), plus Task 4's hand check of five reloads against a real server.
2. **A message arriving while a channel's first page or an older page is loading.**
   - Expected: it shows once, in order.
   - Pinned: Task 13 (`a message that arrives during the first load shows once, in order`, `an older page merges with live messages`).
3. **The send's echo and the send's answer racing, and a retry after a failure.**
   - Expected: one message.
   - Pinned: Task 15 (`an echo before the send's answer shows the message once`, `an echo after the send's answer shows the message once`, `a failed send retried shows once`).
4. **A reconnect (`Ready`) while recent activity is being fetched, or while the user reads.**
   - Expected: unread counts as on desktop; the user's own messages never count; positions only move forward.
   - Pinned: Task 5 (`events during seeding are applied after it, in order, once`, `a newer Ready during seeding discards the older result`, `read positions only move forward and are written at most every 2 s`).
5. **Switching channels or servers quickly while fetches are in flight.**
   - Expected: each channel shows its own messages, and no spinner is left behind.
   - Pinned: Task 13 (`switching channels mid-load shows each channel's own messages`) and Task 11 (`a deleted open channel goes back to channels`).

## File Map

### Rust

| File | Status | Contents |
|---|---|---|
| `crates/opencord-mobile/Cargo.toml` | new | crate types `lib` and `staticlib`; `uniffi`, `opencord-core`, `opencord-common`, `tokio`, `thiserror`, `sha2`, `hex` |
| `crates/opencord-mobile/src/lib.rs` | new | `uniffi::setup_scaffolding!()`, modules |
| `src/mobile.rs` | new | `Mobile`: runtime, client, event pump, cache folder; `mobile()`, `on_runtime` |
| `src/ids.rs` | new | `parse_id`, `id` |
| `src/error.rs` | new | `MobileError`, `From<CoreError>` |
| `src/types.rs` | new | every record and enum of spec §5.3 |
| `src/convert.rs` | new | core types → records |
| `src/events.rs` | new | `MobileEvent`, `EventPump`, the conversion of core and media events |
| `src/files.rs` | new | the attachment cache (spec §5.6) |
| `src/api/{mod,setup,identity,servers,chat,files,voice,music}.rs` | new | the exported functions of spec §5.2 |
| `tests/common/mod.rs`, `tests/events.rs`, `tests/chat.rs` | new | against a real `opencord-server` |
| `Cargo.toml`, `Cargo.lock`, `.github/check.sh` | modify | workspace member, `uniffi`, the client crate list |

### Mobile app (`mobile/`)

| Path | Status | Contents |
|---|---|---|
| `package.json`, `yarn.lock`, `.nvmrc`, `app.config.ts`, `tsconfig.json`, `eslint.config.js`, `jest.config.js`, `metro.config.js` | new | the Expo app, workspaces `packages/*` and `modules/*` |
| `assets/fonts/*`, `assets/icon.svg`, `assets/icon.png`, `assets/adaptive-icon.png` | new | Inter and JetBrains Mono (from `app/assets/fonts`), the icon |
| `scripts/{env.sh,build-core.sh,check-bindings.sh,emulator.sh,screens.sh,make-icons.sh,licenses.js}` | new | see Task 3 |
| `packages/core-native/**` | new | `@opencord/core-native`: `ubrn.config.yaml`, generated bindings, podspec, Android glue |
| `modules/voice-service/**` | new | Android foreground service (Kotlin), iOS no-op, its config plugin |
| `src/app/**` | new | routes (spec §7.1) |
| `src/core/**` | new | `CoreApi`, `NativeCore`, `FakeCore`, errors, boot, event loop, identity, paths, lifecycle |
| `src/state/**` | new | store, reducer, activity, selectors, actions, types |
| `src/ui/**` | new | tokens, theme, primitives, native control wrappers |
| `src/features/{boot,onboarding,servers,you,server,chat,viewer,voice,music,links}/**` | new | screens and their parts |
| `src/test/**` | new | Jest setup: mocks of Expo modules, render helpers with `FakeCore` |

### CI and docs

- **Modify:** `.github/workflows/cicd.yml` (jobs `mobile-android` and `mobile-ios`), `.github/check.sh` (`mobile`), `.gitignore`, `GEMINI.md` and `CLAUDE.md` (the environment line, Current work), `docs/decisions.md`, `docs/releasing.md`, `README.md`.
- **Create:** `docs/mobile.md`.

---

# Milestone R0: Foundations (Claude)

### Task 1: The facade: types, events, setup, identity and servers

**Owner:** Claude. **Score:** 5 (FFI, the event stream every screen depends on, threads).

**Files:**
- Create: `crates/opencord-mobile/{Cargo.toml, src/lib.rs, src/mobile.rs, src/ids.rs, src/error.rs, src/types.rs, src/convert.rs, src/events.rs, src/api/mod.rs, src/api/setup.rs, src/api/identity.rs, src/api/servers.rs, tests/common/mod.rs, tests/events.rs}`
- Modify:
  - root `Cargo.toml` (member; `uniffi` under `[workspace.dependencies]`), `Cargo.lock`;
  - `.github/check.sh` (add `-p opencord-mobile` to `CLIENT_CRATES`);
  - `crates/opencord-core/src/**` only to make an item the facade calls `pub`. List each one in the report.

**Interfaces:**
- Produces:
  - spec §5.2's Setup, Identity and Servers functions;
  - every record, enum and event of spec §5.3–§5.5, so later tasks only add functions;
  - `pub struct Mobile` with:
    - `Mobile::start(data_dir: &Path, cache_dir: &Path, runtime: tokio::runtime::Handle) -> Result<Mobile, MobileError>`;
    - `fn client(&self) -> &opencord_core::client::Client`;
    - `fn cache_dir(&self) -> &Path`;
    - `fn attach(&self) -> u64`;
    - `async fn next_events(&self, generation: u64, max: u32) -> Result<Vec<MobileEvent>, MobileError>`;
  - `pub(crate) fn mobile() -> Result<&'static Mobile, MobileError>`;
  - `pub(crate) async fn on_runtime<T, F>(work: impl FnOnce(Client) -> F) -> Result<T, MobileError>`, the facade's copy of `opencord_core::api::client::on_runtime`;
  - `pub fn parse_id(text: &str) -> Result<i64, MobileError>` and `pub fn id(value: i64) -> String`.

- [ ] **Step 1: Write the failing tests.**
  - **`ids::tests::ids_round_trip_as_decimal_strings`:**
    - `parse_id(&id(i64::MAX)) == i64::MAX` and `parse_id("42") == 42`;
    - each of `""`, `"abc"`, `" 1"`, `"1.0"`, `"+1"` and `"99999999999999999999"` is `MobileError::InvalidInput` with a message starting `"not an id"`.
  - **`convert::tests::access_follows_the_permission_bits`:**
    - `VIEW_CHANNEL` alone: `view` only;
    - `VIEW_CHANNEL | READ_HISTORY | SEND_MESSAGES | ATTACH_FILES`: those four;
    - the four music bits: the four music flags;
    - `ADMINISTRATOR` alone: every flag true.
  - **`convert::tests::numbers_and_ids_cross_as_planned`:**
    - a message with id `i64::MAX`, `created_at_ms` 1_761_500_000_123 and a 3,000,000,000-byte attachment becomes `"9223372036854775807"`, `1761500000123.0` and `3e9`;
    - a track with `local_song_id` 0 becomes `None`; 7 becomes `Some("7")`.
  - **`error::tests::every_core_error_has_its_mobile_error`:** one case per `CoreError` variant. `Server { code: ErrorCode::RateLimited, .. }` becomes `Server { code: "rate_limited", .. }`.
  - **`events::tests`**, against an `EventPump` fed through test senders (no server):
    - `a_batch_holds_what_is_ready_up_to_max`: five events, `max` 3, gives 3 then 2;
    - `dropped_kinds_never_end_a_wait`: a `RoleCreate` then a `MessageCreate` gives one batch holding only the message;
    - `a_new_attach_releases_the_waiting_call`: a call waiting under generation 1 returns `[]` within 100 ms of `attach()`; the next call, under generation 2, receives the next event;
    - `an_old_generation_gets_nothing`: a call made with a stale generation returns `[]` at once;
    - `media_and_server_events_both_arrive`: a media `Speaking` and a server `TypingStart` come back in one batch.
  - **`tests/events.rs`**, against a real server. `tests/common/mod.rs` starts one the way the `TestServer` in `crates/opencord-core/tests/client.rs` does (`opencord_server::server` with a test `Config`):
    - `ready_then_messages_arrive_with_string_ids`:
      1. `Mobile::start` on a temporary folder, load an identity, then `server_add`;
      2. `next_events` returns `Connection(Connecting)`, `Connection(Connected)`, then `Ready`, whose channels include the server's default text channel;
      3. a second core `Client` sends "hello";
      4. a later batch holds `MessageCreated` with that content and a `channel_id` equal to the channel's id string.
    - `an_untrusted_server_asks_first`: `server_add` to the self-signed test server returns `NeedsTrust` with its fingerprint. After `server_trust`, `server_add` returns `Added`.
- [ ] **Step 2: Run them:** `cargo test -p opencord-mobile`. Expect compile errors, since nothing exists yet.
- [ ] **Step 3: Implement.**
  - **The crate:**
    - `[lib] crate-type = ["lib", "staticlib"]`, with `uniffi::setup_scaffolding!()` in `lib.rs`;
    - `core_init` follows `opencord_core::api::client::init`: one multi-thread runtime named `opencord-mobile`, `Client::new`, then `enable_media(MediaOptions { open_devices: true })`; a second call returns `Ok(())`.
  - **`EventPump`:**
    - the core's two receivers sit behind one `tokio::sync::Mutex`, with an `AtomicU64` generation and a `tokio::sync::Notify`;
    - `attach` bumps the generation and wakes every waiter;
    - `next_events` creates and enables its `Notified` before reading the generation, so no wake-up is lost;
    - it waits for the receivers' lock or a newer generation, then for the first event or a newer generation, then drains without waiting up to `max`, converting as it goes;
    - a wait ends empty only when superseded.
  - **Conversions** are total: no `_ =>` arm over the core's `CoreEventPayload` or `MediaEvent`. A new core event then fails to compile here until it is mapped or listed as dropped (spec §5.4).
  - **Errors:** `MobileError` derives `uniffi::Error` and `thiserror::Error`, with `From<CoreError>`. A server `code` is the `ErrorCode` variant's name in snake case.
  - **No std lock across `.await`.** Ids are parsed before any await.
- [ ] **Step 4: Run** the tests, clippy (`-D warnings`) and `cargo fmt --all -- --check`.
- [ ] **Step 5: Link for Android.**
  - Install `cargo-ndk` if it is missing (`cargo install cargo-ndk --locked`) and add the `aarch64-linux-android` target with rustup.
  - Build: `ANDROID_NDK_HOME=/home/amin/sdk/android/ndk/28.2.13676358 cargo ndk -t x86_64 -t arm64-v8a build -p opencord-mobile --release`.
  - Record both `.a` sizes in the report.
- [ ] **Step 6: Licences:** `cargo tree -p opencord-mobile -e normal --target all --format '{p} {l}' | grep -v 'MIT\|Apache\|BSD\|ISC\|Zlib'`. Only UniFFI's MPL-2.0 crates and our own may print.
- [ ] **Step 7: Commit:** `feat(mobile): a UniFFI facade over the client core for the phone apps`.

**Checks:** Step 4's commands, and `cargo test -p opencord-mobile -- --exact events::tests::a_new_attach_releases_the_waiting_call`.

### Task 2: The facade: chat, attachment files, voice and music

**Owner:** Claude. **Score:** 4 (a file cache written from several callers; every call the screens make).

**Files:**
- Create: `crates/opencord-mobile/src/{files.rs, api/chat.rs, api/files.rs, api/voice.rs, api/music.rs}`, `crates/opencord-mobile/tests/chat.rs`.

**Interfaces:**
- Consumes: Task 1's `Mobile`, `on_runtime`, `parse_id`, `id`, records and errors.
- Produces: spec §5.2's Chat, Voice and Music rows, with the names and argument order given there.

- [ ] **Step 1: Write the failing tests.**
  - **`files::tests`:**
    - `a_cached_file_comes_back_without_fetching`: with the file already in place, the fetch closure is never called;
    - `a_failed_fetch_leaves_no_file`: a writer failing halfway leaves nothing at the final path and no `.part` file;
    - `server_folders_are_hashed`: key `"10.0.2.2:7710"` gives the first 16 lowercase hex characters of its SHA-256.
  - **`tests/chat.rs`**, against a real server:
    - `send_fetch_edit_delete_round_trip`:
      1. `message_send` with nonce `"n1"` returns the message, and the `MessageCreated` echo carries `nonce: Some("n1")`;
      2. after two more sends, `messages_fetch(.., None, 2)` returns the newest two, newest first;
      3. `message_edit` gives `MessageUpdated` with the new text and an `edited_at_ms`;
      4. `message_delete` gives `MessageDeleted`.
    - `files_are_fetched_once`:
      1. `message_send_files` with a PNG;
      2. `attachment_file(.., Thumbnail)` and `(.., Original)` each return a path whose bytes match the server's;
      3. after `server_remove`, the same two calls still return those paths from the cache.
    - `music_answers_on_a_fresh_server`: `music_settings` returns `supported: true`. `music_subscribe` to the default voice channel (`General`) returns a session with state `Idle`, no `now_playing`, and an empty queue.
- [ ] **Step 2: Run them:** `cargo test -p opencord-mobile`. Expect compile errors.
- [ ] **Step 3: Implement.**
  - **Each function** wraps the `Client` method the Flutter API uses (`opencord_core::api::client` and `api::attachments`, read them side by side) inside `on_runtime`, converting with Task 1's functions.
  - **`message_send_files`** maps each `OutgoingFileRecord` to `OutgoingFile { path: Some(path), bytes: None, filename, staged_id: None }`.
  - **`music_playback`'s `position_ms`** that is negative, NaN or infinite is `InvalidInput`.
  - **`attachment_file`** follows spec §5.6:
    - thumbnails through `attachment_bytes`;
    - originals through `download_attachment` into the `.part` file;
    - the rename (`std::fs::rename`) as the last step;
    - blocking file work in `spawn_blocking`.
  - **`voice_set_user_volume` and `voice_set_user_local_mute`** are sync, as in the Flutter API.
- [ ] **Step 4: Run** the tests, clippy and fmt.
- [ ] **Step 5: Commit:** `feat(mobile): chat, files, voice and music calls in the phone facade`.

**Checks:** Step 4's commands.

### Task 3: The Expo app, the native package, scripts and CI

**Owner:** Claude. **Score:** 4 (the build every later task runs on, on both systems).

**Files:**
- Create: `mobile/**`, as in the File Map, except `src/core`, `src/state`, `src/ui` and `src/features` (later tasks). Also `docs/mobile.md` (its "Developing" section).
- Modify: `.gitignore`, `.github/workflows/cicd.yml`, `.github/check.sh`, `GEMINI.md` (the "Current work" section names this plan; "Every build command loads the environment" gains `mobile/scripts/env.sh`), `CLAUDE.md` (the same environment line).

**Interfaces:**
- Produces:
  - the scripts and `yarn` commands of the Commands table;
  - `@opencord/core-native`, exporting the generated functions and types;
  - `mobile/src/app/index.tsx`, a first screen showing `coreVersion()`, which Task 7 replaces;
  - the CI jobs.

- [ ] **Step 1: The app.**
  - `npx create-expo-app@latest mobile --template default@sdk-58` (TypeScript). Delete the template's example screens, components, hooks and images.
  - **`app.config.ts`:**
    - name "Opencord", slug `opencord`, scheme `opencord-app`;
    - `ios.bundleIdentifier` and `android.package`: `dev.opencord.mobile`;
    - the permissions and `UIBackgroundModes` of spec §11, with the sentences given there;
    - plugins: `expo-router`, `expo-font` (Inter Regular, Medium, SemiBold, Bold, Italic and SemiBold Italic, and JetBrains Mono Regular, copied from `app/assets/fonts`), `expo-secure-store`, `expo-audio` (the microphone sentence), `expo-image-picker` (the photo sentence).
  - Workspaces: `packages/*` and `modules/*`.
- [ ] **Step 2: The native package.**
  - `npx create-react-native-library@latest packages/core-native`: Turbo module, C++ for Android and iOS, no example app. Name it `@opencord/core-native`.
  - Add `uniffi-bindgen-react-native@0.31.0-6` and `@ubjs/core`.
  - Write `ubrn.config.yaml`:

    ```yaml
    rust:
      directory: ../../..
      manifestPath: crates/opencord-mobile/Cargo.toml
    android:
      targets: [arm64-v8a, x86_64]
      apiLevel: 26
    ios:
      targets: [aarch64-apple-ios, aarch64-apple-ios-sim]
    bindings:
      cpp: cpp/generated
      ts: src/generated
    ```

    API 26 is AAudio's minimum: linking against API 24 fails on `-laaudio`. `expo-build-properties` sets the app's `minSdkVersion` to the same.
  - Point the package's `react-native` and `source` fields at `src/index.tsx`, so Metro reads the source with no build step.
- [ ] **Step 3: The scripts.**
  - **`scripts/env.sh`:**
    - `ANDROID_HOME`, defaulting to `/home/amin/sdk/android` when unset;
    - `ANDROID_NDK_HOME`, at the NDK version React Native 0.88's Gradle build names (read `ndkVersion` after a first `expo prebuild`);
    - `PATH`, adding `platform-tools`, `emulator` and `cmdline-tools/latest/bin`;
    - it sources `.superpowers/sdd/media/env.sh` when present.
  - **`scripts/build-core.sh android|ios [--release]`:** runs `ubrn build <platform> --config ubrn.config.yaml --and-generate`, with `--targets x86_64-linux-android` for a local Android debug build.
  - **`yarn core:bindings`:** a host `cargo build -p opencord-mobile`, then `ubrn generate jsi bindings --library` and `ubrn generate jsi turbo-module`.
  - **`scripts/check-bindings.sh`:** regenerates into a temporary copy and compares it with the committed files. On a difference it prints "The bindings are out of date: run yarn core:bindings" and exits 1.
  - **`scripts/emulator.sh [phone|tablet]`:** boots `Pixel_7` or `Pixel_Tablet` if it isn't running, and waits for `sys.boot_completed`.
    - Create `Pixel_Tablet` once, from the API 33 image already installed: `avdmanager create avd -n Pixel_Tablet -k "system-images;android-33;google_apis;x86_64" -d pixel_tablet`.
  - **`scripts/screens.sh <task> <name>=<route> … [--tablet]`:** for each theme (`adb shell cmd uimode night yes|no`) and each route:
    1. `adb shell am start -W -a android.intent.action.VIEW -d "opencord-app://<route>" dev.opencord.mobile`;
    2. wait 2 s;
    3. `adb exec-out screencap -p` into `.superpowers/sdd/mobile/screens/<task>/<name>-<dark|light>-<phone|tablet>.png`.
  - **`scripts/make-icons.sh`:** `assets/icon.svg` (spec §11: "oc" in Inter Bold, `#EDEDEF` on `#0E0E10`) becomes `icon.png` (1024 px) and `adaptive-icon.png`, with `rsvg-convert` or ImageMagick, whichever is installed.
  - **`scripts/licenses.js`** (`yarn licenses:check`): fails on any production dependency whose licence isn't in the Global Constraints list. It reads each installed package's `package.json`.
- [ ] **Step 4: Tooling.**
  - `tsconfig.json`: strict, with the path alias `@/*` → `src/*`;
  - ESLint: `eslint-config-expo`;
  - Jest: `jest-expo` preset, with the Expo-module mocks in `src/test/setup.ts`;
  - `.nvmrc`: `26`;
  - `.gitignore`: `mobile/node_modules`, `mobile/android`, `mobile/ios`, `mobile/.expo`, `*.a`, `mobile/packages/core-native/{android,ios}/build`.
- [ ] **Step 5: The first screen.** `src/app/index.tsx` shows "Opencord" and `coreVersion()`. Run:
  1. `yarn core:android`, `yarn emulator`, then `yarn android`;
  2. `scripts/screens.sh 3 first=/`.

  The screenshot shows the version.
- [ ] **Step 6: CI.**
  - **`mobile-android`** (`ubuntu-24.04`, `needs: version`):
    - Node from `.nvmrc`, Java 17, Rust with both Android targets, `cargo-ndk`, the NDK version from Step 3, `Swatinem/rust-cache`;
    - `yarn install --frozen-lockfile`;
    - `scripts/check-bindings.sh`, `yarn typecheck`, `yarn lint`, `yarn test`, `yarn licenses:check`;
    - `scripts/build-core.sh android --release`;
    - `npx expo prebuild -p android --no-install`, then `./gradlew assembleRelease`, signed with the debug key for now (Task 21 adds the release key);
    - upload `opencord-android-<version>.apk` as an artifact.
  - **`mobile-ios`** (`macos-latest`, `needs: version`):
    - Rust with both iOS targets;
    - `scripts/build-core.sh ios --release`;
    - `npx expo prebuild -p ios --no-install`, then `pod install`;
    - `xcodebuild -workspace … -scheme Opencord -sdk iphonesimulator -configuration Release CODE_SIGNING_ALLOWED=NO build`;
    - upload the zipped `.app`.
  - Neither job releases anything yet. Lint the workflow with `actionlint`.
- [ ] **Step 7: Local checks.** `.github/check.sh mobile` runs:
  - `cargo clippy` and `cargo test` for `-p opencord-mobile`;
  - `mobile/scripts/check-bindings.sh`;
  - `yarn install --frozen-lockfile` when `node_modules` is missing;
  - `yarn typecheck`, `yarn lint`, `yarn test` and `yarn licenses:check`.

  `.github/check.sh` with no argument runs it too.
- [ ] **Step 8: Docs.** `docs/mobile.md` → "Developing":
  - the one-time setup (SDK paths, emulators, `cargo-ndk`);
  - each command from this plan's table;
  - the generated files list;
  - reaching a local server from the emulator;
  - the fake world.
- [ ] **Step 9: Commit:** `feat(mobile): an Expo app that runs the Rust core on Android and iOS`.

**Checks:** `.github/check.sh mobile`, the Step 5 screenshot, and the pull request's `mobile-android` and `mobile-ios` checks green.

### Task 4: The core layer in TypeScript

**Owner:** Claude. **Score:** 4 (the event loop's takeover after reloads; a fake every later test trusts).

**Files:**
- Create:
  - `mobile/src/core/{api.ts, native.ts, errors.ts, report.ts, boot.ts, eventLoop.ts, identity.ts, paths.ts, index.ts}`;
  - `mobile/src/core/fake/{FakeCore.ts, world.ts}`;
  - `mobile/src/core/fake/assets/` (six grey abstract images made with ImageMagick, our own, no licence);
  - `mobile/src/core/__tests__/{eventLoop.test.ts, boot.test.ts, errors.test.ts, fakeCore.test.ts}`.

**Interfaces:**
- Consumes: `@opencord/core-native` (Task 3), the facade (Tasks 1–2).
- Produces:
  - **`interface CoreApi`:** one method per facade function, with the same camelCase names, arguments and result types (re-exported from the generated bindings); async facade calls return `Promise`s.
  - **`NativeCore: CoreApi`:** the generated bindings, as they are. Their errors are the generated `MobileError`, which `FakeCore` throws too, so code checks errors one way: `MobileError.instanceOf(error)`, then `error.tag`.
  - **`describeError(error: unknown): string`:** the sentences below; `errorKind(error)` for logs.
  - **`report(promise: Promise<unknown>, context: string): void`.**
  - **`getCore(): CoreApi` / `setCore(core: CoreApi): void`.** The core used is `new FakeCore({ scenario: process.env.EXPO_PUBLIC_FAKE_SCENARIO ?? 'default' })` when `process.env.EXPO_PUBLIC_FAKE_CORE === '1'`, else `NativeCore`.
  - **`paths.dataDir()` / `paths.cacheDir()`:** plain paths from `expo-file-system`'s `Paths.document` and `Paths.cache`, without `file://`.
  - **`loadIdentity(): Promise<{ secret: ArrayBuffer, name: string } | null>`, `saveIdentity(secret, name)`, `clearIdentity()`.**
  - **`bootCore(core: CoreApi): Promise<BootResult>`:** `{ state: 'onboarding' } | { state: 'ready', self: SelfInfo } | { state: 'failed', message: string }`.
  - **`startEventLoop(core: CoreApi, onBatch: (events: MobileEvent[]) => void): { stop(): void }`.**
  - **`class FakeCore implements CoreApi`:**
    - constructor `{ now?: () => number, scenario?: 'default' | 'empty' }`;
    - test hooks: `emit(...events)`, `failNext(method, error)`, and `calls` (a log of `{ method, args }`).

- [ ] **Step 1: Write the failing tests.**
  - **`eventLoop.test.ts`:**
    - `delivers batches in order`;
    - `a failing call backs off and keeps going`: two rejections retry after 250 ms then 500 ms (doubling, at most 5 s), then the loop delivers;
    - `stopping ends the loop without another call`;
    - `restarting the loop takes over the stream`: a second `startEventLoop` makes the first loop's pending call return `[]`, and the first loop ends; the second receives the next events.
  - **`boot.test.ts`:**
    - `no identity goes to onboarding`;
    - `a saved identity loads and goes to ready` (`identityLoad` called with the secret and name);
    - `a core that fails to open reports its sentence`.
  - **`errors.test.ts`:** one case per row of the sentence table below.
  - **`fakeCore.test.ts`:**
    - `a send echoes MessageCreated with its nonce and returns the same message`;
    - `fetch pages newest first, before an id`;
    - `uploads move through their phases, and fail.mp3 fails`;
    - `playback changes the session and raises its version`;
    - `joining voice sends VoiceStateChanged and VoiceLink Connected`.
- [ ] **Step 2: Run them:** `cd mobile && yarn test src/core`. Expect failures.
- [ ] **Step 3: Implement.**
  - **The event loop:**
    1. calls `eventsAttach()` once;
    2. loops on `await nextEvents(generation, 256)`;
    3. hands each non-empty batch to `onBatch`;
    4. ends when a batch comes back empty;
    5. on a rejection, waits by the back-off and tries again.
  - **`FakeCore`'s default world** (`world.ts`, fixed ids such as `fake-night-owls`, times relative to `now`):
    - **Night Owls**, connected:
      - text channels `#general` and `#music-requests` with no category, and `#off-topic` under TEXT;
      - voice channel `Lounge` under VOICE;
      - 8 members;
      - 120 messages in `#general`: markdown, mentions, code, quotes, links, images and files, with unread messages and one mention;
      - Kai and Mira in Lounge, Kai speaking every other 2 s;
      - a music session with a track playing, 6 queued and 10 in history.
    - **Rust Club**, `Reconnecting`.
    - **Weekend Raid**, `Failed { reason: Rejected }`.
    - Every call behaves as the core does (rule 11).
    - **The identity:** in fake mode, `bootCore` uses the world's own identity (name "Ava", a fixed fingerprint) instead of reading SecureStore, so deep links to any screen open at once. The `empty` scenario has no identity and no servers.
  - **Sentences** (`errors.ts`):

    | Error | Sentence |
    |---|---|
    | `NotInitialized` | "Opencord couldn't start. Close it and open it again." |
    | `NoIdentity` | "No identity is loaded." |
    | `UnknownServer` | "That server isn't in your list any more." |
    | `NotConnected` | "Not connected to this server right now." |
    | `Timeout` | "The server didn't answer in time." |
    | `InvalidInput` | the error's own message (the core's sentence) |
    | `Server`, code `forbidden` | "You don't have permission to do that." |
    | `Server`, code `not_found` | "That's gone. It may have been deleted." |
    | `Server`, code `rate_limited` | "Slow down a little and try again." |
    | `Server`, code `voice_channel_full` | "This channel is full." |
    | `Server`, code `file_too_large` | "That file is bigger than this server allows." |
    | `Server`, code `file_type_blocked` | "This server doesn't take that kind of file." |
    | `Server`, codes `storage_full`, `storage_unavailable` | "The server can't store files right now." |
    | `Server`, any code starting `music_` | the server's own message |
    | `Server`, any other code | "The server refused that." |
    | `FingerprintMismatch`, `Rejected` with `FingerprintChanged` | "This server's certificate changed. If you didn't expect that, don't trust it." |
    | `Rejected` with `Kicked` | "You were kicked from this server." |
    | `Rejected` with `Banned` | "You're banned from this server." |
    | `Rejected` with `Rejected` | "This server didn't let you in. The invite may be used up or wrong." |
    | `Rejected` with `Incompatible` | "This server runs a different version of Opencord." |
    | `Connection` | "Couldn't connect. Check your connection and try again." |
    | `Storage` | "Couldn't save on this phone. It may be out of space." |
    | `Cancelled` | "Cancelled." |
    | anything else | "Something went wrong." |

    The details are logged (ids and kinds, never content).
- [ ] **Step 4: Run** the tests, `yarn typecheck` and `yarn lint`.
- [ ] **Step 5: Hand check.**
  1. Run `yarn android` with a local server and a desktop client in the same channel.
  2. Reload the JavaScript five times (press `r` in Metro) while the desktop sends messages.
  3. Afterwards, a logged count of `MessageCreated` events equals the messages sent after the last reload, and nothing crashed.
- [ ] **Step 6: Commit:** `feat(mobile): the core layer: native and fake cores, boot and the event loop`.

**Checks:** Step 4's commands, and `yarn test -t 'restarting the loop takes over the stream'`.

### Task 5: The store: snapshots, events, unread and selectors

**Owner:** Claude. **Score:** 4 (unread counting and the held events of a reconnect).

**Files:**
- Create: `mobile/src/state/{store.ts, types.ts, events.ts, activity.ts, readPositions.ts, preview.ts, selectors.ts, actions/servers.ts, actions/ui.ts, actions/prefs.ts, actions/profile.ts}` and tests `mobile/src/state/__tests__/{events.test.ts, activity.test.ts, selectors.test.ts, preview.test.ts, profile.test.ts, prefs.test.ts}`.

**Interfaces:**
- Consumes: Task 4's `CoreApi`, `getCore`, `report`.
- Produces, used by every later task. The types live in `types.ts` and follow spec §6.
  - **`useAppStore`:** Zustand, with `getState`/`setState` for actions and tests.
  - **`applyEvents(state: AppState, events: MobileEvent[]): AppState`:** pure.
  - **`onEvents(events)`:** the event loop's callback. It handles seeding and held events, then calls `applyEvents`.
  - **Selector hooks:**

    | Hook | Returns |
    |---|---|
    | `useBoot()` | `BootState` |
    | `useSelf()` | `SelfInfo \| null` |
    | `useServerRows(query: string)` | `ServerRow[]` |
    | `useServerHeader(serverKey)` | `{ name, iconHash, connection, connected } \| null` |
    | `useChannelSections(serverKey)` | `ChannelSection[]` |
    | `useChannelInfo(serverKey, channelId)` | `{ channel, access } \| null` |
    | `useChannelActivity(serverKey, channelId)` | `ChannelActivity \| undefined` |
    | `useChannelMessages(serverKey, channelId)` | `ChannelMessages` (a stable empty value when unknown) |
    | `useTypingNames(serverKey, channelId)` | `string[]` |
    | `useMember(serverKey, userId)` | `MemberView \| null` |
    | `useMemberName(serverKey, userId)` | `string` |
    | `useMyVoice()` | `MyVoice \| null` |
    | `useVoiceMembers(serverKey, channelId)` | `VoiceMember[]` |
    | `useNowPlaying(serverKey, channelId)` | the now-playing record, or `null` |
    | `useMusicSession(serverKey, channelId)` | the session, or `null` |
    | `useMusicSettings(serverKey)` | the settings, or `null` |
    | `useMusicUploads(serverKey, channelId)` | `MusicUploadView[]` |
    | `usePrefs()` | `Prefs` |
    | `useToasts()` | `Toast[]` |
  - **Actions:**
    - `actions/servers.ts`: `loadServers()`, `addServer(linkOrAddress, claimToken?)` → `AddServerResult`, `trustServer(address, fingerprint)`, `retryServer(key)`, `retryDisconnected()`, `leaveServer(key)`, `markServerRead(key)`, `markChannelRead(key, channelId, messageId)`.
    - `actions/ui.ts`: `setOpenChannel(key, channelId, atBottom)`, `clearOpenChannel()`, `setAppActive(active)`, `toast(text, tone?)`.
    - `actions/prefs.ts`: `loadPrefs()`, `setTheme(theme)`, `setMusicListening(next)`.
    - `actions/profile.ts`: `setDisplayName(name)`, `catchUpName(serverKey)`.
  - **`previewText(content: string, lookup: { member(id): string | undefined, channel(id): string | undefined }): string`.**
  - **The view types:**
    - `ServerRow`: `{ key, name, iconHash?, connection, preview: { channelName, authorName, text, own } | null, lastAtMs: number | null, unread, mentions, inVoice }`.
    - `ChannelSection`: `{ category: ChannelRecord | null, items: ChannelItem[] }`.
    - `ChannelItem`: `{ kind: 'text', channel, access, unread, mentions }` or `{ kind: 'voice', channel, access, members: VoiceMember[], nowPlaying: MusicNowPlayingRecord | null, count, limit }`.
    - `VoiceMember`: `{ userId, name, avatarHash?, muted, deafened, speaking, isSelf }`.

- [ ] **Step 1: Write the failing tests.**
  - **`events.test.ts`:**
    - `ready replaces everything known of a server`;
    - `channel events patch the snapshot, and a deleted channel loses its activity and messages`;
    - `access changes replace the channel's flags`;
    - `members join, change and leave`;
    - `voice states track who is where, and leaving removes them`;
    - `speaking follows the changes`;
    - `a music session with a lower version is ignored`;
    - `typing ends after 10 s or with that user's message`;
    - `my voice follows the link and ends with this device's empty voice state`;
    - `upload progress updates its row, and done marks it queued`;
    - `deleting the last message falls back to the newest loaded one, or none`;
    - `a music notice becomes a toast`.
  - **`activity.test.ts`** (spec §6.2, with `FakeCore` and fake timers):
    - `seeding counts others' messages since the saved position`;
    - `a channel never read starts read at its newest message`;
    - `an empty channel starts at 0 so its next message counts`;
    - `own messages, and messages in the open chat at the bottom, never count`;
    - `mentions count messages with <@self>`;
    - `events during seeding are applied after it, in order, once`;
    - `a newer Ready during seeding discards the older result`;
    - `a failed fetch leaves the channel without activity`;
    - `read positions only move forward and are written at most every 2 s`;
    - `marking a server read clears all its channels`.
  - **`selectors.test.ts`:**
    - `server rows sort by latest activity, then by name`;
    - `a row's preview names the author, You for own messages`;
    - `channel sections put channels without a category first, then categories by position, without channels the user can't view`;
    - `voice members carry speaking and mute flags`;
    - `filtering rows ignores case`.
  - **`preview.test.ts`:** `previewText strips the markdown subset and resolves mentions and channel links`, with these cases:
    - `**bold** _it_ ~~gone~~ \`code\`` → `bold it gone code`;
    - `> quoted` → `quoted`;
    - a fenced block → its text;
    - `<@1001> hi` → `@Kai hi`;
    - `<#10>` → `#general`;
    - an unknown id → `@unknown` and `#unknown`.
  - **`profile.test.ts`:** `a Ready under another name sends the saved one`.
  - **`prefs.test.ts`:** `music listening starts at the core's defaults and applies on change`.
- [ ] **Step 2: Run them:** `yarn test src/state`. Expect failures.
- [ ] **Step 3: Implement**, by spec §6. The seeding follows `app/lib/core/rust/rust_repository.dart` `_recentActivity` and `_finishReady` (held events) line by line. Read them before writing.
- [ ] **Step 4: Run** the tests, `yarn typecheck` and `yarn lint`.
- [ ] **Step 5: Commit:** `feat(mobile): the app's store with unread counts as on desktop`.

**Checks:** Step 4's commands.

**R0 stop:** milestone stop as in the Working Rules (decisions entry D75: "The phone apps' foundations").

---

# Milestone R1: The shell, onboarding and servers

### Task 6: The design system

**Owner:** Gemini. **Score:** 3. **Review focus:** tokens equal to the desktop's; no literal colors; native controls really native on both systems.

**Done by Claude, e8bc9b7.** Read the code before building on it. Where it differs from the steps below:
- **`Segmented`** uses `@expo/ui`: SwiftUI `Picker` with `pickerStyle('segmented')` on iOS, and Compose `SingleChoiceSegmentedButtonRow` on Android. `@react-native-segmented-control/segmented-control` is not a dependency.
- **The Android `Slider`** draws Compose's stops only up to 10. A finer step slides freely and is rounded in JavaScript.
- **The tests** are `tokens.test.ts`, `noColorLiterals.test.ts` and `components.test.tsx`.
- **Jest:** Reanimated and Worklets are mocked by `src/test/reanimated.tsx` and `src/test/worklets.ts` (`jest.config.js`), since the libraries' own mocks need a DOM. Gesture Handler's `jestSetup`, the safe-area mock and the keyboard-controller mock are in `src/test/setup.ts`.
- **Screenshots:** the gallery at phone width only. No tablet shots yet; Task 11 takes the first.

**Skills:** `references/animations/SKILL.md` and `animations.md` (CSS transitions, `:active` press feedback), and `references/gestures/tap-handling.md`.

**Files:**
- Create:
  - `mobile/src/ui/theme/{tokens.ts, typography.ts, space.ts, ThemeProvider.tsx, useTheme.ts}`;
  - `mobile/src/ui/{Text.tsx, icons.ts, Icon.tsx, Avatar.tsx, ServerIcon.tsx, Badge.tsx, Row.tsx, PaneHeader.tsx, Button.tsx, IconButton.tsx, EmptyState.tsx, Banner.tsx, ToastHost.tsx, ProgressBar.tsx, SearchField.tsx, Divider.tsx, Slider.tsx, Slider.ios.tsx, Slider.android.tsx, Switch.tsx, Switch.ios.tsx, Switch.android.tsx, Menu.tsx, Segmented.tsx, index.ts}`;
  - `mobile/src/app/dev/gallery.tsx`;
  - tests `mobile/src/ui/__tests__/{tokens.test.ts, noColorLiterals.test.ts, Text.test.tsx, Avatar.test.tsx, Badge.test.tsx, PaneHeader.test.tsx}`.

**Interfaces:**
- Consumes: Task 5's `usePrefs()` (theme) and `useToasts()`.
- Produces (spec §9):
  - **Theme:**
    - `tokens.dark`, `tokens.light` (`ColorTokens`), `ColorToken`;
    - `typography: Record<TextVariant, TextStyle>`;
    - `space = { xs: 4, sm: 8, md: 12, lg: 16, xl: 20, xxl: 24, xxxl: 32 }`;
    - `radius = { pill: 6, row: 8, image: 12, icon: 14, tile: 16, bubble: 18, sheet: 20 }`;
    - `useTheme(): { colors: ColorTokens, scheme: 'light' | 'dark' }`.
  - **Text and icons:**
    - `Text { variant: TextVariant; color?: ColorToken; numberOfLines?; selectable?; style?; children }`;
    - `IconName` (the names of spec §9.4), `Icon { name; size?: number = 20; color?: ColorToken = 'text' }`.
  - **People and servers:**
    - `Avatar { name: string; file?: string | null; size: number; speaking?: boolean }`: initials from the first letters of the first two words; the shade is `avatarShades[hash(name) % 5]` with `hash = [...name].reduce((h, c) => (Math.imul(h, 31) + (c.codePointAt(0) ?? 0)) >>> 0, 7)`. A ring of 2 pt `voice` is drawn when `speaking`, 3 pt at size 64 and above.
    - `ServerIcon { name; file?; size?: number = 52 }`: 14 pt corners.
    - `Badge { count: number; mention?: boolean }`: nothing at 0; "99+" above 99; with `mention`, "@" before the count.
  - **Structure:**
    - `Row { onPress?; onLongPress?; height?: number; selected?: boolean; accessibilityLabel: string; accessibilityRole?; children }`;
    - `PaneHeader { title: string; subtitle?: string; titleIcon?: IconName; left?: HeaderAction; right?: HeaderAction[] }`, where `HeaderAction = { icon: IconName; label: string; onPress(): void }`. It is 52 pt plus the top inset, with the title centered on iOS and at the left on Android, and a bottom hairline.
  - **Buttons:**
    - `Button { title; onPress; variant: 'primary' | 'secondary' | 'voice' | 'danger'; loading?; disabled?; icon? }`: 52 pt, 14 pt corners;
    - `IconButton { icon; label; onPress; size?: 36 | 44 | 56 | 64; tone?: 'plain' | 'toggle' | 'accent' | 'danger'; active?: boolean }`.
  - **Feedback:**
    - `EmptyState { title; body?; action?: { title; onPress } }`;
    - `Banner { text; tone: 'neutral' | 'error'; action?: { title; onPress } }`;
    - `ToastHost`;
    - `ProgressBar { progress: number; tone?: 'neutral' | 'error' }`: 3 pt.
  - **Inputs:**
    - `SearchField { value; onChangeText; placeholder; onSubmit?; right?: ReactNode }`;
    - `Slider { value; min; max; step?; onChange(value); onCommit?(value); disabled?; accessibilityLabel }`: iOS `@expo/ui/swift-ui` `Slider` in a `Host`, Android `@expo/ui/jetpack-compose` `Slider` in a `Host`;
    - `Switch { value; onChange; disabled?; accessibilityLabel }`: `Toggle` on iOS, `Switch` on Android, each in a `Host`;
    - `Menu { actions: MenuAction[]; onAction(id: string); trigger?: 'press' | 'longPress'; children }`, where `MenuAction = { id; title; icon?: IconName; destructive?; disabled? }`, over `@react-native-menu/menu`'s `MenuView`;
    - `Segmented { values: string[]; index: number; onChange(index: number) }`.

- [ ] **Step 1: Write the failing tests.**
  - `tokens.test.ts`, `tokens match the desktop's OcColors value for value`: reads `app/lib/ui/theme/oc_colors.dart` from the repository, takes every `name: Color(0x…)` in `dark` and `light`, and compares each with `tokens.ts`.
  - `noColorLiterals.test.ts`, `no color literal outside tokens.ts`: scans `src/**/*.{ts,tsx}` for `#[0-9a-fA-F]{3,8}\b`, `rgb(` and `rgba(`.
  - `Text.test.tsx`: `each variant has its size, line height, weight and family`.
  - `Avatar.test.tsx`: `initials come from the first two words`, `the shade is the same for the same name`, `a speaking ring shows only while speaking`.
  - `Badge.test.tsx`: `nothing at zero`, `99+ above 99`, `mentions start with @`.
  - `PaneHeader.test.tsx`: `actions have their labels and call back`.
- [ ] **Step 2: Run them; they fail.**
- [ ] **Step 3: Implement.**
  - `ThemeProvider` follows `usePrefs().theme`. With `system`, it follows `useColorScheme()`. It calls `Appearance.setColorScheme` so native components follow the choice.
  - Press feedback is a CSS transition to `selected` over 80 ms with the `:active` pseudo-selector, if the installed Reanimated is 4.5 or newer. Below 4.5, use the skill's fallback.
  - Every part follows spec §9.
- [ ] **Step 4: The gallery.** `dev/gallery.tsx` shows every component in each of its states, grouped. Run `scripts/screens.sh 6 gallery=/dev/gallery`, and then at tablet width. Look at all four; fix what's off.
- [ ] **Step 5: Run** `yarn typecheck && yarn lint && yarn test`.
- [ ] **Step 6: Commit:** `feat(mobile): the design system`.

### Task 7: Navigation, the boot gate and onboarding

**Owner:** Gemini. **Score:** 3. **Review focus:** a first start, a saved identity and a broken backup all land on the right screen; the secret only ever goes to SecureStore.

**Done by Claude, 1347aee.** Read the code before building on it. Where it differs from the steps below:
- **Onboarding routes:** they live under `/welcome` (`app/welcome/{_layout,index,create,import}.tsx`), not in an `(onboarding)` group. `app/index.tsx` stays and redirects to `/servers` or `/welcome`.
- **Guards:** `_layout.tsx` guards the routes with `Stack.Protected`. `welcome` is reachable only without an identity, `(tabs)` only with one. Register every new full-screen route or sheet inside the `ready` guard.
- **Boot:** `features/boot/BootGate.tsx` exports `boot()` and the gate. `core/lifecycle.ts` exports `startApp()`.
- **Opening the picker:** Welcome's **Import a backup** opens the file picker itself, then goes to `/welcome/import`. `/welcome/import?failed=1` shows "That isn't an Opencord backup.".
- **Fake mode with no identity:** start Metro with `EXPO_PUBLIC_FAKE_CORE=1 EXPO_PUBLIC_FAKE_SCENARIO=empty npx expo start --dev-client -c`. The `-c` matters: `EXPO_PUBLIC_` values are compiled in.
- **Screenshots:** `.superpowers/sdd/mobile/screens/7/`.

**Skills:** `references/gestures/SKILL.md` (where `GestureHandlerRootView` goes).

**Files:**
- Create:
  - routes `mobile/src/app/{_layout.tsx, (onboarding)/_layout.tsx, (onboarding)/index.tsx, (onboarding)/create.tsx, (onboarding)/import.tsx, (tabs)/_layout.tsx, (tabs)/servers/_layout.tsx, (tabs)/servers/index.tsx, (tabs)/you/_layout.tsx, (tabs)/you/index.tsx}`;
  - `mobile/src/features/boot/{BootGate.tsx, BootScreen.tsx}`;
  - `mobile/src/features/onboarding/{WelcomeScreen.tsx, CreateScreen.tsx, ImportScreen.tsx, useCreateIdentity.ts}`;
  - `mobile/src/core/lifecycle.ts`;
  - tests `mobile/src/features/onboarding/__tests__/onboarding.test.tsx`.
- Delete: `mobile/src/app/index.tsx` (Task 3's first screen).

**Interfaces:**
- Consumes: Task 4's `bootCore`, `startEventLoop`, `saveIdentity`; Task 5's store and `onEvents`; Task 6's components.
- Produces:
  - **The routes of spec §7.1** (Task 8 fills Servers and Task 10 fills You). Until then those two routes render `EmptyState` with a `// later: Task 8` or `// later: Task 10` comment, which those tasks remove.
  - **`startApp()`** in `lifecycle.ts`:
    - it loads prefs and servers, then starts the event loop with `onEvents`;
    - it listens to `AppState`: `setAppActive`, and `retryDisconnected()` on becoming active.

- [ ] **Step 1: Write the failing tests** (render the root layout with `FakeCore` and a memory SecureStore):
  - `a first start shows Welcome`;
  - `creating an identity saves it and opens Servers`: the secret is in SecureStore under `opencord.identity`, `mobile.displayName` is set, and `identityLoad` was called;
  - `an empty name is refused with the core's sentence`: "display name must not be empty";
  - `importing a backup decodes it and asks for a name`;
  - `a wrong backup file says so`: "That isn't an Opencord backup.";
  - `a saved identity skips onboarding`;
  - `a failed start shows its sentence and Try again`.
- [ ] **Step 2: Run them; they fail.**
- [ ] **Step 3: Implement**, per spec §7.1 and §8.1.
  - **The root layout:**
    - `GestureHandlerRootView`, `SafeAreaProvider`, `KeyboardProvider`, `ThemeProvider`, `ToastHost`;
    - the splash screen stays until the fonts load and `bootCore` answers.
  - **The tabs:** `NativeTabs` with triggers `servers` and `you`, icons from spec §9.4 (`sf` and `md` props), and labels "Servers" and "You".
  - **Onboarding** has no header; its screens use the safe area and `space.lg` edges.
- [ ] **Step 4: Screenshots:** `scripts/screens.sh 7 welcome=/ create=/create import=/import`, with a fresh install for the first one. Look at them; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): navigation, boot and onboarding`.

### Task 8: The Servers tab

**Owner:** Gemini. **Score:** 3. **Review focus:** the row's states (connecting, reconnecting, failed, unread, mentions, in voice) and its time labels.

**Skills:** `references/gestures/swipeable-and-drawer.md` (`ReanimatedSwipeable`, `overshootFriction` 8), and `references/animations/animations.md` (the badge pop).

**Files:**
- Create: `mobile/src/features/servers/{ServerListScreen.tsx, ServerRow.tsx, formatTime.ts, connectionText.ts}` and tests `mobile/src/features/servers/__tests__/{formatTime.test.ts, connectionText.test.ts, ServerList.test.tsx}`.
- Modify: `mobile/src/app/(tabs)/servers/{_layout.tsx, index.tsx}`.

**Interfaces:**
- Consumes: `useServerRows`, `markServerRead`, `retryServer`, `retryDisconnected`, `leaveServer`, `avatarFile` through `getCore()`, and Task 6's components.
- Produces:
  - `formatTime(atMs: number, nowMs: number, locale: string): string`;
  - `connectionText(connection: ConnectionRecord): { text: string; error: boolean } | null`: `null` when connected;
  - `useImageFile(serverKey, hash?)`, a small hook that caches `avatarFile` paths, in `src/features/servers/useImageFile.ts`. Later tasks reuse it.

- [ ] **Step 1: Write the failing tests.**
  - `formatTime.test.ts`, `times read as today, yesterday, a weekday, or a date`: with `now` fixed at 2026-10-26 21:30 local time, in `en-GB` and in `en-US`:
    - 21:04 today;
    - yesterday;
    - Thursday 22 October as "Thu" (26 October 2026 is itself a Monday);
    - 3 October ("3 Oct" in `en-GB`, "Oct 3" in `en-US`).

    `en-US` times have a narrow no-break space before "PM": compare with whitespace normalised.
  - `connectionText.test.ts`, one case per state:
    - "Connecting…";
    - "Reconnecting…";
    - each `Failed` reason as Task 4's sentence, with `error: true`.
  - `ServerList.test.tsx`:
    - `rows show name, preview, time and badge`;
    - `a row shows the voice glyph while the user is in voice there`;
    - `search filters by name`;
    - `leave asks first and removes the server`: the Alert says "Leave Night Owls? You can come back with an invite.";
    - `the menu offers Retry now only when not connected`;
    - `pull to refresh retries the servers that aren't connected`;
    - `with no servers it offers Add a server`;
    - `tapping a row opens the server`: `router.push('/server/fake-night-owls')`.
- [ ] **Step 2: Run them; they fail.**

  What else this task needs, found while starting it:
  - **Typed routes:** `router.push('/server/…')` and `router.push('/add-server')` don't type-check until those routes exist. Create `app/server/[serverKey]/index.tsx` (an `EmptyState` with the server's name, `// later: Task 11`) and `app/add-server.tsx` (`// later: Task 9`). Register both in `_layout.tsx` inside the `ready` guard, `add-server` as Task 9's sheet. Start Metro once so `.expo/types` is regenerated.
  - **FlashList in Jest:** add `require('@shopify/flash-list/jestSetup')` to `src/test/setup.ts`.
  - **Copy address:** `ServerRow` (in `state/types.ts`) has no address yet. Add `address: string` (`host:port` from the entry) and fill it in `selectors.ts`.
  - **Pictures:** `avatarFile` returns a plain path. `expo-image` needs a `file://` URI, so add `fileUri(path)` to `core/paths.ts`.
- [ ] **Step 3: Implement**, per spec §8.2:
  - the large title and the native search bar (`headerSearchBarOptions`);
  - the **+** header button opens `/add-server`;
  - a FlashList of `ServerRow`;
  - the leading and trailing swipes, the long-press `Menu`, `RefreshControl`, and the empty state.
- [ ] **Step 4: Screenshots:**
  - `scripts/screens.sh 8 servers=/servers` in fake mode, and again with `--tablet`;
  - the `empty` scenario's list (run with `EXPO_PUBLIC_FAKE_SCENARIO=empty`).

  Look at them; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): the server list`.

### Task 9: Add a server, and invite links

**Owner:** Gemini. **Score:** 3. **Review focus:** the trust step adds nothing until the user trusts; the claim token is sent only when filled in; links from outside open the sheet.

**Files:**
- Create:
  - `mobile/src/app/add-server.tsx`, `mobile/src/app/+native-intent.tsx`;
  - `mobile/src/features/servers/{AddServerForm.tsx, TrustStep.tsx, useAddServer.ts}`, `mobile/src/features/links/inviteLink.ts`;
  - tests `mobile/src/features/servers/__tests__/AddServer.test.tsx` and `mobile/src/features/links/__tests__/inviteLink.test.ts`.
- Modify: `mobile/src/app/_layout.tsx`, to present `add-server` as a `formSheet` with detents `[0.6]` and the grabber.

**Interfaces:**
- Consumes: `addServer`, `trustServer`.
- Produces: `inviteRoute(url: string): string | null`, which gives `/add-server?link=<encodeURIComponent(url)>` for `opencord://` and `opencords://` links and `null` otherwise. `+native-intent.tsx` uses it in `redirectSystemPath`.

- [ ] **Step 1: Write the failing tests.**
  - `inviteLink.test.ts`, `invite links open Add server, other links don't`: cases `opencord://host:7710/invite/ABC#fp=…`, `opencords://example.com/invite/X`, `https://example.com`, `opencord-app:///servers` and an empty string.
  - `AddServer.test.tsx`:
    - `adds a server and closes`;
    - `asks to trust an unknown certificate, then adds`: the fingerprint is shown in groups of four;
    - `cancelling the trust step adds nothing`;
    - `the claim token is sent only when filled in`;
    - `errors show under the field`;
    - `a link parameter fills the field`.
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.3. The trust step replaces the form inside the same sheet; it isn't an Alert.
- [ ] **Step 4: Screenshots:** `scripts/screens.sh 9 add=/add-server add-link=/add-server?link=opencord%3A%2F%2F10.0.2.2%3A7710%2Finvite%2FABC`. Then the trust step and an error, against a local server. Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): adding a server, and invite links`.

### Task 10: The You tab

**Owner:** Gemini. **Score:** 2.

**Files:**
- Create: `mobile/src/features/you/{YouScreen.tsx, NameScreen.tsx, MusicListeningRows.tsx}`, `mobile/src/app/(tabs)/you/name.tsx`, and tests `mobile/src/features/you/__tests__/You.test.tsx`.
- Modify: `mobile/src/app/(tabs)/you/index.tsx`.

**Interfaces:**
- Consumes: `useSelf`, `usePrefs`, `setTheme`, `setMusicListening`, `setDisplayName`, `identityBackupEncode`, `loadIdentity`.
- Produces: `MusicListeningRows`, which the music player reuses in Task 18. It shows the volume slider with a percentage, Mute music and Lower music while people talk.

- [ ] **Step 1: Write the failing tests:**
  - `the header shows the name and fingerprint, and a tap copies the fingerprint`;
  - `changing the name saves it and sends it to every connected server`;
  - `backing up writes the file and opens the share sheet`: the file's text equals `identityBackupEncode`'s;
  - `the theme choice is kept and applied`;
  - `the music volume shows a percentage and applies through musicSetListening`.
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.8, as an inset grouped list built from `Row`s.
- [ ] **Step 4: Screenshots:** `scripts/screens.sh 10 you=/you name=/you/name`. Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): the You tab`.

**R1 stop:** milestone stop (D76: "The phone app's shell and servers").

---

# Milestone R2: Inside a server

### Task 11: The server screen: panes, drawer, tablet split, back and gestures

**Owner:** Claude. **Score:** 4. This is the owner's signature interaction: gestures that conflict across the drawer, the stack's swipe back and Android back, and animation on the UI thread.

**Skills:**
- `references/gestures/{SKILL.md, swipeable-and-drawer.md, gesture-composition.md}`;
- `references/animations/{SKILL.md, animations-performance.md}`.

**Files:**
- Create:
  - `mobile/src/app/server/[serverKey]/index.tsx`;
  - `mobile/src/features/server/{ServerScreen.tsx, useServerPanes.ts, PhonePanes.tsx, TabletPanes.tsx, ContentPane.tsx, ChannelList.tsx, ChannelListHeader.tsx, NoChannel.tsx}`;
  - `mobile/src/features/chat/ChatPane.tsx`, `mobile/src/features/voice/VoicePane.tsx`;
  - tests `mobile/src/features/server/__tests__/{useServerPanes.test.ts, ServerScreen.test.tsx}`.

**Interfaces:**
- Consumes: `useServerHeader`, `useChannelSections`, `useChannelInfo`, `PaneHeader`, `Row`, `Icon`.
- Produces:
  - **`useServerPanes(serverKey: string, initialChannelId?: string)`:** returns `{ mode: 'channels' | 'content'; open: { id: string; kind: 'text' | 'voice' } | null; sidebarOpen: boolean; select(channel: ChannelRecord): void; back(): boolean; openSidebar(): void; closeSidebar(): void }`. `back()` returns false when the server screen should close.
  - **`ChannelList { serverKey; variant: 'full' | 'sidebar'; selectedId?: string; onSelect(channel: ChannelRecord): void }`:** rows with icon, name and selection. Task 12 adds the rest.
  - **`ChatPane { serverKey; channelId; onOpenSidebar(): void }`:** its header and an empty body. Task 13 fills the body (marked `// later: Task 13`).
  - **`VoicePane { serverKey; channelId; onOpenSidebar(): void }`:** its header and an empty body (`// later: Task 17`).

- [ ] **Step 1: Write the failing tests.**
  - `useServerPanes.test.ts`:
    - `entering a server starts at channels`;
    - `selecting a channel opens it and closes the drawer`;
    - `back from content goes to channels, and back from channels leaves`;
    - `back with the sidebar open closes it`;
    - `a channel parameter opens that channel at once`;
    - `a deleted open channel goes back to channels`.
  - `ServerScreen.test.tsx`:
    - `the sidebar lists only this server's channels, never servers`;
    - `the stack's swipe back is on only in channels`;
    - `at tablet width the list and the content sit side by side`.
- [ ] **Step 2: Run them; they fail.**
- [ ] **Step 3: Implement**, per spec §7.2.
  - **The drawer:** `ReanimatedDrawerLayout` with `drawerType="slide"`.
    - In `channels`, the drawer is the window's full width and `locked-open`.
    - On select, call `closeDrawer()`. When it has closed, switch it to the sidebar width and `unlocked`.
    - On back to `channels`, set the full width while closed, then `openDrawer()`.
    - The overlay is `scrim` at 40 %. `edgeWidth` is half the window.
  - **If the layout can't change `drawerWidth` smoothly,** build the two panes with one Pan gesture and shared values (the skill's "Custom Swipeable with Pan Gesture"), keeping the same states and test names, and say so in the report.
  - **Back and gestures:** `navigation.setOptions({ gestureEnabled: mode === 'channels' })`. Android back goes through `BackHandler` while focused.
  - **The tablet split** is a plain row: a 320 pt sidebar beside the content.
- [ ] **Step 4: Hand checks on the emulator, in fake mode:**
  - the shift both ways, at 60 fps (the performance overlay shows no dropped frames);
  - a swipe from the left half opens the sidebar;
  - the content stays tappable to close it;
  - Android back follows the table in spec §7.2.

  Screenshots: `scripts/screens.sh 11 channels=/server/fake-night-owls chat=/server/fake-night-owls?channel=<general id>`, plus the sidebar open (`?channel=<id>&sidebar=1`, a parameter used only for screenshots) and `--tablet`.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): the server screen, where channels shift into a sidebar`.

### Task 12: The channel list

**Owner:** Gemini. **Score:** 3. **Review focus:** voice rows with their people, speaking and what plays; channels hidden that the user can't view.

**Skills:** `references/animations/animations.md` (the speaking ring's transition).

**Files:**
- Create: `mobile/src/features/server/{ChannelRow.tsx, VoiceChannelRow.tsx, VoiceMemberRow.tsx, NowPlayingRow.tsx, CategoryHeader.tsx}` and tests `mobile/src/features/server/__tests__/ChannelList.test.tsx`.
- Modify: `ChannelList.tsx`, `ChannelListHeader.tsx`.

**Interfaces:**
- Consumes: `useChannelSections`, `useServerHeader`, `markServerRead`, `leaveServer`, `useImageFile`.
- Produces: the finished `ChannelList` (same props).

- [ ] **Step 1: Write the failing tests:**
  - `channels without a category come first, then each category by position`;
  - `unread channels are bold with the pill, and mentions show a count`;
  - `voice channels list their people with speaking rings and mute glyphs`;
  - `a voice channel shows what plays`;
  - `channels the user can't view are left out`;
  - `the open channel is highlighted in the sidebar`;
  - `the header menu marks all read and leaves the server`.
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.4. The list is a FlashList whose items are the flattened sections, with `getItemType` by row kind.
- [ ] **Step 4: Screenshots:** the `channels` state, and the sidebar, in fake mode, on phone and tablet. Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): the channel list with voice and music`.

### Task 13: Messages: the list, bubbles, markdown, history and reading

**Owner:** Gemini. **Score:** 3. **Review focus:** merging loads with live messages; switching channels mid-load; no jump when older messages arrive; the markdown port matching the desktop's tests.

**Skills:** `references/animations/{layout-animations.md, scroll-and-events.md}` and `references/gestures/tap-handling.md`.

**Files:**
- Create:
  - `mobile/src/features/chat/{MessageList.tsx, MessageBubble.tsx, MarkdownText.tsx, markdown.ts, items.ts, DaySeparator.tsx, NewMessagesDivider.tsx, JumpToLatest.tsx, TypingLine.tsx, useChannelLoader.ts}`;
  - `mobile/src/state/actions/chat.ts`;
  - tests `mobile/src/features/chat/__tests__/{markdown.test.ts, items.test.ts, MessageList.test.tsx}` and `mobile/src/state/__tests__/chatActions.test.ts`.
- Modify: `ChatPane.tsx` (removes Task 11's `later:` comment).

**Interfaces:**
- Consumes: `useChannelMessages`, `useChannelActivity`, `useChannelInfo`, `useMember`, `useTypingNames`, `markChannelRead`, `setOpenChannel`, `useImageFile`.
- Produces:
  - **`parseMarkdown(text: string): MdBlock[]`**, the port of `app/lib/features/chat/markdown.dart`: the same block and span types under the same names, with `MdParagraph`, `MdQuote` and `MdCode`.
  - **`buildChatItems(messages, pending, opts: { selfId; firstUnreadId?: string; nowMs; locale }): ChatItem[]`:** separators, the divider, group positions (`first`, `middle`, `last`, `single`) and own or other.
  - **Chat actions:** `openChannel(key, channelId)`, `closeChannel()`, `loadOlder(key, channelId)`, `markReadIfAtBottom(key, channelId, atBottom)`.

- [ ] **Step 1: Write the failing tests.**
  - **`markdown.test.ts`:** port every test in `app/test/unit/chat/markdown_test.dart`, case for case: one `it` per Dart `test`, with the same names, inputs and expectations. Add the line-break case "a\nb" (two lines in one paragraph).
  - **`items.test.ts`:**
    - `messages group by author within 5 minutes`;
    - `a new day starts a separator`;
    - `the divider sits above the first unread message`;
    - `pending sends come after the loaded messages`;
    - `bubbles in the middle of a group get the small corner on the author's side`.
  - **`chatActions.test.ts`:**
    - `opening a channel loads the newest 50 once`;
    - `a message that arrives during the first load shows once, in order`;
    - `an older page merges with live messages`;
    - `switching channels mid-load shows each channel's own messages`;
    - `loading older stops when a page comes back short`;
    - `reading is marked only at the bottom while the app is active`;
    - `closing a channel keeps its newest 100 and lets older ones load again`.
  - **`MessageList.test.tsx`:**
    - `names show on the first bubble of a group and avatars beside the last`;
    - `a mention shows the member's name`;
    - `a channel link opens that channel`;
    - `jump to latest appears when scrolled up and counts what's new`;
    - `typing reads as one name, two names, or several people`.
- [ ] **Step 2: Run them; they fail.**
- [ ] **Step 3: Implement**, per spec §8.5.
  - **The list:** FlashList v2, not inverted, with `maintainVisibleContentPosition={{ startRenderingFromBottom: true, autoscrollToBottomThreshold: 0.2 }}` and `onStartReached` for older pages.
  - **Bubbles:** the time sits inside the last line when it fits, else on a line of its own. Measure with `onTextLayout`'s last line width.
  - **Live messages** fade in only while at the bottom (`FadeIn` 120 ms, short under Reduce Motion).
- [ ] **Step 4: Screenshots:** `#general` in fake mode, top and bottom, in both themes, on phone and tablet; and a channel with code and quotes. Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): messages with the desktop's markdown`.

### Task 14: Attachments and the image viewer

**Owner:** Gemini. **Score:** 3. **Review focus:** placeholders sized before images load, so nothing jumps; downloads started once per file.

**Skills:** `references/gestures/{continuous-gestures.md, gesture-composition.md, testing.md}`.

**Files:**
- Create:
  - `mobile/src/features/chat/attachments/{AttachmentView.tsx, ImageAttachment.tsx, FileAttachment.tsx, fitSize.ts, useAttachmentFile.ts}`;
  - `mobile/src/app/viewer.tsx`, `mobile/src/features/viewer/{ImageViewer.tsx, useZoomPan.ts}`;
  - tests `mobile/src/features/chat/attachments/__tests__/{fitSize.test.ts, Attachments.test.tsx}` and `mobile/src/features/viewer/__tests__/ImageViewer.test.tsx`.
- Modify: `MessageBubble.tsx`; `_layout.tsx` (presents `viewer` as a transparent full-screen modal).

**Interfaces:**
- Consumes: `attachmentFile`, `expo-sharing`.
- Produces:
  - `fitSize(width: number, height: number, maxW = 260, maxH = 320): { width: number; height: number }`;
  - `useAttachmentFile(serverKey, attachmentId, variant)`: `{ path?: string; loading: boolean; error?: string }`.

- [ ] **Step 1: Write the failing tests:**
  - `fitSize keeps the aspect within 260 × 320`, for wide, tall, small and zero sizes (zero gives 260 × 195);
  - `an image shows a placeholder of its size, then its thumbnail`;
  - `a file card shows the name, size and kind`;
  - `tapping a file downloads it once and opens sharing`, even with two quick taps;
  - `the viewer closes on a swipe down` (`fireGestureHandler`, per the testing reference).
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.5's attachments and image viewer.
- [ ] **Step 4: Screenshots:** a message with two images and a file card, and the viewer. Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): images, files and the image viewer`.

### Task 15: The composer, sending, and message actions

**Owner:** Gemini. **Score:** 3. **Review focus:** one message per send whatever the order of echo and answer; retries; typing throttled to 8 s.

**Skills:** `references/gestures/tap-handling.md`.

**Files:**
- Create: `mobile/src/features/chat/{Composer.tsx, AttachmentChips.tsx, EditingBar.tsx, MessageMenu.tsx, useTypingNotifier.ts}` and tests `mobile/src/features/chat/__tests__/Composer.test.tsx` and `mobile/src/state/__tests__/sending.test.ts`.
- Modify: `mobile/src/state/actions/chat.ts` (adds the sending actions), `ChatPane.tsx`, `MessageBubble.tsx` (the long-press menu).

**Interfaces:**
- Consumes: `messageSend`, `messageSendFiles`, `sendCancel`, `messageEdit`, `messageDelete`, `typingStart`, `expo-image-picker`, `expo-document-picker`.
- Produces:
  - `sendMessage(key, channelId, text)`, `sendWithFiles(key, channelId, text, files: PickedFile[])`, `retrySend(nonce)`, `discardSend(nonce)`, `editMessage(key, messageId, text)`, `deleteMessage(key, messageId)`, `notifyTyping(key, channelId)`;
  - `PickedFile = { path; filename; size; mimeType }`.

- [ ] **Step 1: Write the failing tests.**
  - `sending.test.ts`:
    - `an echo before the send's answer shows the message once`;
    - `an echo after the send's answer shows the message once`;
    - `a failed send retried shows once`;
    - `typing is sent at most every 8 s`;
    - `cancelling a send with files stops it and leaves it failed with Cancelled`.
  - `Composer.test.tsx`:
    - `the composer is replaced when the user can't send`;
    - `the counter appears from 3,900 characters and sending stops above 4,000`;
    - `editing puts the text in the composer and saves it`;
    - `deleting asks first`;
    - `others' messages offer Delete only with Manage messages`;
    - `photos and files show as chips and send with progress`.
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.5's composer:
  - nonces from `crypto.randomUUID()` (Hermes has it);
  - the keyboard through `react-native-keyboard-controller`: a `KeyboardStickyView` for the composer, and the list's `contentInset` kept in step so the newest message stays visible.
- [ ] **Step 4: Screenshots:**
  - the composer empty, with text, and with chips uploading;
  - the editing bar;
  - a failed message;
  - the long-press menu (on the emulator, by hand).

  Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): composing, sending and message actions`.

**R2 stop:** milestone stop (D77: "Channels and chat on the phone").

---

# Milestone R3: Voice

### Task 16: Voice on the phone: permission, audio session, the background, and coming back

**Owner:** Claude. **Score:** 5 (platform audio, a foreground service, background lifetimes).

**Files:**
- Create:
  - `mobile/modules/voice-service/**`: `expo-module.config.json`; `android/src/main/java/dev/opencord/voiceservice/{VoiceServiceModule.kt, VoiceService.kt}`; `ios/VoiceServiceModule.swift` (does nothing); `src/index.ts`; `app.plugin.js`, which declares the service with `android:foregroundServiceType="microphone|mediaPlayback"`;
  - `mobile/src/features/voice/{audioSession.ts, micPermission.ts}`;
  - `mobile/src/state/actions/voice.ts`;
  - tests `mobile/src/state/__tests__/voice.test.ts`.
- Modify: `mobile/app.config.ts` (the plugin), `mobile/src/core/lifecycle.ts`, `FakeCore` (whatever voice behavior the tests need, as the core does it).

**Interfaces:**
- Produces:
  - `joinVoice(key, channelId): Promise<boolean>` (false when the microphone was refused), `leaveVoice()`, `setMuted(muted)`, `setDeafened(deafened)`, `setUserVolume(key, userId, volume)`, `setUserLocalMute(key, userId, muted)`;
  - `VoiceService.start({ title, text })` and `VoiceService.stop()`.

- [ ] **Step 1: Write the failing tests:**
  - `joining asks for the microphone first and stops if refused`;
  - `joining sets the audio session, starts the service, then joins`;
  - `leaving stops the service`;
  - `a join that fails stops the service again`;
  - `moving to another channel keeps one service`;
  - `coming back to the foreground retries the servers that aren't connected`.
- [ ] **Step 2: Run them; they fail.**
- [ ] **Step 3: Implement**, per spec §8.6 (permission sentence) and §11:
  - **The audio session:** `setAudioModeAsync` with the options in §11.
  - **The service's notification:**
    - text "In voice: <channel> · <server>";
    - channel id `voice`, name "Voice", low importance;
    - a tap opens the app;
    - `POST_NOTIFICATIONS` is asked on Android 13 and later, and a refusal doesn't stop the call.
- [ ] **Step 4: Hand checks on the emulator** (started with `-allow-host-audio`), with a local server and a desktop client in the same voice channel:
  - each hears the other;
  - with the screen locked for 2 minutes, the call stays up and the notification shows;
  - leaving removes the notification;
  - a refused microphone gives spec §8.6's message with **Open Settings**.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): voice that keeps going in the background`.

### Task 17: The voice pane, a person's sheet, and the connected bar

**Owner:** Gemini. **Score:** 3. **Review focus:** join disabled with the right reason; the link banner; the connected bar on both screens.

**Skills:** `references/animations/animations.md` (the speaking ring).

**Files:**
- Create:
  - `mobile/src/features/voice/{ParticipantTile.tsx, ControlBar.tsx, LinkBanner.tsx, ConnectedBar.tsx, ParticipantSheet.tsx}`;
  - `mobile/src/app/server/[serverKey]/member/[userId].tsx`;
  - tests `mobile/src/features/voice/__tests__/{VoicePane.test.tsx, ConnectedBar.test.tsx}`.
- Modify: `VoicePane.tsx` (removes the `later:` comment), `ServerListScreen.tsx` and the server screen's panes (the connected bar), `_layout.tsx` (the member sheet).

**Interfaces:**
- Consumes: Task 16's actions, `useVoiceMembers`, `useMyVoice`, `useNowPlaying`, `useMusicSettings`.

- [ ] **Step 1: Write the failing tests:**
  - `not connected, it shows who is there and Join voice`;
  - `join is off without connect, saying why`;
  - `a full channel says so`;
  - `connected, it shows the controls, and mute toggles`;
  - `speaking rings follow the store`;
  - `the link banner shows each state's words, and Disconnected offers Rejoin`;
  - `the person sheet sets the volume and mute for me`;
  - `your own sheet says This is you`;
  - `the connected bar shows what plays and opens the voice channel`.
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.6. Tiles are a grid with 2 columns, or 3 at tablet width.
- [ ] **Step 4: Screenshots:** Lounge not joined and joined (fake mode), the person sheet, and the connected bar on Servers and in a chat, on phone and tablet. Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): the voice pane and the connected bar`.

**R3 stop:** milestone stop (D78: "Voice on the phone").

---

# Milestone R4: Music

### Task 18: The music player: what plays, controls and volume

**Owner:** Gemini. **Score:** 3. **Review focus:** the position formula; the permissions of every control; subscribing exactly while the sheet is open.

**Skills:** `references/animations/animations.md`.

**Files:**
- Create:
  - `mobile/src/app/server/[serverKey]/music/[channelId].tsx`;
  - `mobile/src/features/music/{MusicPlayer.tsx, NowPlaying.tsx, PlayerControls.tsx, position.ts, musicAccess.ts, useMusicSessionSubscription.ts}`;
  - `mobile/src/state/actions/music.ts`;
  - tests `mobile/src/features/music/__tests__/{position.test.ts, MusicPlayer.test.tsx}`.
- Modify: `_layout.tsx` (the sheet: `formSheet`, detents `[0.6, 1]`, the grabber); `VoicePane.tsx` (the Music header button opens it).

**Interfaces:**
- Consumes: `musicSubscribe`, `musicUnsubscribe`, `musicPlayback`, `musicSetMode`, `musicVoteSkip`, `musicArtwork`, `MusicListeningRows` (Task 10), `useMusicSession`, `useChannelInfo`, `useMyVoice`.
- Produces:
  - `positionAt(session: MusicSessionRecord, nowMs: number): number`, the desktop's `positionAt` in `music_session.dart`;
  - `musicAccess(access: ChannelAccess, session, settings, inChannel: boolean)`, which returns for each control `{ allowed: boolean; reason?: string }`, with the sentences of spec §8.7;
  - `subscribeMusic`, `unsubscribeMusic`, `playback`, `setMode`, `voteSkip` in `actions/music.ts`.

- [ ] **Step 1: Write the failing tests.**
  - `position.test.ts`, `positionAt matches the desktop`: paused; playing after the anchor; before the anchor; past the end (capped); a stream of length 0.
  - `MusicPlayer.test.tsx`:
    - `it subscribes when opened and unsubscribes when closed`;
    - `play pauses and resumes`;
    - `controls the user can't use stay visible and say why`;
    - `next becomes Vote to skip without control when vote skip is on`;
    - `repeat goes off, queue, track`;
    - `seeking is offered only on a seekable track with control`;
    - `the volume applies through musicSetListening and is kept`;
    - `outside the channel it offers Join`.
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.7: the top part, controls, volume, and the `Segmented` control. Task 19 fills its sections; until then each is an `EmptyState` marked `// later: Task 19` or `// later: Task 20`.
- [ ] **Step 4: Screenshots:** playing, paused, no permission, nothing playing, and outside the channel (fake mode), in both themes, on phone and tablet. Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): the music player`.

### Task 19: Up next, search and history

**Owner:** Gemini. **Score:** 3. **Review focus:** who may remove or reorder what; searching only on submit; errors from the server shown as its words.

**Skills:** `references/gestures/swipeable-and-drawer.md`.

**Files:**
- Create: `mobile/src/features/music/{UpNext.tsx, QueueRow.tsx, MusicSearch.tsx, SourceMenu.tsx, ResultRow.tsx, History.tsx}` and tests `mobile/src/features/music/__tests__/{UpNext.test.tsx, MusicSearch.test.tsx, History.test.tsx}`.
- Modify: `MusicPlayer.tsx` (removes the `later: Task 19` comments), `actions/music.ts` (`enqueue`, `moveItem`, `removeItems`, `clearQueue`, `search`).

- [ ] **Step 1: Write the failing tests:**
  - `UpNext.test.tsx`:
    - `swipe to remove needs the right permission`;
    - `the menu plays next, moves up and down, and removes`;
    - `clearing asks first`;
    - `an empty queue says what to do`.
  - `MusicSearch.test.tsx`:
    - `search runs on submit and lists results with Local pills`;
    - `tapping a result adds it at the end`;
    - `a long press offers play next and play now with manage queue`;
    - `a server error shows its message`.
  - `History.test.tsx`: `history adds a track again`.
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.7.
- [ ] **Step 4: Screenshots:** each section with content and empty, in both themes. Look; fix.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): the queue, search and history`.

### Task 20: The upload section

**Owner:** Gemini. **Score:** 3. **Review focus:** every phase's words; cancel and retry; the picker's copies deleted in every ending.

**Files:**
- Create: `mobile/src/features/music/{UploadSection.tsx, UploadRow.tsx, pickAudio.ts, uploadText.ts}` and tests `mobile/src/features/music/__tests__/{uploadText.test.ts, UploadSection.test.tsx}`.
- Modify: `MusicPlayer.tsx` (removes the `later: Task 20` comment), `actions/music.ts` (`uploadSongs`, `cancelUpload`, `retryUpload`, `dismissUpload`).

**Interfaces:**
- Consumes: `musicUploadSong`, `musicCancelSong`, `useMusicUploads`, `expo-document-picker` (`type: 'audio/*'`, `multiple: true`, `copyToCacheDirectory: true`), and `expo-file-system` to delete the copies.
- Produces:
  - `uploadSongs(key, channelId, files: PickedFile[])` (Task 15's `PickedFile`), `cancelUpload(uploadId)`, `retryUpload(uploadId)`, `dismissUpload(uploadId)`;
  - `uploadText(phase: UploadPhaseRecord, sent: number, total: number, locale: string): string`, with the sentences of spec §8.7 ("Uploading · 42 % of 8.4 MB").

- [ ] **Step 1: Write the failing tests:**
  - `uploadText.test.ts`, `each phase reads as the spec says`, including sizes in KB, MB and GB;
  - `UploadSection.test.tsx`:
    - `picking files starts one upload per file with its own id`;
    - `rows follow the phases`;
    - `cancel stops an upload`;
    - `a failure shows the server's reason and can be retried`;
    - `a queued row fades within 3 s and its copy is deleted`;
    - `dismissing a failed row deletes its copy`;
    - `without permission the section says what's missing`.
- [ ] **Step 2: Run them; they fail.** **Step 3: Implement**, per spec §8.7's Upload part.
- [ ] **Step 4: Screenshots:** the section empty, with three uploads in different phases, and without permission. Look; fix. Then hand check against a local server with no Lavalink:
  1. upload an MP3 from the emulator's Downloads (push it with `adb push`);
  2. it plays in Lounge for a desktop client;
  3. a second upload of the same file queues at once.
- [ ] **Step 5: Run** the checks. **Step 6: Commit:** `feat(mobile): uploading songs from the phone`.

**R4 stop:** milestone stop (D79: "Music on the phone").

---

# Milestone R5: Release and docs

### Task 21: Release builds

**Owner:** Claude. **Score:** 4 (signing secrets, release assets, version codes).

**Files:**
- Modify: `.github/workflows/cicd.yml`, `mobile/app.config.ts` (the version and `android.versionCode` from the `version` job), `docs/releasing.md`.
- Create: `mobile/plugins/withReleaseSigning.js` (signing from environment variables).

- [ ] **Step 1: Versions.**
  - The app version is the release version, the same as desktop's.
  - `versionCode = major * 10_000_000 + minor * 100_000 + patch * 1_000 + rank * 100 + pre`.
    - `rank` is 0 for beta, 1 for canary and 9 for stable.
    - `pre` is the pre-release number (0 for stable, at most 99).
    - This keeps SemVer's order, beta < canary < stable (docs/releasing.md), so every newer release has a higher code.
  - Write a test in `mobile/plugins/__tests__/versionCode.test.ts`, `version codes keep the release order`:
    - `0.0.2-beta.3` → 2003;
    - `0.0.2-canary.1` → 2101;
    - `0.0.2` → 2900;
    - `0.0.3-beta.1` → 3001;
    - `1.2.3-canary.45` → 10203145;
    - a pre-release number over 99 throws.
- [ ] **Step 2: Signing.**
  - With the secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`, the APK is signed with that key.
  - Without them, it is signed with the debug key and named `…-unsigned-debugkey.apk`.
  - The release job attaches `opencord-android.apk` only when it is signed with the release key.
- [ ] **Step 3:** `docs/releasing.md` gains the phone files and how to make the key (`keytool -genkeypair …`), kept offline like the update key.
- [ ] **Step 4: Run** `actionlint` and `.github/check.sh mobile`, and push. The pull request's mobile jobs pass. **Step 5: Commit:** `ci(mobile): release builds of the phone apps`.

### Task 22: iOS on the Mac

**Owner:** Claude on the Mac. **Score:** 4.

Run this in a Claude Code session on the Mac, on `feat/mobile`, after Tasks 1–21.

- [ ] **Step 1:**
  1. Install the iOS targets;
  2. `cd mobile && yarn install && yarn core:ios && npx expo run:ios` on an iPhone simulator;
  3. then on a real iPhone, with the user's team, following ubrn's "Release check on a device".
- [ ] **Step 2: Hand checks**, with screenshots in `.superpowers/sdd/mobile/screens/22/`:
  - large titles and the search bar on Servers;
  - the stack's swipe back only in the `channels` state, and the sidebar from the left edge in `content`;
  - the sheets' detents;
  - menus and sliders as native SwiftUI and UIKit pieces;
  - the keyboard with the composer;
  - voice with a desktop client, and with the phone locked for 2 minutes;
  - the microphone and photo permission sentences;
  - dark and light.
- [ ] **Step 3:** Fix what fails, in iOS-only files where the fix is iOS-only (rule 15). Commit each fix as `fix(mobile): <what> on iOS`, push, and report which checks passed.

### Task 23: Docs and checks by hand

**Owner:** Gemini. **Score:** 2.

**Files:**
- Modify: `docs/mobile.md`, `README.md`.

- [ ] **Step 1: `docs/mobile.md`.**
  - A user guide in front of Task 3's "Developing" section: installing the APK, adding a server, the sidebar, voice in the background, the music player and its upload section.
  - What isn't there yet: spec §2's "Later" table.
  - Write it from the code ("Write docs from the code", `GEMINI.md`).
- [ ] **Step 2: `README.md`.** A "Phones" part, with two screenshots from `scripts/screens.sh`: the server list and a chat with the sidebar open, in dark.
- [ ] **Step 3: Run the hand checks** on the emulator, against a local server and a desktop client, and record each result in the ledger:
  - onboarding;
  - adding a server through the trust step;
  - unread counts against the desktop's for the same channels;
  - the shift and the sidebar;
  - sending, editing and deleting;
  - images both ways;
  - voice both ways, locked;
  - music: play, skip, seek and the queue;
  - an upload with progress, cancel and a retry;
  - the volume with the desktop's music playing.
- [ ] **Step 4: Commit:** `docs(mobile): the phone apps' guide`.

**R5 stop:** milestone stop (D80: "The phone apps' first release").
