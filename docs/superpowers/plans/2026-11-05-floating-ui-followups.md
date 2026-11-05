# After the floating UI: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The nine changes the owner asked for once the floating UI was built: a draggable right pane, direct music volume, no bubble mode (compact only), no Invisible status, mouse keybinds, working code fences with highlighting, a noise suppression popover with a voice test, that test in Settings, and the ping on the signal bars.

**Architecture:**
- Nearly all of it is in `app/lib/features` and `app/lib/ui`.
- Two tasks reach the Rust core, because the app can't do them alone: mouse buttons in the system hotkey backends (`crates/opencord-media/src/hotkeys`), and hearing yourself while deafened (`crates/opencord-media/src/audio`).
- The microphone test already exists in the core (`set_mic_test`, `Repository.setMicTest`) and no screen uses it. This plan gives it a screen and makes it work in a deafened call.

**Tech Stack:** Flutter 3.44, Riverpod 3, flutter_rust_bridge, `x11rb`, the `windows` crate, `objc2_core_graphics`, and one new package, `re_highlight` (MIT).

**Spec:** `docs/superpowers/specs/2026-11-05-floating-ui-followups-design.md`. Read all of it before Task 1, and the sections a task names before that task. "§" below means this spec. **Every number and string is in the spec, and the plan doesn't repeat them:** a number you can't find there is a question, not a guess.

## Global Constraints

- **Branch and pull request:** `feat/floating-ui-followups`, made in "Before Task 1". One draft pull request into `beta` for the whole plan, kept a draft until the last checkpoint passes. Never merge.
- **Scope:** `app/`, docs, `crates/opencord-media`, `crates/opencord-core`, and the two phone files in Task 7. Nothing in `proto/` or `crates/opencord-server`. One new dependency: `re_highlight`.
- **Platforms:** desktop. Linux runs here. Windows and macOS compile and run their tests only in CI, with hand checks listed in §11.
- **Colours:** `OcColors` tokens only, monochrome (`test/widget/monochrome_test.dart`), code highlighting included.
- **Words:** every user-visible string in the spec is copied exactly.
- **Motion:** only through `OcSpring`, `SpringValue`, `FlipBox` and `OcMotion`. Reduced motion takes every animation to zero.

## Working Rules (read before Task 1)

`GEMINI.md` applies to everything here. Read "How to write code here", "Tests and task boundaries" and "Reports and the ledger" before each milestone.

### Commands

Start every `flutter`, `dart`, `cargo` and `flutter_rust_bridge_codegen` command with `source .superpowers/sdd/media/env.sh &&`. Phone commands start with `source mobile/scripts/env.sh &&`.

| What | Command, from the repository root |
|---|---|
| One test file | `source .superpowers/sdd/media/env.sh && cd app && flutter test test/widget/shell/splitter_test.dart` |
| One test | add `--plain-name 'the_test_name'` |
| Analyze and format | `source .superpowers/sdd/media/env.sh && cd app && flutter analyze && dart format --set-exit-if-changed lib test` |
| One Rust crate's tests | `source .superpowers/sdd/media/env.sh && cargo test --locked -p opencord-media --lib hotkeys` |
| Regenerate the bridge | `source .superpowers/sdd/media/env.sh && cd app && flutter_rust_bridge_codegen generate` |
| Phone tests | `source mobile/scripts/env.sh && cd mobile && yarn -s test --ci src/features/chat src/state` |
| Before each push | `df -h /`, then `.github/check.sh client`. Below 10 GB free, follow `GEMINI.md`'s clean-up. |
| Render the screens | `source .superpowers/sdd/media/env.sh && cd app && flutter test --dart-define=OPENCORD_REVIEW=/home/amin/dev/opencord/.superpowers/sdd/followups/screens/<name> --dart-define=OPENCORD_EMOJI=/usr/share/fonts/apple-color-emoji/apple-color-emoji.ttf test/showcase/floating_review_test.dart` |
| Only some | add `--dart-define=OPENCORD_REVIEW_SCENES=text,music` and/or `--dart-define=OPENCORD_REVIEW_WIDTHS=wide-1280,medium-980` |
| Compare with the prototype | `python3 -I .superpowers/sdd/floating-ui/tools/compare.py .superpowers/sdd/followups/screens/<name> --sheets` |
| A crop at full size | `magick <shot>.png -crop 700x400+740+0 crop.png` (shots are 2×) |

`.github/check.sh client` can fail on timing tests in `opencord-media` and `opencord-voice` when this machine is busy. Report any such failure with its test names and the load average, and re-run those tests alone. Only Tasks 5 and 9 may change `crates/`.

### Task owners

Every task has an **Owner** and a **Score** (1–5, how much a mistake costs and how hard it is to see: native code and the audio path score 4–5).

- **Owner: Gemini.** You implement it. Score 3 means read its **Review focus** line twice: that's what the review checks hardest.
- **Owner: Claude.** Don't implement it. Hand the task to Claude, naming the task number and this plan's path, and wait for its report. Don't edit files or run git while it works: you share the working copy.
  - Then read the report, run the task's **Checks** yourself, and record the task in the ledger with the report's commits.
  - Claude's tasks come first in their milestone, so yours build on them.
- **Claude, when handed a task:** follow the task exactly. Run its checks, finish with the commit it gives, then report the files, commits, checks and anything left undone.
  - A handed-over run may use `flutter`, `dart`, `cargo`, `cd`, `ls`, `source` and plain `git status`, `diff`, `log`, `show`, `add` and `commit`, and it reads images with its Read tool. It has no `python3`, `magick`, scripts, or variables set in front of a command. Anything that needs those is in **Checks for Gemini** on that task.

| Task | Part | Score | Owner |
|---|---|---|---|
| 1 | Right pane splitter | 4 | Claude |
| 2 | Music volume on the panel | 2 | Gemini |
| 3 | Bubble mode removed, skeleton kept | 2 | Gemini |
| 4 | Invisible removed | 1 | Gemini |
| 5 | Mouse buttons in the core and the system backends | 5 | Claude |
| 6 | Recording mouse keybinds, the app's own handling | 3 | Gemini |
| 7 | Fence scanner, composers, phone parser | 3 | Gemini |
| 8 | Language highlighting in grey | 3 | Gemini |
| 9 | The test: hearing yourself while deafened | 5 | Claude |
| 10 | Noise chip, popover, `MicTestPanel` | 3 | Gemini |
| 11 | The test in Voice settings | 2 | Gemini |
| 12 | Ping on the signal bars | 1 | Gemini |
| 13 | Docs, showcase, hand-check list | 1 | Gemini |
| CP1–CP4 | Each checkpoint's review | 4 | Claude |

### Rules reviews will enforce

1. **Passing tests don't finish a UI task.** For every screen a task touches: render it, open it at full size, crop what's small, and sample colours (the screens are at 2×). `compare.py` only checks rectangles. **If you can't open images, that's the first line of your report, and the task isn't done.**
2. **Measure, don't assert presence.** A UI test names a pixel result: a rect, a colour from a decoration, a font size from a style. "Finds the text" is not a look test.
3. **Test in the shell, not only the widget alone.** Removing or moving something can break what reached it. Grep for keys, ancestors, listeners and providers first.
4. **Reuse the named widgets:** `Hoverable`, `OcIconButton`, `OcButton`, `OcSwitch`, `OcTextField`, `SettingsSection`, `SettingsRow`, `showPopover`, `showOcMenu`, `InputLevelMeter`. No Material widget one of them replaces, and no `InkWell`.
5. **Every control twice, checking the effect:** open, close, open again; start, stop, start again.
6. **Change the finder, not the test's meaning.** Never delete an assertion because it fails. If a test's meaning has gone with a removed thing, name it under Deviations. `app/integration_test/` runs only in CI, so grep it too.
7. **Goldens:** after a failure, open `test/golden/failures/*` and look. Update only what this task meant to change.
8. **Don't change the mock world for looks.** `core/mock/` stays. A test builds what it needs itself.
9. **`mounted` after every `await`; `on RepoException catch`; every failure shown.**
10. **Finishing a part removes what it replaces,** in the same task.
11. **A number in a report comes from output you saw,** and the pull request's description comes from the diff.

### Progress ledger

Keep `.superpowers/sdd/followups/progress.md` (git-ignored: never `git add -f`). After each task, append:

```
## Task N — <title> (owner: <Gemini|Claude>, score: N)
- Commit: <hash>
- Tests added: <test names>
- Commands run: <command> → <result copied from the output>
- Screens looked at: <paths>
- Deviations from the plan: <none | what and why, including every fix commit>
- Left undone / doubts: <none | list>
```

### When a step keeps failing

Follow `GEMINI.md` → "When you're stuck: ask Claude", with question files in `.superpowers/sdd/followups/questions/`.

## UI checkpoints and scoring

Each milestone ends at a checkpoint. **Work stops there until Claude's score passes, then the next milestone starts without waiting for the owner**, the flow the floating UI ended on. Keep going until CP4 passes.

The new parts have no prototype: **the spec is their reference.** The parts that didn't change still have one, so `compare.py` must print no line for `rail`, `left`, `head`, `me`, `main` or `composer` at any checkpoint.

**The breakpoints:** `wide-1440`, `wide-1280`, `wide-1180`, `medium-1100`, `medium-980`, `narrow-940`, `compact-759`, as in `test/support/floating_scenes.dart`. Dark at all of them, light at `wide-1280` and `medium-980`. A part that doesn't exist at a width is scored `-`.

| Score | Means |
|---|---|
| 5 | The spec exactly: frames within 1 px, text styles, colours and states right, motion on its curve |
| 4 | Within 2 px, or one detail only visible at full-size crop |
| 3 | A difference you can see |
| 2 | Wrong at this width: overlap, clipping, wrong state |
| 1 | Broken |
| 0 | Missing |

**To pass:** every cell ≥ 4, every motion row 5, every behaviour row 5.

**How each checkpoint runs:**

1. **Gemini:**
   - render the checkpoint's scenes to `.superpowers/sdd/followups/screens/cpN`;
   - run `compare.py` and fix what it lists;
   - open the renders at full size and sample colours;
   - fill in `.superpowers/sdd/followups/scorecard.md` (git-ignored): your own scores, one row per part and one column per breakpoint, with the crop or test that shows each. A 5 means you measured it. If you only looked, say so;
   - run `.github/check.sh client`, push, read the pull request's checks, fix failures;
   - **hand the review to Claude and wait,** as below.
2. **Claude:** re-renders them, looks at the crops at full size, walks every gesture and writes its own scores. **Claude's score is the one that counts.** It fixes small things itself and sends the rest back.
3. **Gemini** acts on the verdict, as below.

**Handing the review over.** Treat it like an Owner: Claude task, with exactly this text, filling in the checkpoint and the round (1 the first time):

```text
Review checkpoint CP<n>, round <r>, of docs/superpowers/plans/2026-11-05-floating-ui-followups.md. Follow "Claude, when handed a checkpoint" in its UI checkpoints section. Commit under review: <hash>.
```

From round 2 on, add what you changed since the last round: each finding by name, with its commit. Don't edit files or run git until the report comes.

**Claude, when handed a checkpoint:** do step 2 for that checkpoint and nothing else.

- Gemini's renders, crops and `compare.py` output are your input, because a handed-over run can't run `compare.py`, `magick` or `.github/check.sh`. Re-render with `flutter test --dart-define`. A part you can't verify with what you have scores 3, with the crop you need named as a finding.
- Write your scores in `scorecard.md` (git-ignored: never force-add it).
- Fix small things and anything in a task you own, and commit them. Don't push.
- End the report with one line, alone: `CP<n> round <r>: PASS` or `CP<n> round <r>: FAIL`. After FAIL, list each finding with its task, part, breakpoint, score and what's wrong. Say so if the owner has to decide something.

**Acting on the verdict:**

- **PASS:**
  1. If Claude committed anything, run `.github/check.sh client`, push and read the pull request's checks. Fix failures; a job that fails on the runner rather than on the code may be re-run once.
  2. Record the verdict in the ledger.
  3. Start the next milestone straight away.
- **FAIL:** fix every finding, redo step 1, and hand it over as the next round.
- **No verdict line:** that isn't a pass. Hand it over as the next round, saying what the report did finish.
- **After CP4's PASS:** mark the pull request ready, report to the owner and stop. The owner merges.

**Stop and ask the owner only when:**

- a checkpoint is still FAIL after round 3;
- Claude says the owner has to decide something, or the plan doesn't match the spec;
- `re_highlight` doesn't build (Task 8);
- the tool you hand work to Claude with tells you to stop: follow its message.

**What each checkpoint scores:**

| Checkpoint | Parts | Motion and behaviour rows | Scenes |
|---|---|---|---|
| **CP1**, after Task 4 | Right splitter and pill, each pane at a dragged width, music volume row, compact lines after bubble, skeleton, Appearance page, the presence choices | drag live; double-click reset; People → Music; open and close after a drag; window shrinking and returning; dragging while muted | `text`, `music`, `pane-resized`, `settings-appearance` |
| **CP2**, after Task 6 | Keybinds page with mouse rows, recorder, footer | recording each button; left and right click; Mouse 4 bound against stepping back. Native code: read against §6, and `Client (Linux)`, `Client (Windows)` and `Client (macOS)` must be green | `settings-keybinds` |
| **CP3**, after Task 8 | Code block in each format, grey tokens, plain fallback | Enter inside and after a fence; Copy; a huge block | `code-block` |
| **CP4**, after Task 13 | Voice card with chip, popover in each mode, `MicTestPanel`, Voice settings, ping tooltip. **Then every scene of CP1–CP3 again, and the floating UI's own scenes at every breakpoint,** to catch what the plan broke | each way a test ends; restored state after each; nothing sent while deafened; tooltip updating | `voice-card`, `noise-popover`, `noise-popover-testing`, `settings-voice`, then all |

## Review Focus

The inputs most likely to break for real people that no ordinary test would catch. Each has a pinned test in the task named.

1. **Left deafened after a test:** closing the popover mid-test, leaving the call, or undeafening by hotkey. Expected: the test ends, and the person is back as they were. Task 9: `undeafening_ends_it`, `leaving_voice_ends_it`. Task 10: `closing_the_popover_ends_the_test`.
2. **Others hearing the test:** expected nothing at all, no frames and no talking indicator, even with push to talk treated as held. Task 9: `a_test_in_a_deafened_call_sends_nothing`.
3. **A mouse keybind breaking ordinary use:** Mouse 4 bound as push to talk also stepping the app back, or a click during recording starting another recording. Expected: one action per press, and left click behaving as it always has. Task 6: `mouse_4_bound_no_longer_goes_back`, `left_click_on_the_recorder_stops_it_once`.
4. **Dragging the pane, then quick clicks or a smaller window.** Expected: no jump, ends at exact widths, main never below its minimum. Task 1: `dragging_then_closing_ends_exact`, `window_shrinking_gives_way_and_returns`.
5. **A huge paste in a fence, or a fence never closed.** Expected: no stutter while scrolling, and Enter never lost. Task 8: `a_huge_block_stays_plain`. Task 7: `enter_inside_an_open_fence_adds_a_line`.

## File Map

**New**

| File | Holds |
|---|---|
| `app/lib/core/fences.dart` | `Fence`, `findFences`, `endsInsideFence` |
| `app/lib/features/chat/code_highlight.dart` | `highlightedCode` |
| `app/lib/features/voice/mic_test.dart` | `micTestProvider` |
| `app/lib/features/voice/mic_test_panel.dart` | `MicTestPanel` |
| `app/lib/features/voice/noise_chip.dart` | `NoiseChip`, the popover |
| `app/lib/ui/widgets/oc_segmented.dart` | `OcSegmented`, from `PaneSwitch` |

**Changed most:** `desktop_shell.dart`, `shell_splitter.dart`, `side_slot.dart`, `spring_value.dart`, `music_controls.dart`, `music_panel.dart`, `chat_row_view.dart`, `message_item.dart`, `message_list.dart`, `private_message_list.dart`, `appearance_page.dart`, `app_settings.dart`, `repository.dart`, `core/hotkeys.dart`, `keybind_recorder.dart`, `keybinds_page.dart`, `providers.dart`, `markdown.dart`, `markdown_view.dart`, `format.dart`, `composer.dart`, `private_composer.dart`, `voice_panel.dart`, `app_pages.dart`, and in Rust `hotkeys/{mod,x11,windows,macos,portal,linux}.rs`, `audio/{processor,capture,playback,engine}.rs`, `api/types.rs`, `media.rs`.

**Removed:** `app/lib/features/chat/message_line.dart`, `app/test/golden/goldens/bubble_dark.png`, `bubble_light.png`, `MusicVolumeButton`.

---

## Before Task 1

Nothing here starts until the owner says so. Then, in order. If a check fails, stop and tell the owner.

- [ ] **Clean tree:** `git status --short` lists only this plan and the spec, both untracked. From here on, nothing else may be working in this copy of the repository.
- [ ] **The floating UI is finished:** `gh pr view 41 --json state --jq .state`.
  - If it prints `MERGED`, `git fetch origin && git switch -c feat/floating-ui-followups origin/beta`.
  - Otherwise `git log --oneline origin/feat/floating-ui..feat/floating-ui` must print nothing, or stop and tell the owner: this plan would be built on commits nobody has reviewed. Then `git switch -c feat/floating-ui-followups feat/floating-ui`.
- [ ] **Disk:** `df -h /`. Below 10 GB, follow `GEMINI.md`'s clean-up now.
- [ ] **The review tools still work:** `ls .superpowers/sdd/floating-ui/reference | wc -l` prints 115. Then `mkdir -p .superpowers/sdd/followups/screens .superpowers/sdd/followups/questions`.
- [ ] **`GEMINI.md`:** replace everything between `## Current work` and `## Every change is a pull request` with exactly this:

  ```markdown
  ## Current work

  You are implementing `docs/superpowers/plans/2026-11-05-floating-ui-followups.md`: nine changes the owner asked for after the floating UI. The design is `docs/superpowers/specs/2026-11-05-floating-ui-followups-design.md`.

  - The plan's **Working Rules** and this file apply to everything you do in this repository. Read "How to write code here" before each milestone: it grows after every review.
  - **One draft pull request for the whole plan.** It stays a draft until the last checkpoint passes. For this plan, that replaces "at the end of each milestone, open a pull request" below.
  - The plan gives behavior, interfaces, names and tests, and the spec gives the numbers and words. You write the code, and the rules below are what the review checks it against.
  - Tasks marked **Owner: Claude** aren't yours. Hand each one to Claude and wait, as the plan's "Task owners" section says. You still run their checks and keep the ledger.
  - **Keep going until CP4 passes; don't wait for the owner between milestones.** At each checkpoint hand the review to Claude and wait for its verdict. That score decides: PASS starts the next milestone, FAIL goes back to fixing. The plan's "UI checkpoints and scoring" says how, and when to stop and ask the owner.
  - **The decisions entry is Claude's** for this plan, at the last review. Don't write one per milestone.

  ```
- [ ] **Commit and open the pull request:**

  ```bash
  git add GEMINI.md docs/superpowers/plans/2026-11-05-floating-ui-followups.md docs/superpowers/specs/2026-11-05-floating-ui-followups-design.md
  git commit -m "docs: what follows the floating UI"
  git push -u origin feat/floating-ui-followups
  ```

  Then `gh pr create --draft --base beta`. It goes into `beta` either way, because CI only runs on pull requests into a release branch. Until #41 merges, its diff also shows #41's commits, so say that in the description. After #41 merges, run `git merge origin/beta` at the start of the next milestone.

---

## Milestone M1: the pane and what goes (CP1)

### Task 1: The right pane resizes

**Owner: Claude. Score 4.** It touches the side slot's springs and the left column's limits, which the owner tuned by hand. Spec §2.

**Files:** `shell_splitter.dart`, `side_slot.dart`, `app/lib/ui/motion/spring_value.dart`, `desktop_shell.dart`, `shell_keys.dart`, `oc_metrics.dart`, `chat_area.dart`, `floating_scenes.dart`, tests `app/test/widget/shell/splitter_test.dart`, `right_pane_test.dart`, `app/test/widget/motion/spring_value_test.dart`.

**Interfaces, produced:**

```dart
enum SideContent { people, music, voiceChat }  // each with defaultWidth, minWidth, the key of §2
final sideWidthProvider = /* family by SideContent */ ...;  // setLive(double?), save(double?), reset(), as SidebarWidthNotifier
// ShellSplitter gains: required String label, Key? gapKey, bool growsLeft = false
// SideSlot and SpringValue gain: bool immediate = false — a new target is reached at once and stops a running spring
abstract final class ShellKeys { static const sideSplitter = Key('float-side-splitter'); }
// OcFloats gains: sideMin 232, sideMusicMin 300, voiceChatMin 300, sideMax 480, mainMin 380, voiceStageMin 600
```

- [ ] **Tests:**
  - `right_pane_drags_live`: at 1280 × 800, drag the splitter −60: side rect `948,40,324,752`, main width `566`, with no spring running between frames.
  - `its_limits_are_the_specs`: at 1280, People stops at `232` and `480`. At 1180 with the left at 288, it stops at `418`. Music stops at `300`. A voice chat's maximum uses `600`, not `380`.
  - `each_content_keeps_its_own_width`: People to 300, switch to Music: 340 on `pane`; Music to 400, back to People: 300; back to Music: 400.
  - `its_widths_are_remembered`: each key of §2, written once, on drag end only. A click without a drag writes nothing.
  - `double_click_and_home_reset_on_the_spring`; `arrow_left_grows_it`.
  - `it_is_a_slider_for_screen_readers`: `Resize the right pane`, and the left splitter keeps `Resize the channel list`.
  - `no_splitter_on_a_drawer`: at 1100, 980, 940 and 759, `ShellKeys.sideSplitter` is absent and the drawer's rect is as `right_drawer_is_at_its_place_at_every_overlay_width`.
  - `no_splitter_while_shut`: appears only with the slot open.
  - `dragging_then_closing_ends_exact`: drag, close, open: exact widths at rest, the notch and the slot never both shut or both open at rest.
  - `window_shrinking_gives_way_and_returns`: pane saved at 480 at 1280; at 1180 it's 418 and the left stays 288; back at 1280 it's 480, both on `pane`. With the left at 420 and Music: Music 300, left 406.
  - `dragging_a_voice_chat_never_flips_it`: dragged to its maximum, it stays in the slot.
  - `the_left_limit_follows_the_pane`: the left splitter's maximum uses the pane's clamped width.
  - `the_springs_still_run`: `quick_open_and_close_ends_exact` and every existing `right_pane_test` pass unchanged.
- [ ] **Build:**
  - `ShellSplitter` takes its label and direction. Both splitters share it.
  - In `_DesktopShellState`, don't copy the left column's six drag fields a second time: put drag state, clamping and the settle spring in one small class (`_SplitterDrag`) used by both.
  - Replace the literal `380.0` in `desktop_shell.dart` with `OcFloats`. Whether a voice chat fits stays decided by `OcFloats.voiceChat`.
  - Add the `pane-resized` scene (People dragged to 340) to `floatingScenes`.
- [ ] **Render** `text`, `music` and `pane-resized` at `wide-1440`, `wide-1280` and `wide-1180`, and look at the pill at full size.
- [ ] Commit `feat(desktop): the right pane resizes`.

**Checks:** `splitter_test.dart`, `right_pane_test.dart`, `floating_frames_test.dart`, `spring_value_test.dart`; `flutter analyze`.

**Checks for Gemini:** `compare.py` on `text` and `music`: no line for the frames it checks.

### Task 2: Music volume on the panel

**Owner: Gemini. Score 2.** Spec §3.

**Files:** `music_controls.dart`, `music_panel.dart`, tests `app/test/widget/music/music_panel_test.dart`.

**Interfaces, produced:** `class MusicVolumeBar extends ConsumerWidget`. `MusicVolumeButton` is removed; `MusicVolumeControls` stays.

- [ ] **Tests:**
  - `volume_needs_no_popover`: `find.byType(MusicVolumeButton)` can't exist; drag the slider and `musicListeningProvider`'s volume follows during the drag; no route was pushed.
  - `volume_row_numbers`: height, gap, value text style and width, by measurement.
  - `speaker_mutes_and_unmutes`: icon, tooltip and value text each way, twice.
  - `dragging_while_muted_unmutes`.
  - `the_row_follows_the_hotkey`: changing `musicListeningProvider` from outside updates icon, slider and value.
  - `it_fits_the_narrowest_pane`: at 300 wide, no overflow.
  - `the_transport_stays_centred`: the play button's centre is the panel's.
  - `the_cards_menu_keeps_its_controls`: `MusicVolumeControls` still opens from the now-playing card's right click.
- [ ] **Build:** share the slider's theme with `MusicProgress` (one private helper, not a copy).
- [ ] **Render** `music` at `wide-1280`, and in light. Crop the row.
- [ ] Commit `feat(desktop): music volume on the panel`.

### Task 3: Bubble mode goes

**Owner: Gemini. Score 2. Review focus:** compact hover bar and menus on channel and private lists, and the skeleton. Spec §4.

**Files:** `app_settings.dart`, `appearance_page.dart`, `chat_row_view.dart`, `message_item.dart`, `message_list.dart`, `private_message_list.dart`, `command_rows.dart`'s comment, `message_line.dart` (removed), tests `app/test/unit/settings/app_settings_test.dart`, `app/test/widget/chat/message_list_test.dart`, `message_actions_test.dart`, `app/test/widget/shell/shell_test.dart`, `app/test/widget/settings/user_settings_test.dart`, `app/test/golden/chat_golden_test.dart`.

- [ ] **Tests:**
  - `an_old_comfortable_choice_opens_as_compact`: settings JSON with `"density": "comfortable"` parses, and `toJson` has no `density`.
  - `appearance_has_no_density`: `Message density`, `Comfortable` and `Compact` aren't on the page, and the page below it hasn't moved out of order.
  - `skeleton_draws_compact_rows`: at 1280, `ChatSkeleton` draws compact rows (small round avatars in `hover`, author and text lines, no bubbles).
  - `channels_load_with_the_skeleton`: a slow first load shows it, and messages replace it.
  - `private_chats_load_with_the_skeleton`: the same; `_Spinner` is gone from the file unless something else uses it.
  - `shell_test`'s `text ×$scale, ${density.name}` loop keeps its scales, without density. Name this under Deviations.
  - Tests that asserted bubble behaviour in `message_list_test` and `message_actions_test`: where the same behaviour exists on compact, change the setup and keep the assertion. Where it only existed on bubbles, remove it and name it under Deviations.
- [ ] **Build:** remove everything §4 lists. Then remove what only bubbles used, whatever else that turns out to be.
- [ ] **Sweep:** `grep -rn "MessageDensity\|MessageBubble\|MessageLine\|DensityMiniature\|density" app/lib app/test app/integration_test` prints nothing about messages. Delete the two bubble goldens and their cases.
- [ ] **Render** `text` at `wide-1280` and `settings-appearance` (add the scene), and write `ChatSkeleton` to a one-off image at the same width. Look at them.
- [ ] Commit `refactor(desktop): one way to show messages`.

### Task 4: Invisible goes

**Owner: Gemini. Score 1.** Spec §5.

**Files:** `repository.dart`, `core_mapping.dart`, `presence.dart`'s comment, `profile_pages.dart`, tests `app/test/unit/settings/local_prefs_test.dart`, `app/test/unit/desktop/tray_menu_test.dart`.

- [ ] **Tests:**
  - `a_saved_invisible_opens_as_online`: `ui.presence` = `invisible` reads as `SelfPresence.online`, and the repository is told online at startup.
  - `the_choices_are_three`: the settings cards, the user panel's menu and the tray menu each list Online, Idle, Do not disturb.
- [ ] **Build:** remove the value and its cases. The analyzer finds each exhaustive switch.
- [ ] **Sweep:** `grep -rni "invisible" app/lib` prints only `window_frame.dart`'s resize handles.
- [ ] **Render** the Profile page's status cards and the user panel's menu at `wide-1280`.
- [ ] Commit `refactor(desktop): no more Invisible`.
- [ ] **CP1.**

---

## Milestone M2: mouse keybinds (CP2)

### Task 5: Mouse buttons in the core and the system backends

**Owner: Claude. Score 5,** native code on three systems, two of which can't run here. A slow mouse hook lags the whole machine. Spec §6.

**Files:** `crates/opencord-media/src/hotkeys/{mod,x11,windows,macos,portal,linux}.rs`, `crates/opencord-core/src/api/types.rs`, `media.rs`, the regenerated bridge, `app/lib/core/hotkeys.dart`, `core_mapping.dart`, `app/lib/core/model/voice.dart`, tests in each Rust file and `app/test/unit/voice/hotkeys_test.dart` (new).

**Interfaces, produced:**

```rust
pub enum MouseButton { Middle, Back, Forward }
// Accelerator gains: pub button: Option<MouseButton>  (key "Mouse3" | "Mouse4" | "Mouse5")
// HotkeySupport::Global { method: String, mouse: bool }, in hotkeys and in api::types
```

```dart
enum MouseButton { middle, back, forward }  // name in text, label, and mask: kMiddleMouseButton, kBackMouseButton, kForwardMouseButton
class Accelerator {
  final LogicalKeyboardKey? key;   // exactly one of key and button
  final MouseButton? button;
}
class HotkeysGlobal { final String method; final bool mouse; }  // mouse defaults to true
```

- [ ] **Tests, Rust:**
  - `parses_mouse_buttons`: `Mouse3`, `Mouse4`, `CTRL+Mouse4`, `CTRL+SHIFT+Mouse5` parse and print back the same. `Mouse1`, `Mouse2`, `Mouse6` don't parse.
  - In `x11.rs`: `a_button_with_its_modifiers_presses_and_releases`; `a_button_without_its_modifiers_does_nothing`; `a_key_and_a_button_on_one_action_press_once`; details 2, 8 and 9 map to the three buttons, and 4 to 7, the wheel, map to none.
  - In `windows.rs`, under the same names: `VK_MBUTTON`, `VK_XBUTTON1`, `VK_XBUTTON2` through its `Matcher`; `the_mouse_hook_is_only_wanted_with_a_mouse_binding`.
  - In `macos.rs`, under the same names: button numbers 2, 3 and 4; `the_tap_only_asks_for_the_mouse_with_a_mouse_binding`.
  - In `portal.rs` or `mod.rs`: `only_keys_reach_the_portal`: mouse bindings are filtered out before `shortcut_ids`, so a key's portal id stays the same, and `Global.mouse` is false.
- [ ] **Tests, Dart:** `a_mouse_accelerator_round_trips`; `its_label_is_the_specs`, Linux and macOS; `other_buttons_dont_parse`; an old saved keybind with a key still parses.
- [ ] **Build:**
  - In each backend, a button is a pseudo-key in the existing `Matcher`, so `Held` and the modifier logic are not written twice.
  - **Windows:** a second hook, `WH_MOUSE_LL`, on the same thread, installed only when a binding uses a button. It never blocks and always calls the next hook.
  - **macOS:** `OtherMouseDown` and `OtherMouseUp` in the tap's mask only when a binding uses a button.
  - **X11:** `RAW_BUTTON_PRESS` and `RAW_BUTTON_RELEASE` alongside the keys.
  - Regenerate the bridge. `leavingKeybinds` and `_InAppHotkeys` skip button accelerators for now, so everything still compiles and behaves; Task 6 handles them.
- [ ] Commit `feat(media): hotkeys can be mouse buttons`.

**Checks:** `cargo test --locked -p opencord-media --lib hotkeys`; `cargo clippy --locked -p opencord-media -p opencord_core --all-targets -- -D warnings`; `flutter analyze`; `app/test/widget/voice/hotkeys_test.dart`.

**Checks for Gemini:** `.github/check.sh client`. Windows and macOS only compile in CI: push, and read `Client (Windows)` and `Client (macOS)` before starting Task 6. If either fails, give Claude the log as a question file. Claude fixes its own task.

### Task 6: Recording mouse keybinds, and the app's own handling

**Owner: Gemini. Score 3. Review focus:** one action per press, in every combination of system support; clicks keep working. Spec §6.

**Files:** `keybind_recorder.dart`, `keybinds_page.dart`, `core/providers/providers.dart` (`_InAppHotkeys`), `desktop_shell.dart`, `shortcuts.dart`, `floating_scenes.dart`, tests `app/test/widget/settings/keybinds_test.dart`, `app/test/widget/voice/hotkeys_test.dart`, `app/test/widget/shell/title_crumb_test.dart`.

**Interfaces, consumed:** Task 5's `MouseButton`, `Accelerator.button`, `HotkeysGlobal.mouse`.

- [ ] **Tests:** with `tester.createGesture(kind: PointerDeviceKind.mouse, buttons: …)`:
  - `recording_takes_a_mouse_button`: each of the three, anywhere in the window; the row shows its label and the draft holds `Mouse4`.
  - `modifiers_go_with_the_button`: Ctrl held, Mouse 4: `CTRL+Mouse4`, `Ctrl + Mouse 4`.
  - `left_click_on_the_recorder_stops_it_once`: it stops and doesn't start again. Left click elsewhere and right click anywhere stop it with no change.
  - `the_recorder_asks_for_either`: `Press keys or a mouse button…`.
  - `recording_mouse_4_does_not_go_back`: history unchanged.
  - `mouse_works_in_the_app_when_focused_only`: `HotkeySupport.focusedOnly`: press and release hold push to talk; a toggle toggles once.
  - `mouse_works_in_the_app_when_the_portal_cant`: `HotkeysGlobal('…', mouse: false)`: the same, and a key keybind isn't handled in the app.
  - `a_system_wide_mouse_keybind_does_not_act_twice`: `HotkeysGlobal('…')`: nothing from the app.
  - `its_modifiers_must_be_held`: `CTRL+Mouse4` without Ctrl does nothing.
  - `up_after_the_window_lost_the_button`: `PointerCancelEvent` releases what it held.
  - `mouse_4_bound_no_longer_goes_back`; `mouse_4_with_ctrl_bound_goes_back_without_ctrl`; `side_buttons_step_with_nothing_bound`.
  - `the_footer_says_where_mouse_works`: only with `mouse: false` and a mouse keybind.
  - `the_empty_row_mentions_the_mouse`.
- [ ] **Build:**
  - `_InAppHotkeys` and the recorder listen through `GestureBinding.instance.pointerRouter.addGlobalRoute`, and remove the route on dispose.
  - Which button went down or up comes from comparing `event.buttons` with the last mask for that pointer.
  - Back and forward in `desktop_shell.dart` check the keybinds first.
- [ ] **Render** `settings-keybinds` (add the scene) at `wide-1280` and `medium-980`, dark and light, with a key row and a mouse row.
- [ ] Commit `feat(desktop): keybinds with the mouse`.
- [ ] **CP2.**

---

## Milestone M3: code blocks (CP3)

### Task 7: Fences, wherever they're read

**Owner: Gemini. Score 3. Review focus:** the table in §7.1, exactly; Enter never loses a message. Spec §7.1, §7.2.

**Files:** `app/lib/core/fences.dart` (new), `markdown.dart`, `app/lib/core/format.dart`, `composer.dart`, `private_composer.dart`, `mobile/src/features/chat/markdown.ts`, `mobile/src/state/preview.ts`, tests `app/test/unit/chat/markdown_test.dart`, `app/test/unit/chat/fences_test.dart` (new), `app/test/unit/format_test.dart`, `app/test/widget/chat/composer_test.dart`, `app/test/widget/home/private_chat_test.dart`, `mobile/src/features/chat/__tests__/markdown.test.ts`, `mobile/src/state/__tests__/preview.test.ts`.

**Interfaces, produced:**

```dart
class Fence { final int start; final int end; final String? language; final String code; }
List<Fence> findFences(String text);
bool endsInsideFence(String textBeforeCursor);
```

- [ ] **Tests:**
  - Every row of §7.1's table, in `markdown_test`, as `parseMarkdown` results.
  - `a_language_can_have_these_characters`: `c++`, `c#`, `objective-c`, `f.x`; 33 characters isn't a language.
  - `indentation_and_blank_lines_inside_stay`; `blank_lines_at_the_ends_go`.
  - `an_opening_fence_mid_line_is_not_a_block`: `see ```x``` here` stays a paragraph.
  - `two_blocks_in_one_message`; `empty_fences_stay_text`; `text_before_and_after_a_block`.
  - The existing fence tests pass unchanged.
  - `previews_read_every_format`: `previewText` gives the code for each row of the table. Today's regular expression loses a one-line fence: test that first.
  - In both composers, `enter_inside_an_open_fence_adds_a_line`: after ` ```py `, Enter inserts a line and sends nothing; after the closing fence, Enter sends.
  - `the_cursor_decides`: the cursor before an opening fence, Enter sends.
  - `a_suggestion_still_takes_enter_first`; `shift_enter_is_unchanged`; `the_send_button_sends_an_open_fence_as_it_is`; `an_input_method_keeps_its_enter` in the private composer.
  - Phone: the same table in `markdown.test.ts` and `preview.test.ts`.
- [ ] **Build:** `findFences` is the only place that knows the rules. `markdown.dart` and `format.dart` call it, and `mentions.dart`'s pattern is left alone since it already stops at the next fence. Inside an open fence, the key handlers do what they do for Shift+Enter.
- [ ] Commit `fix(desktop): code fences in the formats people type`.

**Checks:** the tests above; `source mobile/scripts/env.sh && cd mobile && yarn -s test --ci src/features/chat src/state`.

### Task 8: Highlighting in grey

**Owner: Gemini. Score 3. Review focus:** scrolling past blocks stays smooth; no colour anywhere. Spec §7.3.

**Files:** `app/pubspec.yaml`, `pubspec.lock`, `code_highlight.dart` (new), `markdown_view.dart`, `floating_scenes.dart`, tests `app/test/widget/chat/code_highlight_test.dart` (new).

**Interfaces, produced:** `InlineSpan? highlightedCode(String code, String? language, OcColors colors)`, null when it stays plain.

- [ ] **First, prove the package builds.** Claude read `re_highlight`'s source and README, and has not run it. `flutter pub add re_highlight`, then write `code_is_highlighted_for_its_language` and run it. From its source: `Highlight()`, `registerLanguage('python', langPython)` (`package:re_highlight/languages/python.dart`), `registerAliases`, `highlight(code:, language:)`, `TextSpanRenderer(defaultStyle, theme)`, `result.render(renderer)`, `renderer.span`. **If it doesn't build on Flutter 3.44, or these names don't exist, stop and ask the owner.** Don't write a highlighter of your own or pick another package.
- [ ] **Tests:**
  - `code_is_highlighted_for_its_language`: in a Python block, `def` is `text` weight 700, the string `textSecondary`, the comment `textMuted`, an identifier `text`.
  - `function_names_take_title`: dotted classes such as `title.function_` get the `title` look. Read how the renderer resolves them.
  - `every_alias_works`: each row of §7.3's first table gives the same spans as its full name.
  - `an_unknown_language_stays_plain`; `no_language_stays_plain`; `a_huge_block_stays_plain`: 20001 characters returns null without highlighting.
  - `its_second_call_is_cached`: the same arguments return the same span object; a 65th block drops the oldest.
  - `a_theme_change_rebuilds_it`: light and dark don't share a cached span.
  - `only_grey`: every colour in the spans is an `OcColors` token; `monochrome_test` passes.
  - `copy_gives_what_was_typed`; `the_label_shows_what_was_typed`.
- [ ] **Build:** a block's `Text` becomes `Text.rich` when `highlightedCode` isn't null. Register only §7.3's languages, once, lazily. Don't use `builtinAllLanguages`, which pulls in every grammar.
- [ ] **Look at it:** add a `code-block` scene on the mock world's Rust block, render it dark and light at `wide-1280`, `medium-980` and `narrow-940`. `JetBrainsMono` is bundled in Regular only, so bold is synthesised: crop it and say in the report how it looks.
- [ ] Commit `feat(desktop): code takes the language's shape, in grey`.
- [ ] **CP3.**

---

## Milestone M4: voice (CP4)

### Task 9: The test: hearing yourself while deafened

**Owner: Claude. Score 5,** the audio path of a live call. A mistake here sends someone's voice when they believe they're deafened. Spec §8.4.

**Files:** `crates/opencord-media/src/audio/{processor,capture,playback,engine}.rs`, `crates/opencord-core/src/media.rs`, `app/lib/features/voice/mic_test.dart` (new), tests in those Rust files and `app/test/widget/voice/mic_test_test.dart` (new).

**Interfaces, produced:**

```dart
final micTestProvider = NotifierProvider<MicTestNotifier, bool>(MicTestNotifier.new);
class MicTestNotifier extends Notifier<bool> {
  Future<void> start();  // throws RepoException, having put everything back
  Future<void> stop();   // safe to call when off
}
```

- [ ] **Tests, Rust,** in `processor.rs`:
  - `a_test_in_a_deafened_call_sends_nothing`: `set_deafened(true)`, `set_mic_test(true)`, 100 ticks of speech: `send` is never called, and playback's level matches the microphone.
  - `a_test_while_muted_sends_nothing`: the same with `set_muted(true)`.
  - `deafened_hears_only_the_test`: another user's voice and a soundboard sound arrive: only the microphone is heard.
  - `the_test_is_not_talking`: no `EngineEvent::Talking` from it in a deafened call.
  - `no_muted_reminder_during_a_test`: `take_speaking_while_muted` stays false.
  - `push_to_talk_counts_as_held_during_a_test`; and after stopping, the real key's state applies again.
  - `voice_activity_still_gates_a_test`: quiet ticks play nothing.
  - `stopping_goes_back_to_sending_nothing`: after `set_mic_test(false)` in a deafened call, `send` stays silent and no frame of the test is left to play.
  - `the_mic_test_plays_the_microphone_back` and its neighbours pass unchanged.
- [ ] **Tests, Dart:**
  - `in_a_call_it_deafens_and_restores`: not deafened, start: deafened and `micTest` on; stop: undeafened, `muted` as before.
  - `already_deafened_stays_deafened`.
  - `out_of_a_call_it_does_not_touch_voice`.
  - `undeafening_ends_it`: `toggleDeafen` and `toggleMute` from outside end the test, and don't deafen again.
  - `leaving_voice_ends_it`; `joining_voice_ends_it`.
  - `a_failure_puts_everything_back`: `setMicTest` throws: off, and undeafened.
  - `start_stop_start`: quick calls end on the last one asked.
- [ ] **Build:**
  - In `Processor`: the capture is muted only when `(muted || deafened) && !mic_test`. A frame goes to the test always, and to `send` only when not muted or deafened. Talking events follow what's really sent.
  - In `Playback`: while deafened, the mix is `MIC_TEST_USER` alone, and sounds stay silent.
  - Check `media.rs`: `state.mic_test` must not start an engine of its own after a call ends with the test on.
- [ ] Commit `feat(voice): hear yourself while deafened`.

**Checks:** `cargo test --locked -p opencord-media --lib audio`; `cargo clippy --locked -p opencord-media -p opencord_core --all-targets -- -D warnings`; `mic_test_test.dart`; `flutter analyze`.

**Checks for Gemini:** `.github/check.sh client`.

### Task 10: The noise chip and its popover

**Owner: Gemini. Score 3. Review focus:** the test can't outlive the popover; nothing overflows the card at its narrowest. Spec §8.1, §8.2, §8.3.

**Files:** `noise_chip.dart`, `mic_test_panel.dart` (new), `app/lib/ui/widgets/oc_segmented.dart` (new, from `pane_switch.dart`), `pane_switch.dart`, `voice_panel.dart`, `floating_scenes.dart`, tests `app/test/widget/channels/voice_user_panel_test.dart`, `app/test/widget/voice/noise_chip_test.dart` (new).

**Interfaces, consumed:** Task 9's `micTestProvider`.

**Interfaces, produced:**

```dart
class OcSegmented<T> extends StatelessWidget {
  const OcSegmented({required List<(T, String)> options, required T? value, required ValueChanged<T> onChanged});
}
class MicTestPanel extends ConsumerStatefulWidget {}  // stops the test on dispose if this panel started it
```

- [ ] **Tests:**
  - `chip_sits_left_of_disconnect`: its right edge `6` from Disconnect's left, its height, radius and colours by measurement; Disconnect keeps the rect `voice_user_panel_test` already checks.
  - `chip_shows_the_mode`: all three, with the text colours of §8.1.
  - `popover_opens_beside_it`: to its right; twice.
  - `popover_numbers`: width, gaps and text styles by measurement.
  - `the_switch_picks_standard_and_high`: `audioSettingsProvider` and the mock's `audio` change at once; twice.
  - `turning_off_and_on_comes_back_to_the_same_mode`, and from Off with a fresh opening, `Standard`.
  - `tapping_a_mode_while_off_turns_it_on`.
  - `the_line_follows_the_mode`: the three strings.
  - `the_popover_follows_the_fallback`: `NoiseSuppressionFellBack` moves it to Standard.
  - `test_button_starts_and_stops`: label, primary look, the meter shown only while testing; twice.
  - `closing_the_popover_ends_the_test`: outside click and Esc; deafen restored.
  - `a_test_that_cant_start_says_why`: `setMicTest` fails: a toast, button back to `Test`.
  - `pane_switch_still_works`: `task9_panes_test` passes unchanged after the extraction.
  - `the_card_fits_its_narrowest`: the column at 232, status text ellipsized, no overflow, at the largest text size too.
- [ ] **Render** `voice-card`, `noise-popover` and `noise-popover-testing` (add the scenes) at every inline width, dark and light. Crop the card's first row at 232 and at 288.
- [ ] Commit `feat(desktop): noise suppression on the voice card`.

### Task 11: The test in Voice settings

**Owner: Gemini. Score 2.** Spec §9.

**Files:** `app_pages.dart`, `mic_test_panel.dart`, `settings_draft.dart` if needed, tests `app/test/widget/settings/user_settings_test.dart`.

- [ ] **Tests:**
  - `the_section_is_after_noise_suppression`: title `Microphone test`.
  - `out_of_a_call_it_tests_without_deafening`, and the note's wording.
  - `in_a_call_it_deafens_and_restores`, and the note's wording.
  - `leaving_the_page_ends_it`; `closing_settings_ends_it`.
  - `it_hears_the_draft`: while testing, change noise suppression and input volume on the page: the mock's `audio` follows, unsaved. Stop: it goes back to the saved settings.
  - `saving_during_a_test_keeps_the_saved`: no flicker back to the old ones.
  - `it_always_starts_off`: reopen settings after a test; nothing about it is written to the store.
- [ ] **Render** `settings-voice` (add the scene) at `wide-1280` and `medium-980`, dark and light.
- [ ] Commit `feat(desktop): the voice test in Settings`.

### Task 12: Ping on the signal bars

**Owner: Gemini. Score 1.** Spec §10.

**Files:** `voice_panel.dart`, tests `app/test/widget/channels/voice_user_panel_test.dart`.

- [ ] **Tests:** hovering with `tester.createGesture(kind: PointerDeviceKind.mouse)`:
  - `hovering_the_bars_shows_the_ping`: `42 ms`, with no wait, in the theme's tooltip look.
  - `it_follows_each_report`: a new report while open changes the text.
  - `no_ping_no_tooltip`: before the first measurement.
  - `hovering_the_text_shows_nothing_new`: only the icon has it; `_StatusText`'s own tooltip stays.
  - `a_click_still_opens_the_graph`.
  - `screen_readers_hear_the_ping`: in the row's label.
- [ ] **Build:** watch `voiceStatsProvider.select((stats) => stats?.pingMs)` in a small widget around the icon, so the whole card doesn't rebuild with each report.
- [ ] Commit `feat(desktop): the ping on the signal bars`.

### Task 13: Docs and the hand-check list

**Owner: Gemini. Score 1.** The decisions entry is **Claude's**, at the last review, from the code: leave it.

**Files:** `docs/plans/desktop-ui.md`, `docs/superpowers/specs/2026-11-04-floating-ui-design.md`, `app/test/showcase/showcase_test.dart`, `docs/images/*`, `docs/plans/check-windows-by-hand.md`, `check-macos.md`.

- [ ] **`desktop-ui.md`:** §8.1's density and status lists, and the keybinds, from the code. Grep each name and number.
- [ ] **The earlier spec,** a line each pointing here: §5.5 (the chip), §5.7 (a second splitter), §5.11 (remove "The compact message style stays as it is"), §5.13 (widths).
- [ ] **The showcase:** fix its finders, regenerate `docs/images`. Look at them first.
- [ ] **Hand checks:** every line of §11 in the check documents.
- [ ] Commit `docs: what followed the floating UI`.
- [ ] **CP4.** After its PASS, mark the pull request ready and report to the owner, including everything in §11 that nobody has checked by hand. The owner merges.
