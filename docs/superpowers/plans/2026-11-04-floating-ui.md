# The floating UI: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The desktop app rebuilt 1:1 from the owner's approved prototype: every part floats with its own border under one fixed title row, with the prototype's sizes, paddings and motion.

**Architecture:**
- One title row, then `OcFloat`s on the `rail` ground, laid out by `DesktopShell`.
- Motion lives in one place: `OcSpring` tokens and the `SpringValue` and `FlipBox` widgets.
- The main page's notch is one path (`notchPath`) used for fill, border, shadow and clip.
- Nothing below `app/lib/features` and `app/lib/ui` changes, apart from one icon font rebuild and three constants in the macOS runner.

**Tech Stack:** Flutter 3.44 (`SpringDescription.withDurationAndBounce`), Riverpod 3, existing `OcColors`, `OcText`, `Hoverable`.

**Spec:** `docs/superpowers/specs/2026-11-04-floating-ui-design.md`. Read all of it before Task 1, and the sections a task names before that task. "§" below means the spec. **Every number is in the spec, and the plan doesn't repeat them:** a number you can't find there is a question, not a guess.

## Global Constraints

- **Branch and pull request:** `feat/floating-ui`, made in "Before Task 1". If it starts from `origin/batch/2026-11-01` because pull request #40 is still open, run `git merge origin/beta` at the start of the first milestone after #40 merges. The first milestone opens **one draft pull request into `beta`**; later milestones push to it. Until #40 merges, its diff also shows #40's commits. It stays a draft until the last checkpoint passes. Half of a redesign must not reach a release branch, so this replaces "one pull request per milestone" for this plan. Never merge.
- **Scope:** `app/` and docs only. Nothing in `crates/`, `proto/` or `mobile/`. No new dependency.
- **Platforms:** desktop. Linux runs here; Windows and macOS compile in CI, with hand checks listed in Task 22.
- **Colours:** `OcColors` tokens only, monochrome (`test/widget/monochrome_test.dart`). macOS's native traffic lights are the one exception.
- **Words:** every user-visible string in the spec is copied exactly.
- **Motion:** only through `OcSpring`, `SpringValue`, `FlipBox` and `OcMotion`. No `Duration(milliseconds: …)` or `Curves.…` for motion in `features/`. Reduced motion takes every animation to zero.

## Working Rules (read before Task 1)

`GEMINI.md` applies to everything here. Read "How to write code here" before each milestone.

### Commands

Start every `flutter` and `dart` command with `source .superpowers/sdd/media/env.sh &&`.

| What | Command, from the repository root |
|---|---|
| One test file | `source .superpowers/sdd/media/env.sh && cd app && flutter test test/widget/shell/floating_frames_test.dart` |
| One test | add `--plain-name 'the_test_name'` |
| Analyze and format | `source .superpowers/sdd/media/env.sh && cd app && flutter analyze && dart format --set-exit-if-changed lib test` |
| Before each push | `.github/check.sh client`, and first `df -h /`: **only 7.6 GB was free when this was written.** Below 10 GB, follow `GEMINI.md`'s clean-up. |
| Render the screens | `source .superpowers/sdd/media/env.sh && cd app && flutter test --dart-define=OPENCORD_REVIEW=/home/amin/dev/opencord/.superpowers/sdd/floating-ui/screens/<name> --dart-define=OPENCORD_EMOJI=/usr/share/fonts/apple-color-emoji/apple-color-emoji.ttf test/showcase/floating_review_test.dart` |
| Only some | add `--dart-define=OPENCORD_REVIEW_SCENES=text,voice` and/or `--dart-define=OPENCORD_REVIEW_WIDTHS=wide-1280,medium-980` |
| Compare with the prototype | `python3 -I .superpowers/sdd/floating-ui/tools/compare.py .superpowers/sdd/floating-ui/screens/<name> --sheets` |
| Filmstrips of an animation | as Render, with `--dart-define=OPENCORD_FILM=/home/amin/dev/opencord/.superpowers/sdd/floating-ui/screens/<name>/film`, on `test/showcase/floating_film_test.dart` |
| A crop at full size | `magick <shot>.png -crop 700x400+740+0 crop.png` (shots are 2×) |
| Icon font | `source .superpowers/sdd/media/env.sh && cd app && dart run tool/build_icons.dart` |

### Task owners

Every task has an **Owner** and a **Score** (1–5, how much a mistake costs and how hard it is to see: platform code and the motion core score 4–5).

- **Owner: Gemini.** You implement it. Score 3 means read its **Review focus** line twice: that's what the review checks hardest.
- **Owner: Claude.** Don't implement it. Hand the task to Claude, naming the task number and this plan's path, and wait for its report. Don't edit files or run git while it works: you share the working copy.
  - Then read the report, run the task's **Checks** yourself, and record the task in the ledger with the report's commits.
  - Claude's tasks come first in their milestone, so yours build on them.
- **Claude, when handed a task:** follow the task exactly. Run its checks, finish with the commit it gives, then report the files, commits, checks and anything left undone.
  - A handed-over run may use `flutter`, `dart`, `cargo`, `cd`, `ls`, `source` and plain `git status`, `diff`, `log`, `show`, `add` and `commit`, and it reads images with its Read tool. It has no `python3`, `magick`, scripts, or variables set in front of a command. So use `--dart-define`, and anything that needs the rest (`compare.py`, `.github/check.sh`, `magick`) is in **Checks for Gemini** on that task.

| Task | Part | Score | Owner |
|---|---|---|---|
| 1 | Springs: `OcSpring`, `SpringValue` | 5 | Claude |
| 2 | `OcFloat`, `notchPath`, `NotchedFloat` | 4 | Claude |
| 3 | Review tools: keys, screens, filmstrips | 3 | Claude |
| 4 | Tokens, icons, badges, avatars | 1 | Gemini |
| 5 | `AppTitleBar`, native title band | 4 | Claude |
| 6 | Back, forward and the crumb | 3 | Gemini |
| 7 | The shell: floats, breakpoints, drawers | 3 | Gemini |
| 8 | Side slot, notch, pane button, no rewrap | 5 | Claude |
| 9 | People and Music panes | 2 | Gemini |
| 10 | Main header, pinned float, messages, composer | 3 | Gemini |
| 11 | Rail and its popover | 3 | Gemini |
| 12 | Server head and channel list | 2 | Gemini |
| 13 | Splitter | 3 | Gemini |
| 14 | Voice card and user panel | 2 | Gemini |
| 15 | Tiles: `FlipBox`, stage, focus | 4 | Claude |
| 16 | Voice dock | 3 | Gemini |
| 17 | Voice chat, join prompt | 3 | Gemini |
| 18 | Private: column, tiles, chat frame | 2 | Gemini |
| 19 | Friends and Requests pages | 3 | Gemini |
| 20 | Menus, popovers, dialogs | 2 | Gemini |
| 21 | Everything left, `HeaderBar` removed | 2 | Gemini |
| 22 | Docs, showcase, hand-check list | 1 | Gemini |

### Rules reviews will enforce

These come from what past UI milestones got wrong.

1. **Passing tests don't finish a UI task.** Earlier milestones had green tests on visibly broken screens. For every scene a task touches: render it, run `compare.py`, open the side-by-side sheets at full size, crop what's small, and fix what differs. List the files in the ledger. **If you can't open images, that's the first line of your report, and the task isn't done.**
2. **Measure, don't assert presence.** A UI test names a pixel result: `expect(tester.getRect(key), rectMoreOrLessEquals(…))`, a colour from a `BoxDecoration`, a font size from `Text.style`. "Finds the text" is not a look test.
3. **Numbers are exact.** Don't snap `3`, `5`, `7`, `12.5` or `13.5` to the `OcSpace` scale. Put repeated ones in `oc_metrics.dart` (`OcFloats`).
4. **Reuse the named widgets:** `Hoverable`, `OcIconButton`, `OcAvatar`, `MemberAvatar`, `ServerAvatar`, `ContactAvatar`, `UnreadBadge`, `MentionBadge`, `ConnectionDot`, `ChannelGlyph`, `EllipsisText`, `SectionLabel`, `OcTextField`, `OcButton`, `showOcMenu`, `showPopover`, `confirmAction`, `showPrivateFailure`. No Material widget one of them replaces. No `InkWell`.
5. **Springs are never written by hand.** Use `SpringValue`, or `spring.curve` with `spring.settle` for a route.
6. **Text never animates its layout.** The message list and the stage go through `MainInnerWidth` (Task 8). Don't animate anything that rewraps text.
7. **Every control twice, checking the effect:** open, close, open again; show, hide, show again; a menu opened twice; the pane opened from each way in.
8. **Change the finder, not the test's meaning.** The redesign moves or removes things tests look for:

   | Today | Becomes |
   |---|---|
   | `find.text('Private')`, rail server names | `find.byWidgetPredicate((w) => w is RailItemView && w.name == …)`: add `railItem(name)` to `test/support/` |
   | `byTooltip('Members')`, `byTooltip('Music')` in a header | `byTooltip('Open the right pane')`, then `find.text('People')` or `find.text('Music')` in the switch |
   | `Close member list` | `Close` |
   | `OcSize.sidebarWidth`, `railWidth` | the new values, through the same names |
   | Finders under `DesktopShell.sidebarKey` | keep working: that key stays on the left column |
   | `find.byKey(DesktopShell.membersKey)` being empty when shut | keeps working: that key stays on the pane's content, which is gone once the slot has closed |

   The same goes for `app/integration_test/` (`m6_test.dart` looks for a member list button, for one), which only CI runs: grep it for whatever you move.

   Never delete an assertion because it fails. If a test's meaning has gone with a removed thing, name it under Deviations.
9. **Goldens:** after a failure, open `test/golden/failures/*` and look. Update (`flutter test --update-goldens <file>`) only what this task meant to change, and check the new image against the spec.
10. **Don't change the mock world for looks.** `core/mock/` stays. A test that needs ten people in voice builds them itself.
11. **`mounted` after every `await`; `on RepoException catch`; every failure shown** (`GEMINI.md`). Removing or moving a button keeps its error handling.
12. **No stubs.** The prototype's dead buttons are real ones here. Keep their existing behaviour.
13. **Keep what you didn't add:** §8 lists what the redesign must not lose.
14. **Finishing a part removes what it replaces,** in the same task: the old widget, parameter or constant, once nothing uses it.

### Progress ledger

Keep `.superpowers/sdd/floating-ui/progress.md`. After each task, append:

```
## Task N — <title> (owner: <Gemini|Claude>, score: N)
- Commit: <hash>
- Tests added: <test names>
- Commands run: <command> → <result summary>
- Screens looked at: <paths>; compare.py: <last line of its output>
- Deviations from the plan: <none | what and why>
- Left undone / doubts: <none | list>
```

### When a step keeps failing

Follow `GEMINI.md` → "When you're stuck: ask Claude", with question files in `.superpowers/sdd/floating-ui/questions/`.

## UI checkpoints and scoring

The owner asked for the look to be scored, part by part, at every breakpoint. Each milestone ends at a checkpoint (CP0–CP6). **Work stops there until Claude's score passes and the owner releases the next milestone.**

**The breakpoints:**

| Tag | Size | Class |
|---|---|---|
| `wide-1440` | 1440 × 900 | wide |
| `wide-1280` | 1280 × 800 (the default window) | wide |
| `wide-1180` | 1180 × 800 | wide, narrowest |
| `medium-1100` | 1100 × 800 | medium |
| `medium-980` | 980 × 800 | medium, narrowest |
| `narrow-940` | 940 × 560 (the smallest window) | narrow |
| `compact-759` | 759 × 700 | compact |

Dark at all of them. Light at `wide-1280` and `medium-980`.

**The score for one part at one breakpoint:**

| Score | Means |
|---|---|
| 5 | Same as the prototype: frames within 1 px, text styles, colours and states right, motion on its curve |
| 4 | Within 2 px, or one detail only visible at full-size crop |
| 3 | A difference you can see comparing the two side by side |
| 2 | Wrong at this width: overlap, clipping, wrong state |
| 1 | Broken |
| 0 | Missing |

**To pass:** every cell ≥ 4, and every motion row 5. A 3 or lower goes back to its task's owner, by name, and the checkpoint runs again.

**How each checkpoint runs:**

1. **Gemini:**
   - render the milestone's scenes to `.superpowers/sdd/floating-ui/screens/cpN`;
   - run `compare.py` and fix everything it lists;
   - run the filmstrips;
   - fill in `.superpowers/sdd/floating-ui/scorecard.md`: your own scores, one row per part and one column per breakpoint, with the crop that shows it;
   - run `.github/check.sh client`, push, read the pull request's checks, fix failures;
   - **stop and report.**
2. **Claude:** re-renders them, looks at the crops at full size, reads the filmstrips against `film/`, walks every gesture and writes its own scores beside yours. **Claude's score is the one that counts.** It fixes small things itself and sends the rest back.
3. **The owner** releases the next milestone.

**What each checkpoint scores:**

| Checkpoint | Parts | Motion rows | Scenes |
|---|---|---|---|
| **CP0**, after Task 4 | Float, notch shape at 52, 26, 0; badges; avatars | Every spring against §4.3 | goldens and unit tests only |
| **CP1**, after Task 7 | Title bar on Linux, Windows and macOS layouts; crumb; arrows; ground; every frame's rectangle; drawers | drawers | `text`, `text-pane-closed`, `homelab`, `drawer-right`, `drawer-left` |
| **CP2**, after Task 10 | Notch, pane button, side slot, switch, People, Music, main header, pinned float, messages, composer | pane opens, pane closes, quick open and close, People → Music, no rewrap | `text`, `text-pane-closed`, `music` |
| **CP3**, after Task 14 | Rail, popover, faces, server head, its menu, channels, categories, splitter, voice card, user panel | popover in, move and out; rail morph; voice card in and out; splitter pill; arrow turn | `text`, `server-menu`, `homelab`, `rail-pop-*` |
| **CP4**, after Task 17 | Stage, tiles, focus, strip, dock, join prompt, voice chat | tiles in, focus, back; dock shows and hides; join prompt | `voice`, `voice-focus`, `music` |
| **CP5**, after Task 19 | Home tiles, direct messages, chat header, bottom bars, Friends, Pending, Blocked, Requests | page fade, press scale, Accept moving a row | `dm`, `friends`, `pending`, `requests` |
| **CP6**, after Task 22 | **Everything, every scene, every breakpoint:** menus, popovers, dialogs, reconnect bar, onboarding, empty states, large text | every row again | all |

## Review Focus

The inputs most likely to break for real people that no ordinary test would catch. Each has a pinned test in the task named.

1. **Quick clicks on the pane:** open then X at once, over and over, or `Ctrl+Shift+U` held down. Expected: no jump, ends at exact widths, the notch and the slot never both shut or both open at rest. Task 8: `quick_open_and_close_ends_exact`.
2. **The dock's own popups:** a popup's barrier takes the pointer away from the page, which looks like the pointer leaving. Expected: the dock stays while its soundboard, screen picker or camera menu is open. Task 16: `dock_stays_while_its_popup_is_open`.
3. **Back onto a voice channel, a deleted channel or a left server.** Expected: opening a voice view never joins or leaves; anything gone is skipped. Task 6: `going_back_to_a_voice_channel_does_not_join`, `going_back_skips_what_is_gone`.
4. **The smallest window with everything showing:** 940 × 560, voice card, reconnect bar, pinned message, Music in its drawer. Expected: no overflow stripes, every float still usable, the channel list scrolls. Task 7: `smallest_window_with_everything_has_no_overflow`.
5. **Large text** (the app's own font-size setting). Expected: headers, the composer and the user panel grow together and stay level; nothing clips. Task 21: `large_text_keeps_the_frame`.

## File Map

**New**

| File | Holds |
|---|---|
| `app/lib/ui/motion/oc_spring.dart` | `OcSpring` |
| `app/lib/ui/motion/spring_value.dart` | `SpringValue` |
| `app/lib/ui/motion/flip_box.dart` | `FlipBox` |
| `app/lib/ui/widgets/oc_float.dart` | `OcFloat` |
| `app/lib/ui/widgets/notched_float.dart` | `notchPath`, `NotchedFloat` |
| `app/lib/ui/widgets/panel_header.dart` | `PanelHeader` |
| `app/lib/features/window/app_title_bar.dart` | `AppTitleBar` |
| `app/lib/features/shell/shell_keys.dart` | `ShellKeys` |
| `app/lib/features/shell/navigation_history.dart` | `navigationHistoryProvider`, `NavEntry` |
| `app/lib/features/shell/title_crumb.dart` | `TitleCrumb`, `HistoryArrows` |
| `app/lib/features/shell/shell_drawer.dart` | `ShellDrawer` |
| `app/lib/features/shell/side_slot.dart` | `SideSlot`, `MainInnerWidth` |
| `app/lib/features/shell/pane_button.dart` | `PaneButton` |
| `app/lib/features/shell/shell_splitter.dart` | `ShellSplitter`, `sidebarWidthProvider` |
| `app/lib/features/servers/rail_popover.dart` | `RailPopover`, `VoiceFaces` |
| `app/lib/features/channels/server_head.dart` | `ServerHead` |
| `app/lib/features/voice/voice_dock.dart` | `VoiceDock` |
| `app/lib/features/home/home_tiles.dart` | `HomeTiles` |
| `app/lib/features/home/person_card.dart` | `PersonCard`, the tabs |
| `app/lib/features/home/requests_page.dart` | `RequestsPage` |
| `app/test/showcase/floating_review_test.dart`, `floating_film_test.dart` | the checks' renders |

**Changed most:** `desktop_shell.dart`, `server_rail.dart`, `channel_sidebar.dart`, `channel_row.dart`, `voice_channel_row.dart`, `voice_panel.dart`, `user_panel.dart`, `chat_area.dart`, `chat_header.dart`, `composer_bar.dart`, `pinned_bar.dart`, `message_list.dart`, `bubble.dart`, `member_panel.dart`, `music_panel.dart`, `voice_view.dart`, `voice_tile.dart`, `voice_layout.dart`, `voice_controls.dart`, `home_sidebar.dart`, `home_main.dart`, `friends_page.dart`, `friend_rows.dart`, `private_chat.dart`, `private_header.dart`, `oc_menu.dart`, `popover.dart`, `popup_route.dart`, `oc_dialog.dart`, `oc_metrics.dart`, `oc_motion.dart`, `oc_theme.dart`, `oc_colors.dart`, `badges.dart`, `avatar.dart`, `window_buttons.dart`.

**Removed by Task 21:** `features/window/header_bar.dart`, `RightPanelModeBar`, `_ControlStrip`.

---

## Before Task 1

Nothing here starts until the owner says so. Then, in order. If a check fails, stop and tell the owner.

- [ ] **Clean tree:** `git status --short` lists only this plan, the spec and the prototype, all three untracked. From here on, nothing else may be working in this copy of the repository.
- [ ] **Disk:** `df -h /`. Below 10 GB, follow `GEMINI.md`'s clean-up now.
- [ ] **References:** `ls .superpowers/sdd/floating-ui/reference | wc -l` prints 115, and `ls .superpowers/sdd/floating-ui/film | wc -l` prints 61. They exist on this machine only.
- [ ] **Branch:** `git fetch origin`, then `gh pr view 40 --json state --jq .state`. If it prints `MERGED`, `git switch -c feat/floating-ui origin/beta`. Otherwise `git switch -c feat/floating-ui origin/batch/2026-11-01`. The three untracked files come along.
- [ ] **`GEMINI.md`:** replace everything between `## Current work` and `## Every change is a pull request` with exactly this:

  ```markdown
  ## Current work

  You are implementing `docs/superpowers/plans/2026-11-04-floating-ui.md`: the desktop app rebuilt as floating panels, 1:1 with the owner's prototype. The design it builds on is `docs/superpowers/specs/2026-11-04-floating-ui-design.md`, which holds every number; where they disagree, the prototype beside it wins. The phone apps plan before it is finished.

  - The plan's **Working Rules** and this file apply to everything you do in this repository. Read "How to write code here" before each milestone: it grows after every review.
  - **One draft pull request for the whole plan.** The first milestone opens it into `beta`, later ones push to it. For this plan, that replaces "at the end of each milestone, open a pull request" below.
  - The plan gives behavior, interfaces, names, numbers and tests, not finished code. You write the code, and the rules below are what the review checks it against.
  - Tasks marked **Owner: Claude** aren't yours. Hand each one to Claude and wait, as the plan's "Task owners" section says. You still run their checks and keep the ledger.
  - **The owner releases one milestone at a time. Stop at each checkpoint (CP0–CP6) and report.** Claude scores the screens there, and that score decides.
  - **The decisions entry is Claude's** for this plan, at the last review. Don't write one per milestone.
  ```

- [ ] **Commit:** `git add GEMINI.md docs/superpowers/plans/2026-11-04-floating-ui.md docs/superpowers/specs/2026-11-04-floating-ui-design.md docs/superpowers/specs/2026-11-04-floating-ui-prototype.dc.html`, then `git commit -m "docs: the floating UI's design and plan"`.

---

## Milestone M0: foundations (CP0)

### Task 1: Springs

**Owner: Claude. Score 5.** The owner's first concern. Everything that moves uses this. Spec §4.

**Files:** `app/lib/ui/motion/oc_spring.dart`, `app/lib/ui/motion/spring_value.dart`, `app/lib/ui/theme/oc_motion.dart`, tests `app/test/unit/motion/oc_spring_test.dart`, `app/test/widget/motion/spring_value_test.dart`.

**Interfaces, produced for every later task:**

```dart
class OcSpring {
  const OcSpring(this.seconds, this.bounce);
  static const pane = OcSpring(0.28, 0);
  static const paneOut = OcSpring(0.2, 0);
  static const pop = OcSpring(0.2, 0.1);
  static const tile = OcSpring(0.3, 0.07);
  static const move = OcSpring(0.3, 0);
  double valueAt(Duration time);        // 0 to the target 1, the spec's formula
  Duration get settle;                  // last time more than 0.4 % from 1
  Curve get curve;                      // valueAt over settle, ending exactly at 1
  SpringSimulation simulation(double from, double to, double velocity);
}

class SpringValue extends StatefulWidget {
  const SpringValue({
    required double target,
    required OcSpring spring,   // used when the target rises
    OcSpring? falling,          // when it falls; defaults to spring
    Duration delay,             // held still first
    Duration fallingDelay,      // when falling
    required ValueWidgetBuilder<double> builder,
    Widget? child,
  });
}
```

`OcMotion` gains `press` 80, `exit` 130, `fade` 120, `turn` 140, all through `_maybe`, and `curveIn` (`Curves.easeIn`).

- [ ] **Tests first:**
  - `each_token_settles_where_the_prototype_does`: `settle` of pane, paneOut, pop, tile, move within 4 ms of 342, 244, 177, 295, 366.
  - `pane_follows_the_sampled_curve`: `264 * pane.valueAt(t)` for t in 40, 80, 120, 160, 200, 280 within 0.5 of §4.3's side slot row.
  - `pop_has_its_small_overshoot_and_pane_has_none`: the maximum of `valueAt` over 0–500 ms is above 1 for pop and at most 1 for pane. A plain ease curve fails this.
  - `curve_ends_exactly_at_one`.
  - `spring_value_is_on_the_curve_frame_by_frame`: pump 16 ms frames; every built value within 0.5 % of `valueAt`.
  - `retargeting_keeps_the_speed`: flip the target at 80 ms; the next frame's step has the same sign as before and the value doesn't jump. A restart from rest fails this.
  - `delay_holds_the_value`: with 50 ms delay, the value at 40 ms equals the start; the delay runs on the falling side when `fallingDelay` is given.
  - `reduced_motion_goes_straight_to_the_target`: with `MediaQuery(disableAnimations: true)`.
  - `a_finished_spring_leaves_no_ticker`: `tester.hasRunningAnimations` false after settle.
- [ ] **Build:**
  - `valueAt` from §4.2's equation; critically damped when bounce is 0.
  - `SpringValue`: an unbounded `AnimationController` and `animateWith(spring.simulation(value, target, velocity))` with `Tolerance(distance: 0.004 × distance, velocity: double.infinity)`, which ends it where §4.2's "settled" does, setting the exact target at the end.
  - Don't start a simulation when the target is unchanged.
- [ ] Commit `feat(desktop): the app's springs`.

**Checks:** both test files pass; `flutter analyze`.

### Task 2: Floats and the notch

**Owner: Claude. Score 4.** Spec §2.3, §5.9.

**Files:** `app/lib/ui/widgets/oc_float.dart`, `app/lib/ui/widgets/notched_float.dart`, `app/lib/ui/theme/oc_theme.dart`, `app/lib/ui/theme/oc_metrics.dart`, tests in `app/test/widget/widgets/floats_test.dart`, `app/test/golden/floats_golden_test.dart`.

**Interfaces, produced:**

```dart
abstract final class OcFloats {
  static const double gap = 8, radius = 16, titleBar = 40, header = 52,
      rail = 62, side = 264, sideMusic = 340, voiceChat = 380, bottom = 62,
      notch = 52, paneButton = 44, paneButtonInset = 4;
  static const double leftDefault = 288, leftNarrow = 264, leftMin = 232, leftMax = 420;
}
class OcFloat extends StatelessWidget { const OcFloat({required Widget child, Color? color, double radius}); }
Path notchPath(Size size, {required double radius, required double notch, double inset = 0});
class NotchedFloat extends StatelessWidget {
  const NotchedFloat({required ValueListenable<double> notch, required Widget child, Color? color});
}
// OcShadows.float(colors), and OcShadows.menu(colors) at the spec's values.
```

- [ ] **Tests first:**
  - `float_has_the_spec_surface`: colour, border, radius and shadow values, dark and light.
  - `content_sits_inside_the_border`: a child's rect is the float's, deflated by 1.
  - `notch_path_cuts_the_corner`: at notch 52 in a 600 × 400 box: `(590, 10)` and `(560, 40)` are out; `(540, 10)` and `(590, 70)` are in; the concave corner (centre `(564, 36)`, radius 16): `(552, 49)` and `(547, 40)` are in, `(553, 45)` and `(549, 40)` are out.
  - `notch_path_at_zero_is_the_plain_float`: sample 400 points; `contains` matches `RRect` everywhere.
  - `notch_path_never_breaks_on_the_way`: for every notch 0–52 by 0.5, `getBounds` is the box, and the path contains the box's centre.
  - `clip_uses_the_notch`: a child in the notch's area doesn't receive a hit, or paint there (golden).
  - Goldens: notch 52, 26, 0 in dark and light.
- [ ] **Build:**
  - `notchPath` as §5.9 says: the notched outline with joints `min(radius, notch/2)`, intersected with the plain rounded rectangle through `Path.combine`.
  - `inset` moves the outer edges in, and the notch's edges out into the page, so a 1 px stroke at `inset: 0.5` sits as the prototype's does.
  - `NotchedFloat` paints fill, shadow (the path shifted and blurred with `MaskFilter.blur`, sigma as `BoxShadow` would use) and a 1 px stroke, and clips its child with `inset: 1`.
  - Cache the paths per size and notch while nothing changes. Skip `Path.combine` when the notch is 0.
- [ ] Commit `feat(desktop): floats and the main page's notch`.

**Checks:** both test files pass; look at the goldens at full size.

### Task 3: Review tools

**Owner: Claude. Score 3.** Every later checkpoint rests on it. Spec §6.

**Files:** `app/lib/features/shell/shell_keys.dart`, `app/test/showcase/floating_review_test.dart`, `app/test/showcase/floating_film_test.dart`, `app/test/support/floating_scenes.dart`, `app/test/support/` (`railItem`).

**Interfaces, produced:**

```dart
abstract final class ShellKeys { static const titlebar = Key('float-titlebar'); /* … one per name in §6 */ }
```

`DesktopShell.railKey`, `sidebarKey`, `mainKey` become aliases of `rail`, `left`, `main`. `membersKey` becomes an alias of `panel`, the pane's content, which exists only while the slot is open or moving.

- [ ] **Scenes** in `floating_scenes.dart`, using `MockApp.pump`, named as the files in `reference/`: `text` (joined General, then #general), `text-pane-closed`, `voice`, `voice-focus`, `music`, `dm`, `friends`, `pending`, `requests`, `server-menu`, `homelab`, `rail-pop-dev`, `rail-pop-rust`, `rail-pop-homelab`, `rail-pop-private`, `drawer-right`, `drawer-left`. They use only what a person does: taps and hovers. A scene whose widget doesn't exist yet is skipped with a printed line. `me-menu` in `reference/` is left out: it's the prototype's stand-in, and the real cog opens User settings.
- [ ] **`floating_review_test.dart`:** read its settings with `String.fromEnvironment`, not `Platform.environment` (`OPENCORD_REVIEW`, `OPENCORD_REVIEW_SCENES`, `OPENCORD_REVIEW_WIDTHS`, `OPENCORD_EMOJI`). Skipped unless `OPENCORD_REVIEW` is set. For each scene × breakpoint × theme (§ checkpoints; filters as in Commands): render at pixel ratio 2 from `MockApp.boundaryKey`, write `<scene>_<width>_<theme>.png`, and add `measure.json`'s entry: `{"<scene>@<width>": {"<key>": [l, t, w, h]}}` for each `ShellKeys` found on screen, rounded to 0.1. The file holds every scene rendered so far in that folder: read it first and merge.
- [ ] **`floating_film_test.dart`:** the same way, skipped unless `OPENCORD_FILM` is set. Scenes `pane-open`, `pane-close`, `voice-leave`, `voice-open`, `tile-focus`, `tile-unfocus`, `music-swap`, `server-menu`, each frozen with `tester.pump` at 0, 40, 80, 120, 160, 240, 360 ms, at 1280 × 800, written as `<scene>_<ms>.png`, the names in `film/`.
- [ ] **Try them** on today's shell: the tests run and write files, and you can read a PNG back.
- [ ] Commit `test(desktop): renders to compare the app with the prototype`.

**Checks:** one run of each writes files; `flutter test` with neither set skips both.

**Checks for Gemini:** `compare.py` on that folder prints its report.

### Task 4: Tokens and small widgets

**Owner: Gemini. Score 1.** Spec §2.3, §3, §5.2, §5.4, §5.19.

**Files:** `oc_colors.dart`, `oc_metrics.dart`, `oc_icons.dart` (generated), `tool/build_icons.dart`, `badges.dart`, `avatar.dart`, `oc_icon_button.dart`, `window_buttons.dart`, tests beside them.

**Consumes:** `OcFloats` (Task 2).

- [ ] **Tests:**
  - `badge_is_18_tall`: `UnreadBadge` and `MentionBadge`: height 18, minimum width 18, padding 5, text 11/600.
  - `initials_follow_the_spec`: font size for avatar 32, 36, 72 is `0.31 · size + 1.7`.
  - `presence_ring_is_two`.
  - `disabled_button_uses_the_given_colour`.
  - `icon_disabled_token`: `selected` in dark, `0xFFC1C1C8` in light.
- [ ] **Build:**
  - `iconDisabled` in `OcColors`, its `copyWith`, `lerp` and `all`.
  - `OcSize.railWidth` 62, `sidebarWidth` 288 from `OcFloats`; keep `sidebarNarrowWidth`, `memberPanelWidth`, `musicPanelWidth`.
  - `arrowForward` and `dockToLeft` in `build_icons.dart`; run it.
  - `OcIconButton.disabledColor`.
  - `LinuxWindowButtons.extent`, `GnomeWindowButton.extent`, default 36.
- [ ] Update the goldens this changes, after looking at them.
- [ ] Commit `feat(desktop): the floating UI's tokens`.
- [ ] **CP0.**

---

## Milestone M1: the frame (CP1)

### Task 5: The title bar

**Owner: Claude. Score 4,** native code. Spec §5.1, §5.19.

**Files:** `app/lib/features/window/app_title_bar.dart`, `app/macos/Runner/MainFlutterWindow.swift`, `app/lib/features/onboarding/onboarding_view.dart`, `keyring_unavailable_view.dart`, `app/test/widget/window/window_chrome_test.dart`.

**Interfaces, produced:**

```dart
class AppTitleBar extends ConsumerWidget {
  const AppTitleBar({Widget? leading, Widget? center});  // the arrows and the crumb
}
```

- [ ] **Tests** (rewrite `window_chrome_test.dart` for the row; keep each thing it checks that still applies):
  - `title_bar_is_40_and_flush`: rect `0,0,w,40` on Linux, Windows and macOS.
  - `center_is_at_the_windows_centre`: with a long left side and short right side.
  - `linux_buttons_follow_the_layout`: right-hand: last circle's edge 8 from the window's edge, circles 6 apart. Left-hand: before `leading`.
  - `windows_caption_is_flush_top_right`: rect `w-138,0,138,32`; `leading` and `center` unaffected.
  - `macos_keeps_room_for_the_lights`: `leading` starts at 78; 8 in full screen.
  - `drag_area_fills_the_empty_row`: a drag on empty space calls `startDrag`; on an arrow it doesn't. Double-click toggles maximize. Right-click opens the window menu.
- [ ] **Build:**
  - `AppTitleBar` per §5.1.
  - Swift: `bandHeight` 40, `bandWidth` 78, `lightsInset` 14. Windows' caption maths don't change: confirm `kCaptionHeight` stays 32 and say so in the report.
  - Onboarding and the keyring screen use it with neither slot.
- [ ] **Hand check owed:** macOS lights centred in the row, in the list for Task 22.
- [ ] Commit `feat(desktop): one title row`.

**Checks:** `window_chrome_test.dart`; `flutter analyze`.

**Checks for Gemini:** the Swift compiles only in CI, on the pull request's `Client (macOS)` job: read it at CP1.

### Task 6: Back, forward and the crumb

**Owner: Gemini. Score 3. Review focus:** a step never joins or leaves voice; things that have gone are skipped; the arrows' state follows. Spec §5.1.

**Files:** `navigation_history.dart`, `title_crumb.dart`, `shortcuts.dart`, tests `app/test/unit/shell/navigation_history_test.dart`, `app/test/widget/shell/title_crumb_test.dart`.

**Interfaces, produced:**

```dart
sealed class NavEntry {}  // ChannelEntry(server, channel), PeerEntry(peer), FriendsEntry(), RequestsEntry()
final navigationHistoryProvider = NotifierProvider<NavigationHistory, HistoryState>(…);
class HistoryState { List<NavEntry> entries; int index; bool get canBack; bool get canForward; NavEntry? backTarget; NavEntry? forwardTarget; }
class NavigationHistory { void back(); void forward(); }
class HistoryArrows extends ConsumerWidget {}
class TitleCrumb extends ConsumerWidget {}
```

- [ ] **Tests:**
  - `opening_channels_builds_history`; `back_then_forward_returns`; `going_somewhere_new_drops_what_is_ahead`; `reopening_the_same_place_adds_nothing`; `history_keeps_100`.
  - `going_back_to_a_voice_channel_does_not_join`: `voiceSessionProvider` unchanged, view opens. And it doesn't leave: back to a text channel stays in the call.
  - `going_back_skips_what_is_gone`: a deleted channel; a removed server. Both in both directions.
  - `the_first_resolved_channel_is_not_a_step`: a server's data arriving and picking its first channel replaces the entry rather than adding one.
  - `friends_and_requests_are_steps_tabs_are_not`.
  - `arrows_disable_at_the_ends`: colour `iconDisabled`, no tap.
  - `arrow_tooltip_names_the_target`: all forms in §5.1.
  - `mouse_side_buttons_step` and `alt_arrows_step`; on macOS `⌘[`, `⌘]`.
  - `crumb_shows_server_and_channel`: 20 px server icon radius 6; text styles and gap by measurement; each form: text channel, announcement, voice, private chat, Friends, Requests, no server.
  - `crumb_is_centred_and_clips`: a long name ends in an ellipsis, inside 56 % of the width.
- [ ] **Build:**
  - The notifier listens to `navigationProvider`, `currentChannelProvider` and `friendsTabProvider`, and stores a `NavEntry` for each user-made change.
  - Stepping calls `openChannel`, `openConversation`, `openFriends`, plus the tab for Requests, under a flag so it doesn't record itself.
  - Pointer buttons through a `Listener` at the shell's root.
  - `BackIntent`, `ForwardIntent` in `shellShortcuts` and `_actions`.
- [ ] Commit `feat(desktop): back and forward, and where you are`.

### Task 7: The shell

**Owner: Gemini. Score 3. Review focus:** every rectangle in §2.1 at every width; nothing lost from what `DesktopShell` did; the home view's hamburger. Spec §2, §5.8 (the header's frame only), §5.18.

**Files:** `desktop_shell.dart`, `shell_layout.dart`, `shell_drawer.dart`, `app/lib/ui/widgets/panel_header.dart`, `channel_sidebar.dart`, `home_sidebar.dart`, `chat_area.dart`, `chat_header.dart`, `home_main.dart`, `friends_page.dart`, `private_chat.dart`, `private_header.dart`, `member_panel.dart`, `music_panel.dart`, tests `app/test/widget/shell/floating_frames_test.dart`, `shell_test.dart`.

**Consumes:** `AppTitleBar`, `HistoryArrows`, `TitleCrumb`, `OcFloat`, `NotchedFloat` (notch 0 for now), `ShellKeys`.

**Interfaces, produced:**

```dart
class PanelHeader extends StatelessWidget {
  const PanelHeader({required Widget child, EdgeInsets padding, Color? color});  // 52 with its line; no drag
}
class ShellDrawer extends StatefulWidget {  // §5.18: replaces _Drawer
  const ShellDrawer({required bool fromRight, required VoidCallback onClose, required double width, required Widget child, double inset});
}
```

- [ ] **Tests:**
  - `frames_match_the_prototype`: at all seven breakpoints: rail, left, main, composer, and inside the left column head, me, against §2.1 with `rectMoreOrLessEquals(epsilon: 1)`. A table in the test, copied from `measure.json`.
  - `ground_shows_between_floats`: the shell's background is `rail`.
  - `header_is_52_with_its_line`; `headers_do_not_drag`.
  - `left_drawer_opens_from_the_hamburger`: compact: rect `78,40,285,652`; `+` the home view has one too.
  - `right_drawer_floats`: medium: rect `828,40,264,752`; scrim rect `0,40,w,h-40`; Esc and a scrim click both close; twice each.
  - `drawers_open_on_the_springs`: x at 40 and 120 ms against `OcSpring.pane`.
  - `smallest_window_with_everything_has_no_overflow`: 940 × 560, in voice, with reconnect bar, pinned message, Music drawer: `tester.takeException()` null; the channel list scrolls.
  - `reconnect_bar_floats`: margins `0 8 8`, height 40; on `pane`/`paneOut`.
  - Everything `shell_test.dart` checks that still applies.
- [ ] **Build:**
  - `DesktopShell`: `Column(AppTitleBar(leading: HistoryArrows(), center: TitleCrumb()), VoiceReconnectBar, Expanded(row of floats))`, on `rail`.
  - Keep its listeners, shortcuts, actions and `_openLinks`.
  - `ShellLayout`: `sidebarWidth` 288/264.
  - **All `HeaderBar`s in the shell become `PanelHeader`s.** Remove `leadingControls` and `trailingControls` from every widget in the Files list, and `showsTrailingWindowControls`' uses. The X in member and music panels always shows now.
  - The left column gets a head slot, a channels float, the voice panel and the user panel, each in its own `OcFloat`, with the numbers of later tasks left as they are.
  - The side slot: a plain `OcFloat` beside main for now, opened and closed by today's providers.
  - The composer and its replacements: an `OcFloat` under main.
- [ ] Render `text`, `text-pane-closed`, `homelab` and the drawers at every breakpoint. Run `compare.py`: **no line for `rail`, `left`, `head`, `me`, `main`, `composer`.**
- [ ] Commit `feat(desktop): every part floats`.
- [ ] **CP1.**

---

## Milestone M2: the right pane and the main page (CP2)

### Task 8: Side slot, notch, pane button, no rewrap

**Owner: Claude. Score 5.** The part the owner tuned by hand. Spec §4.3, §4.4, §5.9, §5.13's frame.

**Files:** `side_slot.dart`, `pane_button.dart`, `desktop_shell.dart`, `chat_area.dart`, `chat_header.dart`, tests `app/test/widget/shell/right_pane_test.dart`.

**Interfaces, produced:**

```dart
class SideSlot extends StatelessWidget { const SideSlot({required bool open, required double width, required Widget child}); }
class PaneButton extends ConsumerWidget { const PaneButton({required ValueListenable<double> notch, required bool open, required VoidCallback onOpen}); }
/// Lays its child out at the width main will end at (§4.4).
class MainInnerWidth extends StatelessWidget { const MainInnerWidth({required Widget child}); }
// ShellMetrics, an InheritedWidget from DesktopShell: mainInner, notch (ValueListenable<double>), layout.
```

- [ ] **Tests** (`t` from the click, tolerance 2 px or 0.03 unless it says, all against §4.3's tables):
  - `pane_opens_on_the_prototypes_curve`: side slot width, main width, notch size, button opacity and scale at each `t`.
  - `pane_closes_on_the_prototypes_curve`.
  - `people_to_music_grows_on_the_curve`.
  - `quick_open_and_close_ends_exact`: click, X after 60 ms, click after 60 ms, X after 20 ms; each change continuous within a frame's movement; at rest the widths are exact and the notch is 52 or 0, matching the slot.
  - `pane_opens_closes_opens`: twice over; final rects equal the table each time.
  - `notch_is_where_the_prototype_has_it`: at 1280 and 1100: `1220,40,52,52`, `1224,44,44,44`; button radii and surface.
  - `header_buttons_clear_the_notch`: `acts`' right edge 62 from main's right beside the button, 8 once it's gone.
  - `no_rewrap_while_the_pane_moves`: the message list's inner width at t = 40, 120, 360 is the final width; a `RenderParagraph`'s line count doesn't change after the first frame.
  - `slot_content_keeps_its_width`: the panel's width is 264 at t = 40.
  - `shortcut_still_toggles_people`, `music_command_opens_music`.
  - `no_button_in_private_messages`: and main has no notch there.
  - `resizing_across_1180_keeps_things_consistent`: open inline, shrink to 1100, grow back.
  - `reduced_motion_jumps`.
- [ ] **Build** per the spec's sections. The notch's `SpringValue` is `pane` rising, `paneOut` falling with `fallingDelay` 50 ms, and feeds `NotchedFloat`, `PaneButton` and the header's inset from one listenable. Remove the Members and Music buttons from `ChatHeader`. The slot keeps its child mounted only while open or moving; `ShellKeys.panel` is on that child.
- [ ] Render, then read `pane-open`, `pane-close`, `music-swap` frame by frame against `film/`.
- [ ] Commit `feat(desktop): the right pane, as the owner tuned it`.

**Checks:** `right_pane_test.dart`, `music_panel_test.dart`, `floating_frames_test.dart`.

**Checks for Gemini:** `compare.py` shows nothing for `main`, `side`, `notch`, `paneTab`, `composer` in `text` and `text-pane-closed`.

### Task 9: People and Music panes

**Owner: Gemini. Score 2.** Spec §5.13.

**Files:** `member_panel.dart`, `member_sections.dart`, `music_panel.dart`, `music_controls.dart`, `music_queue.dart`, `music_search.dart`, tests.

- [ ] **Tests:**
  - `switch_replaces_the_buttons`: `People` and `Music` where music is offered, `Members` and the count where it isn't; sizes and colours measured.
  - `switch_changes_the_pane_twice`.
  - `member_row_numbers`: height 44, avatar 34, gap 10, margins; offline at 0.55.
  - `section_padding`, `search_is_the_spec_pill`.
  - `music_numbers`: art height 172, title, progress height 4, transport gaps, play 48 inverted while playing.
  - Every existing `music_panel_test` behaviour stays.
- [ ] **Build** per §5.13. Remove `isOverlay`, `onSelectMembers`, `onSelectMusic`, and `RightPanelModeBar` if nothing uses it. `MusicTransport`'s extra modes stay in the row; if the row doesn't fit at 340, name it in your report rather than dropping any.
- [ ] Commit `feat(desktop): People and Music in the side slot`.

### Task 10: The main page

**Owner: Gemini. Score 3. Review focus:** the composer at rest is exactly 62 and level with the user panel; the pinned float never hides the first message; chats still start from the bottom. Spec §5.8, §5.10, §5.11, §5.12.

**Files:** `chat_header.dart`, `pinned_bar.dart`, `chat_area.dart`, `message_list.dart`, `chat_row_view.dart`, `message_rows.dart`, `bubble.dart`, `message_item.dart`, `composer_bar.dart`, `composer.dart`, `private_composer.dart`, `user_panel.dart`, tests.

- [ ] **Tests:**
  - `header_numbers`: title and subtitle styles; button sizes and gaps; typing line.
  - `pinned_float_numbers`: rect at 1280 `387,101,600,*`; surface; bar `2 × 30`; shadow.
  - `messages_clear_the_pinned_float`: scrolled to the top, the first message's top ≥ the float's bottom.
  - `day_pill_moves_under_the_pinned_float`, and back when it's closed.
  - `column_is_880_with_its_padding`: main wider than 880 centres it.
  - `short_chats_sit_at_the_bottom`: a channel with one message: its bottom is 12 from the list's bottom. The same in a private chat.
  - `bubble_numbers`: padding, radii when grouped, border on both kinds, mention edge.
  - `reply_quote_reaction_unread_system_numbers`.
  - `composer_rests_at_62`: exactly 62, and the user panel's top equals its top.
  - `composer_grows_upward_and_the_panel_stays_level`: type three lines; a reply and staged files sit inside; all resets after sending.
  - `recording_cover_fills_the_float`.
  - Every existing chat test's behaviour stays.
- [ ] **Build** per the sections. The pinned float goes in a `Stack` over the list; `MessageList` gets `topInset`. Both the list and any other growing content go through `MainInnerWidth`. Composer buttons at rest are centred: `_buttonInset` 12, `fieldPadding` vertical 20.5.
- [ ] Commit `feat(desktop): the main page's numbers`.
- [ ] **CP2.**

---

## Milestone M3: the left side (CP3)

### Task 11: Rail and popover

**Owner: Gemini. Score 3. Review focus:** dragging, right-click and keyboard still work with no names under the icons; the popover follows §5.2's motion exactly; voice faces come from each server's own data; it's gone whenever its item is. Spec §5.2.

**Files:** `server_rail.dart`, `rail_popover.dart`, tests `app/test/widget/channels/rail_test.dart`, `sidebar_test.dart`, `app/test/golden/sidebar_golden_test.dart`.

**Interfaces, produced:**

```dart
class VoiceFaces extends StatelessWidget { const VoiceFaces({required String serverKey, required List<int> userIds, required Map<int, String> names}); }  // up to 4, then +N
```

- [ ] **Tests:**
  - `rail_has_no_names`: no `Text` under an item; item height 52; list padding; 6 between.
  - `popover_shows_on_hover_and_focus`: 78 from the left, centred on the item; name and status; surface numbers; the arrow.
  - `popover_never_takes_the_pointer`: a click through it reaches main.
  - `four_faces_then_a_count`: 10 people: 4 avatars, `+6`, `10 in voice`. 4 people: 4 avatars and no `+`. None: no voice row. **A test that builds `VoiceFaces` with 10 ids,** and one through the real provider with the mock's General.
  - `each_server_shows_its_own_voice`: hover two servers.
  - `popover_appears_on_the_curve`: scale at 40 and 120 ms against `OcSpring.pop` from 0.96.
  - `moving_between_icons_slides_without_reappearing`: within 90 ms; y on `paneOut`; scale stays 1.
  - `leaving_hides_after_90_ms`, then in 80 ms.
  - `dragging_hides_it`; `removing_the_server_removes_it`.
  - `labels_stay_for_screen_readers`: `name, state`.
  - `private_and_add_have_their_lines`.
  - Rail reorder and menus stay: `sidebar_test`'s finders change per Rule 8.
- [ ] **Build:**
  - `RailPopover` in an `OverlayPortal` from the rail, so the float's clip doesn't cut it: one instance, fed `(id, y)` by each item's `Hoverable`; a 90 ms `Timer` cancelled in `dispose`.
  - Remove the `Tooltip`s and the name from `RailItemView`; keep `name`, for semantics and tests.
  - macOS's 52 px spacer goes.
- [ ] Commit `feat(desktop): the rail shows who is in voice`.

### Task 12: Server head and channel list

**Owner: Gemini. Score 2.** Spec §5.3, §5.4.

**Files:** `server_head.dart`, `channel_sidebar.dart`, `channel_row.dart`, `voice_channel_row.dart`, `key_hint.dart`, `channel_glyph.dart`, tests.

- [ ] **Tests:**
  - `head_is_its_own_float`: rect `78,40,288,52`; hover colour across the whole float.
  - `head_menu_is_as_wide_as_the_float`: at 288 and after resizing to 240; its y; the arrow turns over 140 ms and back when closed; twice.
  - `head_says_when_it_is_not_connected`: all status lines.
  - `search_pill_numbers`; `category_plus_only_on_hover`; `row_numbers` for channel and voice rows; `person_in_voice_numbers`.
  - Existing sidebar and reorder tests stay.
- [ ] **Build:** `showOcMenu` gains an optional `width`.
- [ ] Commit `feat(desktop): the server's head and the channel list`.

### Task 13: Splitter

**Owner: Gemini. Score 3. Review focus:** one gesture, one result: a drag writes the setting once, on release; the column follows the window's limit and comes back. Spec §5.7.

**Files:** `shell_splitter.dart`, `desktop_shell.dart`, tests.

**Interfaces, produced:** `sidebarWidthProvider`, `NotifierProvider<…, double?>`, key `ui.sidebarWidth`. Null means the class's default.

- [ ] **Tests:**
  - `drag_resizes_live_without_animation`: the rect follows the pointer on each pump; head, channels, voice card and me all follow; main shrinks.
  - `drag_stops_at_the_limits`: 232, and the computed maximum.
  - `written_once_on_release`: a fake store counts writes.
  - `remembered_across_launches`.
  - `double_click_resets_on_the_spring`.
  - `window_narrower_pulls_it_in_and_wider_restores_it`: on `pane`.
  - `keys_step_16_and_home_resets`; `focusable`, `semantics_is_a_slider`.
  - `pill_appears_on_hover`: 3 × 40; opacity and height over time.
  - `no_splitter_in_compact`.
- [ ] Commit `feat(desktop): drag the channel list wider`.

### Task 14: Voice card and user panel

**Owner: Gemini. Score 2. Review focus:** the card's gap appears with it, so nothing jumps; it shows its last channel while leaving. Spec §5.5, §5.6.

**Files:** `voice_panel.dart`, `user_panel.dart`, tests.

- [ ] **Tests:**
  - `card_numbers`: padding, line, toggles 36 with radius 10.
  - `card_leaves_on_the_curve`: height against §4.3's row; channels grows by what it gives up, gap included.
  - `card_joins_and_leaves_twice`.
  - `it_keeps_its_channel_while_leaving`.
  - `panel_numbers`: 62, padding, avatar, name, status, chevrons 14 × 28.
  - `muted_and_deafened_buttons_stay_active`; existing voice panel tests stay.
- [ ] **Build:** a `SpringValue` driving height through `ClipRRect` and `Align(heightFactor: …)`.
- [ ] Commit `feat(desktop): the voice card and you`.
- [ ] **CP3.**

---

## Milestone M4: the voice page (CP4)

### Task 15: Tiles

**Owner: Claude. Score 4.** Spec §5.14 (stage, tile, focus, motion).

**Files:** `app/lib/ui/motion/flip_box.dart`, `voice_view.dart`, `voice_tile.dart`, `voice_layout.dart`, tests `app/test/widget/voice/voice_stage_test.dart`, `app/test/unit/voice/voice_layout_test.dart`.

**Interfaces, produced:**

```dart
/// Paints its child moving from where it was to where it is, on OcSpring.move.
/// Arrives on OcSpring.tile when it has no earlier rectangle.
class FlipBox extends SingleChildRenderObjectWidget { const FlipBox({required Object generation, Duration arriveDelay, Widget? child}); }
```

- [ ] **Tests:**
  - `grid_is_16_by_10_in_at_most_1040`: 6 tiles at 1280: rects from `measure.json`'s `tiles`, within 1; 2 tiles at most 520.
  - `tile_numbers`: surface, speaking border, name pill, `LIVE`, avatar, watch button.
  - `focus_fills_above_the_strip`: rect `383,101,608,682` holds both; strip 76; each at least 96, at most 200; scrolls past that.
  - `focus_moves_on_the_spring`: a tile's painted rect at 40 and 120 ms against `OcSpring.move`.
  - `contents_fade_back_on_a_big_change`: opacity at 70 and 210 ms; none on a small change.
  - `tiles_arrive_staggered`: delays 0, 16, 32 … at most 80; on joining and on opening the view; **not** on a window resize or a pane opening.
  - `a_new_tile_arrives_and_the_others_move`: a camera turned on, a screen share started.
  - `video_keeps_its_state`: the `VoiceTileView`'s `State` and any texture are the same object before and after focusing.
  - `stage_ignores_the_panes_animation`: tile rects inside the stage don't change while the pane moves.
  - `wants_report_final_sizes`: `videoWantsProvider` gets the final size once, not one per frame.
- [ ] **Build:**
  - `FlipBox` measures in `paint`, so its first frame is already in the old place.
  - `GlobalObjectKey(tileId)` for each tile.
  - `fitTiles` at 16:10; the two clamps; the focus layout; `contain` in a focused tile.
  - `MainInnerWidth` around the stage.
- [ ] Commit `feat(desktop): tiles that move as the owner saw`.

**Checks:** both test files; `voice_view_test.dart`, `voice_media_test.dart`.

### Task 16: The dock

**Owner: Gemini. Score 3. Review focus:** it never hides under a popup it opened, under focus, or with the pointer on it; timers cancel on leaving the page. Spec §5.14 (the dock).

**Files:** `voice_dock.dart`, `voice_view.dart`, `voice_controls.dart`, `chat_area.dart`, `oc_icon_button.dart`, tests `app/test/widget/voice/voice_dock_test.dart`, `app/test/golden/voice_bar_golden_test.dart`.

**Interfaces, produced:**

```dart
class VoiceDockActivity extends InheritedNotifier<…> {}  // main float's MouseRegion feeds it
```

- [ ] **Tests:**
  - `dock_numbers`: height 64, centred in the main float, bottom edge 15 above the float's bottom edge (14 and its border); with the chat button its width is 517.7 within 2; buttons 48, line, Disconnect, on states.
  - `moving_shows_and_resting_hides`: hidden at 2.5 s, not before; a move at 2 s restarts it; twice over.
  - `opening_the_page_shows_it_first`.
  - `leaving_the_float_hides`, including over the header it stays, because the header is in the float.
  - `pointer_on_it_keeps_it`; `focus_in_it_keeps_it`: then hides after either ends.
  - `dock_stays_while_its_popup_is_open`: soundboard, screen picker, camera menu, then hides 2.5 s after it closes.
  - `hidden_it_takes_no_clicks`: a tile underneath gets the tap.
  - `shows_and_hides_on_the_curves`: §4.3's table, `pop` rising.
  - `press_scales_it`: 0.97 at 80 ms.
  - `not_joined_has_no_dock`, `dock_leaves_no_timer_behind`: `dispose` in a test with the timer running.
  - `narrow_page_scrolls_the_dock`.
- [ ] **Build:**
  - The `_ControlStrip` and `composerHeight` go.
  - `MouseRegion` around the main float, only on a voice page.
  - The dock waits for the `Future` of a popup it opens: a small counter `hold()`/`release()` passed to the buttons that open one.
- [ ] Commit `feat(desktop): the voice controls float in the page and step aside`.

### Task 17: Voice chat and join prompt

**Owner: Gemini. Score 3. Review focus:** both ways of showing the chat, and every way in and out. Spec §5.14 (join), §5.15.

**Files:** `chat_area.dart`, `desktop_shell.dart`, `voice_view.dart`, `voice_controls.dart`, tests.

- [ ] **Tests:**
  - `chat_beside_the_tiles_when_there_is_room`: at 1600: slot 380, its header, list and composer float, bottom level with the user panel; main keeps its tiles; opens on `pane`.
  - `chat_replaces_the_tiles_when_there_is_not`: at 1280: main holds the chat; composer float beneath.
  - `opening_the_right_pane_closes_the_chat`, as today, and the reverse.
  - `chat_button_toggles_twice`, from the dock and, when alone, from the header.
  - `join_prompt_numbers`, `join_prompt_rises_on_the_curve`.
  - `no_permission_note_stays`; `faces_stay`.
- [ ] Commit `feat(desktop): a voice channel's chat beside or instead of its tiles`.
- [ ] **CP4.**

---

## Milestone M5: private messages (CP5)

### Task 18: The private side

**Owner: Gemini. Score 2.** Spec §5.3 (Private), §5.12 (bars), §5.16 (tiles, rows, header).

**Files:** `home_sidebar.dart`, `home_tiles.dart`, `home_main.dart`, `private_chat.dart`, `private_header.dart`, `request_bar.dart`, `key_banners.dart`, tests.

- [ ] **Tests:**
  - `tiles_numbers`: grid, min height 92, icon tile 32, text; open state inverted; press 0.97.
  - `tiles_count_what_is_waiting`: `2 online`, badge; `2 waiting`, `All answered`.
  - `tiles_open_their_pages_twice`.
  - `conversation_row_numbers`; unread, hover close, menu stay.
  - `chat_header_numbers`: relay chip; lock.
  - `every_bottom_bar_is_a_float`: request, blocked, no shared server, not reachable, not connected: each in the composer's rectangle at least 62 tall.
  - `head_in_private_is_not_a_button`.
- [ ] Commit `feat(desktop): private messages float`.

### Task 19: Friends and Requests pages

**Owner: Gemini. Score 3. Review focus:** Accept and Decline work and show failures; both pages' empty states; a tile's page against what's in the tabs. Spec §5.16.

**Files:** `friends_page.dart`, `friend_rows.dart`, `person_card.dart`, `requests_page.dart`, tests `app/test/widget/home/friends_page_test.dart`, `requests_page_test.dart`.

- [ ] **Tests:**
  - `tabs_have_counts`: Online, All, Pending, Blocked; numbers change with events; `Message requests` isn't a tab; numbers on height, radius, on state.
  - `person_card_numbers`: and offline fades.
  - `friend_actions_work`: Message opens the chat, whichever conversation state; More opens today's menu.
  - `incoming_has_accept_and_decline`: both call the repository; a failure shows through `showPrivateFailure` and the row stays.
  - `sent_has_cancel`; `blocked_has_unblock`; `spam_fold_stays`, twice.
  - `requests_page_is_cards`: grid at 1280: 2 columns; card numbers; `via` from the relay's name; time; message from `preview`, or `Wants to message you` when empty.
  - `accept_opens_the_chat`; `decline_asks_first_then_removes`; `decline_cancelled_keeps_it`.
  - `a_click_on_the_card_opens_the_chat`.
  - `empty_says_so`, on each tab and on Requests.
  - `page_fades_in_tabs_do_not_slide`.
- [ ] **Build:**
  - `FriendsTab.requests` stays as state, and `FriendsPage` shows `RequestsPage` for it, with its own `PanelHeader`.
  - `friendsTabProvider` is still remembered.
  - Accept, Decline and their failures reuse `RequestBar`'s calls. Move the shared part to one function rather than copying it.
- [ ] Commit `feat(desktop): friends and message requests you can't miss`.
- [ ] **CP5.**

---

## Milestone M6: everything else, and the whole check (CP6)

### Task 20: Menus, popovers, dialogs

**Owner: Gemini. Score 2.** Reaches every menu in the app. Spec §5.17.

**Files:** `oc_menu.dart`, `popover.dart`, `popup_route.dart`, `oc_dialog.dart`, `oc_theme.dart`, tests `app/test/widget/widgets/menus_test.dart`, `app/test/golden/widgets_golden_test.dart`.

- [ ] **Tests:**
  - `menu_numbers`: surface; item 32, padding, radius 6, text 13.5; icon 18; divider margin `4 6`.
  - `menu_opens_on_the_curve`: §4.3's table, scale, opacity, offset.
  - `it_grows_from_the_corner_nearest_the_trigger`: alignment for a click in each quadrant.
  - `closing_is_instant_and_the_action_still_runs`: gone on the next frame; twice.
  - `keyboard_still_works`, `submenu_still_works`.
  - `popover_opens_the_same_way`; `popover_flips_as_today`.
  - `dialog_opens_on_pop`, `closes_in_100ms`.
  - `reduced_motion_is_instant`.
- [ ] **Build:** `OcPopupRoute` takes the spring and `reverseTransitionDuration` zero.
- [ ] Commit `feat(desktop): menus and popovers, spring and sizes`.

### Task 21: Everything left

**Owner: Gemini. Score 2.** Spec §5.19, §8.

**Files:** anything that still uses what's going; `header_bar.dart` removed; tests `app/test/widget/polish/floating_polish_test.dart`.

- [ ] **Tests:**
  - `large_text_keeps_the_frame`: at the font-size setting's largest: headers grow past 52 only as far as needed, composer and user panel stay level, no overflow.
  - `every_motion_respects_reduced_motion`: open the pane, a menu, the voice card, the dock with it on: `hasRunningAnimations` is false the next frame.
  - `no_servers_view_has_no_notch`; `onboarding_has_the_row`.
  - `monochrome_test` passes.
- [ ] **Sweep:**
  - `grep -rn "HeaderBar\|showsTrailingWindowControls\|trailingControls\|leadingControls\|RightPanelModeBar\|_ControlStrip" app/lib` prints nothing.
  - `grep -rn "Curves\.\|Duration(milliseconds" app/lib/features` prints nothing new beyond timers and non-motion durations: list what's left in your report.
- [ ] Commit `refactor(desktop): nothing left of the old frame`.

### Task 22: Docs and the hand-check list

**Owner: Gemini. Score 1.** Spec §7. The decisions entry is **Claude's**, at the last review, from the code: leave it.

**Files:** `docs/plans/desktop-ui.md`, `app/test/showcase/showcase_test.dart`, `docs/images/*`, `docs/plans/check-windows-by-hand.md`, `check-macos.md`.

- [ ] **`desktop-ui.md`:** the sections in §7, from the code, with this note at the top of §3: "Since D-next the app's frame is floating: `docs/superpowers/specs/2026-11-04-floating-ui-design.md`." Grep each name and number you write.
- [ ] **The showcase:** fix its finders, regenerate `docs/images`. Look at them first.
- [ ] **Hand checks** (added to the check docs):
  - **Windows:** caption buttons and snap layouts on the new row; dragging by the row; double-click; the splitter's cursor; text in the crumb at 125 % and 150 % scale.
  - **macOS:** traffic lights centred in the row, and back after full screen; arrows clear of them; ⌘[ and ⌘].
  - **Linux:** GNOME left-hand buttons; KDE; a tiling window (no shadow margin).
  - **All:** mouse side buttons; a real video and a real screen share through focus and back; a two-person call with the dock hiding.
- [ ] Commit `docs: the floating UI`.
- [ ] **CP6.** The full matrix: every scene, every breakpoint, dark at all of them, light at two. Mark the pull request ready only after Claude's scorecard passes. The owner merges.
