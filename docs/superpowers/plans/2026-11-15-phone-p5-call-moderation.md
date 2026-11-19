# Phone P5: Moderating a Call — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In a call on the phone, a moderator can server-mute, server-deafen, move and disconnect people and block them from music; anyone can see how their connection is doing, and get back into a call they were dropped from.

**Architecture:**
- **The actions sit on the member's profile** (P0's `MemberProfile`), which already opens from a tap in the call. The rules for who may do what are the desktop's `app/lib/features/members/member_actions.dart` (`_may`, the role hierarchy, the channel's permissions), ported as one pure function.
- **The facade gains the voice admin calls** and two dropped events: the media event `ConnectionStats` and the core event `VoiceRejoinFailed`.
- **The reconnect bar** is the desktop's `voice_reconnect_bar.dart` (D88): a float above the call bar while this device is out of a call it didn't leave.

**Tech Stack:** as P0. No new dependencies.

**Spec:** design §3 (P5). Desktop: `member_actions.dart`, `app/lib/features/voice/connection_info.dart`, `voice_reconnect_bar.dart`, and `app/lib/core/rust/rust_repository.dart:674` (how `VoiceRejoinFailed` reaches the bar).

## Global Constraints

- **Branch:** `feat/mobile-features`, the one branch and **one pull request into `beta`** for every phone plan (the owner, 2026-11-15). It already holds the design and the plans. Work on it in order, P0 first; never on a branch of its own. Never merge; never push to `beta`, `canary` or `main`. The pull request is open already (P0 opened it): push to it, add this plan's section to its description, and read its checks at each stage end.
- **Scope:** `crates/opencord-mobile/`, `mobile/src/`, `docs/`. No core, server, protocol or desktop change.
- **Priority speaker** (Claude's call): the desktop holds it with a key. On the phone, for those with `PRIORITY_SPEAKER` in the channel, a long press on the mute button holds it while the finger is down (`set_priority_speaker(true)` on press, `false` on release or cancel), with a haptic tick and the label `Priority speaker` over the button.
- P0's Global Constraints from "Ids are decimal strings" on apply.
- **Numbers follow the facade's rule** (`crates/opencord-mobile/src/types.rs:1-3`): ids are decimal strings; times (ms since the epoch), sizes, durations, positions and counters cross as `f64` through `convert::number` and `convert::unsigned`; a draft id comes back through `convert::draft_id`. No `u64` or `i64` field in a record JavaScript reads, whatever the core's type: a `u64` becomes a JavaScript `bigint` and breaks every comparison.

## Working Rules

P0's apply word for word. **Ledger:** `.superpowers/sdd/phone-p5/progress.md`.

Scores (the owner, 2026-11-15): **0** no code; **1–3 Gemini**; **4–5 Claude**, handed over with claude-bridge `delegate_task`, each a cold session, so the task's section is self-contained. The rubric is P0's "Task owners". A Claude task runs first in its stage where its dependencies allow (memory: "my tasks come first"); the order is under the table.

| Task | Score | Owner | Why |
|---|---|---|---|
| 1 Facade: voice admin calls, stats, rejoin | 2 | Gemini | Calls and two events in the facade's pattern |
| 2 The JavaScript core and the fake | 2 | Gemini | A fake with a timer |
| 3 `mayActOn`, the rules | 3 | Gemini | Extends P4's port with the voice branches. Focus: every voice branch named; P4's cases untouched |
| 4 Actions on a profile in a call | 2 | Gemini | Buttons over Task 3's answer |
| 5 Priority speaker | 4 | Claude | Real-time: a held state in the audio pipeline that must be released on every way out (gesture cancel, background, leaving), on a SwiftUI button on iOS |
| 6 Connection | 2 | Gemini | A sheet and a sparkline from one event |
| 7 The reconnect bar | 3 | Gemini | State kept across an app kill. Focus: once per cut; nothing after a deliberate leave |
| 8 Live checks, docs, D-entry, PR | 2 | Gemini | Running the checks and writing from `git diff` |

**Order:** Stage B: **5**, then 4, 6, 7.

Stages: A = 1–3, B = 4–7 with a review, C = 8. Commands as P0.

## Review Focus

1. **The member leaves, or moves, while their menu is open.** Expected: the menu closes, or the action is refused by the server and shows its error; never a stale action on the wrong channel. Pinned in Task 4: `an action on someone who left is not sent`.
2. **Priority speaker held while the app goes to the background or the call drops.** Expected: it's released (`false` sent) on every way out: release, cancel, background, leaving the call. Pinned in Task 5: `priority speaker is released on every way out`.
3. **`Move to`.** Expected: only voice channels the moderator may move into and the member may connect to, as `member_actions.dart:160-170`; the member's own channel isn't offered. Pinned in Task 3's table test.
4. **The app is killed in a call and reopened.** Expected: the bar offers `Reconnect` with `Opencord closed while you were in <channel>.`, once; `Dismiss` forgets it. Pinned in Task 7: `a call cut by the app closing offers reconnect once`.
5. **Stats arriving after the call ended,** or for another server. Expected: ignored; the Connection sheet shows `Waiting for the first measurement.` in a new call. Pinned in Task 6: `stats from an old call are ignored`.

---

### Task 1: The facade

**Files:** modify `crates/opencord-mobile/src/api/voice.rs`, `src/types.rs`, `src/events.rs`, `src/convert.rs`.

**Interfaces (produces):** `server_mute(server_key, user_id, value)`, `server_deafen(server_key, user_id, value)`, `move_member(server_key, user_id, channel_id)`, `disconnect_member(server_key, user_id)`, `priority_speaker_set(held: bool)` (`set_priority_speaker`), `music_blocks(server_key) → Vec<MusicBlockRecord>`, `block_from_music(…)` with the core's arguments (`crates/opencord-core/src/music.rs:301-320`; read them and its unblock counterpart). Events: `MobileEvent::ConnectionStats { server_key, channel_id, stats: VoiceStatsRecord { ping_ms: Option<u32>, jitter_ms: Option<f32>, loss_in_percent: Option<f32>, loss_out_percent: Option<f32>, bitrate_in: f64, bitrate_out: f64, voice_node: String } }` and `MobileEvent::VoiceRejoinFailed { server_key, channel_id, message }`; both leave the dropped lists.
- [ ] Tests: `stats_reach_javascript`, `a_rejoin_failure_reaches_javascript`, `voice_admin_ids_are_strings`. Run (fail), implement, run (pass); clippy; bindings; commit `feat(mobile): voice moderation calls and connection stats in the facade`.

### Task 2: The JavaScript core and the fake

`CoreApi`, `NativeCore`, `FakeCore` (the fake's call has stats once a second with a ping between 30 and 90 ms; `serverMute` and the others change the fake's voice states and emit them). Test `the fake's moderation changes voice states`. Commit `feat(mobile): the core API and the fake for call moderation`.

### Task 3: `mayActOn`, the rules

**Files:** modify `mobile/src/features/members/mayActOn.ts` (made by P4 Task 10 for kick, ban and roles); test `__tests__/mayActOn.test.ts`.

**Interfaces (produces):** `mayActOn` gains `serverMute`, `serverDeafen`, `move: string[]` (the channel ids offered), `disconnect` and `blockFromMusic`, the rest of `member_actions.dart`'s `_may`: never on yourself or the owner; voice actions need the permission in the member's current channel (P4's overwrites included) and the member in voice.
- [ ] A table test with a case for every branch of `_may` (read the desktop file; name each case after its branch). P4's cases still pass unchanged. Run (fail), implement, run (pass); commit `feat(mobile): the voice actions join mayActOn`.

### Task 4: Actions on a profile in a call

**Files:** modify `features/members/MemberProfile.tsx`, `features/voice/ParticipantSheet.tsx`; create `features/members/MoveToSheet.tsx`.

- Buttons below the profile, as the desktop's menu words: `Server mute` / `Server deafen` as switch rows (on when set), `Move to` (a list of the offered channels), `Disconnect`, `Block from music` / `Unblock` (confirm `Block <name> from music?`, button `Block`). Each only when `mayActOn` allows it.
- [ ] Tests: `an action on someone who left is not sent`, `each action sends its call`, `move offers only the allowed channels`. Implement; screenshots from a call on the fake; commit `feat(mobile): moderate people in a call`.

### Task 5: Priority speaker

**Files:** modify `features/voice/ControlBar.tsx` (Android) and `features/voice/ControlBar.ios.tsx` (the SwiftUI glass buttons since #53: the long press is the SwiftUI button's own `onLongPressGesture`, reported to JavaScript as press-down and press-up; add those two events to the native button if `@expo/ui` doesn't give them, with a ruling); create `features/voice/usePrioritySpeaker.ts`.

- As the Global Constraints say. Released on finger up, gesture cancel, `AppState` leaving `active`, leaving the call, and the component unmounting.
- [ ] Tests: `priority speaker is released on every way out` (one case per way), `only with the permission`. Implement; on the emulator with the desktop in the call, hold and see the desktop lower others (ledger it); commit `feat(mobile): hold for priority speaker`.

### Task 6: Connection

**Files:** create `features/voice/ConnectionSheet.tsx`, `features/voice/Sparkline.tsx`, `state/voiceStats.ts` (the last 300 samples of the current call, dropped on leave); modify `CallSheet.tsx` (a signal-bars button in the call's header, its bars from the desktop's `connection_info.dart` thresholds).

- The sheet, the desktop's words: title `Connection`; `Average ping`, `Last ping`, `Packet loss rate`, `Jitter`, the voice node; a sparkline of the ping with the desktop's caption (`Ping over the last five minutes, …`, whole); `Waiting for the first measurement.` before the first sample.
- [ ] Tests: `stats from an old call are ignored`, `bars follow the desktop's thresholds` (table). Implement; screenshots; commit `feat(mobile): the connection's quality in a call`.

### Task 7: The reconnect bar

**Files:** create `features/voice/ReconnectBar.tsx`, `state/actions/rejoin.ts`; modify `features/voice/ConnectedBar.tsx`'s host (the bar sits where the call bar would), `state/actions/voice.ts` (remember the call in prefs while in one; clear it on a deliberate leave).

- Three causes, the desktop's words: `VoiceRejoinFailed` → the core's message; the call moved to another device (the server's voice state for this user appears in a channel this device didn't join) → `Your call in <channel> moved to another device.`; the app closed in a call (a remembered call at boot) → `Opencord closed while you were in <channel>.`. Buttons `Reconnect` (`voiceJoin`) and `Dismiss`.
- [ ] Tests: `a call cut by the app closing offers reconnect once`, `a deliberate leave offers nothing`, `a rejoin failure shows the core's message`. Implement; screenshots; commit `feat(mobile): get back into a dropped call`. Stage B's end and review.

### Task 8: Live checks, docs, D-entry and the pull request

- [ ] With the desktop in the call as a plain member: server-mute, deafen, move and disconnect it from the phone; hold priority speaker; kill the phone app mid-call and reopen; ledger each.
- [ ] `docs/mobile.md` "Voice"; the iPhone hand checks (the long press on mute, the bar after the app is killed).
- [ ] The next D-number: the long press for priority speaker, the rule port. From `git diff`.
- [ ] `.github/check.sh`; push; PR description; `gh pr checks` green; review round; report.
