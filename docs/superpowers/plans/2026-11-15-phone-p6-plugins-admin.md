# Phone P6: Plugins, Managed — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Someone with Manage plugins installs, reviews, configures, starts, stops, restarts and uninstalls a server's plugins from the phone, reads a plugin's log, and creates plugin channels.

**Architecture:**
- **The desktop's Plugins page** (`app/lib/features/settings/server/plugins/`) becomes the phone's `Plugins` row in the server sheet: a list, a plugin's page, the install review, the config fields, the log.
- **The facade gains the plugin management calls and events** it drops today (`PluginUpdate`, `PluginRemove`, `PluginReviewUpdate`, `PluginReviewRemove`, `PluginLogAppend`; `crates/opencord-mobile/src/events.rs:540-569`).
- **A package goes from Files to the server** through `upload_plugin_package(bytes)`. The phone reads the picked file in Rust from its path (a new facade call taking a path), so 50 MB never crosses the JavaScript bridge as bytes.

**Tech Stack:** as P0. No new dependencies (`expo-document-picker` is in).

**Spec:** design §3 (P6). Plugins design `docs/superpowers/specs/2026-11-07-plugins-design.md` (the install review, permissions, config, signatures) and `docs/plugins.md`. The desktop's words: `plugins_page.dart`, `plugin_page.dart`, `review_dialog.dart`, `plugin_config_fields.dart`.

## Global Constraints

- **Branch:** `feat/mobile-features`, the one branch and **one pull request into `beta`** for every phone plan (the owner, 2026-11-15). It already holds the design and the plans. Work on it in order, P0 first; never on a branch of its own. Never merge; never push to `beta`, `canary` or `main`. The pull request is open already (P0 opened it): push to it, add this plan's section to its description, and read its checks at each stage end.
- **Scope:** `crates/opencord-mobile/`, `mobile/src/`, `docs/`. No core, server, protocol or desktop change.
- **Not in P6:** dev tokens (desktop only, design §3).
- **The package limit** is the server's `max_package_mb` (default 50, `crates/opencord-server/src/config.rs:125`); the phone refuses a bigger file before reading it, with `This package is over the server's limit of <N> MB.` and the limit from the server's refusal when it isn't known.
- **Never log** config values: a `Secret` field's value is never logged, shown after saving, or kept in JavaScript state longer than its text field (it's shown as `••••••` with `Replace`).
- P0's Global Constraints from "Ids are decimal strings" on apply.
- **Numbers follow the facade's rule** (`crates/opencord-mobile/src/types.rs:1-3`): ids are decimal strings; times (ms since the epoch), sizes, durations, positions and counters cross as `f64` through `convert::number` and `convert::unsigned`; a draft id comes back through `convert::draft_id`. No `u64` or `i64` field in a record JavaScript reads, whatever the core's type: a `u64` becomes a JavaScript `bigint` and breaks every comparison.

## Working Rules

P0's apply word for word. **Ledger:** `.superpowers/sdd/phone-p6/progress.md`.

Scores (the owner, 2026-11-15): **0** no code; **1–3 Gemini**; **4–5 Claude**, handed over with claude-bridge `delegate_task`, each a cold session, so the task's section is self-contained. The rubric is P0's "Task owners". A Claude task runs first in its stage where its dependencies allow (memory: "my tasks come first"); the order is under the table.

| Task | Score | Owner | Why |
|---|---|---|---|
| 1 Facade: plugin calls and events | 4 | Claude | Reads packages up to 50 MB in Rust beside the bridge, and carries secret config values; five dropped events join the queue |
| 2 The JavaScript core and the fake | 2 | Gemini | A fake following the pattern |
| 3 The plugins list and installing | 4 | Claude | Security: the install consent for code that runs on the server (unsigned and mismatched warnings, the licence, discarding an abandoned review) |
| 4 A plugin's page: overview, permissions, running | 2 | Gemini | A page of rows over Task 1's calls |
| 5 Config fields | 3 | Gemini | Eight field kinds and secret handling, all testable. Focus: a secret never shown, logged or resent |
| 6 The log | 2 | Gemini | A capped list that follows the bottom |
| 7 Plugin channels | 1 | Gemini | One more kind in P4's create sheet |
| 8 Live checks, docs, D-entry, PR | 2 | Gemini | Running the checks and writing from `git diff` |

**Order:** Stage A: **1**, then 2. Stage B: **3**, then 4, 5, 6, 7.

Stages: A = 1–2, B = 3–7 with a review, C = 8. Commands as P0.

## Review Focus

1. **An unsigned or mismatched package.** Expected: the review shows `Unsigned: anyone could have changed this plugin. Install it only if you trust where it came from.` or `GitHub publishes a different key` before `Install`, as the desktop; `Install` needs `I accept the licence` when the package has one. Pinned in Task 3: `an unsigned package warns before install`.
2. **The review sheet is closed without installing** (back, swipe down, the app killed). Expected: the review is discarded on the server (`discard_plugin_review`), as the desktop's close does; one left by a killed app shows in the list as `Review` / `Retry`. Pinned in Task 3: `closing the review discards it`.
3. **A secret config field.** Expected: never shown, logged or stored; saving other fields doesn't resend it. Pinned in Task 5: `a secret isn't resent when other fields save`.
4. **A plugin crashes while its page is open.** Expected: `Plugin has crashed` with `Start again`, from the `PluginUpdate` event. Pinned in Task 4: `a crash shows start again`.
5. **The log while the plugin writes fast.** Expected: at most 1,000 lines kept, newest at the bottom, the list following only while it's at the bottom. Pinned in Task 6: `the log keeps 1000 lines`.

---

### Task 1: The facade

**Files:** create `crates/opencord-mobile/src/api/plugins_admin.rs`; modify `src/types.rs`, `src/convert.rs`, `src/events.rs`.

**Interfaces (produces):** records for `Plugin`, `PluginReview`, `PluginConfigField`, `PluginConfigValue`, `PluginLogLine` with the core's fields (`crates/opencord-core/src/api/types.rs`, near `PluginState`, `PluginSignature`, `GithubCheck`, `PluginConfigKind`); calls `fetch_plugins`, `fetch_plugin_reviews`, `upload_plugin_package_file(server_key, path)` (reads the file in Rust, refuses over `max_package_mb` if the facade knows it, calls `upload_plugin_package`), `confirm_plugin_review(server_key, review_id, granted: Vec<String>, config: Vec<PluginConfigValueRecord>, accept_license: bool)`, `discard_plugin_review(server_key, review_id)`, `set_plugin_permissions`, `set_plugin_config`, `set_plugin_running`, `restart_plugin`, `uninstall_plugin(server_key, plugin_id, delete_data)`, `fetch_plugin_log(server_key, plugin_id, before: Option<f64>, limit)` (`before` back to `u64` through a check like `convert::draft_id`'s), `watch_plugin_log(server_key, plugin_id, watch: bool)`, `create_plugin_channel(server_key, plugin_id, type_key, name, topic, parent_id)`. Events `PluginUpserted`, `PluginRemoved`, `PluginReviewUpserted`, `PluginReviewRemoved`, `PluginLogAppended`, leaving the dropped list.
- [ ] Tests: `a_package_is_read_from_its_path`, `every_plugin_enum_round_trips`, `plugin_events_reach_javascript`. Run (fail), implement, run (pass); clippy; bindings; commit `feat(mobile): plugin management in the facade`.

### Task 2: The JavaScript core and the fake

The fake has two installed plugins (one running, signed and verified, with every config kind; one crashed, unsigned) and accepts a package path as a pending review. Test `the fake's review installs`. Commit `feat(mobile): the core API and the fake for plugin management`.

### Task 3: The plugins list and installing

**Files:** create `mobile/src/features/pluginsAdmin/PluginsPage.tsx`, `ReviewSheet.tsx`; routes under `app/server/[serverKey]/settings/plugins/`; register `plugins` in `serverRows.ts`.

- The list: `Installed plugins` (icon, name, version, state); pending reviews with `Review` and `Retry`; empty `No plugins yet.`; `Choose a file` in the header (`expo-document-picker`, any type, then the size check, then `upload_plugin_package_file`).
- The review sheet, the desktop's words (`review_dialog.dart`): `Inspecting plugin package...`; title `Install <name>` or `Update <name>`; `This replaces <name> <version> by <author>` on an update; the signature (`Verified: github.com/<repo>`, `Signed`, `Unsigned: …`, `GitHub publishes a different key`); `Permissions`, `Domains`, `Configuration` sections; `I accept the licence`; `Install` / `Update` and `Cancel`.
- [ ] Tests: `an unsigned package warns before install`, `closing the review discards it`, `a big file is refused before reading`. Implement; screenshots; commit `feat(mobile): install plugins from the phone`.

### Task 4: A plugin's page

**Files:** create `features/pluginsAdmin/PluginPage.tsx`.

- Sections as the desktop's tabs, as one scrolling page: `Overview` (`Details`: version, author, `GitHub`, `Licence`, `Key fingerprint`, `Installed`, `Updated`, `Storage usage`), `Permissions` (a switch per requested permission: `set_plugin_permissions` with the granted list), `Settings` (Task 5), then a `Running` switch, `Restart` (toast `Plugin restarted`), `Log` (Task 6) and `Uninstall` (confirm `Uninstall <name>?`, `Are you sure you want to uninstall <name>?`, a switch `Also delete its data and files`, button `Uninstall`). A crashed plugin shows `Plugin has crashed` and `Start again` at the top. A removed plugin shows `Plugin not found.` and `Back to plugins`.
- [ ] Tests: `a crash shows start again`, `uninstall sends delete data when switched`, `each permission switch sends the granted list`. Implement; screenshots; commit `feat(mobile): a plugin's page`.

### Task 5: Config fields

**Files:** create `features/pluginsAdmin/ConfigFields.tsx`; test with every `PluginConfigKind`.

- One row kind per field kind, the desktop's placeholders and errors (`plugin_config_fields.dart`): `Text`, `Secret` (`Required secret`, shown as dots with `Replace`), `Number` (`Must be a valid number`), `Boolean`, `Choice`, `Role` (`Select role...`), `Channel` (`Select channel...`), `User` (`Select user...`). `Required` fields block `Save` until filled. Saving sends only changed values.
- [ ] Tests: `a secret isn't resent when other fields save`, `required fields block save`, `each kind renders and saves`. Implement; screenshots; commit `feat(mobile): plugin settings`.

### Task 6: The log

**Files:** create `features/pluginsAdmin/PluginLog.tsx`.

- Mono lines with the level as a coloured tag, loaded with `fetch_plugin_log` (100, older on scroll up), new lines from `PluginLogAppended` while `watch_plugin_log` is on (on while the page is open, off on leave and on background). At most 1,000 lines; follows only at the bottom; share the shown lines as text.
- [ ] Tests: `the log keeps 1000 lines`, `watching stops on leave`. Implement; screenshots; commit `feat(mobile): a plugin's log`.

### Task 7: Plugin channels

**Files:** modify P4's channel create sheet: the kind list gains each plugin channel type (`PluginChannelTypeRecord`, with the plugin's icon); choosing one calls `create_plugin_channel`.
- [ ] Test `a plugin channel type creates through the plugin`; implement; screenshots; commit `feat(mobile): create plugin channels`. Stage B's end and review.

### Task 8: Live checks, docs, D-entry and the pull request

- [ ] Against a local server: install plugins from `examples/plugins/` from a file copied to the emulator (`adb push`), configure one, crash and restart it, read its log, create its channel, uninstall it with its data; ledger each.
- [ ] `docs/mobile.md` and `docs/plugins.md` (the phone can manage plugins); the iPhone hand checks (picking a package from Files).
- [ ] The next D-number. From `git diff`.
- [ ] `.github/check.sh`; push; PR description; `gh pr checks` green; review round; report.
