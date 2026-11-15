# Signed Releases and Background Updates: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every release is signed and checked. Opencord opens at once and updates in the background with a voice-friendly download. A square under the server list offers the update, and clicking it plays a full-window spring animation across the restart, then rejoins voice.

**Architecture:**
- **`crates/opencord-update`** (new, MPL-2.0) holds what the launcher and the app must agree on: the manifest, track selection, the signature and hash checks, unpacking, the install folder's JSON files, and the rules for a waiting update.
- **The launcher** (`crates/opencord-launcher`) starts the app at once, switches to a prepared update first, unpacks downloads the app hands it (`--prepare`), and holds the covered picture during an update restart (`--cover`).
- **The core** (`crates/opencord-core`) checks GitHub and downloads in the background, throttled by the voice call's ping.
- **The app** shows the square, plays the animation, hands the cover to the launcher, and rejoins voice.

**Tech Stack:**
- **Rust 2024:** reqwest (blocking in the launcher, async in the core), ed25519-dalek, sha2, winit 0.30 and softbuffer 0.4 in the launcher.
- **Flutter 3.44** with Riverpod 3 and flutter_rust_bridge 2.
- **Dependencies:** none new unless a task names one.

**Spec:** `docs/superpowers/specs/2026-11-12-background-updates-design.md`. Read all of it before Task 1, and open the two mockups beside it (`…-square.html`, `…-animation.html`) in a browser. "Design" below means that file. If the design and this plan disagree, or a step can't be done as written, stop and ask the user.

## Global Constraints

**Branches and pull requests:**
- **Stage 0:** `ci/update-signing`, made from `docs/background-updates` (this plan's branch), with a pull request into `beta`.
- **Stages A–E:** `feat/background-updates`. Make it from `beta` once the Stage 0 pull request is merged, or from `ci/update-signing` if it isn't yet. Push it after Task 3 and open **one draft pull request into `beta`**. Push at the end of every stage. It stays a draft until the Stage E review passes, then `gh pr ready`.
- **Never merge,** and never push to `beta`, `canary` or `main`.

**Before Task 1:**
- `git status --short` is empty, and `df -h /` shows at least 10 GB free.
- `git log -1 --format=%s` on `docs/background-updates` is `docs: background updates plan`.

If any of these fails, stop and ask the user.

**Scope:**
- `crates/opencord-update/` (new), `crates/opencord-launcher/`, `crates/opencord-core/`;
- `app/`, including its runners;
- `.github/check.sh` and `.github/workflows/cicd.yml`;
- `docs/`, and `Cargo.toml` (the workspace member list is `crates/*`, so only `[workspace.dependencies]` changes).

Nothing else. The phone apps (`mobile/`, `crates/opencord-mobile/`) get nothing. Only keep `opencord-mobile` compiling if the core's types change.

**Signing:**
- **Claude never runs** `openssl genpkey`, `gh secret set` or `gh variable set`. The owner sets the key, under the design's "Turning it on".
- **Never weaken a check to pass a test.** Tests sign with a fixed test key, never a real one.

**Licences:** `opencord-update` is MPL-2.0, like the launcher. A new dependency must be MIT, Apache-2.0, BSD, ISC or Zlib on every target (`GEMINI.md`'s licence check).

**Values come from the design,** copied exactly:

| What | Value |
| --- | --- |
| A waiting update is fresh for | 24 h from `max(prepared_at, confirmed_at)` |
| `restart.json` counts for | 2 min |
| First check / later checks | 15 s after "started" / every 4 h |
| Chunk / per-chunk timeout | 1 MiB / 30 s |
| Throttle | baseline = median of 5 readings; start 256 KiB/s; hurting = ping > baseline + 30 ms or loss in > 2 % in one reading; hurting → rate ÷ 2, floor 32 KiB/s; hurting at the floor → pause until 3 calm readings in a row; 2 calm readings in a row → rate × 1.25; leaving the call → full speed; joining partway → pause for 5 readings, then 256 KiB/s |
| Launcher splash on a plain start | only if switching takes over 1 s |
| Cover: app waits for `cover.shown` / launcher waits for `cover.taken` | 3 s / 30 s |
| Rejoining voice: window / waiting for the server | 2 min / 30 s |
| Square | float 62 × 62, radius 16, 8 px under the rail; tile 46 × 46, radius 14 |
| Tile colour, dark / light | `#1F1F23` (hover `#27272C`) / `#F2F2F4` (hover `#E7E7EA`), plus the wash |
| Wash | the icon gradient at 16 % opacity, 24 % on hover |
| Icon | `OcIcons.download` (already exists), 26 px, weight 500, 150° gradient `#3D85F7` → `#8B5CF6`, the same in both themes |
| Fill | 150° gradient `#2D63D8` → `#6E47D3`; native window background on the covered path `#4D55D6` |
| Springs | `.smooth` = 0.5 s, bounce 0; `.snappy` = 0.5 s, bounce 0.15; `.bouncy` = 0.5 s, bounce 0.3 (`SpringDescription.withDurationAndBounce`) |

**Words the user sees,** exactly:

| Where | Text |
| --- | --- |
| Square popover, title | "Update ready" |
| Square popover, subtitle | "Click to restart on <version>" |
| Square, screen readers | "Update ready. Restart to update to <version>." |
| Cover caption, step 2 | "Updating Opencord", with "This takes a few seconds" under it |
| Cover caption, step 5 | "Updated to <version>" |
| Settings → Updates (Claude's wording) | "Opencord downloads updates in the background and offers them under the server list. Switching tracks restarts it on that track's newest build." |

`<version>` is the version as `release.json` writes it, for example `0.0.2-beta.6`.

**Logs:**
- **Never log** signatures, key material or download contents.
- **Do log** versions, sizes, rates and error kinds, to `launcher.log` and the session log.

**Generated files,** never edited by hand: `app/lib/src/rust/**` and `crates/opencord-core/src/frb_generated.rs`.

**The decisions entry is Claude's,** written at the Stage E review. Don't write one.

## Working Rules (read before Task 1)

`GEMINI.md` applies to everything here. Read "How to write code here" before each stage.

### Environment

As in `GEMINI.md`, start **every** `cargo`, `flutter`, `dart` and `flutter_rust_bridge_codegen` command with `source .superpowers/sdd/media/env.sh &&`. Create `env.sh` as `GEMINI.md` says if this machine lacks it. Check `df -h /` before the full checks. Never run `cargo clean`.

**`GEMINI.md` is longer than agy loads** (it stops at 24,000 bytes). Read the whole file with a file read before Task 1 and before each stage.

**Claude's tasks through the bridge** can't use `source`, `VAR=… cmd`, `export`, `python3`, scripts (`bash x.sh`, `.github/check.sh`), `flutter_rust_bridge_codegen`, or `cd … && git`. In their place:
- `cargo <cmd> --config build.incremental=false --config profile.dev.debug=0 …`, with environment variables as `--config 'env.NAME="value"'`;
- a lone `cd /abs/path`, then plain `git` or `flutter`.

A check a Claude task can't run there is marked **(Gemini runs)**. You run it after the task's report and paste it in the ledger.

### Commands

| What | Command, from the repository root |
| --- | --- |
| Format | `cargo fmt --all`; `cd app && dart format lib test` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` |
| Shared crate | `cargo test -p opencord-update` |
| Launcher | `cargo test -p opencord-launcher` |
| Core updater | `cargo test -p opencord_core --lib updates` |
| One unit test | `cargo test -p <crate> --lib -- --exact <module::path::test_name>` |
| App | `cd app && flutter analyze && flutter test` |
| Before each push | `.github/check.sh` |
| Dart bindings, after any change under `crates/opencord-core/src/api/` | `cd app && flutter_rust_bridge_codegen generate` |

### Task owners

Every task has an **Owner** and a **Score** (1–5: how much a mistake costs, and how hard it is to see). Scores 4–5 go to Claude, and 1–3 go to Gemini.

- **Owner: Gemini.** You implement it. On a score of 3, read its **Review focus** line twice.
- **Owner: Claude.** Don't implement it.
  - Hand the task to Claude with `delegate_task`, naming the task number and this plan's path, and wait for its report.
  - Don't edit files or run git while it works: you share the working copy.
  - Afterwards, run the task's **Checks** yourself and record it in the ledger with the report's commits.
- **Claude, when handed a task:** each Claude task is written to be done from its own section plus the Global Constraints. Follow it exactly and run its checks. Finish with the commit it gives, then report the files, commits, checks and anything left undone.

| Task | Part | Score | Owner |
| --- | --- | --- | --- |
| 1 | The key is checked when the launcher is built | 4 | Claude |
| 2 | CI: the release environment and the key checks | 4 | Claude |
| 3 | `opencord-update`: move the shared code out of the launcher | 3 | Gemini |
| 4 | `opencord-update`: the new install-folder files and their rules | 3 | Gemini |
| 5 | Launcher: start at once, switch to a waiting update, go online only when needed | 4 | Claude |
| 6 | Launcher: `--prepare` | 5 | Claude |
| 7 | Launcher: `--cover` | 5 | Claude |
| 8 | Core: checking and the resumable download | 4 | Claude |
| 9 | Core: the voice throttle | 5 | Claude |
| 10 | Core API and bindings | 3 | Gemini |
| 11 | App: the update service | 3 | Gemini |
| 12 | App: Apple's springs and the update square | 3 | Gemini |
| 13 | App: the cover animation | 3 | Gemini |
| 14 | Runners: show the window on its first frame | 5 | Claude |
| 15 | App: the hand-off and opening covered | 4 | Claude |
| 16 | App: rejoining voice after an update | 3 | Gemini |
| 17 | Docs | 2 | Gemini |

### Rules reviews will enforce

These hold on top of `GEMINI.md`:

1. **Every test names, in a comment, the change to the code that makes it fail.** For example: "fails if `is_fresh` ignores `confirmed_at`". A "mutation" line in the ledger has the pasted failing output of that change, with `file:line old → new`, or it isn't claimed.
2. **Tests drive the entry point that ships.** That's the launcher's `run` with a fake source, the core's `check_once` against a fake server, and the widget the user sees through the real provider. A helper's test doesn't count for the path that calls it.
3. **The app and the launcher use the same rule.** Whether a waiting update is fresh, and which build a track takes, come only from `opencord-update`. Grep that must come back empty in `app/lib` and `crates/opencord-launcher/src`: `rg -n "24 \* 60|Duration::from_secs\(86400\)|hours: 24"`.
4. **Every file in the install folder is written whole,** through a `.partial` and a rename. Grep that must come back empty in new code: `rg -n "fs::write\(|writeAsString\(" crates/opencord-update/src crates/opencord-launcher/src app/lib/features/updates`, except inside the one helper that does the rename.
5. **Every file this plan adds names when it is removed.** `pending.json`, `restart.json`, the `cover.*` files, `downloads/*` and the lock all appear in a task's "removed when" line, and a test shows one of them.
6. **Wait until no check is pending, then paste.** At every stage end, paste `gh pr checks` once no check is still running.

### Progress ledger

Keep `.superpowers/sdd/background-updates/progress.md` (git-ignored; never `git add -f`), one entry per task in the format of the earlier plans: commits, checks pasted, RED run, mutations, deviations.

### When a step keeps failing

Follow `GEMINI.md` → "When you're stuck: ask Claude", with question files in `.superpowers/sdd/background-updates/questions/`.

### Stages and their reviews

| Stage | Tasks | At its end |
| --- | --- | --- |
| 0: Signing | 1–2 | Full checks; push `ci/update-signing`; open its pull request into `beta`; read its checks; **stop and tell the user**. The owner merges it and moves it up the tracks before setting the key. Stage A doesn't wait for the key. |
| A: The shared crate | 3–4 | Full checks; push; open the draft pull request; **Claude reviews stage A** |
| B: The launcher | 5–7 | Full checks; push |
| C: The core | 8–10 | Full checks, plus `app/tool/check_platforms.sh`; push; **Claude reviews stage C** (Task 10, and the joins between 8–10) |
| D: The app, part 1 | 11–13 | Full checks; push; **Claude reviews stage D** |
| E: The app, part 2 | 14–17 | After Task 14, push and read `client-windows` and `client-macos`; at the end, full checks; push; **Claude reviews stage E**; then stop and report to the user |

**A review** is a bridge task, "Review stage <X>, round <r> of docs/superpowers/plans/2026-11-15-background-updates.md".
- Its report ends with `Stage <X> round <r>: PASS` or `FAIL`.
- On FAIL, fix every finding, run the full checks, push, and hand over the next round.
- Stop and ask the user after three failed rounds, or when a finding needs the user's decision.

## Review Focus

These are the inputs most likely to hurt real people while ordinary tests pass. Each has a pinned test in the task named.

1. **The app and the launcher touch the install folder at the same time.** A prepare is still unpacking when the user clicks the square, or a second launcher starts during a prepare. Expected: the lock serialises them. The restart waits for the prepare, and nothing half-unpacked ever starts. Pinned in Task 6 (`a_second_prepare_waits_for_the_lock`) and Task 5 (`a_half_unpacked_waiting_update_is_not_switched_to`).
2. **A release that crashes on start.** Expected: the next start goes online with the splash and installs whatever is newer. Pinned in Task 5 (`a_start_that_never_started_goes_online`).
3. **A download cut off part way and resumed,** when the server ignores `Range` and answers 200. Expected: it starts over and the hash still holds; a wrong byte is never installed. Pinned in Task 8 (`a_server_ignoring_range_starts_over`, `a_resumed_file_with_a_wrong_byte_is_refused`).
4. **An old launcher with a new app** (`OPENCORD_LAUNCHER_FEATURES` unset). Expected: no checks, no square, and no second Opencord started by an unknown flag. Pinned in Task 11 (`without_the_prepare_feature_nothing_checks`).
5. **Opencord is clicked while it's already running** (in the tray, or just behind another window). The single-instance check hands the start over to the running app, which keeps running its old version. Expected: the launcher switches nothing while `app_running()`; it only starts the app as today. Otherwise `installed.json` names a version that never ran, and the next start looks like a crash. Pinned in Task 5 (`a_start_while_the_app_runs_switches_nothing`).
6. **The user switches track while an update waits.** Expected: the square goes, and the next start switches tracks the usual way instead of installing the waiting build. Pinned in Task 4 (`a_waiting_update_for_another_track_is_not_switched_to`) and Task 11 (`switching_track_hides_the_square`).

## Claude's calls in this plan (for the user to check)

1. **The Settings → Updates description** is reworded as given in Global Constraints.
2. **Stage 0's pull request goes in on its own.** Stage A starts without waiting for it to merge, from `ci/update-signing` if needed.
3. **The core gets voice readings from a `watch` channel** that `media.rs` publishes to, not through Dart.
4. **Settings → Updates gets a real check.** "Check for updates" checks now, and with an update waiting the row offers "Restart", which switches at once without the animation; the square is the animated way. The row's words are in Task 11.
5. **The launcher never switches while the app runs.** A second start hands over to the running app (single instance), so switching then would leave `installed.json` on a version that never ran.
6. **The cover is the visible window only:** on Linux it leaves out the 14 px transparent frame margin.

## File Map

| File | Task |
| --- | --- |
| `crates/opencord-launcher/build.rs` | 1 |
| `.github/workflows/cicd.yml` (release job, key checks, the key in `flutter build` steps) | 2 |
| `crates/opencord-update/{Cargo.toml,src/lib.rs,src/release.rs,src/track.rs,src/signature.rs,src/archive.rs,src/layout.rs,src/check.rs}` | 3 |
| `crates/opencord-update/src/{pending.rs,markers.rs,restart.rs}` | 4 |
| `crates/opencord-launcher/src/{main.rs,update.rs}` | 5 |
| `crates/opencord-launcher/src/prepare.rs` | 6 |
| `crates/opencord-launcher/src/cover.rs` | 7 |
| `crates/opencord-core/src/updates/{mod.rs,github.rs,download.rs}` | 8 |
| `crates/opencord-core/src/updates/throttle.rs`, `crates/opencord-core/src/media.rs` (the readings channel) | 9 |
| `crates/opencord-core/src/api/updates.rs`, `app/lib/src/rust/**` (generated) | 10 |
| `app/lib/features/updates/{update_service.dart,launcher_process.dart}`, `app/lib/features/settings/user/app_pages.dart`, `app/lib/app_start.dart` | 11 |
| `app/lib/ui/theme/oc_springs.dart`, `app/lib/features/updates/update_square.dart`, `app/lib/features/shell/desktop_shell.dart` | 12 |
| `app/lib/features/updates/update_cover.dart` | 13 |
| `app/lib/features/updates/{restart_handoff.dart,restart_note.dart}`, `app/lib/app_start.dart`, `app/lib/features/window/window_startup.dart` | 15 |
| `app/windows/runner/*`, `app/macos/Runner/*`, `app/lib/features/window/native_window.dart` | 14 |
| `app/lib/features/updates/voice_rejoin.dart` | 16 |
| `docs/updater.md`, `docs/releasing.md` | 17 |

---

## Stage 0: Signing

### Task 1: The key is checked when the launcher is built

**Owner:** Claude. **Score:** 4 (a bad key would crash every user's launcher).

**Files:** Create `crates/opencord-launcher/build.rs`. Modify `crates/opencord-launcher/src/signature.rs`, `crates/opencord-launcher/Cargo.toml` (build dependency `data-encoding`, already in the workspace).

**What to build:**
- `build.rs` reads `OPENCORD_UPDATE_PUBLIC_KEY` and emits `cargo:rerun-if-env-changed` for it.
  - If the key is set and not blank, it must decode as base64 to 32 raw bytes, or to 44 bytes of DER whose first 12 are `30 2a 30 05 06 03 2b 65 70 03 21 00`. Otherwise the build panics with `OPENCORD_UPDATE_PUBLIC_KEY is not a base64 Ed25519 public key (raw 32 bytes or DER)`.
  - It doesn't check that the bytes are a valid curve point, since that needs ed25519-dalek in the build. The runtime check below covers it.
- `built_in_key()` keeps its signature. If the key still can't be read at run time (an invalid point), it logs `the built-in update key can't be read` and returns `Err`. Change the return type to `Result<Option<VerifyingKey>, KeyError>`, and have `main.rs` refuse to update when it gets `Err` and start the installed version. It never panics and never accepts unsigned releases.
- **Test:** `a_key_that_is_not_a_point_refuses_updates_without_panicking`.

**Checks:** `cargo test -p opencord-launcher signature`, plus `cargo build -p opencord-launcher --config 'env.OPENCORD_UPDATE_PUBLIC_KEY="abc"'`, which must fail with the message above (paste its last line). Clippy. (Task 3 moves `build.rs` and `signature.rs` into `opencord-update`; this check moves with them.)

**Commit:** `fix(launcher): check the update key when building`.

### Task 2: CI: the release environment and the key checks

**Owner:** Claude. **Score:** 4 (a mistake stops a track updating).

**Files:** Create `.github/release/update-key.sh` (subcommands `match <pem-file> <public-key-base64>` and `verify <public-key-base64> <file> <sig-file>`, exit 1 with the `::error::` lines below). Modify `.github/workflows/cicd.yml` to call it.

**What to build** (the design's Stage 0, changes 1–5):
- **The release job** (`release:`, around line 936) gets `environment: release` on push runs only. Pull request runs don't release, so they skip the job or don't name the environment.
- **A step at the start of the release job** fails the run when:
  - only one of `secrets.UPDATE_SIGNING_KEY` and `vars.UPDATE_PUBLIC_KEY` is set, with `::error::UPDATE_SIGNING_KEY and UPDATE_PUBLIC_KEY must be set together`;
  - both are set, but the raw 32 bytes of the public key derived from the secret (`openssl pkey -pubout -outform DER | tail -c 32`) differ from the variable's last 32 bytes after base64 decoding, with `::error::UPDATE_PUBLIC_KEY doesn't match UPDATE_SIGNING_KEY`.
- **The `Sign` step** also verifies `release.json.sig` against `vars.UPDATE_PUBLIC_KEY` itself: rebuild a PEM from the variable's last 32 bytes, then `openssl pkeyutl -verify`.
- **The key for the core:** the `flutter build linux|windows|macos` steps get `OPENCORD_UPDATE_PUBLIC_KEY: ${{ vars.UPDATE_PUBLIC_KEY }}`.
- **Leave the key unset.** With neither value set (today), every job behaves as now, and the `Sign` warning stays.

**Checks (Gemini runs):**
- `actionlint` if installed, or `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/cicd.yml'))"`.
- `update-key.sh` against two throwaway keys, made with `openssl genpkey` under `$TMPDIR` and deleted after. Paste four runs: `match` with the right key passes and with the other key fails with its message; `verify` with a good signature passes and with a changed file fails.

**Commit:** `ci: refuse a half or mismatched update key`.

**Then, at the end of Stage 0:**
1. Push `ci/update-signing` and open its pull request into `beta`. The body gives the design's "Turning it on" steps for the owner, word for word.
2. Wait until no check is pending, and paste `gh pr checks`.
3. Stop and tell the user.

---

## Stage A: The shared crate

### Task 3: `opencord-update`: move the shared code out of the launcher

**Owner:** Gemini. **Score:** 3. **Review focus:** no behaviour change; every moved test still runs and passes; the launcher's binary-only parts (winit, splash, setup, launch, log) stay in the launcher.

**Files:** Create `crates/opencord-update/` (`Cargo.toml` with `license = "MPL-2.0"`, `publish = false`). Move these from `crates/opencord-launcher/src/` with `git mv`, so history follows: `release.rs`, `track.rs`, `signature.rs`, `archive.rs`, `layout.rs`, and the selection half of `github.rs` (the `Listed` parsing and choosing a track's newest). Move Task 1's `crates/opencord-launcher/build.rs` to `crates/opencord-update/build.rs`, because `built_in_key()` reads the key where `signature.rs` compiles. Move `REPOSITORY` (the `OPENCORD_UPDATE_REPOSITORY` rule) to `opencord_update::REPOSITORY`. Modify `crates/opencord-launcher/{Cargo.toml,src/main.rs,src/update.rs,src/github.rs}`. Add `-p opencord-update` to `CLIENT_CRATES` in `.github/check.sh`, and to the `client-linux` test step in `cicd.yml` that lists `-p opencord-launcher`.

**Interfaces (produced):**
- `opencord_update::{release, track, signature, archive, layout}`, public with the same names as today. `opencord_update::signature::built_in_key() -> Result<Option<VerifyingKey>, KeyError>` (Task 1) is the only place the key is read; the launcher and the core both call it.
- New `opencord_update::check::newest(listing: &[Listed], track: Track) -> Option<&Listed>` for what `github.rs` chooses today. Networking stays in the launcher (`github.rs` keeps `Reqwest` and `Github`).
- `Error` stays in the launcher. The crate's functions return their own error types: `ManifestError`, `ArchiveError`.

- [ ] **Step 1:** Move the files and fix the imports. Their `#[cfg(test)]` modules move with them.
- [ ] **Step 2:** `cargo test -p opencord-update` and `cargo test -p opencord-launcher` both pass. Paste the totals. Together they must equal the launcher's test count before the move (paste that first, from `git stash` or the parent commit).
- [ ] **Step 3:** `cargo tree -p opencord-update -e normal` shows no winit, softbuffer or ab_glyph.
- [ ] **Step 4:** Commit `refactor(update): move the shared update code into opencord-update`.

### Task 4: `opencord-update`: the new install-folder files and their rules

**Owner:** Gemini. **Score:** 3. **Review focus:**
- freshness uses the later of `prepared_at` and `confirmed_at`;
- an update restart skips the age check only for its own `to`;
- every file is written whole (Rule 4).

**Files:** Create `crates/opencord-update/src/{pending.rs,markers.rs,restart.rs}`. Modify `src/layout.rs`.

**Interfaces (produced):**
- **Writing files:** `Layout::write_json` becomes `pub(crate)`, and is the only writer.
- **The waiting update:**
  - `pending.rs`: `Pending { version: Version, track: String, prepared_at: u64, confirmed_at: Option<u64> }`, with times in Unix milliseconds;
  - `Layout::read_pending() -> Option<Pending>`, `write_pending(&Pending)`, `remove_pending()`, and `confirm_pending(version: &Version, now: u64) -> io::Result<bool>`, which sets `confirmed_at` only when the waiting version is `version`.
- **`pending::FRESH_FOR: Duration = 24 h`**, and `Pending::is_fresh(&self, now: u64) -> bool`, true when `now - max(prepared_at, confirmed_at) < FRESH_FOR`.
- **Whether to switch:** `pending::switchable(layout: &Layout, track: Track, now: u64, restart: Option<&RestartNote>) -> Option<Version>`. It is `Some(version)` when the waiting update:
  - has a whole folder (`is_whole`);
  - is for `track`;
  - is newer than `current()` (compared within the track, as `update::update` does);
  - and is fresh, **or** `restart` is fresh (under 2 min) with `restart.to == version`.
- **The download note:** `pending.rs` also has `DownloadNote { tag: String, version: Version, track: String, file: String, url: String, size: u64, sha256: String }`, the JSON of `downloads/<file>.json`. `Layout::{read_download_note(path), write_download_note(&DownloadNote)}` read and write it. Task 8 writes it and Task 6 reads it.
- **The app is running:** `markers.rs` has `Layout::app_lock()` (`app.lock` in the root) and `Layout::app_running() -> bool`, which is true when an exclusive `File::try_lock` on `app.lock` fails (stable since Rust 1.89). The running app holds that lock (Task 10's `mark_started`). Windows and Linux both take the lock through Rust's `File` locks, so the two sides agree.
- **Start markers:** `markers.rs` has `Mark { version: Version, at: u64 }`, `Layout::{write_launching, read_launching, write_started, read_started}`, and `Layout::last_start_crashed() -> bool`. That is true when `launching.version` is `current()` and `started` is missing or names another version.
- **The restart note:** `restart.rs` has `RestartNote { from: Version, to: Version, at: u64, voice: Option<VoiceNote>, window: Option<WindowNote>, cover: bool }`, with `VoiceNote { server_key: String, channel_id: i64 }` and `WindowNote { x: i32, y: i32, width: u32, height: u32 }` in physical pixels. It comes with `Layout::{read_restart, write_restart, remove_restart}`, `RESTART_FOR: Duration = 2 min`, and `RestartNote::is_fresh(now)`.
- **Path helpers:** `Layout::{pending_file, launching_file, started_file, restart_file, cover_image, cover_shown, cover_taken, prepare_lock}()`, which are `pending.json`, `launching.json`, `started.json`, `restart.json`, `cover.rgba`, `cover.shown`, `cover.taken` and `prepare.lock` in the root.
- **`Layout::prune`** also keeps the waiting version. It removes only those `downloads/` files that are not named by `downloads/*.json` for the newest waiting or in-progress release.
- **When each file is removed** (Rule 5):
  - `pending.json`: by the launcher when it switches;
  - `restart.json` and `cover.*`: by the new app (Task 15);
  - `downloads/`: by prune;
  - `prepare.lock`: released when the prepare ends (Task 6);
  - `app.lock`: released when the app exits; the file itself stays.
- **A shared fixture:** `crates/opencord-update/tests/fixtures/restart.json`, a full `RestartNote` with voice and window. Task 4's test parses it, and Task 15's Dart test parses the same file (`../crates/opencord-update/tests/fixtures/restart.json` from `app/`) and writes it back field for field.

- [ ] **Step 1: Write the failing tests:**
  - `a_fresh_waiting_update_is_switchable`, `one_older_than_a_day_is_not`;
  - `a_recent_confirmation_keeps_it_fresh` (fails if `is_fresh` ignores `confirmed_at`);
  - `an_update_restart_switches_an_old_one_but_only_its_own_version`;
  - `a_waiting_update_for_another_track_is_not_switched_to` (Review Focus 6);
  - `a_half_unpacked_one_is_not_switchable`;
  - `one_older_than_the_installed_version_is_not_switchable`;
  - `a_launch_without_a_started_mark_is_a_crash`, `a_started_mark_for_another_version_is_too`;
  - `prune_keeps_the_waiting_version_and_its_download`;
  - `confirm_only_touches_the_named_version`;
  - `files_are_written_through_a_partial` (no `.partial` is left behind, and the file reads back);
  - `the_restart_fixture_parses`;
  - `app_running_sees_a_held_lock` (hold the lock on another `File` handle).
- [ ] **Steps 2–4:** RED, implement, GREEN.
- [ ] **Step 5:** Commit `feat(update): waiting updates, start marks and the restart note`.

---

## Stage B: The launcher

### Task 5: Launcher: start at once, switch to a waiting update, go online only when needed

**Owner:** Claude. **Score:** 4 (every user's start, and prune deletes folders).

**Files:** Modify `crates/opencord-launcher/src/{main.rs,update.rs,launch.rs}`.

**What to build** (design: "The launcher's start", "Recovering from a bad release", "Old launchers"):
- **Choosing the path in `run`:**
  - Go online first, with the splash and today's `update_until_installed`, when:
    - nothing is installed;
    - a track switch is waiting;
    - it runs as the installer;
    - or `layout.last_start_crashed()`.
  - Otherwise there is no network. If `layout.app_running()`, switch nothing, write nothing, and start the app as today (Review Focus 5). Otherwise call `pending::switchable(…, restart)`, where `restart` is `read_restart()` only when this run has `--after-pid`.
    - If it gives a version: write `installed.json` for it, run `update_launcher`, remove `pending.json`, then prune.
    - Show the splash only if that takes over 1 s, using a timer around the switch.
- **Before starting the app,** `write_launching(current)`, except when `app_running()`. Start the app with `OPENCORD_LAUNCHER_FEATURES=prepare cover`, beside the existing variables.
- **Link opens** follow the same no-network path, as today's do.
- **Tests** go in `main.rs` or `update.rs` with fake `Source`s, and the source panics if it is asked anything on the no-network path:
  - `a_plain_start_with_nothing_waiting_makes_no_request`;
  - `a_fresh_waiting_update_is_switched_to_without_a_request`;
  - `a_half_unpacked_waiting_update_is_not_switched_to` (Review Focus 1);
  - `a_start_that_never_started_goes_online` (Review Focus 2);
  - `the_installer_goes_online`;
  - `an_update_restart_switches_an_old_waiting_update`;
  - `the_app_is_started_with_the_features_variable`;
  - `a_start_while_the_app_runs_switches_nothing` (Review Focus 5: hold `app.lock` in the test).

**Checks:** `cargo test -p opencord-launcher` and `cargo test -p opencord-update`, clippy.

**Commit:** `feat(launcher): start at once and switch to a waiting update`.

### Task 6: Launcher: `--prepare`

**Owner:** Claude. **Score:** 5 (signature and hash checks on files from the network).

**Files:** Create `crates/opencord-launcher/src/prepare.rs`. Modify `main.rs` (argument parsing: `--prepare <path>`).

**What to build** (design: "`--prepare`"):
1. Take `prepare.lock` with an exclusive file lock (`File::try_lock` on Rust 1.89 or later, otherwise `fs2`-style `flock`/`LockFileEx` through the `windows` crate already in use). If it's held, wait up to 10 minutes for it, then exit 3.
2. Read `downloads/<file>.json`.
3. When `signature::built_in_key()` gives a key, verify `downloads/release.json.sig` over `downloads/release.json`.
4. Parse the manifest with `Manifest::parse(bytes, tag, release::PLATFORM)`, and check its tag, version, track (`Track::of`) and asset (file name, size, SHA-256) against the download's JSON.
5. Hash the archive and compare.
6. Unpack it to `partial_dir`, check `app_executable`, and rename the folder.
7. Write `pending.json` with `prepared_at = now`.
8. Exit 0.

On failure, write `downloads/<file>.error` (one line, the error kind and message) and exit 1. A failed prepare leaves no `.partial` folder, and the lock is released on every path.

It shows no window, never starts the app, and touches neither `installed.json` nor prune.

**Tests:**
- `a_good_download_is_prepared_and_waits`;
- `a_bad_signature_prepares_nothing`;
- `a_wrong_hash_prepares_nothing`;
- `another_platforms_manifest_is_refused`;
- `a_second_prepare_waits_for_the_lock` (Review Focus 1: hold the lock in the test, start a prepare on a thread, release the lock, and the prepare completes);
- `a_failed_prepare_writes_its_error_file_and_leaves_no_partial`.

**Checks:** `cargo test -p opencord-launcher prepare`, clippy. Lint the Windows lock code from Linux: `cargo check --target x86_64-pc-windows-gnu -p opencord-launcher` if that target is installed, otherwise a scratch crate that `#[path]`-includes the Windows-only module. Say which was done.

**Commit:** `feat(launcher): prepare a download the app hands over`.

### Task 7: Launcher: `--cover`

**Owner:** Claude. **Score:** 5 (platform windowing, a hand-off between processes).

**Files:** Create `crates/opencord-launcher/src/cover.rs`. Modify `main.rs` (the flag) and `splash.rs` only to share its winit and softbuffer setup.

**What to build** (design: "The launcher holds the cover", steps 2, 4 and 6):
- **Order:** with `--cover`, read `restart.json` and `cover.rgba` first. The header is `b"OCCV"`, then `u32` width, `u32` height and `u32` stride (little-endian), then RGBA rows.
- **The window:** borderless, not resizable, at `WindowNote`'s physical position and size (`PhysicalPosition`, `PhysicalSize`), drawing the image 1:1. It sits above other windows only while the app's is gone, and doesn't take focus where the platform allows (`with_active(false)`).
- **Then** write `cover.shown`, and only then wait on `--after-pid`. Waiting first would deadlock, because the app waits for `cover.shown`.
- **After the switch** (Task 5's path), start the app, and keep pumping events until `cover.taken` appears or 30 s pass. Then close the window and exit.
- **A missing or broken image, or missing geometry:** log it, skip the window, write `cover.shown` anyway so the app doesn't wait, and carry on with the restart.
- **Tests:** put the windowing behind a small trait (`CoverWindow { show(&Image, WindowNote), close() }`) and test with a fake:
  - `the_cover_is_shown_before_waiting_on_the_pid` (a fake that records the order);
  - `cover_taken_closes_it`;
  - `without_cover_taken_it_closes_after_30_s` (use an injected clock);
  - `a_broken_image_still_writes_cover_shown`;
  - `the_image_header_is_read_with_its_stride`.

**The window's rectangle** is the app's visible content: on Linux the window has a 14 px transparent frame margin (`windowFrameMargin`) that `WindowNote` and `cover.rgba` both leave out (Tasks 14 and 15).

**Checks:** `cargo test -p opencord-launcher cover`, clippy. **Owed by hand** (not in the bridge run): on Linux X11, a debug build shows a generated image at a set position. Add it to Task 14's list of OS checks.

**Commit:** `feat(launcher): hold the cover across an update restart`.

---

## Stage C: The core

### Task 8: Core: checking and the resumable download

**Owner:** Claude. **Score:** 4 (an external protocol; files that will be run).

**Files:** Create `crates/opencord-core/src/updates/{mod.rs,github.rs,download.rs}`. Modify `crates/opencord-core/{Cargo.toml,src/lib.rs}` (dependency `opencord-update`). The key comes from `opencord_update::signature::built_in_key()`; don't read `OPENCORD_UPDATE_PUBLIC_KEY` again. Gate the `updates` module, and the dependency, off the phone targets: `#[cfg(not(any(target_os = "android", target_os = "ios")))]`.

**Interfaces (produced, used by Tasks 9 and 10):**
- **`Throttle`** (in `updates/mod.rs`): `pub trait Throttle: Send + Sync { fn take(&self, bytes: usize) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>; }`, and `Unlimited`, which returns at once.
- **`Updater::new(root: PathBuf, track: Track, installed: Version, platform: &'static str, http: reqwest::Client, throttle: Arc<dyn Throttle>)`.**
- **`Updater::check_once(&self) -> Result<CheckOutcome, UpdateError>`**, where `CheckOutcome` is one of:
  - `UpToDate`;
  - `AlreadyWaiting(Version)`: `confirm_pending` was called;
  - `Downloaded { version, download_json: PathBuf }`: `downloads/<file>.json`, `release.json` and `release.json.sig` are in place for `--prepare`.
- **`UpdateError`** variants: `Network`, `NoRelease`, `Manifest`, `Signature`, `Download`, `Io`.
- **Which release:** chosen as the launcher does, through `opencord_update::check`. Stable reads `releases/latest/download/release.json`; beta and canary list `api.github.com/repos/<repo>/releases?per_page=100`. The repository is `REPOSITORY` from the launcher's `OPENCORD_UPDATE_REPOSITORY` rule, moved to `opencord-update` in this task.
- **The download** comes in `Range: bytes=<from>-<to>` chunks of 1 MiB, each with a 30 s timeout, appended to `downloads/<file>.part`.
  - Resume when `downloads/<file>.json` matches the tag and the SHA-256.
  - A 200 answer, instead of 206, truncates the file and starts over.
  - When complete, read the file back to hash it, then rename `.part` to the file.
  - Every chunk waits on `throttle.take(n)` before it is read. Tests use `Unlimited`; Task 9 makes the voice one and Task 10 passes it in.

**Tests** (a local `axum` or `hyper` test server in `updates/tests.rs`, signing with a fixed key):
- `a_newer_signed_release_is_downloaded_for_prepare`;
- `an_unsigned_release_is_refused_with_a_key`;
- `the_same_waiting_version_is_confirmed_not_downloaded`;
- `a_download_resumes_from_its_part_file`;
- `a_server_ignoring_range_starts_over` (Review Focus 3);
- `a_resumed_file_with_a_wrong_byte_is_refused` (Review Focus 3);
- `another_tracks_release_is_never_taken`.

**Checks:** `cargo test -p opencord_core --lib updates`, clippy.

**Commit:** `feat(core): check and download updates in the background`.

### Task 9: Core: the voice throttle

**Owner:** Claude. **Score:** 5 (real time; a voice call's quality).

**Files:** Create `crates/opencord-core/src/updates/throttle.rs`. Modify `crates/opencord-core/src/media.rs`. Where `Media::stats()` sends `MediaEvent::ConnectionStats`, it also calls `publish_voice_reading(Some(VoiceReading { ping_ms, loss_in_percent }))`, and leaving voice calls `publish_voice_reading(None)`.

**Interfaces (produced):**
- In `throttle.rs`: `VoiceReading { ping_ms: Option<u32>, loss_in_percent: Option<f32> }`; a process-wide `static READINGS: LazyLock<watch::Sender<Option<VoiceReading>>>` (one call per process, so one channel); `pub(crate) fn publish_voice_reading(Option<VoiceReading>)` and `pub(crate) fn voice_readings() -> watch::Receiver<Option<VoiceReading>>`.
- `VoiceThrottle::new(readings: watch::Receiver<Option<VoiceReading>>) -> VoiceThrottle`, which implements Task 8's `Throttle`.

**The controller,** a pure state machine (`Controller::on_reading(Option<VoiceReading>) -> Rate`, where `Rate` is `Unlimited | Limited(bytes_per_s) | Paused`), follows Global Constraints' throttle line exactly:
- **The baseline** is the median of 5 readings.
- **Joining partway** pauses until 5 readings are in.
- **A reading with no ping** counts as neither calm nor hurting.

A token bucket enforces `Limited`, sized one second of the rate.

**Tests** (the controller with scripted readings, plus one bucket test with tokio's paused clock):
- `outside_a_call_it_is_unlimited`;
- `a_call_starts_at_256_kib`;
- `one_hurting_reading_halves`;
- `it_never_goes_under_32_kib`;
- `hurting_at_the_floor_pauses_until_3_calm`;
- `two_calm_readings_grow_by_a_quarter`;
- `leaving_the_call_is_unlimited_at_once`;
- `joining_partway_pauses_for_a_baseline`;
- `loss_over_2_percent_hurts_without_ping_rising`;
- `the_bucket_holds_the_rate_over_ten_seconds` (within ±5 %).

**Checks:** `cargo test -p opencord_core --lib updates::throttle`, clippy.

**At the end of Stage C** also run `app/tool/check_platforms.sh` (Gemini runs), so the phone targets still build without `updates`.

**Commit:** `feat(core): slow update downloads when they hurt a call`.

### Task 10: Core API and bindings

**Owner:** Gemini. **Score:** 3. **Review focus:**
- `Downloaded` is sent only once all three files `--prepare` needs are in place;
- every failure is sent as `Failed`, which the app logs;
- nothing runs until `start_updates` is called.

**Files:** Create `crates/opencord-core/src/api/updates.rs`. Regenerate `app/lib/src/rust/**`. Modify `crates/opencord-core/src/api/mod.rs`.

**Interfaces (produced):**
- **`start_updates(root: String, track: String, installed: String) -> Result<(), String>`:**
  - builds the `Updater` with `Arc::new(VoiceThrottle::new(voice_readings()))`;
  - spawns one task running `check_once` 15 s after it's called, then every 4 h;
  - a second call replaces the first task;
  - `stop_updates()` ends it.
- **`update_events() -> StreamSink<UpdateEvent>`.** `UpdateEvent` is one of:
  - `Downloaded { version: String, download_json: String }`;
  - `AlreadyWaiting { version: String }`;
  - `Failed { kind: String, message: String }`.
- **`check_updates_now()`** runs `check_once` at once, for tests and the Settings page.
- **`mark_started(root: String, version: String) -> Result<(), String>`** writes `started.json`, through Task 4's `write_started`, and takes `app.lock` and holds it until the process exits. A lock held by another process is an error: `another Opencord holds the install`.
- **`waiting_version(root: String) -> Option<String>`** is the version `pending.json` names, for the macOS prepare check (Task 11).

- [ ] **Step 1: Write the failing tests:** `start_updates_checks_after_15_s` (paused clock), `stop_updates_ends_the_task`, `a_failure_is_sent_as_failed`, `mark_started_writes_started_json`, `mark_started_holds_the_app_lock`, `waiting_version_reads_pending`.
- [ ] **Steps 2–4:** RED, implement, GREEN, then `flutter_rust_bridge_codegen generate`.
- [ ] **Step 5:** Commit `feat(core): update API for the app`.

---

## Stage D: The app, part 1

### Task 11: App: the update service

**Owner:** Gemini. **Score:** 3. **Review focus:**
- "started" is written in all three cases: shown, hidden in the tray, and onboarding;
- nothing checks without the `prepare` feature;
- on macOS the launcher is started through `open -n -g`, like `Installation.restart`.

**Files:** Create `app/lib/features/updates/{update_service.dart,launcher_process.dart}`. Modify `app/lib/app_start.dart`, `app/lib/features/desktop/installation.dart` (add `features`, a `Set<String>` from `OPENCORD_LAUNCHER_FEATURES` split on spaces), `app/lib/features/settings/user/app_pages.dart` (the new description from Global Constraints, and the row below), and the tests.

**The Settings row** that says "Check for updates now / Restarts Opencord" today becomes:
- **with an update waiting:** "Update ready", subtitle "Restart to update to <version>", button "Restart", which runs `Installation.restart()` and switches at once, without the animation (Claude's call 4);
- **with none:** "Check for updates", subtitle "Looks for a newer build now", button "Check", which calls `check_updates_now`. While it runs, the subtitle says "Checking…". Afterwards it says "Opencord is up to date", or the row turns into the waiting state.

Add these strings to the Global Constraints table's words when you build it; they're Claude's wording.

**Interfaces (produced):**
- **`updateReadyProvider`:** `Notifier<String?>`, the waiting version or null.
- **`UpdateService`:**
  - **When it runs:** only when `installation != null && installation.features.contains('prepare')`.
  - **Starting:** once "started" is reached, it calls Task 10's `mark_started` and then `start_updates`.
  - **"Started"** means `app_start` has finished and the `WidgetsBinding.instance.endOfFrame` after `runWidget` has completed.
  - **On `Downloaded`:** run the launcher with `--prepare <download_json>` through `launcher_process.dart`, and wait for its exit.
    - Exit 0 sets `updateReadyProvider` to the version.
    - Anything else reads `downloads/<file>.error` into the session log and leaves the state unchanged.
  - **On `AlreadyWaiting`:** set the version.
  - **Switching track** clears the state.
- **`launcher_process.dart`:** `Future<int> runLauncher(Installation, List<String> args)`.
  - **On macOS:** `/usr/bin/open -n -g -W <bundle> --args …`, where `-W` waits for it to exit. `open -W` doesn't pass the exit code, so on macOS success means Task 10's `waiting_version` names the version afterwards.
  - **Elsewhere:** `Process.run` with the launcher path.

- [ ] **Step 1: Write the failing tests** (unit tests with a fake core and a fake launcher runner, driving the real provider):
  - `without_the_prepare_feature_nothing_checks` (Review Focus 4);
  - `started_is_written_when_hidden_in_the_tray`, `and_on_onboarding`;
  - `a_prepared_download_sets_ready`;
  - `a_failed_prepare_logs_its_error_and_stays_unset`;
  - `already_waiting_sets_ready_without_a_prepare`;
  - `switching_track_hides_the_square` (Review Focus 6);
  - `settings_shows_the_new_description`;
  - `the_settings_row_checks_then_says_up_to_date`, `and_offers_restart_when_one_waits`.
- [ ] **Steps 2–4:** RED, implement, GREEN.
- [ ] **Step 5:** Commit `feat(app): background update service`.

### Task 12: App: Apple's springs and the update square

**Owner:** Gemini. **Score:** 3. **Review focus:**
- the springs are `SpringDescription.withDurationAndBounce` with the exact presets, with no `Curves.*` anywhere in the new files (grep: `rg -n "Curves\." app/lib/features/updates app/lib/ui/theme/oc_springs.dart` must be empty);
- the square looks like the mockup in both themes;
- the rail gives up its room smoothly.

**Files:** Create `app/lib/ui/theme/oc_springs.dart`, `app/lib/features/updates/update_square.dart`. Modify `app/lib/features/shell/desktop_shell.dart`: the left `SizedBox(width: OcSize.railWidth)` holding `OcFloat(child: ServerRail())` becomes a `Column` with the rail `Expanded` and `UpdateSquare` under it.

**Interfaces (produced):**
- **`OcSprings`:** `smooth`, `snappy` and `bouncy` as `SpringDescription`s, plus `OcSprings.smoothFor(Duration)`.
- **`OcSprings.simulate(AnimationController, {required SpringDescription spring, double from, double to})`** runs a `SpringSimulation` on a controller, unbounded so a bounce can pass 1.
- **With reduce motion on** (`OcMotion`), it jumps to the end.

**`UpdateSquare`:**
- **What it watches:** `updateReadyProvider`. It takes no room when that is null.
- **Arrival:** on `.snappy`, height 0 → 62 plus the 8 px gap, with opacity and a 14 px rise. The arrow drops in once from −8 px on `.smooth`.
- **The tile:** as Global Constraints give it. The icon is drawn through a `ShaderMask` with the 150° gradient.
- **Hover:** the arrow drops 2 px on `.snappy`, the wash goes to 24 %, and the tile takes its hover colour. The popover is the rail's own: reuse `RailPopover`'s look by adding an `id == 'update'` case that shows the two strings.
- **Press:** scale to 0.96 on `.snappy`.
- **Activating it:** a click, Enter or Space calls `onActivate`, which Task 15 wires to the hand-off. Until then it calls `Installation.restart`.
- **Screen readers:** the label from Global Constraints.

- [ ] **Step 1: Write the failing tests:**
  - `springs_match_swiftui` (each preset's stiffness `(2π/0.5)²` and damping ratio `1 − bounce`, within 1e-9);
  - `no_square_without_a_waiting_update`;
  - `the_square_arrives_when_ready`;
  - `hover_shows_the_popover_with_the_version`;
  - `enter_activates_it`;
  - `its_semantics_label`;
  - goldens `update_square_{dark,light}.png` at rest and on hover.
- [ ] **Steps 2–4:** RED, implement, GREEN. Then render and look, comparing with `…-square.html` at full size in both themes (`GEMINI.md` → "What the user sees").
- [ ] **Step 5:** Commit `feat(app): the update square`.

### Task 13: App: the cover animation

**Owner:** Gemini. **Score:** 3. **Review focus:**
- every step uses the spring the design's table gives;
- the wave covers the whole window from the square's centre at any window size;
- the covered first frame (Task 15) matches the end of step 2 exactly.

**Files:** Create `app/lib/features/updates/update_cover.dart` and its tests.

**Interfaces (produced, used by Task 15):**
- **`UpdateCoverController`:**
  - `Future<void> cover({required Rect squareRect})` plays steps 1–2. It glides the square to the window's centre (18 px above the middle), scaling to 1.2 on `.snappy`. Then the wave grows from radius `31 × 1.2` to the window's far corner + 4 px on `.smooth(0.7 s)`. The ring is 8 + 20·t px ahead of the edge, its opacity `1 − 1.15·t`. The caption fades in on `.smooth` once the wave's value passes 0.85, and the arrow turns white once it passes 0.25.
  - `Future<void> uncover({required Rect squareHome, required bool updated})` plays steps 5–7.
    - **Updated:** the arrow → check on `.bouncy`, and the caption becomes "Updated to <version>". Hold ~0.95 s, then the caption fades. The wave shrinks back on `.smooth(0.6 s)`, the square glides home on `.snappy`, and the square folds on `.smooth`.
    - **Not updated** (the failed path): the download arrow stays, there's no caption, and the square glides back and stays.
  - `void showCovered()` draws the end state of step 2 at once: the full fill, the square in the middle, the white arrow and the step 2 caption.
- **`UpdateCover`** is a full-window overlay widget, placed above the desktop shell's content and title bar, painting from the controller. The wave and ring are a `CustomPainter`, and the fill is the 150° gradient.
- **Reduce motion:** `cover` fades the covered state in over 0.2 s, and `uncover` fades it out.

- [ ] **Step 1: Write the failing tests:**
  - `cover_ends_fully_covered_at_any_size` (sample the painter at the four corners at sizes 800×600 and 2560×1440);
  - `show_covered_matches_the_end_of_cover` (golden of both, identical);
  - `uncover_updated_ends_with_the_square_gone`;
  - `uncover_failed_brings_the_square_back`;
  - `reduce_motion_fades_without_moving`;
  - goldens at four moments of `cover` and `uncover`.
- [ ] **Steps 2–4:** RED, implement, GREEN. Then render and look, comparing with `…-animation.html` in slow motion.
- [ ] **Step 5:** Commit `feat(app): the update cover animation`.

---

## Stage E: The app, part 2

### Task 14: Runners: show the window on its first frame

**Owner:** Claude. **Score:** 5 (Windows and macOS native code, untestable here).

**Files:** Modify `app/windows/runner/{flutter_window.cpp,window_channel.cpp}`, `app/macos/Runner/{MainFlutterWindow.swift,…}`, and `app/lib/features/window/native_window.dart` (`physicalBounds()`, which Task 15 uses).

**What to build:**
- When `configure` is called with `hidden: false` on the covered path (a new `showOnFirstFrame: true` argument), Windows uses `flutter_controller_->engine()->SetNextFrameCallback` to show the window. macOS shows it after the first frame (`FlutterViewController`'s first-frame notification, or `engine.viewController` drawing once).
- The Linux runner already does this (`first_frame_cb`). It only needs to accept the argument.
- **`physicalBounds()`** returns `{x, y, width, height}` in physical pixels on all three systems: the visible content, without Linux's 14 px transparent frame margin.

**Checks:**
- `flutter analyze` and the app tests.
- C++ and Swift can't be built here: after this task, push, wait until no check is pending, and read the pull request's `client-windows` and `client-macos` jobs before Task 15 starts (Gemini runs). Fix any failure on this task first.
- **The OS checks owed**, as new numbered steps in `docs/plans/media-check-windows.md` and `docs/plans/media-check-macos.md` for the OS agents, plus a Linux X11 list in the pull request body:
  - every item of the design's "Checks owed" (macOS quarantine, `open -n -g`, old launcher with new app; Windows antivirus during the rename; the cover's placement on high-DPI and second monitors, and no flash at either hand-off);
  - Task 7's X11 cover window;
  - Task 15's full click → restart → reopen between two builds.

  Name them in the report as owed.

**Commit:** `feat(app): show the window on its first frame when opening covered`.

### Task 15: App: the hand-off and opening covered

**Owner:** Claude. **Score:** 4 (a hand-off between two processes; the restart must never strand the user).

**Files:** Create `app/lib/features/updates/{restart_handoff.dart,restart_note.dart}`. Modify `app/lib/app_start.dart`, `app/lib/features/window/window_startup.dart`, `app/lib/features/updates/update_square.dart` (wire `onActivate`), `app/lib/features/desktop/installation.dart`.

**What to build** (design: "The launcher holds the cover", steps 1, 3, 5 and 7, and "A failed update"):

**When the square is clicked:**
1. Record the call the user is in (`VoiceSessionNotifier`'s server key and channel) for the note, then leave voice.
2. Play `cover`.
3. Capture the window: a `RepaintBoundary` at the app's root, `toImage(pixelRatio: devicePixelRatio)`, cropped to `physicalBounds()` (on Linux, without the 14 px frame margin). Write `cover.rgba` in Task 7's format, through a `.partial` file and a rename.
4. Write `restart.json`. `restart_note.dart` mirrors `RestartNote`'s JSON field for field. Its `window` holds the physical geometry from `NativeWindow` (Task 14's `physicalBounds()`). Its `cover` is false on Wayland (`Platform.environment['WAYLAND_DISPLAY']` set and `GDK_BACKEND` not `x11`).
5. Start `launcher --after-pid <pid> --cover`, or the same without `--cover` on Wayland, through `launcher_process.dart` without waiting.
6. Wait for `cover.shown` for up to 3 s, then `exit(0)`. Don't hide to the tray.

**At start, before `startWindow`:**
- Read `restart.json`. If it is fresh, pass `background: 0xFF4D55D6` to `configure` and build the app with `UpdateCover` in `showCovered()`.
- After the first frame, write `cover.taken`.
- Play `uncover(updated: installation.version == note.to)`.
- Remove `restart.json`, `cover.rgba`, `cover.shown` and `cover.taken`.

**Tests:**
- `the_handoff_writes_its_files_in_order_and_waits_for_cover_shown` (fake launcher runner, fake clock);
- `it_exits_after_3_s_without_cover_shown`;
- `wayland_skips_the_cover`;
- `a_fresh_note_opens_covered_and_writes_cover_taken`;
- `a_note_for_another_version_takes_the_failed_path`;
- `an_old_note_is_ignored_and_removed`;
- `the_rgba_header_matches_the_launchers` (the same bytes Task 7's test reads).

**Checks:** `cd app && flutter analyze && flutter test test/unit/updates test/widget/updates` (a lone `cd`, then `flutter`). Also `the_restart_note_round_trips_the_shared_fixture`. The by-hand restart between two builds is owed (Task 14's list).

**Commit:** `feat(app): hand the cover to the launcher and open covered`.

### Task 16: App: rejoining voice after an update

**Owner:** Gemini. **Score:** 3. **Review focus:**
- it rejoins only after an update restart within 2 min;
- mute and deafen are as before (confirm where they are kept: `local_prefs.dart` writes `muted`);
- camera and screen share stay off;
- failing to rejoin shows nothing.

**Files:** Create `app/lib/features/updates/voice_rejoin.dart`. Modify `app/lib/app_start.dart`, or wherever Task 15 reads the note, to hand `note.voice` over.

**Interfaces:**
- **`rejoinAfterUpdate(Ref, VoiceNote)`** waits for that server's connection to be `connected`, for up to 30 s. Then it calls `VoiceSessionNotifier.join(serverKey, channelId)`.
- **When it can't rejoin** (any error, a timeout, or the channel gone): it logs one session-log line and shows nothing.
- **The failed path** (`updated: false`) also rejoins.

- [ ] **Step 1: Write the failing tests** (the fake repository, through the real `VoiceSessionNotifier`):
  - `it_rejoins_the_channel_after_the_server_connects`;
  - `it_gives_up_after_30_s_quietly`;
  - `a_missing_channel_shows_nothing`;
  - `mute_and_deafen_are_kept`;
  - `an_ordinary_start_never_rejoins`.
- [ ] **Steps 2–4:** RED, implement, GREEN.
- [ ] **Step 5:** Commit `feat(app): rejoin voice after an update restart`.

### Task 17: Docs

**Owner:** Gemini. **Score:** 2.

**Files:** Modify `docs/updater.md` and `docs/releasing.md`.

**What to write:**
- `docs/updater.md` "Updating" is rewritten for the new flow: the start path, the online fallbacks, `--prepare`, `--cover`, the new files, the throttle, and the square.
- "Signing" gets "Turning it on" from the design, word for word.
- "Not done" loses "Updates while running".
- `docs/releasing.md` says the key's order (workflow on all three branches first) and that macOS users need a new disk image for background updates.
- Write from the code, not from the plan (`GEMINI.md` → "Write docs from the code"). Grep every file, flag and identifier you name.

**Commit:** `docs: background updates and signed releases`.

**Then, at the end of Stage E:**
1. Run the full checks, push, and wait until no check is pending.
2. Paste `gh pr checks`.
3. Hand the Stage E review to Claude.
