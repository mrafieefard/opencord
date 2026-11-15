# The phone catches up with the desktop: design

**Status:** the owner answered every open question on 2026-11-15 (§9). The plans are for later; nothing is built yet.

## 1. What the owner asked for

> "I want to implement every feature exist in desktop in mobile that can cross platformable. I first need a list of the features then writing plan for each." (2026-11-15)

> "You can also use the share button in server alongside bottom nav in iphone to act as Command and when user click on it, it shows a list of settings just for shortcut." (2026-11-15)

The second message sets how settings are reached on an iPhone: inside a server, the round glass button beside the tab bar (where the Search button sits on the servers list) becomes a **server button**. Tapping it opens a list of shortcuts into that server's settings and actions (§6.1). The owner, later the same day: "in iPhone you can use that button as shortcut to server settings or anywhere else that was just a tip. But keep in mind … I don't want to search in servers while I'm in a server."

## 2. How this list was made

- **The desktop:** every file under `app/lib/features`, the section headings of `docs/plans/desktop-ui.md`, and every public call of `opencord_core::Client` (`crates/opencord-core/src/client.rs`).
- **The phone:** every file under `mobile/src` and every export of `crates/opencord-mobile/src/api/*.rs`, including the core events the facade drops (`crates/opencord-mobile/src/events.rs`, `core_event` and `media_event`).
- **Checked against the tree on `feat/plugins-phone` at 039bce2** (plugins milestone 4), then against `beta` at 616ced8 after #46–#53 merged; #53's changes to the call controls and sheets are in P0's lessons and P5. The "Not there yet" table in `docs/mobile.md` is out of date: it still lists the soundboard, voice messages and commands as missing, and the phone has all three.

### What the phone already has (not planned again)

Onboarding and identity backup; servers (add, trust a fingerprint, retry, leave, unread and mention counts); channels and categories; chat (markdown with mentions, attachments, images in a zoomable viewer, files saved through the share sheet, edit, delete, typing, jump to latest); voice (join, mute, deafen, per-person volume and mute for you, the call sheet, background audio); the soundboard (play, preview, favourites); voice messages; music (player, queue, search, history, uploads, listening settings); chat commands; plugin channels and activities; the theme.

### Reactions, pins, replies, announcement channels and read states

The desktop draws them only for the mock repository: the real core turns all five off (`app/lib/core/rust/rust_repository.dart:75`). The server doesn't support them yet, so there is nothing to port. They are left out of every plan here.

## 3. What the phone is missing

Grouped as the plans are. "Facade" means the group needs new calls, records or events in `crates/opencord-mobile` before any screen. The facade drops these core events today: `RoleCreate/Update/Delete`, `PresenceUpdate`, `Stream*`, `ServerAddressesUpdate`, `NetworkStatus`, `Lavalink*`, `PrivateSettingsUpdate`, `StorageUpdate`, `VoiceRejoinFailed`, `StagedUploadProgress`, `DownloadProgress`, `Plugin*` management events. It also drops the media events `DevicesChanged`, `InputLevel`, `NoiseSuppressionFellBack`, `Video*`, `CameraStopped`, `ScreenShareStopped` and `ConnectionStats`. The core's separate `private_events()` stream doesn't cross at all.

| # | Group | What it holds | Facade work | Plan |
|---|---|---|---|---|
| P0 | **Foundation** | Roles and server-wide permissions in the snapshot and events; presence; the members list; a settings screen kit; the photo picker with crop; the server button (§6.1) | roles, presence, server permissions, `update_presence` | `2026-11-15-phone-p0-foundation.md` |
| P1 | **You and your profile** | Avatar; status (Online, Idle, Do not disturb); voice and audio settings (noise suppression, input and output volume, the microphone test with a level meter); sounds (soundboard volume, mute all soundboards, the voice sounds, which the phone doesn't play at all today); the attachment cache's size and Clear; session logs to export | avatar upload, audio settings, mic test, level meter, voice sounds, cache size, logs | `2026-11-15-phone-p1-you.md` |
| P2 | **Chat, the rest of it** | `@`, `#` and `:` autocomplete in the composer; an emoji picker; the members list and a member's profile from the chat; videos and audio files playing in the chat; text file previews; code highlighting; the reconnecting banner; creating and sharing an invite; a server whose certificate changed; channels and people in Search; text chat in voice channels | invites, download progress, a changed identity | `2026-11-15-phone-p2-chat.md` |
| P3 | **Private messages and friends** | Friends, requests, blocking; private chats with edit, delete, retry, typing, read and mute; moving a chat to another server; the chat-key words, verifying a person, approving this phone as one of your devices; privacy settings | the whole private API and its event stream | `2026-11-15-phone-p3-private.md` |
| P4 | **Running a server** | Overview (name, description, icon); channels (create, edit, delete, reorder, permissions); roles (create, edit, reorder, permissions); members (roles, kick, ban); invites; bans; voice and video settings; upload settings; the private message policy; commands on and off; soundboard sounds (upload with trim, rename, delete) | every admin call | `2026-11-15-phone-p4-admin.md` |
| P5 | **Moderating a call** | Server mute and deafen, moving someone to another channel, disconnecting them; the priority speaker; the connection's ping and quality; rejoining after a failed reconnect | voice admin calls, `ConnectionStats`, `VoiceRejoinFailed` | `2026-11-15-phone-p5-call-moderation.md` |
| P6 | **Plugins, managed** | The Plugins page: install a package from Files, review it, permissions, config, run and stop, restart, uninstall, the log | every plugin management call and event | `2026-11-15-phone-p6-plugins-admin.md` |
| P7 | **Camera and screen sharing** | The camera in a call, sharing the phone's screen, watching other people's streams | a native video bridge; the core takes encoded video from and gives it to native code (§7.2) | `2026-11-15-phone-p7-video.md` |
| P8 | **Notifications** | Notifications for mentions and private messages while the app is running or in a call; muting servers and channels; mentions only (notifications while the app is closed are skipped, §7.3) | none | `2026-11-15-phone-p8-notifications.md` |

### Hosting settings (skipped for now, the owner, 2026-11-15)

`Domains & network` (domains, tunnels, ports, certificates, DNS providers), `Music` (Lavalink nodes, local songs) and storage (S3 back ends, moving files) run the server's machine. They work across platforms, but they are long forms with tests that take minutes, and a mistake can take the server offline. **The owner: skip them for now.** They stay desktop only, shown in the server button's list as a row that says `Open on a computer`. They could become a P9 later.

### Not on the desktop either

The core has `update_nickname`, but the desktop never calls it: there's no nickname editor to port. Push to talk and lowering other apps need a key or a system API phones don't offer the same way, so P1 leaves them out (Claude's call; a hold-to-talk button in the call could be a later idea). The desktop's `Font size` and `Reduce motion` are the phone system's own settings, which the app already follows.

### Desktop only (not planned)

The tray, window controls, global hotkeys and push-to-talk keys, start at login, the updater and the install folder, the Mac menu bar, the pop-out media window, choosing which screen or window to share, keybinds, plugin dev tokens, and the desktop's debug menu.

## 4. Goals, non-goals and anti-goals

**Goals:** a member who owns only a phone can do everything the desktop lets them do in §3, with the desktop's words, permissions and limits.

**Non-goals:** new features the desktop doesn't have; changing the desktop; the server's protocol; notifications while the app is closed (skipped by the owner).

**Anti-goals** (failure even if it works):
- **A desktop port.** Settings drawn as the desktop's dialog: tabs holding one button, set-once switches in the open, forms wider than a thumb. The owner wants Apple and Material feel, with small details right (memory, 2026-10-29).
- **The same data showing differently** on the phone and the desktop: a permission the phone lets someone use that the server refuses, words that differ, counts that disagree.
- **Admin by accident:** destructive actions (kick, ban, delete a channel or role, uninstall a plugin) one tap away without a confirmation in the system's own style.
- **Each plan inventing its own settings screen,** member row or photo picker. P0 builds them once.

## 5. Constraints

- **Every change is a pull request into `beta`** (CLAUDE.md; memory 2026-10-29). One branch and one pull request per plan, from the branch §8 names.
- **The phone apps' rules hold:** ids cross as decimal strings; `CoreApi`, `NativeCore` and `FakeCore` change together; `mobile/scripts/check-bindings.sh` stays green; every screen is looked at in dark and light, on the phone and the tablet, reached by its real button (the M4 plan's Working Rules 1–10, copied into each plan).
- **iOS isn't run here.** CI compiles it. Each plan lists its iPhone hand checks.
- **No new server or protocol work** except P8. If a task needs it, stop and ask.
- **The desktop's words.** Where the desktop says something, the phone says the same words.

## 6. How it looks

### 6.1 The server button (the owner's)

**iOS 26:** on the Servers tab the glass button beside the tab bar is Search (`role="search"` in `mobile/src/app/(tabs)/_layout.tsx`). While a server's screen is in front, that slot shows the server's button instead: the system's `ellipsis` symbol in the same glass circle. Tapping it opens a sheet at medium height:

```
┌──────────────────────────────────────┐
│  [icon]  Tickets HQ                  │
│          12 members · 3 online       │
├──────────────────────────────────────┤
│  Invite people                     › │
│  Mark as read                        │
│  Mute server                    [ ] │   (P8)
├──────────────────────────────────────┤
│  Overview                          › │   only with the right permissions (P4)
│  Channels                          › │
│  Roles                             › │
│  Members                           › │
│  Invites · Bans                    › │
│  Voice & video · Soundboard        › │
│  Commands · Private messages       › │
│  Plugins                           › │   (P6)
│  Hosting       Open on a computer    │
├──────────────────────────────────────┤
│  Leave server                        │   red
└──────────────────────────────────────┘
```

- **While a chat is in front the tab bar is hidden** (`hidden={chatInFront}` in `(tabs)/_layout.tsx`), so the button isn't there. Then the sheet opens from the server's name at the top of the channel sidebar, on both systems.
- **No search inside a server** (the owner): the button replaces Search there, and nothing else in a server offers a search of servers. Search stays on the Servers tab.
- **"Or anywhere else":** the owner allows the button for other shortcuts later; for now it's the server's list.
- Rows appear only with the permission the desktop asks for each page (`app/lib/features/settings/server_settings.dart`), so a member sees four rows and the owner sees all of them.
- Each row pushes its page in the sheet's own stack. A page grows the sheet to full height.
- **Claude's call: Android has no such slot.** There the same sheet opens from a `more_vert` button in the server's header, and from a long press on the server's name.
- **Claude's call:** the long press on a server in the servers list keeps its menu and gains `Settings`, which opens the same sheet.

### 6.2 Settings pages

- iOS: grouped inset lists (`UITableView` style). Android: Material 3 list items with section headers. Built once in P0 as `SettingsList`, `SettingsSection` and `SettingsRow` (text, value, switch, slider, choice, link, destructive), using the `mobile/src/ui` controls that already pick per platform (`Switch`, `Slider`, `Segmented`, `Menu`, `confirm`).
- **The save rule:** a switch or a choice takes effect when it's touched, as iOS Settings does, and shows a toast if the server refuses it. Text fields (names, descriptions, topics) save with a `Save` button in the header, enabled when the text changed. **Claude's call:** this replaces the desktop's save bar, which doesn't fit a phone.
- Destructive actions confirm with the system's own dialog (`ui/confirm`), with the desktop's words.

### 6.3 Where things open

| What | Opens from |
|---|---|
| The members list | the chat header's people button (new), as a sheet; on the tablet, a column beside the chat |
| A member's profile | a tap on a name or avatar in the chat, the members list or a call (it already opens from a call) |
| Private messages | a new **Messages** tab between Servers and You, with a badge |
| Your profile, status, voice, sounds, privacy | the You tab, as rows that push pages |
| A server's settings | the server button (§6.1) |

## 7. Approaches

### 7.1 How settings are reached (decided by the owner)

1. **The server button** (the owner's, §6.1), with a header button on Android.
2. A settings tab inside each server: rejected, because a tab bar inside a tab bar isn't how either system works.
3. A cog in the channel list's header: kept only as the Android fallback.

### 7.2 Camera and screen sharing (P7): B, decided for performance

The owner (2026-11-15): "Between A and B which one is lighter for phone performance is priority."

- **A. Rust through the systems' codecs.** It would reuse the desktop's pipeline, which turns frames into packed pictures in memory (`crates/opencord-media/src/video/picture.rs`), converts and scales them on the CPU (`video/codec/convert.rs`, `scale.rs`), and hands every tile RGBA rows (`FrameSink::present` in `crates/opencord-core/src/video.rs`). On a phone that's a full-size copy and a colour conversion per frame per tile, which is heat and battery.
- **B. Native capture, encode, decode and draw (chosen).** The camera feeds the hardware encoder's input surface directly; the hardware decoder draws straight into the tile's view (`SurfaceView` on Android, `AVSampleBufferDisplayLayer` on iOS). No frame is ever copied to the CPU, converted, or crosses into Rust or JavaScript. Rust keeps what it does best and already has: the transport, which takes encoded H.264 per layer (`Transport::send_video(VideoFrame)`) and gives back assembled pictures (`ReceivedVideo`), and its decisions (which layers to send and at what bitrate, keyframe requests, which tiles get which layer: `VideoSender::encode`, `keyframe`, `video/priorities.rs`). Only small encoded packets cross between native code and Rust, through a small C interface (P7 Task 1), never through JavaScript.
- **C. OpenH264 in software:** rejected; it's the heaviest of the three.

The cost of B is two native pipelines (Kotlin and Swift) beside the desktop's, and a few core changes so the core can drive an outside encoder and decoder (P7 says which). iOS screen sharing needs a Broadcast Upload Extension, which is native code in any of the three.

### 7.3 Notifications (P8): only while the app runs

The owner (2026-11-15): "Skip offline notification." P8 notifies while the app is in the foreground, or in the background during a call (when the phone keeps it running). Waking a closed app needs Apple's and Google's push services and a relay to hold their credentials; that's left out, and the server and protocol don't change.

### 7.4 Videos in the chat

The desktop plays them through FFmpeg in the core (`media_open`). The phone has no FFmpeg. **Claude's call:** the phone downloads the file to the attachment cache (it already does for images, `attachment_file`) and plays it with the system's player (`expo-video`: AVPlayer, ExoPlayer). It's better on a phone than the desktop's player, and it needs no Rust.

## 8. Order and dependencies

```
P0 Foundation ── P1 You ── P2 Chat ── P3 Private ── P4 Admin ──┬── P5 Call moderation
                                                               └── P6 Plugins admin
P8 Notifications: after P1 (status) and P3 (private messages)
P7 Video: any time after P0; it touches the core, so not alongside P4's facade work
```

Each plan branches from the one before it, so the order is a chain. The real dependencies are fewer: P1, P2 and P4 need only P0; P5 needs P4's permission sets; P6 needs P4's pages; P8 needs P1 and P3. 

- **P0 first:** every other plan uses its roles, permissions, members list, settings kit and server button.
- **Then P1 and P2:** the most people see them every day, and both are small.
- **Then P3, P4, P5, P6** in that order.
- **P7 and P8 last,** after their designs.
- **Base branch:** plugins milestones 1–4 are merged into `beta` (#46–#52, and #53, 2026-11-15). P0 branches from `beta`; each later plan branches from the one before.

## 9. The owner's answers (2026-11-15)

1. **The server button:** yes, the iPhone's button beside the tab bar is the shortcut to the server's settings (and may hold other shortcuts later). No search of servers while in a server.
2. **Who builds:** Gemini, with tasks scored 0–5: 1–3 Gemini, 4–5 Claude through the bridge (P0's rubric).
3. **Base branch:** the plugin pull requests are merged; plans are written on a new branch from `beta`, and P0 branches from `beta`.
4. **Notifications while the app is closed:** skipped.
5. **Hosting settings on the phone:** skipped for now.
6. **Video:** whichever is lighter for the phone: B (§7.2).
7. **Private messages:** now, in P3. The Anchor (account sync) is built after this plan.

## 10. Claude's calls (for the owner to check)

1. The server button opens a sheet with permission-gated rows (§6.1); on Android, a header button.
2. Settings save on touch, and text saves with a header `Save` (§6.2).
3. A Messages tab for private messages (§6.3).
4. Videos in the chat play through the system's player, not the core's (§7.4).
5. Hosting settings stay desktop only for now (§3, the owner's).
6. Reactions, pins, replies, announcements and read states stay out until the server has them (§2).
7. P0 branches from `beta` (§8).
