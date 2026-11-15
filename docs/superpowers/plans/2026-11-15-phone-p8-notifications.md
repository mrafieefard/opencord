# Phone P8: Notifications — Plan

> **The owner, 2026-11-15: skip notifications while the app is closed.** This plan covers only the app running in the foreground, or in the background during a call. There's no push relay and no server change.

**Goal:** The phone tells you about mentions and private messages as the desktop does, with per-server and per-channel mute and "mentions only", while the app runs (in the foreground, or in the background during a call).

**Architecture:**
- **The rules are the desktop's** (`app/lib/features/desktop/notification_policy.dart`): nothing in do not disturb (P1's status) or with notifications off; in muted channels and with "mentions only", just mentions; a private message or friend request unless that chat is on screen. They're pure functions, ported with their tests.
- **Local notifications while the app runs.** The app is alive in the foreground, and in the background while in a call (Android's voice service, iOS's audio background mode). New messages there go through `expo-notifications` as local notifications. Tapping one opens the channel or the private chat.
- **Not here:** waking a closed app (Apple's and Google's push services and a relay); skipped by the owner.

**Tech Stack:** as P0. **New dependency:** `expo-notifications` (Expo's own, ~58).

**Spec:** design §3 (P8), §7.3. Desktop: `notification_policy.dart`, `notifications.dart`, `app/lib/core/settings/local_prefs.dart` (the mute and "mentions only" settings), docs/plans/desktop-ui.md §8.1 (Notifications) and §15.

## Global Constraints

- **Branch:** `feat/mobile-features`, the one branch and **one pull request into `beta`** for every phone plan (the owner, 2026-11-15). It already holds the design and the plans. Work on it in order, P0 first; never on a branch of its own. Never merge; never push to `beta`, `canary` or `main`. The pull request is open already (P0 opened it): push to it, add this plan's section to its description, and read its checks at each stage end.
- **Scope:** `mobile/`, `docs/`. No facade, server or protocol change (messages and private events already reach JavaScript). If one seems needed, stop and ask.
- **Never put message text in a notification** on the lock screen unless the user turns on `Show message previews` (default off, Claude's call): the title is the server and channel, or the person; the body `New message` or `Mentioned you`. A private message's text never leaves the phone's own process.
- P0's Global Constraints from "Ids are decimal strings" on apply.

## Working Rules

P0's apply word for word. **Ledger:** `.superpowers/sdd/phone-p8/progress.md`.

Scores (the owner, 2026-11-15): **0** no code; **1–3 Gemini**; **4–5 Claude**, handed over with claude-bridge `delegate_task`, each a cold session, so the task's section is self-contained. The rubric is P0's "Task owners". A Claude task runs first in its stage where its dependencies allow (memory: "my tasks come first"); the order is under the table.

| Task | Score | Owner | Why |
|---|---|---|---|
| 1 The rules, ported | 2 | Gemini | A port with the desktop's tests |
| 2 Mute and mentions only | 2 | Gemini | A port of `NotificationPrefs` and two menu items |
| 3 Local notifications | 4 | Claude | Platform code: Android notification channels, iOS background delivery during a call, routing a tap into a cold app |
| 4 Settings and the server sheet's row | 2 | Gemini | Switches and the permission state |
| 5 Live checks, docs, D-entry, PR | 2 | Gemini | Running the checks and writing from `git diff` |

**Order:** **3** needs 1 and 2: Gemini does 1 and 2 first, then hands over 3, then 4.

## Review Focus

1. **The chat is on screen.** Expected: no notification for it, as the desktop's "focused window" rule; the phone's own unread rules stay as they are. Pinned in Task 1.
2. **Do not disturb set on the phone** (P1's status). Expected: no notifications at all, but the badges still count. Pinned in Task 1.
3. **Tapping a notification after the app was in the background a long time.** Expected: it opens the right server and channel, or private chat, even if the server is still reconnecting. Pinned in Task 3: `a tapped notification opens its chat`.
4. **Many messages at once** (a reconnect delivers 200). Expected: one notification per channel, updated, not 200; nothing for messages older than the reconnect. Pinned in Task 3: `a burst becomes one notification per channel`.
5. **Notification permission refused.** Expected: the settings page says so with a button to the system settings; nothing else breaks. Pinned in Task 4.

## The tasks

### Task 1: The rules, ported

Create `mobile/src/features/notifications/policy.ts`: `messageNotice(message, context): 'none' | 'notify' | 'mention'` and `privateNotice(event, context): 'none' | 'notify'`, ports of `notification_policy.dart`'s functions with the same inputs (on screen instead of window focused). Port `app/test/unit/desktop/notification_policy_test.dart` and `private_notifications_test.dart`. Commit `feat(mobile): the desktop's notification rules`.

### Task 2: Mute and mentions only

Create `state/actions/notifyPrefs.ts`, a port of the desktop's `NotificationPrefs` (`app/lib/core/settings/local_prefs.dart:27`): muted servers and muted channels, a channel muted directly or through its server; and the app-wide `mentionsOnly` (default off, `app/lib/core/settings/app_settings.dart`). The channel list's long press gains `Mute channel` / `Unmute channel`; muted channels are drawn as the desktop's (dimmed, no unread dot, mentions still counted). Tests `a muted server mutes all its channels`, `mentions only lets mentions through`. Commit `feat(mobile): mute servers and channels`.

### Task 3: Local notifications

Create `features/notifications/notifier.ts` (subscribes to the store's message and private events, applies the policy, schedules with `expo-notifications`, groups by channel with a thread id, and clears a channel's when it's read) and `useNotificationTaps.ts` (routes a tap). Android: a `Messages` channel (default importance) and a `Mentions` channel (high), named so in the system's settings. Tests `a tapped notification opens its chat`, `a burst becomes one notification per channel`, `reading clears the channel's notifications`. On the emulator, in a call with the app in the background: a desktop mention shows. Commit `feat(mobile): notifications while the app runs`.

### Task 4: Settings and the server sheet's row

You → `Notifications`: `Notifications` (on/off), `Only mentions` (the desktop's `mentionsOnly`; copy its label from the desktop's settings page), `Show message previews`, and the system permission's state with `Open settings` when refused. The server sheet's `Notifications` row (design §6.1) becomes a `Mute server` switch, the desktop's only per-server choice. Tests for each control. Commit `feat(mobile): notification settings`.

### Task 5: Live checks, docs, D-entry, PR

Hand checks on the emulator (and owed on an iPhone): a mention in the foreground, in a call in the background, a private message, do not disturb. `docs/mobile.md` says notifications arrive only while the app runs or is in a call. The next D-number. `.github/check.sh`; push; PR; checks green; review; report.
