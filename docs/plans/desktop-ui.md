# Opencord — Desktop UI Plan (Flutter)

> Companion to `opencord-phase1-plan.md`. This document defines **how the desktop client looks and behaves**. Mobile UI is out of scope and will get its own plan later.
>
> **Style in one sentence:** the calm consistency and straightforward layout of **Telegram Desktop**, with the feature set of the **Discord desktop client**, in a strictly **monochrome** dark or light theme.

---

## 0. Instructions for Claude Code

- Implement this as part of milestones **M5 and M6** of the Phase 1 plan. Before the Rust core is ready, build every screen against a **mock data layer** (see §11) so the UI can be developed and reviewed on its own.
- Follow the design tokens in §2 exactly. **No hard-coded colors anywhere** outside the token file. A lint-style check (§13) should enforce this.
- Target: **Linux desktop first** (Arch Linux on Hyprland, Wayland), then Windows and macOS. Mobile layouts are not part of this plan.
- Use Flutter's built-in widgets plus the packages from the Phase 1 plan (`flutter_riverpod`, `go_router`). Don't add UI kit packages (no `google_fonts`, no component libraries); fonts are bundled as assets. **Platform integration packages are allowed** where §3.1 and §15 call for them (window management, tray, deep links); check their latest versions and platform support before adding them, and prefer small native code in the platform runners over a package when a package can't do something properly.
- Performance matters: virtualized lists everywhere, no rebuilding the whole tree on every event, `const` widgets wherever possible, `RepaintBoundary` around message list items and video tiles.
- When something isn't specified here, pick the option that looks most like Telegram Desktop, and record it in `docs/decisions.md`.

---

## 1. Design principles

1. **Monochrome only.** Dark and light themes built from neutral greys, black and white. **No blue, orange, red, green or any other hue** — not for links, not for errors, not for presence, not for roles. Contrast, weight, shape and inversion do the work that color usually does. Phase 4 (Media uploads and voice messages) introduces two strictly scoped exceptions: the `voice` hue (`#4C8DFF` dark, `#2F6FEB` light) for voice message waveforms, and the `recording` hue (`#F2555A` dark, `#DC3E42` light) for the recording indicator dot and cancellation state. Resting screens and all other UI elements remain strictly monochrome.
2. **Telegram consistency.** Same row heights, same radii, same paddings everywhere. Two-line list rows (title + preview + time + badge). Rounded chat bubbles. Calm, flat surfaces with no gradients and almost no shadows.
3. **Discord capability.** Server rail, channel categories, voice channels with live participants, member list grouped by role, roles and permissions editor, voice/camera/screenshare controls, quick switcher, rich context menus.
4. **Inversion = emphasis.** The strongest state (selected server, unread badge, primary button, active toggle, own reaction) is shown by **inverting** foreground and background (white on black in dark, black on white in light).
5. **Shape over color.** Presence, connection state, speaking, live streaming and mentions are all distinguishable without color (see §5.3).
6. **Keyboard first.** Everything important has a shortcut (§9).
7. **Fast and quiet.** Animations run on physical springs (`OcSpring`: `pane`, `paneOut`, `pop`, `tile`, `move`), never bouncy, and never block input. When reduced motion is enabled, all animations jump instantly (`Duration.zero`).

---

## 2. Design tokens

All tokens live in one `ThemeExtension` (`OcColors`) plus a spacing/radius/typography file. Widgets read them via `context.oc`.

### 2.1 Colors

| Token | Dark | Light | Used for |
|---|---|---|---|
| `rail` | `#0A0A0B` | `#E9E9EC` | Server rail background |
| `sidebar` | `#121214` | `#FFFFFF` | Channel sidebar, headers, member panel, composer bar |
| `chat` | `#0E0E10` | `#F3F3F5` | Message area background |
| `surface` | `#18181B` | `#FFFFFF` | Cards, video tiles, settings sections |
| `elevated` | `#1F1F23` | `#FFFFFF` | Dialogs, menus, popovers |
| `hover` | `#1C1C20` | `#F2F2F4` | Hovered rows, input fills |
| `selected` | `#27272C` | `#E7E7EA` | Selected rows, hovered buttons, pills |
| `border` | `#222226` | `#E3E3E7` | 1 px dividers and outlines |
| `text` | `#EDEDEF` | `#111114` | Primary text and icons |
| `textSecondary` | `#A1A1A8` | `#55555C` | Secondary text, inactive icons |
| `textMuted` | `#6E6E76` | `#8A8A92` | Timestamps, hints, previews, section labels |
| `accent` | `#EDEDEF` | `#111114` | Inverted emphasis background (same as `text`) |
| `onAccent` | `#0E0E10` | `#FFFFFF` | Foreground on `accent` |
| `bubbleIn` | `#1A1A1E` | `#FFFFFF` | Incoming message bubble |
| `bubbleOut` | `#2A2A30` | `#E4E4E8` | Own message bubble |
| `mentionBg` | `#26262B` | `#E2E2E6` | Mention and inline-code highlight |
| `scrim` | `#000000` at 70% | `#000000` at 40% | Dialog barrier |
| `avatarShades` | `#2B2B30` `#34343A` `#3E3E45` `#494951` `#55555D` | `#D9D9DE` `#CDCDD3` `#C1C1C8` `#E3E3E7` `#B6B6BE` | Avatar backgrounds, picked by a hash of the user/server ID |

Rules:
- Emoji are user content and may render in color; that's the only exception.
- Text on `accent` always uses `onAccent`.
- Minimum contrast: body text ≥ 7:1 against its background, muted text ≥ 4.5:1.

### 2.2 Typography

- Font: **Inter** (bundled asset) for UI, **JetBrains Mono** (bundled) for code, keys and fingerprints. Fallback to system fonts.
- Scale:

| Style | Size / weight | Used for |
|---|---|---|
| `title` | 17 / 600 | Dialog and settings page titles |
| `header` | 15 / 600 | Server name, channel header, panel titles |
| `body` | 14 / 400, line height 1.35 | Messages, row titles |
| `bodyStrong` | 14 / 600 | Unread row titles, author names |
| `small` | 12.5 / 400 | Previews, subtitles |
| `meta` | 11 / 400 | Timestamps in bubbles, "edited" |
| `label` | 11.5 / 700, letter spacing 0.7, uppercase | Category and section labels |
| `mono` | 12.5 / 400 | Code blocks, fingerprints, invite links |

- Respect the user's font-size setting (§8.1, Appearance) through `MediaQuery.textScaler`.

### 2.3 Spacing, radius, sizes

- Spacing scale: 2, 4, 6, 8, 10, 12, 16, 20, 24, 32.
- Floating layout gaps: window padding 8, gaps between floats 8.
- Radius: panels and floats 14, drawers 14, dialogs 16, menus 10 (items 6), voice reconnect bar 16, voice dock 31, search pill 18, bubbles 16 (5 on grouped edges), badges fully round, server icons 23 → 14 on hover or selection.
- Heights: title bar 40, panel headers 52 (minHeight 52), composer at rest 62, user panel at rest 62, dock buttons 48, rail items 52.
- Widths: rail 62, left column 240–380 (default 288), side slot 264 (People) or 340 (Music) / 380 (voice chat), voice stage max width 1040 (or 520 with fewer than 3 tiles).
- Icon sizes: 16 (inline), 18 (rows, menus), 20 (buttons, dock), 24 (large glyphs).
- Icon button hit area: 36 (default), 28 (compact), 48 (voice dock buttons).
- Icons: Material Symbols **Rounded** set only. Never mix icon families.

### 2.4 Motion

Motion is modelled on critically damped and subtle springs (`OcSpring`) matching physical movement without bouncing:
- `OcSpring.pane`: opening side pane and drawers (mass 1.0, stiffness 260.0, damping 26.0, settling ~297 ms).
- `OcSpring.paneOut`: closing side pane and drawers (mass 1.0, stiffness 220.0, damping 28.0, settling ~267 ms).
- `OcSpring.pop`: menus, popovers, dialogs, dock appearance (mass 1.0, stiffness 180.0, damping 22.0, settling ~177 ms).
- `OcSpring.tile`: voice tiles arriving (mass 1.0, stiffness 200.0, damping 24.0, settling ~227 ms), staggered 16 ms up to 80 ms.
- `OcSpring.move`: tile rearrangement and focus resizing (mass 1.0, stiffness 160.0, damping 24.0, settling ~263 ms).
- Dialog close: 100 ms fade (`Curves.easeIn`). Menus and popovers close instantly (0 ms).
- Color changes on hover/press: 120 ms ease-out; button press scale: 0.97.
- **Reduced motion:** When the OS or user preference enables reduced motion, all animations complete instantly (`Duration.zero`) with no running tickers or frames.

---

## 3. Window layout

> Since D-next the app's frame is floating: `docs/superpowers/specs/2026-11-04-floating-ui-design.md`.

```
┌──────────────────────────────────────────────────────────────────────────────────────────────────┐
│ TITLE BAR (40px) [ < > ]    #general · Opencord Dev                                   [ — □ ✕ ] │
├────┬─────────────────────┬───────────────────────────────────────────────┬───────────────────────┤
│    │ HEAD (52px)         │ CHAT HEADER (52px)                    [Pane]  │ PANEL HEADER (52px)   │
│ R  ├─────────────────────┼───────────────────────────────────────────────┤───────────────────────┤
│ A  │                     │ Pinned bar (floating overlay)                 │                       │
│ I  │ CHANNELS / DMs      │                                               │ MEMBER / MUSIC PANEL  │
│ L  │                     │ Message list (centered max 880px)             │ 264px (People)        │
│    │                     │                                               │ 340px (Music)         │
│ 62 ├─────────────────────┤                                               │                       │
│ px │ VOICE CARD          │                                               │                       │
│    ├─────────────────────┼───────────────────────────────────────────────┤                       │
│    │ USER PANEL (62px)   │ COMPOSER (62px at rest)                       │                       │
└────┴─────────────────────┴───────────────────────────────────────────────┴───────────────────────┘
```

- Layout is composed of floating panels with an 8 px outer window padding and 8 px gaps between floats, resting on a `chat` token background.
- Columns and panels are separated by 8 px gaps and soft shadows (`OcShadow.float`), not border divider lines.
- **Responsive rules (desktop window sizes):**
  - `wide-1440` (≥ 1440 px): all four columns; inline side slot (264 px for People, 340 px for Music, 380 px for voice chat).
  - `wide-1280` (1280–1439 px): standard desktop layout; all columns inline.
  - `wide-1180` (1180–1279 px): minimum breakpoint for inline side slot.
  - `medium-1100` (1100–1179 px): side slot becomes an overlay drawer (sliding from right on `pane`); main expands.
  - `medium-980` (980–1099 px): left column narrows to 240 px; side slot is an overlay drawer.
  - `narrow-940` (940–979 px): minimum non-compact layout; left column at 240 px.
  - `compact-759` (< 940 px): left column collapses to an overlay drawer (sliding from -24 px over scrim); hamburger menu opens it.
- The message list and composer content are centered with a **max width of 880 px** (`MainInnerWidth`), while floating panels adapt to available space.
- A dedicated 40 px **top title bar** (`AppTitleBar`) runs across the top of the window, hosting window controls, navigation arrows, and breadcrumb. See §3.1.

### 3.1 Window chrome and title bars

**Floating title bar row (`AppTitleBar`):**
A single 40 px bar sits at the top of the window, leaving 8 px margins to surrounding floats. It serves as the primary drag region and window chrome:
- **Left:** Navigation arrows (`<` and `>` buttons, 32 × 32 px, 2 px apart) for history traversal (Alt+←/→, ⌘[/], or mouse side buttons). On macOS, the arrows sit clear to the right of the traffic lights (`78` from window left).
- **Center:** Breadcrumb trail (`BreadcrumbCrumb`), showing the current server and channel/view (`#general · Opencord Dev`).
- **Drag region:** Empty space across the title bar is an interactive window drag region. Double-clicking toggles maximize/restore; right-clicking opens the system window menu where supported.
- **Window controls:**
  - **Windows:** Frameless window with native Minimize, Maximize/Restore, Close caption buttons (46 × 32 px) flush at top-right. Snap layouts appear when hovering Maximize (`WM_NCHITTEST` returning `HTMAXBUTTON`).
  - **Linux:** Client-side decoration buttons following `org.gnome.desktop.wm.preferences button-layout` (left or right side). Fully undecorated on tiling window managers (no shadow margin or rounded corners).
  - **macOS:** Native traffic lights centered vertically in the 40 px band inset from top-left, remaining clear of navigation arrows. Fullscreen hides traffic lights and reclaims space.

---

## 4. Screens and components

### 4.1 Server rail (Telegram folder bar + Discord server list)

- Float: 62 px wide floating bar on the left (radius 14), 8 px padding top and bottom.
- Top: Private messages button (`RailItemView`, id: 'private', glyph: `OcIcons.chat`).
- Items: 46 px rounded server icon (radius 23 → 14 on hover/selection), item height 52 px, 6 px gap between items.
- No text labels are rendered under the icons. Instead, hovering or focusing an item displays the **Rail popover** (`RailPopover`):
  - Sits 78 px from the left edge, vertically centered on the active item.
  - Displays server name, connection status, and voice participant avatars (`VoiceFaces`: up to 4 avatars + count, e.g. "+6", "10 in voice").
  - Appears on `OcSpring.pop` (scale 0.96 → 1.0, opacity fade), slides vertically between icons on `OcSpring.paneOut`, and disappears after 90 ms when leaving the rail.
  - Transparent to clicks (pointer events pass through to underlying content).
- States:
  - Default: bg `hover`, circle (radius 23).
  - Hover: bg `selected`, radius morphs to 14, left indicator pill 18 px tall.
  - Selected: inverted icon (bg `accent`, text `onAccent`), radius 14, left indicator pill 34 px tall.
  - Unread (not muted): left indicator pill 8 px tall.
  - Mentions: count badge (inverted) at top-right of icon with 2 px ring.
  - Disconnected: status chip at bottom-right.
- Bottom: "+" button (Add server dialog).
- Drag to reorder servers.

### 4.2 Channel sidebar & Left column

The left column consists of floating panels (width 240–380 px, default 288 px):
- **Server head (`ServerHead`):** A separate 52 px tall floating panel at the top. Displays server name, online/member counts, and chevron. Clicking opens the server menu (sized to match the float's width). In private mode, it shows a static "Private messages" title.
- **Search launcher:** Search pill (radius 18, bg `hover`) with `Ctrl K` / `⌘K` hint.
- **Channel list:** Floating panel below the head holding categories, text channels, and voice channels.
- **Channel rows:**
  - Text channel: 44 px row, 32 px glyph, unread/mention badges, 10 px radius.
  - Voice channel: speaker icon, name, participant count, indented participant list with speaking rings and live indicators.
- **Voice connected card (`VoiceCard`):** Floating card above the user panel when connected to voice. Displays connection state, channel name, latency, Disconnect button, and Camera/Screen toggles.
- **User panel (`UserPanel`):** Floating panel at the bottom of the left column (height 62 px at rest), perfectly level with the chat composer. Displays user avatar, presence, display name, and Mute, Deafen, and Settings buttons.
- **Splitter (`LeftSplitter`):** An 8 px draggable region between the left column and main panel, with resize hover cursor.

### 4.3 Chat header (`ChatHeader`)

- Floating header panel: height 52 px (minHeight 52 px to accommodate text scaling), radius 14.
- Left: Channel name (15/600), topic/subtitle, animated typing indicator when members type.
- Right: Search button, Pinned messages button, right pane toggle (`PaneButton`), and More options menu.
- **Notch connection:** When the right side panel is open inline, the header animates an outward notch (`NotchedFloat`) uniting the chat header with the side panel.

### 4.4 Pinned bar (`PinnedBar`)

- Floating overlay pill positioned in a Stack over the message list (rect at 1280: `387, 101, 600, *`), `surface` background, 2 × 30 px accent bar, and soft shadow.
- The message list includes a `topInset` so that scrolling to the top leaves the first message fully visible below the pinned float.
- Day separator pills tuck neatly under the pinned bar when it is visible.
- Cycles through pinned messages on click, with an unpin button for permitted users.

### 4.5 Message list

- **Reverse virtualized list** (newest at the bottom), loads older pages when scrolling near the top, keeps scroll position stable when older messages are prepended.
- **Start-of-channel header** at the very top of history: 64 px glyph, "Welcome to #general", "This is the start of the #general channel."
- **Date separators**: centered pill (bg `selected`, 12 px, 600, `textSecondary`): "Today", "Yesterday", weekday name within the past week, otherwise "5 October" (with year if not the current year). The pill sticks to the top while scrolling through that day (Telegram).
- **Unread separator**: full-width thin line with "Unread messages" centered, placed before the first unread message on entering a channel. The list opens scrolled to this point.
- **System messages** (member joined, channel created, pin added): centered pill like date separators, with a small icon.
- **Jump to bottom** button: 44 px round button (bg `elevated`, border) bottom-right of the list when scrolled up more than one screen, with an unread count badge when new messages arrived while scrolled up.

**Grouping**: consecutive messages from the same author within 5 minutes, on the same day, and not replies, form a group.

**Bubble layout (Telegram):**
- Incoming messages on the **left** with the author's 36 px avatar next to the **last** bubble of the group (bottom-aligned). Own messages on the **right** without avatar.
- Max bubble width 560 px. Padding 12 × 7.
- Corners: 16 px; inner grouped corners 5 px (e.g. incoming middle bubbles have small left corners), giving the Telegram "stacked" look.
- Incoming bg `bubbleIn` with a 0.8 px `border` outline; own bg `bubbleOut`, no outline.
- Author name (13, 600) at the top of the **first** bubble of an incoming group, followed by the author's highest role name in `textMuted` ("Kai · Maintainer"). **Role names are never colored.**
- **Reply quote** inside the bubble on top: 2 px vertical line in `text`, author name (600) and one-line preview, bg `text` at 6% opacity, radius 6. Click scrolls to and briefly highlights the original.
- **Meta** (timestamp `HH:mm`, "edited", and for own messages a status icon: clock = pending, double check = delivered) sits **inline at the bottom-right of the last text line** using the Telegram spacer trick (reserve width at the end of the text, then position the meta there). If the message ends with a code block, meta goes on its own line, right-aligned.
- **Reactions** below the text inside the bubble: chips "👍 3" (radius 12). Chips the user reacted with are **inverted**. Click toggles; hover shows who reacted.
- **Mentions**: `@name` in 600 weight with `mentionBg` highlight. A message mentioning the current user gets a 2 px `text`-colored bar on the left edge of the bubble.
- Failed send: meta shows an outlined error icon and a "Retry" text button (no red).

**Markdown subset (rendered by our own lightweight parser, no package):** `**bold**`, `*italic*` / `_italic_`, `~~strike~~`, `` `inline code` ``, fenced code blocks with optional language label and a copy button, `> quote`, links (underlined, open in the browser after a confirm dialog for unknown domains), `@mentions`, `#channel` links. Code blocks are full bubble width with horizontal scrolling and bg `chat`.

**Hover action bar (Discord):** when hovering a message, a small floating bar (bg `elevated`, border, radius 10) appears next to the bubble on the outer side: **React · Reply · More**. It must not shift layout (reserve the space or overlay it).

**Interactions:**
- Double-click a message → reply (Telegram).
- Right-click → context menu: Reply · Add reaction · Edit (own) · Copy text · Pin / Unpin (permitted) · Copy message link · Copy message ID · — · Delete (own, or with `MANAGE_MESSAGES`, with a confirm dialog).
- Text selection inside a single message is allowed (selectable text) without breaking the context menu.
- Quick reaction popover: a horizontal pill with 8 frequent emoji plus "more" that opens the emoji picker.

### 4.6 Composer (`ComposerBar`, `Composer`)

- Floating composer panel at the bottom of the chat area, radius 14.
- **Rest height 62 px:** Perfectly level with the user panel at rest (62 px).
- Grows upward when typing multiline text (up to 8 lines) while the user panel stays level.
- Attachments button, multiline text input, emoji picker button, and send button (centered vertically at rest with 12 px button insets and 20.5 px field padding).
- Direction adapts to RTL (Persian, Arabic, Hebrew) based on first strong character.
- Reply and edit bars display inside the floating container above the text input.
- Voice recording overlay covers the entire composer float.

### 4.7 Member panel & Music panel (Side slot)

The side slot (width 264 px for People, 340 px for Music, 380 px for voice chat) floats alongside the main area:
- **Header:** 52 px floating header (minHeight 52 px) with a segmented toggle between **People** and **Music** (or "Members" with count when music is disabled) and a close button.
- **People pane (`MemberPanel`):**
  - Search pill (margin `0 0 4`).
  - Hoisted role sections ("MAINTAINER — 2", "ONLINE — n", "OFFLINE — n" at 0.55 opacity).
  - Member rows: height 44 px, avatar 34 px, gap 10 px, status line.
- **Music pane (`MusicPanel`):**
  - Album artwork: height 172 px, radius 14, 1 px border.
  - Track title (15/600), artist, added-by info.
  - Progress bar: height 4 px, radius 2.
  - Transport controls: previous, play/pause (48 px inverted circle), next, mode toggles.
  - Search / drop zone pill and "Up next" queue list.

### 4.8 Member profile dialog (380 px)

- 64 px avatar with presence, display name (19, 600), presence label.
- Sections with `label` headers: **IDENTITY KEY** (fingerprint in mono, grouped in 4-character blocks, copy button) · **ROLES** (outlined chips, never colored) · **MEMBER SINCE** · **ACTIVITY**.
- Buttons: Mention (secondary) · Message (primary, disabled with tooltip "Direct messages are coming in a later phase").

### 4.9 Quick switcher (`Ctrl+K`)

- Centered near the top (110 px from top), 600 px wide, bg `elevated`, radius 16, scrim behind.
- Large input "Jump to a channel, server or member". Results: channels across **all** servers (with server name as subtitle), servers, and members of the current server. Ranking: prefix match > word match > contains; recently visited first when the query is empty.
- `↑ ↓` navigate, `Enter` open, `Esc` close. Footer shows these key hints.

### 4.10 Voice view (`VoiceView`, `VoiceTileView`, `VoiceDock`)

- **Stage:** Main floating panel without composer, padding 12 px, participant tiles spaced 8 px apart. Sized up to 1040 px wide (520 px for <3 tiles), centered.
- **Tiles:** 16:10 aspect ratio, radius 14, `surface` background, 1 px `border`. 72 px avatar (or camera stream / screenshare), name pill at bottom-left (8, 8), inverted `LIVE` pill for screenshares.
- **Speaking indicator:** 2 px `text` border ring.
- **Focus mode:** Clicking a tile expands it to fill the stage above a 76 px horizontal strip of remaining participants.
- **Motion:** Tiles enter on `OcSpring.tile` (staggered 16 ms up to 80 ms); moves/resizes run on `OcSpring.move`.
- **Floating voice dock (`VoiceDock`):**
  - Pill-shaped floating bar (radius 31) centered 14 px above the bottom of the stage inside the page.
  - Controls: Chat toggle, 28 px divider, mic, camera, screen share, soundboard, deafen, and Disconnect button (48 px tall).
  - Auto-hiding: Pointer movement shows the dock; hides after 2.5 s of inactivity or pointer departure. Never hides while pointer is over the dock, during keyboard focus, or while a control popup is open.
- **Voice chat:** When opened, chat floats in the side slot (width 380 px) if window width permits (≥ 1180 px); otherwise replaces the tiles in the main float.

### 4.11 Dialogs (shared shell)

- Centered, bg `elevated`, radius 16, 1 px `border`, scrim behind, max width per dialog, 24 px padding, title `title` style, close on `Esc` and on barrier click.
- Buttons bottom-right: secondary (bg `hover`) then primary (inverted). Destructive actions use a **primary button with explicit wording** ("Delete channel", "Ban Kai"), never a red color, plus a confirm step.
- **Add server dialog** (3 steps in one dialog):
  1. Invite link or `host:port` input, optional "Owner claim token" field (collapsed under "I'm the owner").
  2. If the link has no fingerprint: **Verify server** step showing the fingerprint in mono, grouped by 4, with "Compare this with the fingerprint the server owner shared with you." Buttons: Cancel · Trust and connect.
  3. Connecting state (small spinner + "Connecting to host…"), then close and select the new server. Errors are shown inline in `text` with an outlined error icon.
  - Fingerprint mismatch on a known server: a separate blocking dialog "Server identity changed" with both fingerprints and only "Disconnect" and "Forget server" options.
- **Invite people dialog**: link in a mono field with a Copy button (inverted), expiry selector (1 hour / 1 day / 7 days / Never) and max uses, plus a note that the link includes the fingerprint.
- **Create channel dialog**: type choice as three selectable cards (Text · Voice · Announcement; the selected card has a 1.5 px `text` border), name field with a `#` prefix for text channels (auto lowercase-kebab), "Private channel" switch.
- **Confirm dialog**: title, one sentence, Cancel + explicit primary action.

### 4.12 Toasts

- Small inverted pill centered at the **top** of the window (Telegram style), 1.8 s, max one visible at a time (new ones replace the old). Used for "Copied", "Invite link copied", "Coming in a later phase", etc.

### 4.13 Empty and edge states

- No servers: centered illustration-free state: icon, "No servers yet", "Join a server with an invite link or host your own." and an "Add server" primary button.
- Server failed to connect: the chat area shows the last cached state greyed out with a top banner "Can't reach server · Retrying in 8 s" and a "Retry now" button.
- Empty channel: only the start-of-channel header.
- Loading: skeleton rows using `hover` blocks, no spinners in lists.

---

## 5. Shared widgets

### 5.1 Base widgets (in `lib/ui/widgets/`)
`Hoverable` (hover state + tap, double-tap, secondary tap) · `OcIconButton` · `OcButton` (primary inverted / secondary / ghost; normal and dense) · `OcTextField` and input decoration helper · `OcAvatar` · `PresenceBadge` · `UnreadBadge` · `MentionBadge` · `LivePill` · `ChannelGlyph` · `SectionLabel` · `KeyHint` · `OcDialog` · `showOcMenu` (context menu at cursor) · `showPopover` (anchored popover) · `showOcToast` · `SettingsSection`, `SettingsRow`, `SettingsSwitchRow`, `SettingsChoiceCards`.

### 5.2 Avatars
- Circle (users) or rounded square (servers, channel glyphs).
- Initials (max two letters) in `text` on a grey from `avatarShades` chosen by hashing the ID, so a user always gets the same grey.
- Optional presence badge bottom-right with a 2.5 px ring in the color of whatever is behind it.

### 5.3 Shape language (replaces color)

| Meaning | Shape |
|---|---|
| Online | Filled circle |
| Idle | Filled circle with a moon cut-out |
| Do not disturb | Filled circle with a horizontal bar cut-out |
| Offline / invisible | Hollow ring |
| Connected (server) | Filled dot |
| Connecting / reconnecting | Hollow dot + sync icon chip on the rail |
| Failed | Outlined error icon |
| Speaking | 2 px `text` ring around avatar / tile |
| Live screenshare | Inverted `LIVE` pill |
| Unread | Inverted count pill; grey pill when muted |
| Mention | Inverted "@" circle |
| Active toggle | Inverted button |

---

## 6. Chat behaviors

### 6.1 Core behaviors

- Optimistic send: message appears immediately with the clock icon, switches to double check on server ack.
- Edits update in place with "edited". Deletes remove the bubble with a 100 ms fade (no "message deleted" placeholder).
- New message while scrolled up: don't auto-scroll; increment the jump-to-bottom badge.
- New message while at the bottom: stick to the bottom.
- Entering a channel: scroll to the unread separator if there is one, otherwise to the bottom.
- Unread and mention counts update live in the channel list, server rail and window title ("(3) Opencord").
- Typing indicator: send `StartTyping` at most every 8 s while typing; show others' typing for 10 s or until their message arrives.

### 6.2 Floating layout behaviors

- **Short chats bottom alignment:** Channels or private chats with only a few messages sit anchored to the bottom of the floating list area (12 px from bottom), rather than floating at the top.
- **Centred message column:** Message list and composer content center at a maximum width of 880 px (`MainInnerWidth`).
- **Pinned bar clearance:** When scrolled to the top, messages clear the bottom of the floating pinned bar; day pills move smoothly under the pinned bar.
- **Jump to bottom button:** Floats bottom-right above the composer float.

---

## 7. Keyboard shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+K` | Quick switcher |
| `Ctrl+,` | User settings |
| `Alt+↑` / `Alt+↓` | Previous / next channel |
| `Alt+Shift+↑` / `Alt+Shift+↓` | Previous / next unread channel |
| `Ctrl+Alt+↑` / `Ctrl+Alt+↓` | Previous / next server |
| `Ctrl+Shift+M` | Toggle mute |
| `Ctrl+Shift+D` | Toggle deafen |
| `Ctrl+Shift+U` | Toggle member list |
| `Enter` / `Shift+Enter` | Send / newline |
| `↑` (empty composer) | Edit last own message |
| `Esc` | Cancel reply/edit, close dialog, leave focus mode, mark channel read |
| `Page Up` / `Page Down` | Scroll messages |
| Double-click message | Reply |

**On macOS, `Ctrl` becomes `⌘` and `Alt` becomes `⌥`** for every shortcut above, and the standard macOS shortcuts work too (`⌘W` close window, `⌘Q` quit, `⌘M` minimize, `⌘H` hide, `⌃⌘F` fullscreen). On Windows and Linux also support `F11` for fullscreen. Key hints in the UI (tooltips, `KeyHint` chips, the quick switcher footer) always show the current platform's modifier symbols.

All shortcuts are listed in Settings → Keybinds and are rebindable later.

---

## 8. Settings

One dialog shell for both user and server settings, styled like Telegram's settings but with Discord's sidebar navigation:

- Size: up to 980 × 700, bg `elevated`, radius 16.
- Left nav (240 px, bg `sidebar`): title, grouped items (`label` group headers), each item with icon + name, selected item bg `selected`.
- Right: page title bar (60 px) with a close button, content scrolls, 24 px padding. Content is built from `SettingsSection` blocks (bordered, radius 12, rows separated by 1 px dividers).

### 8.1 User settings
- **My profile:** avatar (upload later), display name, presence selector as choice cards.
- **Identity & keys:** public key fingerprint (mono, grouped), Copy public key, Export identity backup, Import identity, plain warning text about keeping the backup private.
- **Appearance:** theme choice cards **System · Dark · Light** (each card shows a miniature of the layout in that theme), message density (Comfortable / Compact — compact removes bubbles and lays messages out as Discord does: the avatar, name, role and time over a group's first message, the time beside later ones while hovered, the hovered row highlighted; still monochrome), font size slider (12–18), reduce motion switch.
- **Windows & behavior** (desktop only): close button minimizes to tray (on by default on Windows and Linux, off on macOS), start minimized, launch at login, restore last window position.
- **Voice & audio**, **Video**, **Screen share**, **Sounds** and **Voice diagnostics:** full specification in §17.
- **Notifications:** desktop notifications, mentions only, message sound, flash taskbar.
- **Keybinds:** the table from §7.
- **Trusted servers:** list of servers with address and pinned fingerprint, "Forget" action.
- **About:** version, license, links. On desktop, **Logs** (D82, docs/logs.md): export this session's log, export the last session's when it ended unexpectedly, open the logs folder.

### 8.2 Server settings (items shown only with the right permissions)
- **Overview:** name, description, icon, open-join switch, read-only address and fingerprint.
- **Roles:** two panes. Left: role list ordered by position (drag to reorder, `@everyone` pinned at the bottom), member count per role, "Create role". Right: role name, "Display separately" (hoist) switch, "Allow anyone to @mention" switch, then permission groups (General · Text · Members · Voice & video · Advanced) as switch rows with one-line descriptions, matching the bits in the Phase 1 plan §6.1. `ADMINISTRATOR` shows an explanatory warning. Disabled switches (with a tooltip) for permissions the editor doesn't have.
- **Channels:** categories and channels with drag reordering, edit and delete; channel edit has **Overview** and **Permissions** tabs; the Permissions tab lists overwrites per role/member with a three-state control per permission (Deny · Inherit · Allow, shown as ✕ / – / ✓ segmented control, inverted on the active segment).
- **Members:** searchable table with roles, joined date, Kick/Ban.
- **Invites:** table of code, creator, uses / max, expiry, copy and revoke; Create invite.
- **Bans:** list with reason and Unban.

---

## 9. Accessibility

- All interactive elements are focusable and show a 1.5 px `text` focus ring on keyboard focus.
- Semantics labels on icon-only buttons (use the tooltip text).
- Never rely on hover only: every hover action is also in the context menu and reachable by keyboard.
- Screen-reader announcements for new messages in the open channel (polite).
- Text scale up to 1.5× must not break layouts (test it).

---

## 10. Project structure (Flutter)

```
app/lib/
├── main.dart
├── core/
│   ├── providers/            # riverpod providers fed by the Rust core event stream
│   ├── mock/                 # mock data layer (§11)
│   └── format.dart           # time/day labels, fingerprint grouping
├── ui/
│   ├── theme/                # OcColors tokens, typography, ThemeData builder
│   └── widgets/              # §5.1 shared widgets
└── features/
    ├── shell/                # desktop shell, responsive rules, shortcuts
    ├── window/               # window chrome per platform, drag regions, caption buttons, tray, window state
    ├── servers/              # rail, add-server, invite, server menu
    ├── channels/             # sidebar, categories, rows, user + voice panels
    ├── chat/                 # header, pinned bar, message list, bubble, markdown, composer
    ├── members/              # member panel, profile dialog
    ├── voice/                # voice view, tiles, control bar
    ├── search/               # quick switcher
    └── settings/             # settings shell, user pages, server pages
```

- State: `flutter_riverpod` with **granular providers** (per server, per channel messages, per member presence) so a typing event or speaking update only rebuilds the widgets that show it.
- Each screen widget takes plain view-model objects, not Rust types, so the mock layer and the real core are interchangeable.

---

## 11. Mock data layer

Build this first so the whole UI can be reviewed before the Rust core exists:

- `MockRepository` implementing the same interface as the real core-backed repository (servers, channels, messages, members, roles, invites, voice state, event stream).
- Mock content: 3 servers (one owned by the user, one where the user is a normal member, one in "reconnecting" state and muted), categories with text, announcement (read-only) and voice channels, ~12 users with all four presences and activities, hoisted roles, a realistic `#general` conversation spanning yesterday and today with replies, reactions, a code block, an edited message, a mention of the user, a pinned message, an unread separator and a system message.
- Simulated life: after the user sends a message, another member "types" for 1–3 s and replies; voice participants randomly toggle "speaking" every ~650 ms while the user is in voice; one voice participant is screensharing, one has the camera on, one is muted.
- A debug-only menu (`Ctrl+Shift+F12`) to trigger states: server disconnect/reconnect, fingerprint mismatch, empty server list, long channel names, 10 000-message channel for scroll performance.

---

## 12. Implementation order

1. **Tokens & theme:** `OcColors` (dark + light), typography with bundled fonts, `ThemeData` builder, theme switching (System/Dark/Light) persisted.
2. **Shared widgets** (§5.1) with a hidden **widget gallery** route that shows every component in both themes.
3. **Mock data layer** (§11).
4. **Shell & responsive layout** (§3) with shortcuts (§7).
5. **Window chrome** (§3.1) on Linux first (all three Linux modes, test on Hyprland and GNOME), then Windows (including snap layouts), then macOS; plus window state persistence and the Window frame setting.
6. **Server rail** (§4.1) and **channel sidebar** (§4.2), including user panel and voice connected panel.
7. **Chat**: header, pinned bar, message list, bubbles, markdown, reactions, hover bar, context menus, composer with reply/edit, autocomplete (§4.3–4.6, §6).
8. **Member panel** and **profile dialog** (§4.7–4.8).
9. **Quick switcher** (§4.9).
10. **Dialogs**: add server with TOFU, invite, create channel, confirm (§4.11), toasts (§4.12).
11. **Settings**: user pages, then server pages including roles and channel permission overwrites (§8).
12. **Voice view** on mock data (§4.10) and **all voice, audio and volume controls** (§17): quick audio menu, per-user menus, moderator actions, settings pages, global hotkeys, visual states.
13. **Platform integration and UX polish** (§15, §16).
14. **Empty/edge states** (§4.13) and **accessibility** pass (§9).
15. Swap the mock repository for the Rust-core repository as M4 lands.

---

## 13. Acceptance criteria

- Switching between dark and light updates the whole app instantly, and **no screen contains any hue**: a test renders the widget gallery and every main screen in both themes and asserts that every painted color has saturation 0 (emoji excluded).
- `grep` check in CI: no `Color(0x` or `Colors.` usage outside `lib/ui/theme/` (except `Colors.transparent`).
- All screens in §4 work on mock data at 1440 × 900, 1180 × 800 and 900 × 700 windows without overflow errors, and at 1.5× text scale.
- Scrolling a 10 000-message channel stays at 60 fps+ on a mid-range laptop in profile mode; opening a channel takes < 100 ms with mock data.
- Every action reachable by mouse is also reachable by keyboard or context menu.
- Golden tests for: server rail item states, channel row states, bubble variants (incoming/own, first/middle/last, reply, reactions, code block, edited, pending), presence shapes, settings section, in both themes.
- **Window chrome checklist** passes on each platform:
  - Linux: GNOME (round controls following the button-layout setting, rounded corners, drag, double-click maximize, resize from all edges), KDE and Hyprland (the same header bar; a tiled window has square corners and no shadow).
  - Windows 11: caption buttons look native, snap layouts appear on maximize hover, Aero Snap and resize work, close hover is inverted.
  - macOS: traffic lights sit above the rail, the header row drags the window, fullscreen hides the spacer.
- Window size, position and maximized state survive a restart; a saved position on a disconnected monitor falls back to the primary monitor.
- **Voice controls checklist** (on mock data until Phase 2): every control in §17 exists and is reachable; per-user volume and local mute persist across restarts and across servers for the same identity; the four mute states in §17.6 are visually distinct in both themes; global push-to-talk works on Hyprland through the GlobalShortcuts portal, on GNOME, on Windows and on macOS (after the permission prompt).
- `flutter analyze` is clean and widget tests pass.

---

## 14. Out of scope (desktop UI, Phase 1)

Mobile layouts, DMs, threads, file and image attachments UI beyond the disabled attach button, custom emoji and stickers, server icons upload, message search results view, notifications center, theming beyond dark/light. Leave clear extension points for all of them.

---

## 15. Platform integration

Make the app feel native on each desktop, not like a web page in a window:

| Feature | Windows | Linux | macOS |
|---|---|---|---|
| Tray / menu bar icon | Monochrome tray icon (white or black per taskbar theme) with a small dot when there are unreads; menu: Open, Mute, Deafen, Status ▸, Quit | StatusNotifierItem tray (works with Waybar on Hyprland, KDE, GNOME with the AppIndicator extension); same menu | Optional menu bar icon (off by default); the Dock is primary |
| Unread indicator | Taskbar overlay badge + taskbar flash on mention | Window urgency hint on mention | Dock badge with the mention count |
| App menu | — | — | Native menu bar (`PlatformMenuBar`): Opencord, Edit, View, Server, Window, Help, with the shortcuts from §7 |
| Deep links | Register `opencord://` (registry) | `.desktop` file with `MimeType=x-scheme-handler/opencord` | `CFBundleURLTypes` in Info.plist |
| Single instance | Second launch focuses the existing window and passes on the deep link | Same (via D-Bus or a local socket) | Native |
| Notifications | Native toast notifications | `org.freedesktop.Notifications` via D-Bus | `UserNotifications` |
| Launch at login | Registry run key | XDG autostart `.desktop` file | Login item |
| Scrolling | Smooth wheel scrolling (animated per notch), touchpad precise scrolling | Smooth wheel + kinetic touchpad scrolling | Native momentum scrolling and rubber-banding |
| Scrollbars | Thin overlay scrollbars that widen on hover | Thin overlay scrollbars | Overlay scrollbars that follow the system "show scroll bars" setting |
| Text rendering | Use the bundled fonts everywhere for a consistent look | Same | Same |
| High DPI / mixed DPI | Per-monitor DPI v2; re-layout when the window moves between monitors | Fractional scaling on Wayland (Hyprland, GNOME) must stay sharp | Native Retina |

- The tray icon, Dock badge and taskbar badge are monochrome too.
- Notifications are a later phase on the backend side, but wire the platform plumbing now so the setting in §8.1 has something to control.

---

## 16. UX polish (what makes it great to use)

- **Instant feel:** the app window shows the last-known UI from a local snapshot within 300 ms of launch, then connects in the background. Never show a blank window or a full-screen spinner.
- **Remember everything:** last open server and channel per server, scroll position per channel, drafts per channel, collapsed categories, member panel visibility, window state.
- **Never lose input:** drafts survive app restarts; a failed send keeps the text with Retry; closing a settings dialog with unsaved changes asks first.
- **Predictable focus:** typing any printable key anywhere in the main view focuses the composer and inserts the key (Discord behavior). After closing a dialog or menu, focus returns to where it was.
- **Hover intent:** tooltips after 450 ms, hover action bars appear and disappear immediately, as Discord's do; moving from the row onto its bar keeps it. Bubbles' bars have no More button (right-click opens the menu); compact lines' bars have More, and Edit on your own messages.
- **Undo instead of confirm where safe:** leaving a voice channel, unpinning, removing a reaction or hiding a channel show a toast with **Undo** instead of a confirm dialog. Destructive actions on other people (kick, ban, delete others' messages, delete channel) still confirm.
- **Drag and drop:** reorder servers in the rail, channels and categories in the sidebar (with permission), roles in settings; dropped files on the chat show a "Attachments are coming later" overlay for now.
- **Context everywhere:** every object (server, channel, message, member, role, invite) has a right-click menu with Copy ID/link at the bottom, so power users never need to dig.
- **Small details:** hovering a timestamp shows the full date and time; clicking an avatar or name opens the profile; clicking a `#channel` link opens it; the window title, tray and badges always agree on unread counts; long names truncate with ellipsis and show the full name in a tooltip.
- **Density:** a Compact mode (§8.1) for people who want more messages on screen, and a comfortable default that matches Telegram.
- **First run:** after onboarding, an empty state that explains the two ways in (join with an invite, or host your own server with a link to the docs) instead of an empty window.

---

## 17. Voice, audio and volume controls

The client must have **every voice and volume control people expect from TeamSpeak and Discord**. In Phase 1 all of these are built as UI on mock data and saved to local settings; Phase 2 connects them to the real audio pipeline and the server. Anything that needs the server (server mute, move, bitrate) must be added to the protocol in Phase 2 as voice-state requests/events, using the reserved permission bits from the Phase 1 plan §6.1.

### 17.1 Controls on yourself (always one click away)

| Control | Where | Behavior |
|---|---|---|
| **Mute** (self mute) | User panel, voice control bar, tray menu, global hotkey | Stops sending your mic. Plays the mute sound. |
| **Deafen** (self deafen) | Same places | Silences everyone for you **and** mutes your mic. Undeafen restores your previous mute state (if you were muted before deafening, you stay muted). |
| **Push-to-talk / voice activity** | Quick audio menu, settings | Switch input mode without opening settings. |
| **Input volume** | Quick audio menu, settings | 0–200 % mic gain with live level meter. |
| **Output volume** (master) | Quick audio menu, settings | 0–200 % for everything you hear. |
| **Priority speaker** (if permitted) | Hotkey | While held, others are lowered for everyone (§17.4). |
| **Leave voice** | Voice panel, control bar, hotkey | Disconnect

> **Not yet received:** the plan was cut off here when it was pasted (message length limit). The rest of §17.1, §17.2 onward and any later sections are still to come from the user.
