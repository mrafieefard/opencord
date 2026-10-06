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

1. **Monochrome only.** Dark and light themes built from neutral greys, black and white. **No blue, orange, red, green or any other hue** — not for links, not for errors, not for presence, not for roles. Contrast, weight, shape and inversion do the work that color usually does.
2. **Telegram consistency.** Same row heights, same radii, same paddings everywhere. Two-line list rows (title + preview + time + badge). Rounded chat bubbles. Calm, flat surfaces with no gradients and almost no shadows.
3. **Discord capability.** Server rail, channel categories, voice channels with live participants, member list grouped by role, roles and permissions editor, voice/camera/screenshare controls, quick switcher, rich context menus.
4. **Inversion = emphasis.** The strongest state (selected server, unread badge, primary button, active toggle, own reaction) is shown by **inverting** foreground and background (white on black in dark, black on white in light).
5. **Shape over color.** Presence, connection state, speaking, live streaming and mentions are all distinguishable without color (see §5.3).
6. **Keyboard first.** Everything important has a shortcut (§9).
7. **Fast and quiet.** Animations are short (100–160 ms), never bouncy, and never block input.

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
- Radius: rows 10, inputs 10, search pill 18, bubbles 16 (5 on grouped edges), dialogs 16, menus 10, badges fully round, server icons 23 → 14 on hover or selection.
- Icon sizes: 16 (inline), 18 (rows), 20 (buttons).
- Icon button hit area: 36 (default), 28 (compact), 48 (voice control bar).
- Icons: Material Symbols **Rounded** set only. Never mix icon families.

### 2.4 Motion

- Hover/press color changes: 120 ms ease-out.
- Server icon morph, category collapse arrow, badge appear: 150 ms.
- Dialog open: 120 ms fade + 0.98 → 1.0 scale. Menus: 90 ms fade.
- No animation on message arrival other than a 100 ms fade (Telegram style). Respect the OS "reduce motion" setting by disabling all non-essential animations.

---

## 3. Window layout

```
┌──────┬─────────────────────┬───────────────────────────────────────┬────────────────────┐
│ RAIL │ CHANNEL SIDEBAR     │ CHAT HEADER                           │ MEMBER PANEL       │
│ 76px │ 304px (264 narrow)  │───────────────────────────────────────│ 264px              │
│      │ server header       │ pinned bar (optional)                 │ header             │
│ [≡]  │ [ search  Ctrl K ]  │                                       │ [ search ]         │
│ ──── │ ▾ CATEGORY          │      message list (centered column,   │ ROLE — 2           │
│ (S1) │  #  channel  12:04  │      max width 880px)                 │  ● member          │
│ (S2) │     Kai: preview (3)│                                       │ ONLINE — 5         │
│ (S3) │ ▾ VOICE             │                                       │  ◐ member          │
│      │  🔊 General       3 │                                       │ OFFLINE — 2        │
│      │     ○ Kai           │                                       │  ○ member          │
│      │     ○ Mira  LIVE    │───────────────────────────────────────│                    │
│      │ voice connected pnl │ composer bar                          │                    │
│ [+]  │ user panel          │                                       │                    │
└──────┴─────────────────────┴───────────────────────────────────────┴────────────────────┘
```

- Columns are separated by 1 px `border` lines, never by shadows.
- **Responsive rules (desktop window sizes):**
  - ≥ 1180 px: all four columns.
  - 980–1179 px: member panel hidden by default (toggle opens it as an overlay drawer from the right).
  - < 980 px: sidebar narrows to 264 px; member panel only as overlay.
  - < 760 px: sidebar collapses; a hamburger in the chat header opens it as an overlay. (Mobile gets its own plan; this is only for small desktop windows.)
- The message list and composer content are centered with a **max width of 880 px** (Telegram adaptive layout), while their backgrounds span the full width.
- The app draws its **own window chrome on every platform** and uses the whole window, with no separate title bar row. See §3.1.

### 3.1 Window chrome and title bars

**Principle: unified headers.** There is no extra title bar strip. The existing top row of the app (rail top, 60 px server header, 60 px chat header, 60 px member panel header) **is** the title bar. Empty space in that row is a drag region, and the window controls live inside it in the native style of each platform. This gives the content the full window height, the way GNOME apps with header bars, Discord on macOS and modern Windows apps (Teams, Windows Terminal) do it.

Shared rules for all platforms:
- **Drag regions:** every empty area of the top row (header backgrounds, the area around titles, the rail top). Interactive elements inside the row (buttons, the server header menu, search) are never drag regions.
- **Double-click** on a drag region toggles maximize (on macOS it follows the system "double-click a window's title bar to" setting: zoom or minimize).
- **Right-click** on a drag region opens the native window menu where the platform has one (Windows system menu, GNOME window menu).
- **Window title** (shown in the taskbar, dock, alt-tab and window switchers) is always set: `(3) #general · Opencord Dev — Opencord`, with the unread count prefix only when there are unreads.
- **States:** normal, maximized, tiled/snapped, fullscreen. Maximized and tiled windows have no rounded corners and no resize borders; fullscreen hides the window controls and the macOS traffic-light spacer.
- **Minimum window size:** 940 × 560. Restore the last size, position, monitor and maximized state on launch (and fall back to centered 1280 × 800 on the primary monitor if the saved position is off-screen).
- Window controls are drawn in **monochrome** using the app tokens (they follow the dark/light theme), except the macOS traffic lights, which stay native.
- A setting **Appearance → Window frame: Auto · Custom · System** lets users fall back to the native title bar if the custom one misbehaves on their setup. "Auto" applies the per-platform rules below.

#### Windows (Windows 10/11 style)
- Frameless window that keeps native behavior: Aero Snap, **Windows 11 snap layouts** on hovering the maximize button, native resize borders, DWM shadow and Windows 11 rounded corners. This needs custom handling in the Windows runner (`WM_NCCALCSIZE` to remove the frame, `WM_NCHITTEST` returning `HTCAPTION` for drag regions and `HTMAXBUTTON` over the maximize button so snap layouts appear). Use a window-management package only if it supports all of this; otherwise write it in the runner.
- **Caption buttons** at the **top-right corner** of the window, flush with the top and right edges: Minimize, Maximize/Restore, Close. Each **46 × 32 px**, glyphs from *Segoe Fluent Icons* (fallback *Segoe MDL2 Assets*) at 10 px: `E921` minimize, `E922` maximize, `E923` restore, `E8BB` close.
- They sit in the top 32 px of the rightmost header (member panel header when it's open, otherwise the chat header). That header reserves 138 px on the right; its own icon buttons stay vertically centered to the left of the caption buttons.
- Colors: transparent at rest; hover `hover`, pressed `selected`. **Close hover is inverted** (`accent` background, `onAccent` glyph) instead of the usual Windows red, to keep the app monochrome. Glyphs use `textSecondary` when the window is inactive and `text` when active.
- No Mica/Acrylic backdrop (they tint with wallpaper colors); use solid token backgrounds.
- Top 1 px of the window stays a resize area when not maximized.

#### Linux (GNOME / libadwaita header bar style)
Linux desktops differ a lot, so "Auto" picks a mode at startup from `XDG_CURRENT_DESKTOP` and `XDG_SESSION_TYPE`:

| Desktop | Auto mode |
|---|---|
| GNOME, Budgie, Cinnamon, Pantheon, XFCE, MATE and unknown floating desktops | **Custom header bar** (client-side decorations, GNOME style) |
| KDE Plasma | **System** decorations (server-side, Breeze), because KDE users expect them and KWin draws them well |
| Tiling compositors: **Hyprland**, Sway, i3, river, niri, bspwm, dwm | **No window controls and no decorations.** The compositor manages the window; the headers are just headers. Request server-side/no decorations via xdg-decoration where available |

Custom header bar details (GNOME style):
- Window controls are **round 24 px buttons** with 18 px hit-padding, 6 px apart, vertically centered in the 60 px header row. Icons: GNOME-style minimize (line), maximize (square), restore (two squares), close (×), 16 px, `text` color.
- Button background: `hover` at rest (like Adwaita's subtle circle), `selected` on hover, inverted on press. Close is the same as the others (no red).
- **Respect the user's button layout** from `org.gnome.desktop.wm.preferences button-layout` (read via `gsettings` or the GTK setting `gtk-decoration-layout`). Default GNOME shows only Close on the right; show exactly the buttons the setting lists, on the side it lists them (left layouts put them at the top-left of the rail/server header; right layouts at the right end of the rightmost header).
- When not maximized or tiled: **12 px rounded window corners**, 1 px `border` outline, and a soft shadow drawn by the app in a transparent margin (only if the compositor supports transparency; otherwise square corners without shadow). The Linux runner must create the window undecorated with an RGBA visual for this.
- Resize handles: 6 px invisible edges and 12 px corners around the window that start a native resize (`gtk_window_begin_resize_drag` / xdg-toplevel resize), with the matching resize cursors.
- Right-click on a drag region shows the compositor's window menu where supported.

#### macOS (Discord style)
- **Transparent, full-size content title bar**: hidden title text, content extends under the title bar area, native **traffic lights kept** and repositioned to sit at the top-left above the server rail (inset about 20 px from the left, centered in a 52 px top band).
- The server rail gets a **52 px top spacer** under the traffic lights; the menu button moves below it. The rest of the top row (server, chat and member headers) stays 60 px and acts as the drag region, so no vertical space is lost.
- Fullscreen: traffic lights hide automatically; remove the 52 px spacer.
- Keep the native window shadow and rounded corners; no custom resize handling.
- Respect the system "double-click title bar" action and native full-screen and tiling (green button menu).

#### Fallback: System frame
When "System" is selected (or Auto chooses it), the native title bar and decorations are used, the headers stay as they are, and the app draws no window controls.

---

## 4. Screens and components

### 4.1 Server rail (Telegram folder bar + Discord server list)

- Top: menu button (opens user settings). Then a short divider.
- One item per server: a **46 px rounded icon** with the server initials (or uploaded icon later), and the **server name as a small label underneath** (11 px, one line, ellipsis) — this is the Telegram folder style.
- States:
  - Default: icon bg `hover`, circle (radius 23).
  - Hover: bg `selected`, radius morphs to 14, left indicator pill 18 px tall.
  - Selected: **inverted** icon (bg `accent`, text `onAccent`), radius 14, left indicator pill 34 px tall, label in `text` color and weight 600.
  - Has unread (not muted): left indicator pill 8 px tall.
  - Has mentions: count badge (inverted) at the top-right of the icon, with a 2 px ring in `rail` color.
  - Not connected: small status chip at bottom-right (sync icon for connecting/reconnecting, outlined error icon for failed).
- Tooltip: server name and connection state.
- Right-click menu: Mark as read · Mute / Unmute notifications · Invite people · Server settings (if permitted) · Copy address · — · Leave server.
- Bottom: "+" button (Add server dialog).
- Drag to reorder servers (persisted locally).

### 4.2 Channel sidebar

**Server header** (60 px): server name (`header`), below it a status line with a small connection dot and "128 online · 1,204 members" (or "Connecting…", "Reconnecting…", "Connection failed"). Chevron on the right. Click opens the server menu: Invite people · Server settings · Create channel · Create category · Notification settings · Copy server address · — · Leave server. Items only appear if the user has the permission.

**Search launcher**: a pill (radius 18, bg `hover`) with a search icon, "Search" placeholder and a `Ctrl K` key hint. Clicking opens the quick switcher (§4.9).

**Categories**: label row (`label` style) with a collapse chevron; on hover show a "+" (create channel) if permitted. Collapsed categories still show the selected channel and the voice channel the user is connected to.

**Text / announcement channel row — Telegram chat-list style, two lines, 56 px:**
- Left: 40 px rounded-square glyph (`#` for text, megaphone for announcement, small lock overlay for read-only/private). Glyph bg `selected`; inverted when the row is selected.
- Line 1: channel name (600 if unread or selected, otherwise 500 in `textSecondary`) and the time of the last message on the right (`HH:mm` today, weekday this week, `dd.MM.yy` older).
- Line 2: last message preview "Author: text" in `textMuted`, one line, ellipsis. On the right: mention badge (inverted circle with "@") and unread count badge (inverted pill; grey `selected` pill if the server/channel is muted).
- Row bg: `selected` when selected, `hover` on hover, radius 10, 8 px side margin.
- Right-click: Mark as read · Mute channel · Copy link · Edit channel (permitted) · Delete channel (permitted).

**Voice channel row — Discord style:**
- One line: speaker icon, name, participant count on the right.
- Under it, participants indented 44 px: 22 px avatar (with speaking ring, §5.3), name, and state icons: mic-off, headset-off, camera, and an inverted `LIVE` pill when screensharing.
- Click: join the voice channel and open the voice view. Right-click: Join · Open in view · Copy link · Edit/Delete (permitted).

**Voice connected panel** (only while in voice), above the user panel:
- Line 1: waveform icon + "Voice connected" (600), line 2: "Channel / Server" muted. Clicking the text opens the voice view. Disconnect button on the right.
- A row of two equal toggles: **Camera** and **Screen**. Inverted when active.

**User panel** (58 px, top border):
- Avatar with presence + display name + presence label. Click opens a presence menu: Online · Idle · Do not disturb · Invisible.
- Buttons: Mute (mic), Deafen (headset), Settings. Muted/deafened buttons are shown **active** (bg `selected`, icon `text`) with the "off" icon variant.
- The Mute and Deafen buttons are **split buttons**: a small chevron next to each opens the quick audio menu from §17.2 (device switching, volumes, input mode).

### 4.3 Chat header (60 px, bg `sidebar`, bottom border)

- Left: channel name (`header`) and a subtitle line: "1,204 members, 128 online · topic". **While someone is typing, the subtitle is replaced by "Kai is typing" with three animated dots** (Telegram behavior). Two people: "Kai and Mira are typing". More: "Several people are typing".
- Right icon buttons: Search in channel · Pinned messages · Member list toggle (active state when shown) · More (Notification settings, Channel settings, Copy link, Mark as read).

### 4.4 Pinned bar (Telegram)

- 46 px bar under the header: 2 px vertical line in `text`, "Pinned message" (600, 12.5) above a one-line preview. Click scrolls to the message. Unpin button on the right (permitted users only). With several pins, the bar cycles through them on click and shows "Pinned message 2 of 4".

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

### 4.6 Composer (bg `sidebar`, top border, content centered max 880 px)

- Telegram arrangement: **attach** button (left) · multiline input in a rounded field (radius 20, bg `hover`, 1–8 lines then scroll) · **emoji** button · **send** button.
- Send button is an inverted circle when there is text; when empty it shows a muted mic icon (voice messages are a later phase; clicking shows a "coming later" toast).
- Placeholder: "Message #general".
- **Reply / edit bar** above the input: reply or edit icon, 2 px vertical line, "Reply to Kai" / "Edit message" (600) with a one-line preview, and a close button.
- Keys: `Enter` send · `Shift+Enter` newline · `Esc` cancel reply/edit · `↑` in an empty input edits your last message · `Ctrl+V` image paste (later phase, show toast for now). Respect IME composition (don't send while composing).
- Autocomplete popups above the composer: `@` members, `#` channels, `:` emoji. Arrow keys + Enter/Tab to pick.
- Drafts are kept per channel when switching.
- No permission to send: the composer is replaced by a centered muted line "You do not have permission to send messages in this channel."
- Slowmode or rate-limited: inline muted countdown next to the send button.

### 4.7 Member panel (Telegram "group info" style)

- Header (60 px): "Members" + count, close button.
- Search field (pill).
- Sections, each with a `label`-style header "MAINTAINER — 2":
  1. One section per **hoisted role** (highest position first), containing online members whose highest hoisted role is that role.
  2. "ONLINE — n" for the remaining online members.
  3. "OFFLINE — n" (names in `textMuted`, collapsed by default when large).
- Member row (radius 10): 34 px avatar with presence shape, name (500), a small star icon for the owner, and an optional activity line in `textMuted` ("In voice · General", "Editing main.rs").
- Click → profile popover/dialog. Right-click → Profile · Mention · Copy identity fingerprint · (permitted) Roles ▸ (checkbox list) · Kick · Ban.
- Virtualized for large servers.

### 4.8 Member profile dialog (380 px)

- 64 px avatar with presence, display name (19, 600), presence label.
- Sections with `label` headers: **IDENTITY KEY** (fingerprint in mono, grouped in 4-character blocks, copy button) · **ROLES** (outlined chips, never colored) · **MEMBER SINCE** · **ACTIVITY**.
- Buttons: Mention (secondary) · Message (primary, disabled with tooltip "Direct messages are coming in a later phase").

### 4.9 Quick switcher (`Ctrl+K`)

- Centered near the top (110 px from top), 600 px wide, bg `elevated`, radius 16, scrim behind.
- Large input "Jump to a channel, server or member". Results: channels across **all** servers (with server name as subtitle), servers, and members of the current server. Ranking: prefix match > word match > contains; recently visited first when the query is empty.
- `↑ ↓` navigate, `Enter` open, `Esc` close. Footer shows these key hints.

### 4.10 Voice view (main area when a voice channel is opened)

- Header as §4.3 with "3 connected" subtitle.
- **Tile grid**: one tile per participant plus an extra tile per active screenshare. Tiles are 16:9, radius 14, bg `surface` (screenshare tiles `rail`), 1 px `border`. Grid columns: 1 tile → 1, ≤4 → 2, ≤9 → 3, else 4; tiles sized to fit without scrolling.
- Tile content: camera video when on; otherwise the 72 px avatar. Bottom-left name pill (bg `elevated` 90%) with a mic-off icon when muted. Screenshare tiles show the stream and an inverted `LIVE` pill top-right.
- **Speaking indicator**: 2 px `text`-colored tile border (no green).
- **Focus mode** (Discord): click a tile (or its expand button on hover) to show it large with the other tiles in a horizontal strip underneath. Click again or press `Esc` to return to the grid.
- **Control bar** centered at the bottom: Mute · Camera · Screenshare · Deafen · (gap) · Disconnect (wide inverted pill). Toggle buttons are 48 px circles; active state is inverted.
- Screenshare button opens a source picker dialog (screens/windows; on Linux this is handed to the xdg-desktop-portal picker) with quality options: 720p30 / 1080p30 / 1080p60 / Source.
- Not connected yet: centered prompt with the channel name, participant avatars and a "Join voice" primary button.
- Voice logic is a later phase; in Phase 1 this view runs on mock data only (§11).

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

- Optimistic send: message appears immediately with the clock icon, switches to double check on server ack.
- Edits update in place with "edited". Deletes remove the bubble with a 100 ms fade (no "message deleted" placeholder).
- New message while scrolled up: don't auto-scroll; increment the jump-to-bottom badge.
- New message while at the bottom: stick to the bottom.
- Entering a channel: scroll to the unread separator if there is one, otherwise to the bottom.
- Unread and mention counts update live in the channel list, server rail and window title ("(3) Opencord").
- Typing indicator: send `StartTyping` at most every 8 s while typing; show others' typing for 10 s or until their message arrives.

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
- **Appearance:** theme choice cards **System · Dark · Light** (each card shows a miniature of the layout in that theme), message density (Comfortable / Compact — compact removes bubbles and uses Discord-like left-aligned lines, still monochrome), font size slider (12–18), reduce motion switch, **window frame** (Auto · Custom · System, §3.1; applies after restart if the platform needs it).
- **Windows & behavior** (desktop only): close button minimizes to tray (on by default on Windows and Linux, off on macOS), start minimized, launch at login, restore last window position.
- **Voice & audio**, **Video**, **Screen share**, **Sounds** and **Voice diagnostics:** full specification in §17.
- **Notifications:** desktop notifications, mentions only, message sound, flash taskbar.
- **Keybinds:** the table from §7.
- **Trusted servers:** list of servers with address and pinned fingerprint, "Forget" action.
- **About:** version, license, links.

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
  - Linux: Hyprland (no controls, content fills the tile), GNOME (round controls following the button-layout setting, rounded corners, drag, double-click maximize, resize from all edges), KDE (system frame).
  - Windows 11: caption buttons look native, snap layouts appear on maximize hover, Aero Snap and resize work, close hover is inverted.
  - macOS: traffic lights sit above the rail, the header row drags the window, fullscreen hides the spacer.
  - Switching Window frame to System works on all three.
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
- **Hover intent:** tooltips after 450 ms, hover action bars appear immediately but disappear with a 150 ms delay so the pointer can reach them.
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
