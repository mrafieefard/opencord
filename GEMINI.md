# Instructions for Gemini

## Current work

You are implementing `docs/superpowers/plans/2026-10-26-mobile-client.md`: the phone apps, in React Native on the Rust core. The design it builds on is `docs/superpowers/specs/2026-10-26-mobile-client-design.md`. Milestone R0 (the facade, the app's build, its core layer and its store) is Claude's and comes first; the screens from R1 on are mostly yours. The private messages plan before it is finished.

- **UI work follows Software Mansion's React Native skills** (`~/.claude/plugins/marketplaces/swmansion/skills/react-native-best-practices`), as the plan's "Skills for UI work" says.
- The plan's **Working Rules** and this file apply to everything you do in this repository. Read "How to write code here" before each milestone: it grows after every review.
- Each milestone ends in a pull request (see "Every change is a pull request"). Its code is reviewed and fixed there, and its decisions entry gets a **Review** bullet. Read the latest one before starting, and build on the fixed code, not on what you remember writing.
- The plan gives behavior, interfaces, names and tests, not finished code. Only the proto, the SQL and a few types are given exactly: copy those as they are. You write the rest, and the rules below are what the review checks it against.
- Tasks marked **Owner: Claude** aren't yours. Hand each one to Claude and wait, as the plan's "Task owners" section says. You still run their checks and keep the ledger.
- The user releases one milestone at a time. Stop at the end of each milestone and report.

## Every change is a pull request

Nothing is committed or pushed straight to `beta`, `canary` or `main`: each push to them is a release (docs/releasing.md). This holds for every plan from now on.

- **Work on the plan's branch.** If the plan names none, ask the user which branch to use before the first commit.
- **Before pushing,** run `.github/check.sh` (with the environment, as below). Everything passes. A failure that has nothing to do with your change is still reported, never skipped or hidden.
- **At the end of each milestone,** push the branch and open a pull request into the branch the plan names:

  ```bash
  git push -u origin <branch>
  gh pr create --base <target> --title "<milestone>" --body-file <report>
  ```

  The body is your milestone report: what was built, the deviations, how it was tested, and what couldn't be tested here (Windows, macOS, hardware).
- **Read the pull request's checks.** CI runs the full workflow on it: Linux, Windows, macOS, the integration tests and the Docker image. Watch it with `gh pr checks <number> --watch`. Read each failure with `gh run view <run> --log-failed`, fix it on the same branch, run `.github/check.sh` again, then push. The three-attempts rule in "When you're stuck" applies to these failures too.
- **Never merge.** The review happens on the pull request, and the user merges.

## Every build command loads the environment

Each shell command you run starts in a fresh shell, so an `export` from an earlier command is gone. Start **every** `cargo`, `flutter`, `dart`, `sqlx` and `flutter_rust_bridge_codegen` command with:

```bash
source .superpowers/sdd/media/env.sh && <command>
```

Without it, builds keep incremental caches and debug info, and the nearly full disk fills up.

`env.sh` is git-ignored. If it's missing on this machine, create it with:

```bash
export PATH="$HOME/.cargo/bin:$HOME/.local/share/flutter/bin:$PATH"
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_RELEASE_DEBUG=0
```

**The phone apps** (`mobile/`) have their own: start every `yarn`, `npx`, `ubrn`, `adb`, `emulator` and Gradle command for them with `source mobile/scripts/env.sh &&`. It loads the line above too, and sets the Android SDK, React Native's NDK and Java (docs/mobile.md).

Check `df -h /` before the full checks. With less than 10 GB free, delete `target/debug/incremental` and the test binaries in `target/debug/deps` older than 45 minutes. Never run `cargo clean` without asking.

## How to write code here

Every rule here was broken at least once and caught in review.

### Order of work

- **Check cheapest first:** who is asking (token, permissions held in memory), then other in-memory checks, then database reads, then request bodies or files, then writes.
  - A handler never reads a body before it knows who sent it.
  - When the plan lists a handler's steps, they are already in this order. Keep it.

### Errors

- **Never turn an error into "nothing".** Propagate with `?`.
  - Never wrap a query in `if let Ok(..)`, `.ok()` or `unwrap_or_default()` when its result decides what gets deleted or skipped. A failed query then looks like an empty one.
  - Never write `let _ =` on a `Result` (database, files, network). If a failure is truly harmless, log it with `tracing` and say why in a comment.
- **In Dart, catch what you expect:** `on RepoException catch (error)`, never a bare `catch` or `catch (_) {}`.
  - An error the user can cause reaches them, as a toast or an inline error.
  - An `unawaited` future catches its own errors; nothing else will.
- **In the phone app, a promise started from a tap goes through `report(promise, 'what')`** (`@/core`), which shows its failure as a toast. `onPress={leaveVoice}` drops it.
- **A worker thread's error reaches its caller.**
  - A thread that can fail (opening a device, encoding) stores its error where the caller's next call returns it, and says so at once through the event stream.
  - `Err(_) => None` on a thread is the same swallowed error as anywhere else.
- **Know how a library reports failure.** Iterators that just end, an `Option`, a default value: where an error and "nothing more" look the same, call the lower-level function that tells them apart. A download that fails halfway is a failure, not the end of the file.
- **Production code never bends for tests.** No `try`/`catch` "for tests where X isn't set up", and no prop or branch only a test uses: fix the test's setup instead, or drive the real path (a header's search through the options the screen sets). When a test needs to stand in for something (a call, a device), put a small trait in front of it, as `TapEngine` does, and give the test a fake.

### Shared state and races

- **State changes are conditional.** Write each transition as `UPDATE … WHERE id = $1 AND state = '<expected>'` and return `rows_affected() == 1` as a `bool`.
  - `false` means the row was cancelled, sent or changed meanwhile. Handle it the way the task says; if the task doesn't say, ask.
- **A transaction holds only writes.**
  - SQLite runs in WAL mode. A transaction that has read fails at once with `SQLITE_BUSY_SNAPSHOT` when it writes after another connection has committed.
  - So read and check first, on a plain connection, then `begin()` for the writes.
  - When a read must still hold at the write, make the write conditional instead of reading inside the transaction.
- **A check holds only under the lock that guards the change.**
  - A handler that checks, then awaits (a network test, a file), then writes must check again after taking the write lock, next to the write. The early check only refuses sooner.
  - Work in flight counts. A background job records where it will write (a storage, a channel) on its row, under the write lock, before it writes. Then a check for "anything there?" sees it.
  - Don't rebuild in-memory state that another handler keeps in step with the database. Update it where the row changes.
- **A lock guard lives until the end of its statement.**
  - A guard taken in the head of a `for`, `while let`, `if let` or `match` is held for the whole body, so locking again inside deadlocks.
  - Copy what you need into a local first, then loop.
- **Std locks:**
  - Use `.lock().unwrap_or_else(PoisonError::into_inner)`, and the same for `read()` and `write()`, as `state.rs` does.
  - Never hold one across `.await`.
- **A slot several owners can take is released only by its holder.**
  - Whatever holds one thing at a time (an output slot, "now playing") gives each holder an id. Releasing checks the id, so one holder stopping never cuts off the one that took over.
  - A holder that comes back after another took the slot takes it again before using it.
  - A holder taking the slot it already has changes nothing. Taking it never stops the one asking: play after pause must not pause itself.
- **Hold what has its own lifetime weakly.** A player, a task or a cache that points at something the user can end (a call, a connection) keeps a `Weak`. A strong reference keeps the call's devices open after the user left.
- **Each event reaches a listener once.** Route it through the existing router (`_route` in `providers.dart`) or subscribe to the repository's events, never both.
- **After every `await` in a widget's `State`, check `mounted`** before touching `context`, `ref` or `setState`. The page may have closed while it waited.

### What the user sees

- **Every failure is shown, where the user is looking.** If a model has an `error` or `failed` field, some widget renders it. A step that replaces a form (a confirmation, a trust step) shows its own errors.
- **Use it on the device before calling it done.** Tests check text and calls, not what is drawn or how it feels. For every screen a task touches:
  - take its screenshots in dark and light and look at each one;
  - try every gesture on it (tap, long press, both swipes, pull) and every way in from outside the app (`adb shell am start -d '<link>'`);
  - restart Metro with `-c` after editing if the screen didn't change: a stale bundle looks like a fix that didn't work;
  - crop the screenshot at full size (`magick shot.png -crop 1080x700+0+1500 crop.png`) and measure: paddings, sizes and gaps against the plan's numbers (1 pt = 2.625 px on the Pixel 7), text that wraps early or breaks inside a word, things that overlap or touch the screen's edges;
  - with a text field: open the keyboard and type several lines. What the user was reading stays above the field.

  If you can't run the emulator, the task isn't done. Say so in the first line of your report.

  Running the phone app, each line starting with `source mobile/scripts/env.sh &&`:
  - `adb devices` shows `emulator-5554` when the `Pixel_7` emulator is up; else `cd mobile && yarn emulator`. The app is installed; after a change to `app.config.ts` or a native package, reinstall with `cd mobile && yarn android:fake`.
  - Metro on the fake world: `cd mobile && EXPO_PUBLIC_FAKE_CORE=1 npx expo start --dev-client -c`, run in the background, then `adb reverse tcp:8081 tcp:8081`. Add `EXPO_PUBLIC_FAKE_SCENARIO=empty` for no identity and no servers.
  - Open a screen: `adb shell am start -W -a android.intent.action.VIEW -d 'opencord-app:///servers' dev.opencord.mobile`. A blank screen right after Metro starts means the app didn't reach it: run `adb reverse` again and open it a second time.
  - Screenshots: `mobile/scripts/screens.sh <task> <name>=<route> …` writes both themes to `.superpowers/sdd/mobile/screens/<task>/`. Open each PNG and look at it.
  - Gestures: `adb shell input tap X Y`, `adb shell input swipe X1 Y1 X2 Y2 300` (a swipe), `adb shell input swipe X Y X Y 1200` (a long press). Screenshots are 1080 × 2400.
  - Stop Metro when you're done, by its pid (`ss -ltnp | grep 8081`).
- **A surface differs from what it sits on.** A card is `surface` on `chat`, never on a background of the same colour. A row with swipe actions behind it has an opaque background.
- **Overlays in the phone app are the app's own (D78).**
  - **Sheets:** a sheet fits its content (`sheetAllowedDetents: 'fitToContents'`) unless the plan names detents. It is `elevated` with `radius.sheet` corners and starts with `SheetHeader`. Look for blank space under the last control in its screenshot.
  - **Menus and questions:** use `Menu` and `confirm()` from `@/ui`, never `Alert.alert` or a library's popup. Android's own PopupMenu and AlertDialog ignore the theme.
  - **Colours:** a colour a system component picks for itself (Material's tinted indicator, ripple, accent) is a bug. Find its prop and give it a token.
- **Colours and glyphs in the phone app (D79 review).**
  - **On a hue:** words and glyphs on `voice` or `recording` are `onHue` (white) in both themes, never `onAccent`, which is near-black in the dark theme. `Button`'s `voice` and `danger` variants do this; use them rather than building a button.
  - **Toggles:** a toggle's glyph shows its state: `mic`/`micOff`, `listen`/`deafen`. A glyph that always shows "off" reads as off.
- **A sheet opens on what is set.** A value a sheet edits (a volume, a switch) comes from the store, and its action writes the store as well as the core. A `useState(default)` in the sheet forgets it when the sheet closes.
- **One copy of a bar per screen.** Before adding a bar or a banner to a pane, check what the screen around it already draws, on a phone and a tablet. On the server's own screen, open its channels in place (`panes.select`); `router.push` there stacks a second copy of the screen.
- **The screen's bottom edge belongs to the system.** Whatever sits at the very bottom stands on `insets.bottom`; look at the screenshot's navigation buttons.
- **A screenshot with a red dev banner over it isn't done:** dismiss the banner (its ✕) or fix what it says, then take it again.
- **What the system needs is part of the feature.** URL schemes, permissions and intent filters go in `app.config.ts` in the task that needs them, checked from outside the app.
- **Events carry the real state.** An update sent for play, pause or a setting change carries the last known position and values, never a placeholder `0` or empty value.
- **A retry starts clean and sends the same thing.**
  - It resets what belongs to one attempt: progress, errors.
  - If part of the original request is gone, it refuses rather than sending less (a message without its files).
- **A gate checks what it guards.** When an action is allowed only after a check (Save after Test), the check runs on exactly the values the action will send, and any change locks it again.
- **One gesture, one action.** A drag moves a preview and acts once, on release; nothing starts work on every pointer move.
- **A control does what it shows, or it isn't there.** A button that only flips a flag (a play button that plays nothing) is a stub: leave it out and list it under "Left undone".
- **A control works from every state it can reach:** after the end, after an error, after something else took over. Walk each one: play to the end and press play; start something else and come back.
  - Each walk is a test that does it twice (play, pause, play; play to the end, play again) and checks the effect afterwards, such as the position moving on. A button that turns to Pause proves nothing.
- **A size or a length from outside (a probe, the sender) can be 0 or wrong.**
  - Nothing found is `None`, never `0`. Send it only when it was found.
  - Layout keeps every element usable whatever it was given: a tile fills its box at the given shape, never shrinks to a speck.
- **What a file is comes from its content, not its label.** An extension or a container brand doesn't say whether a file has a picture; read its tracks.
- **Settings wait for the save bar, as in Discord.** A setting changed in a settings dialog applies only when the bar's Save changes is pressed, and Discard drops it. No page applies a change at once or has a Save button of its own:
  - client settings edit the drafts in `settings_draft.dart` (`clientSettingsDraftProvider`, read through the `shown…Provider`s);
  - a page with a draft mixes in `SaveBarChanges` (in `settings_dialog.dart`): `unsavedChanges`, `saveChanges` (throws `RepoException` when it can't), `discardChanges`, and `changesEdited()` after every edit. `unsavedChanges` must not use `ref`: keep what is saved in a field;
  - one switch or value on its own is a `PendingValue<T>` (saved, save, builder);
  - this covers switches, reordering by dragging (channels, roles), pickers and values chosen in a small dialog (the dialog's button says Done, not Save). Actions stay immediate: create, delete, invite, ban, test, detect. New server settings do this too.
- **Masked values never go back.** Fields the server answers masked (access keys, secrets) start empty in an edit form and are sent only when the user types new ones.

### Resources

- **Every in-memory map says when its entries leave.**
  - Remove the entry where it is used, and on every error path. A guard whose `Drop` removes it covers both.
  - No map may grow with the number of uploads, messages or users ever seen.
- **Feeding something that plays in real time (a speaker, the call), wait for room.**
  - Write only while the queue is under a small target (tens of milliseconds), as the audio engine does with its speaker. Never push with `let _ =` and drop what doesn't fit.
  - Work in steps the size of the output: decode a packet, make a tick, write it. Decoding a whole file up front costs memory and freezes what later changes (speed, seek) should affect.
  - A loop like this sleeps only when it has nothing to do. A fixed sleep after each piece of work makes catching up (after a seek) take seconds.
  - Settings the user changes while it plays (volume, mute) apply where the output is written, not where it is decoded ahead.
  - It keeps up in a debug build, which is what developers run. Time each step per tick in a debug build; a crate doing signal work every tick (filtering, stretching, resampling) gets `opt-level = 3` under `[profile.dev.package]` in the root `Cargo.toml`, as the others there do.
  - At a neutral setting (1× speed, no effect), skip the processor: many change the sound even then.
  - Test it with a sink drained at real time: check the total length heard, and at the neutral setting compare what comes out with the decoder's own output, sample for sample.
- **Know what a library call does to its state** before streaming through it, above all after `flush`, `finish` or `reset` (`wsola::TimeStretch` replays the input it still holds when pulled after `flush`). Test the whole stream's length end to end.
- **CPU-heavy work runs in `tokio::task::spawn_blocking`:** decoding images, audio or video; hashing, sealing or compressing large data.

### Reuse and conventions

- **Reuse what exists.** Before writing a helper, grep for one. The same ten lines in two places is a review finding.
  - One mapping (an icon per file type, a validation pass, a list of extensions) lives in one function or constant, not one copy per widget.
  - The app has its own widgets in `app/lib/ui/widgets` (`ChoiceChips`, `OcSwitch`, `OcButton`, …) and text styles in `OcText` (`OcText.mono` for code). Never use the Material widget one of them replaces.
  - Follow the wrappers the file already uses (in `rust_repository.dart`: `_call` for async core calls, `_now` for sync ones).
  - The server's attachments code already has `attachments::open_key` and `key_label`, `attachments::http::attachment_to_proto`, `to_status_proto` and `readable_attachment`, and `attachments::process::base_fields`.
- **Layers stay apart.**
  - Generated types (`app/lib/src/rust/`) stay in `core/rust`. The repository, its events and the widgets use app models from `core/model`, mapped in `core_mapping.dart`.
  - `core/` never imports `features/`: shared state goes in `core/providers`.
  - The repository's methods mirror the core's: no parameters the core ignores.
- **The two hues are for their one use each:** `voice` for the voice waveform, `recording` for the recording dot and the cancel state. Everything else is monochrome, even a selected chip.
- **No percentage size inside a parent sized by its content.** A `width: '100%'` or `maxWidth: '78%'` inside a view that shrinks to its content has nothing to resolve against, and text then wraps mid-word. Measure the container (`onLayout`) and pass numbers.
- **Weights come from the font's family** (`fonts.semiBold`), never `fontWeight` on Inter: Android then picks a wrong file. The type scale's variants already carry theirs. `caption` is uppercase: never for sentences.
- **Selectable text takes the long press.** Where a long press opens a menu, the text in it isn't selectable; the menu offers Copy.
- **Chat screens use keyboard-controller's chat pieces:** the list's scroll view is `KeyboardChatScrollView` and the composer sits in a `KeyboardStickyView`, both offset by the bottom inset (see `features/chat`).
- **A style passed to a text variant replaces its fields.** To change a variant's size, pass `fontSize` and `lineHeight`; another variant's style brings its font too.
- **Numbers come from the plan's table.** A step, limit or duration the plan's Numbers table gives is used as given, never a guess.
- **Check both themes.** Theme colours (`colors.text`, `colors.scrim`) flip with the theme. Anything drawn on a photo or a video uses the fixed dark set, `OcColors.dark`, as the image grid's overlay does.
- **Finishing a feature removes what stood in for it.** Grep for its "coming later" markers, placeholder toasts and `later:` flags, and remove them in the same task.
- **A value kept on both sides changes on both sides.** A permission set, a default, a limit or a list that exists in Rust and in Dart is updated in both, in the same task.
- **A rule ported from another language matches its meaning, not its look.**
  - Rust's `chars()` counts Unicode scalar values; Dart's `length` counts UTF-16 units, so use `runes`.
  - Rust's `is_control()` includes U+0080–U+009F.
  - Add each such edge case to the shared fixture so both sides test it.
- **HTTP replies** are axum tuples (`(StatusCode, headers, body).into_response()`), never `Response::builder()…unwrap()`.
- **New dependencies:** check the licence of the crate and of everything it pulls in, on every system, not just Linux:

  ```bash
  cargo tree -p <crate> -e normal --target all --format '{p} {l}' | grep -v 'MIT\|Apache\|BSD\|ISC\|Zlib'
  ```

  Only the plan's licence list is allowed. Any line this prints that isn't LGPL goes to the user before you keep the crate.

### Tests and task boundaries

- **Test against what ships.** When the app carries its own build of a library (FFmpeg in `target/ffmpeg`), run the tests that use it with that build, as CI does: prefix the command with `PKG_CONFIG_PATH=$PWD/target/ffmpeg/lib/pkgconfig LD_LIBRARY_PATH=$PWD/target/ffmpeg/lib`. The system's copy has everything; a test that passes on it says nothing about the app.
- **A test must be able to fail.** Before trusting one, name a wrong implementation that would still pass it. An assertion on a value the code echoes back (a seek reporting its own target) proves nothing; check an effect only the real behavior produces, such as how much sound came out and how long it took.
- **A test sets up everything it depends on.** Never rely on a default (an empty list, a setting's starting value) or on another test's data. Set it in the test.
- **Test every mode of a flow, not just the first.** A form that adds and edits gets a test for editing; a send that can fail and retry gets a test for the retry.
- **Fakes answer like the real thing.** A mock repository masks what the server masks and sends the last event the core sends (on a cancel, too). It ends like the real thing too: once the core's worker is gone (a player that played to the end), the fake ignores what that worker would have answered. A fake speaker plays silence when it runs dry and never banks the time for later. Otherwise tests pass on behavior real users never get.
- **Keep what you didn't add.** Editing an existing file, change only what the task needs. An attribution table, a licence note or another milestone's section stays, even when the task "rewrites" the file.
- **Every commit passes the checks on its own.**
  - Code that only a later task uses goes in with that task. Otherwise `-D warnings` fails on dead code.
  - Don't add `#[allow(dead_code)]` to get past it.

## Write docs from the code, not from memory

Every milestone ends with a decisions entry in `docs/decisions.md`.

- **When the plan gives the entry's text:**
  - copy it exactly and fill in the commit hashes;
  - check every sentence against the code. If one doesn't match what you built, fix the code or the sentence, and say which in your report.
- **When the plan doesn't give it (from A5 on):** write it from `git diff <first commit>^..HEAD`, in the shape of the previous entry:
  - **Why**, with the commits;
  - one bullet per part, saying what it does;
  - **Tests**.
- **Describe the code, not the plan.** For each sentence, open the code it describes, and copy labels, counts and messages from there. The plan says what was asked for; only the code says what was built.
- **Numbers and behavior count too:** a wrong quality setting, a wrong count or a wrong "X doesn't happen" is as bad as a wrong name. Checksums, versions and hashes are copied from the file, never typed from memory.
- **Name only things that exist.** Copy every message, request, event, function and file name, and every number, from the code or the diff. This covers the decisions entry, the progress ledger and your report.
- **Search before you finish.** Grep for each name you wrote (`grep -rn`). If a name isn't found, it doesn't go in.

## Reports and the ledger

- **A deviation** is anything you did differently from the plan, or added without the plan asking for it:
  - a behavior, user-visible text, interface or dependency that differs from the plan's, or that the plan doesn't ask for;
  - where the plan gives code, any non-test line that isn't in it, even one that only makes a test pass.

  Private helpers you needed to build what the plan describes are not deviations.

  Name each one in the ledger and in the report, with the failure that made you add it. If `git diff` shows one, "Deviations: none" is wrong.
- **A required step you skipped leads the report** (screenshots, a check, a by-hand step), with why. The ledger alone doesn't count.
- **Every problem names its test and pastes its error line.** A claim without them is left out.
- **Re-run a flaky test alone,** one test per command, and paste the result:

  ```bash
  cargo test -p <crate> --lib -- --exact <module::path::test_name>
  ```

## When you're stuck: ask Claude

If the same step fails **three times in a row**, stop retrying and ask Claude. "Fails" means the same test, build, lint or check still fails, or fails in a new way, after three different fix attempts. Claude is a stronger model that knows this codebase and wrote the plan.

1. **Write the question** to `.superpowers/sdd/private/questions/NN-short-name.md`. `NN` counts up: `01`, `02`, … Use this template and fill in every part:

   ```
   I'm Gemini, implementing docs/superpowers/plans/2026-10-22-private-messages.md in this repository. Don't edit any files: tell me the cause and the exact fix.

   Task and step: Task N, Step M: <title>
   What the plan asks: <quote the relevant lines>
   Command that fails: <the exact command>
   Error output (the last 80 lines or so):
   <paste>
   What I tried, and what happened each time:
   1. <attempt>: <result>
   2. <attempt>: <result>
   3. <attempt>: <result>
   Files involved: <paths>
   ```

2. **Ask** from the repository root. It can take a few minutes:

   ```bash
   claude -p "$(cat .superpowers/sdd/private/questions/NN-short-name.md)" --model opus --disallowedTools Edit Write NotebookEdit Bash
   ```

3. **Apply the answer.**
   - Add Claude's answer to the same file under a `## Answer` heading.
   - Make the fix yourself.
   - Run the failing command again.

4. **Record it** in that task's progress ledger entry: `Asked Claude: questions/NN-short-name.md`, plus one line saying what the fix was.

### Limits

- **At most two questions per problem.** If Claude's fix doesn't work either, stop and ask the user.
- **Claude does not replace the user.** These still go to the user, not Claude:
  - the plan and the design disagree;
  - a step can't be done as written;
  - a tool is missing;
  - anything about scope, product choices, merging, pushing anywhere but your own branch, or deleting work.
- **Claude only advises.** In this mode it has no edit or shell tools. You make every change, and the change is yours.
