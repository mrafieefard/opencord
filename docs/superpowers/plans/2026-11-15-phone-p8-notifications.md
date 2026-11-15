# Phone P8: Notifications — Plan

> **Stage A is a task-level plan; Stage B is an outline** that stops at the owner's decision (design §9 Q4). Stage B is written as tasks after the answer.

**Goal:** The phone tells you about mentions and private messages as the desktop does, with per-server and per-channel mute and "mentions only", first while the app runs, then (Stage B) while it's closed.

**Architecture:**
- **The rules are the desktop's** (`app/lib/features/desktop/notification_policy.dart`): nothing in do not disturb (P1's status) or with notifications off; in muted channels and with "mentions only", just mentions; a private message or friend request unless that chat is on screen. They're pure functions, ported with their tests.
- **Stage A: local notifications while the app runs.** The app is alive in the foreground, and in the background while in a call (Android's voice service, iOS's audio background mode). New messages there go through `expo-notifications` as local notifications. Tapping one opens the channel or the private chat.
- **Stage B: push while the app is closed** needs one of design §7.3's options.

**Tech Stack:** as P0. **New dependency:** `expo-notifications` (Expo's own, ~58).

**Spec:** design §3 (P8), §7.3. Desktop: `notification_policy.dart`, `notifications.dart`, `app/lib/core/settings/local_prefs.dart` (the mute and "mentions only" settings), docs/plans/desktop-ui.md §8.1 (Notifications) and §15.

## Global Constraints

- **Branch:** `feat/phone-notifications`, from the last finished phone plan (it needs P1's status and, for private messages, P3). Draft PR into `beta`, stacked.
- **Scope (Stage A):** `mobile/`, `docs/`. No facade change is expected (messages and private events already reach JavaScript). Stage B changes the server and the protocol: stop and ask before any of it.
- **Never put message text in a notification** on the lock screen unless the user turns on `Show message previews` (default off, Claude's call): the title is the server and channel, or the person; the body `New message` or `Mentioned you`. A private message's text never leaves the phone's own process.
- P0's Global Constraints from "Ids are decimal strings" on apply.

## Working Rules

P0's apply word for word. **Ledger:** `.superpowers/sdd/phone-p8/progress.md`.

| Task | Score | Owner |
|---|---|---|
| 1 The rules, ported | 2 | Gemini |
| 2 Mute and mentions only | 3 | Gemini (focus: a muted server mutes all its channels) |
| 3 Local notifications | 4 | Claude |
| 4 Settings and the server sheet's row | 2 | Gemini |
| 5 Live checks, docs, D-entry, PR (Stage A) | 2 | Gemini |

## Review Focus

1. **The chat is on screen.** Expected: no notification for it, as the desktop's "focused window" rule; the phone's own unread rules stay as they are. Pinned in Task 1.
2. **Do not disturb set on the phone** (P1's status). Expected: no notifications at all, but the badges still count. Pinned in Task 1.
3. **Tapping a notification after the app was in the background a long time.** Expected: it opens the right server and channel, or private chat, even if the server is still reconnecting. Pinned in Task 3: `a tapped notification opens its chat`.
4. **Many messages at once** (a reconnect delivers 200). Expected: one notification per channel, updated, not 200; nothing for messages older than the reconnect. Pinned in Task 3: `a burst becomes one notification per channel`.
5. **Notification permission refused.** Expected: the settings page says so with a button to the system settings; nothing else breaks. Pinned in Task 4.

## Stage A — While the app runs

### Task 1: The rules, ported

Create `mobile/src/features/notifications/policy.ts`: `messageNotice(message, context): 'none' | 'notify' | 'mention'` and `privateNotice(event, context): 'none' | 'notify'`, ports of `notification_policy.dart`'s functions with the same inputs (on screen instead of window focused). Port `app/test/unit/desktop/notification_policy_test.dart` and `private_notifications_test.dart`. Commit `feat(mobile): the desktop's notification rules`.

### Task 2: Mute and mentions only

Create `state/actions/notifyPrefs.ts`, a port of the desktop's `NotificationPrefs` (`app/lib/core/settings/local_prefs.dart:27`): muted servers and muted channels, a channel muted directly or through its server; and the app-wide `mentionsOnly` (default off, `app/lib/core/settings/app_settings.dart`). The channel list's long press gains `Mute channel` / `Unmute channel`; muted channels are drawn as the desktop's (dimmed, no unread dot, mentions still counted). Tests `a muted server mutes all its channels`, `mentions only lets mentions through`. Commit `feat(mobile): mute servers and channels`.

### Task 3: Local notifications

Create `features/notifications/notifier.ts` (subscribes to the store's message and private events, applies the policy, schedules with `expo-notifications`, groups by channel with a thread id, and clears a channel's when it's read) and `useNotificationTaps.ts` (routes a tap). Android: a `Messages` channel (default importance) and a `Mentions` channel (high), named so in the system's settings. Tests `a tapped notification opens its chat`, `a burst becomes one notification per channel`, `reading clears the channel's notifications`. On the emulator, in a call with the app in the background: a desktop mention shows. Commit `feat(mobile): notifications while the app runs`.

### Task 4: Settings and the server sheet's row

You → `Notifications`: `Notifications` (on/off), `Only mentions` (the desktop's `mentionsOnly`; copy its label from the desktop's settings page), `Show message previews`, and the system permission's state with `Open settings` when refused. The server sheet's `Notifications` row (design §6.1) becomes a `Mute server` switch, the desktop's only per-server choice. Tests for each control. Commit `feat(mobile): notification settings`.

### Task 5: Live checks, docs, D-entry, PR

Hand checks on the emulator (and owed on an iPhone): a mention in the foreground, in a call in the background, a private message, do not disturb. `docs/mobile.md` says notifications arrive only while the app runs or is in a call, until Stage B. The next D-number. `.github/check.sh`; push; PR; checks green; review; report.

## Stage B — While the app is closed (outline)

Needs design §9 Q4. With **A (a relay run by Opencord)**, Claude's pick:
1. **The relay:** a small service (`crates/opencord-push-relay`, Rust, axum) holding the APNs key and the FCM service account, taking `{ device_token, platform, wake }` from servers, signed by the server's identity key and rate-limited per server. It sees no content and no user ids, only a token and that something happened.
2. **The server:** a `[push]` section (`relay_url`, off by default); members register device tokens (a new protocol message); on a mention or private message for an offline member with a token, the server asks the relay to wake that device.
3. **The phone:** registers its APNs/FCM token with each server; on a silent push, connects briefly in the background (iOS gives about 30 s), fetches what's new, and shows Stage A's notification. A private message is decrypted on the phone, never by the relay or the server.
4. **Operations:** who runs the relay, its domain, the Apple and Google accounts, and what self-hosters do who don't want it (option C: UnifiedPush on Android, nothing on iOS).

Questions for the owner: who runs the relay and under which Apple/Google accounts; whether servers may opt out of it; whether UnifiedPush should be offered on Android as well.
