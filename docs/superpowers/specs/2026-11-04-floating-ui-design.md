# The floating UI: design

The desktop app's look, rebuilt: every part floats with its own border under one fixed title row. UI only; no server, core or protocol change.

**The prototype is the authority.** `2026-11-04-floating-ui-prototype.dc.html`, beside this file, is what the owner approved, tuned by hand over several rounds. This document transcribes it. Where the two disagree, the prototype's CSS wins: stop and ask.

**Reference material** (git-ignored, on this machine), taken from the running prototype in Chromium at 2× scale:

| What | Where |
|---|---|
| Screens, `<scene>_<width>_<theme>.png` | `.superpowers/sdd/floating-ui/reference/` |
| Every panel's rectangle, per scene and width | `.superpowers/sdd/floating-ui/reference/measure.json` |
| Animations frozen at exact times, `<scene>_<ms>.png` | `.superpowers/sdd/floating-ui/film/` |
| Sampled curves | `.superpowers/sdd/floating-ui/film/anim.json` |
| Compare the app with all of it | `.superpowers/sdd/floating-ui/tools/compare.py` |

Numbers are logical pixels. Colours are the existing `OcColors` tokens by name.

## 1. Goals

- **1:1 with the prototype:** frames, sizes, paddings, radii, text styles, states and motion.
- **Even better where Flutter allows it:** springs keep their speed when interrupted, and the main page's shadow follows its notch.
- **Nothing the app does today is lost.** The prototype's data and some of its controls are stand-ins; §8 lists what stays from the real app.

**Not in scope:** the server, the core, the protocol, the phone apps (`mobile/`), and the media viewer's window.

**Anti-goals:** a screen that passes its tests and looks wrong; springs that bounce layout or text; a slow app. The owner's words on the first springs: "a little bit off". §4 says why they were, and those mistakes must not come back.

## 2. The frame

```
┌───────────────────────────────────────────────────────────────────────┐
│ ← →            [OD] Opencord Dev / # general                  – □ ×   │ 40, flush, no border
│┌──┐┌──────────┐┌────────────────────────────┐┌─────────┐              │
││  ││ head  52 ││ main float, header 52  ┌───┐│ side     │              │
││  │├──────────┤│                        │ ▯ ││ slot     │  ▯ = the pane
││  ││ channels ││                        └───┘│          │  button, only
││  ││          ││                             │          │  while the
││  │├──────────┤│                             │          │  slot is shut
││  ││ voice    ││                             │          │              │
││  │├──────────┤├────────────────────────────┤│          │              │
││  ││ me    62 ││ composer 62                ││          │              │
│└──┘└──────────┘└────────────────────────────┘└─────────┘              │
└───────────────────────────────────────────────────────────────────────┘
```

- **Ground:** the whole window behind the floats is `rail`.
- **Gap:** `8` between floats and to the window's left, right and bottom edges. No gap between the title row and the floats: the row's 40 is the gap.
- **Columns:** rail `62`, left column `288` (the owner can drag it, §5.7), main takes the rest, side slot `264` for People or `340` for Music.

### 2.1 Rectangles (the check for every width)

`[left, top, width, height]`, voice connected, right pane open where it fits. The whole set is in `measure.json`.

| Width × height | rail | left column | main float | composer | side slot |
|---|---|---|---|---|---|
| 1440 × 900 | 8,40,62,852 | 78,40,288,852 | 374,40,786,782 | 374,830,786,62 | 1168,40,264,852 |
| 1280 × 800 | 8,40,62,752 | 78,40,288,752 | 374,40,626,682 | 374,730,626,62 | 1008,40,264,752 |
| 1180 × 800 | 8,40,62,752 | 78,40,288,752 | 374,40,526,682 | 374,730,526,62 | 908,40,264,752 |
| 1100 × 800 | 8,40,62,752 | 78,40,288,752 | 374,40,718,682 | 374,730,718,62 | overlay, §5.18 |
| 980 × 800 | 8,40,62,752 | 78,40,288,752 | 374,40,598,682 | 374,730,598,62 | overlay |
| 940 × 560 | 8,40,62,512 | 78,40,264,512 | 350,40,582,442 | 350,490,582,62 | overlay |
| 759 × 700 | 8,40,62,652 | overlay, §5.18 | 78,40,673,582 | 78,630,673,62 | overlay |

Inside the left column at 1280 × 800: head `78,40,288,52`; channels `78,100,288,510.6`; voice card `78,618.6,288,103.4`; me `78,730,288,62`.

With the right pane shut, main and composer stretch to the right gap: at 1280 main is `374,40,898,682`, the notch `1220,40,52,52` and the pane button `1224,44,44,44`.

On a voice page there is no composer, so main is the full height: at 1280 `374,40,626,752`.

### 2.2 Breakpoints

`ShellLayout` keeps its four classes and limits:

| Class | Width | Left column | Side slot |
|---|---|---|---|
| wide | ≥ 1180 | inline, default 288 | inline |
| medium | 980–1179 | inline, default 288 | overlay |
| narrow | 760–979 | inline, default 264 | overlay |
| compact | < 760 | overlay, hamburger in the header | overlay |

The window's minimum stays 940 × 560, so compact is reached only in tests, or if that minimum changes.

### 2.3 A float

`OcFloat`: background `sidebar`, 1 px `border` inside the box, radius `16`, the float shadow. Its content sits inside the border and is clipped to radius `15`.

| Shadow | Dark | Light |
|---|---|---|
| Float | `0x73000000`, offset (0, 8), `blurRadius` 19.9 | `0x14111114`, offset (0, 6), `blurRadius` 16.4 |
| Menu | `0x80000000`, offset (0, 4), `blurRadius` 13 | `0x1F111114`, offset (0, 4), `blurRadius` 13 |

These equal the prototype's CSS (`0 8px 24px`, `0 6px 20px`, `0 4px 16px`): Flutter's `blurRadius` for a CSS blur `b` is `(b/2 − 0.5) / 0.57735`.

The main float's background is `chat`. New colour token: `iconDisabled`, `selected` in dark and `#C1C1C8` in light.

## 3. Icons and type

- **Icons:** the prototype draws stand-ins. The app keeps Material Symbols Rounded through `OcIcons`, at the prototype's sizes:

  | Prototype | `OcIcons` | Notes |
  |---|---|---|
  | back / forward | `arrowBack`, `arrowForward` | `arrowForward` is new (`arrow_forward`) |
  | right pane | `dockToLeft` | new (`dock_to_left`) |
  | others | the widget's existing icon | `mic`, `micOff`, `headphones`, `headsetOff`, `settings`, `callEnd`, `graphicEq`, `screenShare`, `musicNote`, `videocam`, `videocamOff`, `search`, `pushPin`, `moreVert`, `tag`, `campaign`, `volumeUp`, `lock`, `close`, `attachFile`, `mood`/the composer's own, `mail`, `group`, `chat` |

  New icons go in `icons` in `app/tool/build_icons.dart`, then `dart run tool/build_icons.dart`.
- **Type:** Inter and JetBrains Mono through `OcText`. Sizes below are `size/weight`. Line heights come from `OcText` unless given. Letter spacing is `0` unless given: `OcText` must say so, or each style inherits `0.25` from Material's default text style and runs 3–4 % wider than the prototype.

## 4. Motion

This is the part the owner cares about most.

### 4.1 What went wrong first, and the rules it left

The first version used one bouncy 620 ms spring on everything. It felt off because:

- text rewrapped on every frame, and again on the overshoot;
- layout overshot;
- hover effects and small arrows sprang;
- closing took as long as opening.

**So:**

1. **Layout never overshoots.** Bounce is only for what arrives (menus, popovers, the voice controls, tiles).
2. **Text never rewraps during a pane's animation.** Chat and tiles take their final width once, at the start, and only the frame moves (§4.4).
3. **Closing is faster than opening.**
4. **Hover and small state changes are plain, short and have no spring.**
5. **What is used constantly barely animates:** switching channels, servers or tabs doesn't animate at all.
6. **Small changes of scale:** nothing grows from less than `0.96`, except the pane button and a camera's silhouette, which are small and quick.

### 4.2 The tokens

A spring is `SpringDescription.withDurationAndBounce(duration, bounce)`: `mass 1`, `stiffness (2π/d)²`, `damping (1−b)·4π/d`, the same model as the prototype's `spring()`.

| Token | Duration | Bounce | 90 % there | Settled | Used by |
|---|---|---|---|---|---|
| `pane` | 0.28 s | 0 | 174 ms | 342 ms | a side pane opening, the voice card appearing, the left column's width, drawers opening, the notch re-opening |
| `paneOut` | 0.20 s | 0 | 124 ms | 244 ms | a side pane closing, the voice card leaving, the notch closing, drawers closing on the right, the rail popover sliding between icons |
| `pop` | 0.20 s | 0.10 | 109 ms | 177 ms | menus, popovers, the rail popover's scale, dialogs, the pane button coming back, the voice controls appearing, the join prompt, a camera's silhouette |
| `tile` | 0.30 s | 0.07 | 170 ms | 295 ms | a tile arriving |
| `move` | 0.30 s | 0 | 186 ms | 366 ms | a tile moving or resizing |

"Settled" is the last moment it is more than 0.4 % from the target. An uninterrupted spring follows the formula exactly. **Better than the prototype:** retargeted mid-way, a spring starts from its current value *and speed*, so a quick open then close never jerks.

| Plain | Duration | Curve |
|---|---|---|
| hover colours | 120 ms | `easeOut` |
| press scale to `0.97` | 80 ms | `easeOut` |
| small rotations (a dropdown's arrow) | 140 ms | `easeOut` |
| what leaves on its own (voice controls, pane button) | 130 ms | `easeIn` |
| a fade in | 100–120 ms | `easeOut` |
| rail icon morph, its indicator | 150 ms | `easeOut` |

CSS's `ease-in` and `ease-out` are Flutter's `Curves.easeIn` and `Curves.easeOut`. Under reduced motion (`OcMotion`'s existing rule) every one of these takes no time.

### 4.3 Sampled curves (tests pin these)

From the running prototype. `t` is ms after the click; the first frame is `t = 0`.

**The right pane opens** (pane button clicked, at 1280, People):

| t | 0 | 40 | 80 | 120 | 160 | 200 | 280 | 360 |
|---|---|---|---|---|---|---|---|---|
| side slot width | 0 | 59.9 | 141 | 197.6 | 230.4 | 247.6 | 260.3 | 264 |
| main width | 898 | 836.2 | 752.7 | 694.5 | 660.6 | 642.9 | 629.8 | 626 |
| notch size (starts after 50 ms) | 52 | 52 | 39.3 | 18.5 | 7.3 | 2.7 | 0.3 | 0 |
| pane button opacity | 1 | .86 | .55 | .12 | 0 | 0 | 0 | 0 |
| pane button scale | 1 | .946 | .821 | .650 | .6 | .6 | .6 | .6 |

**The right pane closes** (its X):

| t | 0 | 40 | 80 | 120 | 160 | 200 | 280 | 360 |
|---|---|---|---|---|---|---|---|---|
| side slot width | 264 | 169.6 | 75.3 | 29.2 | 10.5 | 3.5 | 0 | 0 |
| notch size | 0 | 11.8 | 27.8 | 38.9 | 45.4 | 48.8 | 51.3 | 52 |
| pane button opacity | 0 | .57 | .94 | 1 | 1 | 1 | 1 | 1 |
| pane button scale | .6 | .751 | .904 | .973 | .995 | 1 | 1 | 1 |

**People → Music** (side slot width): 264, 281.3, 304.6, 320.9, 330.3, 335.3, 338.9, 340 at the same times.

**The voice card leaves** (height): 103.4, 66.5, 29.5, 11.4, 4.1, 1.4, 0, 0.

**The voice controls hide** (`translateY`, opacity): 0/1, 2.2/.86, 7.2/.55, 14/.12, 16/0 at t = 0, 40, 80, 120, 160.

**A menu opens:**

| t | 0 | 40 | 80 | 120 | 160 | 200 |
|---|---|---|---|---|---|---|
| scale | .97 | .981 | .993 | .998 | 1 | 1 |
| opacity | 0 | .38 | .76 | .93 | .99 | 1 |
| offset from its place (y) | −2 | −1.25 | −.48 | −.13 | −.02 | 0 |

### 4.4 No rewrap

While a pane opens or closes, the message list and the tile stage are laid out at `mainInner`, the width main will have at the end, whatever its frame measures at that moment:

```
mainInner = max(240, windowWidth − 2·gap − railPart − leftPart − sidePart − 2)
railPart  = 62 + gap
leftPart  = left column inline ? leftWidth + gap : 0
sidePart  = slot inline and open ? slotWidth + gap : 0
```

An `OverflowBox` anchored top-left inside the float gives them that width. So text jumps once, on the first frame, then stays still while the frame slides over it. Header buttons and the composer follow the frame. Measurable: `RenderParagraph` layouts of a message don't change between t = 40 and t = 360.

## 5. The parts

Each part names the widget that becomes it. A number here replaces the widget's current number.

### 5.1 Title bar (`AppTitleBar`, new; replaces `HeaderBar`)

- **Row:** height `40`, full width, background `rail` (the ground), no border, padding `0 8`. Three zones: left, centre, right, with the centre at the window's true centre.
- **Drag:** the row's empty space is `WindowDragArea` (drag, double-click, right-click menu). The floats and the gaps between them no longer drag.
- **Left:** `OcIconButton`s, compact (28, icon 18), `2` apart: back, forward. Enabled colour as any icon button; disabled colour `iconDisabled`, no hover.
  - Tooltip: `Back to #dev-core · Opencord Dev`, `Forward to Mira Okafor`, `Back to General · Opencord Dev`, `Back to Friends`, `Back to Message requests`; `Back`/`Forward` when disabled.
- **Centre (the crumb):** a row, gap `7`, at most 56 % of the window's width, ignoring the pointer so it drags:
  - server icon: `ServerAvatar`, 20 px, radius `6`, background `selected`; initials 8.5/700 in `text`. In Private messages: a 20 px, radius 6 tile with `OcIcons.chat` at 13;
  - server name 13/500 `textSecondary`, one line, ellipsis;
  - `/` 13 `textMuted`;
  - the channel's icon (`channelIcon`) at 16 in `textSecondary`, or the peer's 20 px avatar (initials 8), or `group` for Friends, `mail` for Message requests;
  - the channel's name without `#`, or the peer's name, or `Friends`, or `Message requests`: 13/600 `text`, one line, ellipsis.
  - With no server and no contacts: nothing.
- **Right:**
  - **Linux:** `LinuxWindowButtons`: 24 px circles, background `hover`, hover `selected`, `6` apart, last one's edge `8` from the window's edge. The button's hit extent is 30 here, so the row's right padding is 5. The glyphs stay GNOME's, with minimise low in its circle: the owner's call over the prototype's centred stand-in. The user's button layout still decides which show and on which side; left-hand ones come before the back arrow.
  - **Windows:** `WindowsCaptionButtons`, 46 × 32, flush with the top right corner, unchanged.
  - **macOS:** native traffic lights: band height `40`, inset `14` (`bandHeight`, `lightsInset` in `MainFlutterWindow.swift`; `bandWidth` becomes `78`). The arrows start at x = 78, or 8 in full screen.
- **Fullscreen:** the row stays, without window controls.

**Back and forward:**

- History holds what the user opened: a channel, a voice channel's view, a private chat, Friends, Message requests. At most 100 entries, for this run of the app only. Going somewhere new drops what's ahead.
- A step that lands on something gone (a deleted channel, a left server, a closed chat) skips to the next that exists in that direction; if none does, the button is disabled.
- Stepping to a voice channel opens its view and **never joins or leaves voice**.
- **Also (my additions):** mouse buttons 4 and 5, and Alt+←/→ (⌘[ and ⌘] on macOS).

### 5.2 Rail (`ServerRail`, `RailItemView`) and its popover (`RailPopover`, new)

- **Rail:** a float, width `62`. List padding `8 0`, items `6` apart. **No server names and no "Private" text under the icons.** Items still drag to reorder, and keep their right-click menus.
- **An item:** icon `46`, padding `3`, so `52` tall. Indicator pill at the float's left edge, `4` wide, right corners radius `4`, centred on the icon, `text` colour, height `0`/`8` unread/`18` hover/`34` selected, 150 ms.
- **Icon:** radius `23` → `14` on hover or when selected, 150 ms; background `hover` → `selected` on hover (120 ms); selected `accent` with `onAccent`. Initials 15/600. Glyph 20.
- **Badge:** §5.4's badge at top `−4`, right `−4`, ringed `2` in `sidebar`.
- **Connecting chip:** `18` circle, background `sidebar`, right/bottom `−3`, icon 12 in `textSecondary` (`sync`, or `error` when failed).
- **Divider** after Private: `1` tall, inset `18` each side, `2` above and below (plus the list's gap).
- **Add server:** at the bottom, `44` tall, `4` from the sides and bottom, radius `10`, icon 20 `textSecondary`; hover `hover`/`text`.
- **The popover replaces the tooltip.** Shows on hover or keyboard focus, to the right of the item, centred on it, never takes the pointer:
  - left edge at `gap + 62 + 8` = 78, width to content, between `168` and `280`;
  - padding `10 12`, radius `12`, background `elevated`, 1 px `border`, the menu shadow;
  - an arrow on its left: an 8 px square rotated 45°, centred, `elevated` with `border` on its two outer sides, 5 px over the edge;
  - **line 1:** the server's name, 13.5/600 `text`, one line;
  - **line 2:** 12 `textMuted`: `10 online · 13 members`, or `Connecting…`, `Reconnecting…`, `Connection failed` (the existing labels);
  - **when anyone is in voice on that server:** `8` below, a 1 px `border` line, `8` more, then a row with gap `10`: up to **4 faces** and a count.
    - Faces: 26 px avatars, each overlapping the last by `6`, ringed `2` in `elevated`.
    - More than 4: a 26 px circle, `selected`, `+6`, 10.5/600 `text`. 10 people show 4 faces and `+6`.
    - Count: `volumeUp` at 16 in `textSecondary`, `4`, then `10 in voice`, 12 `textSecondary`.
  - Private messages: name `Private messages`; line 2 `2 friend requests · 2 message requests` (`1 friend request`, `1 message request`, singular at one). Add server: `Add a server`, no line 2.
- **Its motion:**
  - appears: opacity 80 ms `easeOut`, scale `0.96` → 1 on `pop`, from its left centre;
  - leaves: 90 ms after the pointer goes, opacity and scale 80 ms `easeIn`;
  - moving between icons within those 90 ms: it stays up and slides to the new item on `paneOut`, without re-appearing;
  - while dragging a rail item: hidden.
- **Screen readers** read what the tooltip gave: `name, state`.

### 5.3 Server head (`ServerHead`, new, from `_ServerHeader`)

- A float of its own above the channel list, `52` tall, padding `0 10 0 14`, gap `8`. The whole float is the button: hover background `hover`, 120 ms.
- Name 15/600 `text`, line height 1.25, ellipsis. Below it `ConnectionDot` (6), gap `6`, status 12 `textMuted`, line height 1.25. Status text as today.
- `expandMore` at 18 `textMuted`, turning 180° while the menu is open, 140 ms `easeOut`.
- **Menu:** today's entries, at x = the float's left, y = float's bottom + 6, **as wide as the float, at most 288**.
- **In Private messages** the same float, not a button, no arrow: `Private messages`, then the dot and `2 online`.

### 5.4 Channels float (`ChannelSidebar`, `ChannelRow`, `VoiceChannelRow`)

- A float taking the column's free height.
- **Search pill:** margin `8`, height `36`, padding `0 12`, gap `8`, radius `18`, background `hover` (hover `selected`), icon 18 and `Search` 14 in `textMuted`. `KeyHint`: JetBrains Mono 11, padding `1 5`, 1 px `border`, radius `5`, `textMuted`.
- **List:** bottom padding `8`, top `0`.
- **Category row:** padding `8 12 4 10`. Toggle: chevron 16, gap `4`, label 11.5/700, spacing 0.7, upper case, `textMuted`. The chevron turns −90° when collapsed, 150 ms. `+`: `20` × `20`, icon 16 `textMuted`, **only while the row is hovered or the `+` focused** (opacity 120 ms).
- **Channel row:** margin `0 8 2`, height `56`, padding `0 8`, gap `10`, radius `10`; `selected` when open, `hover` on hover.
  - Glyph `40`, radius `12`, background `selected`, icon 20 `textSecondary`; inverted when open.
  - Lock chip: `16` circle at right/bottom `−3`, background `sidebar`, icon 10 `textSecondary`.
  - Line 1: name 14/500 `textSecondary` (600 `text` when unread or open), gap `8`, time 11 `textMuted`.
  - Line 2, `2` below: preview 12.5 `textMuted`, gap `8`, badges.
- **Badges (`UnreadBadge`, `MentionBadge`, everywhere):** height and minimum width `18`, padding `0 5`, radius `9`, 11/600, `accent`/`onAccent`.
- **Voice row:** margin `0 8 2`, height `36`, padding `0 10`, gap `10`, radius `10`; icon 18; name `textSecondary`, 600 `text` when open; count 12.5 `textMuted`.
- **Person in voice:** height `30`, margin `0 16 0 42`, gap `8`, 13 `textSecondary`; avatar 22 (initials 8.5), ring `2` `text` while speaking; state icons 16 `textMuted`; `LIVE` 10.5/700, spacing 0.4, padding `1 6`, radius `5`, `accent`/`onAccent`.

### 5.5 Voice card (`VoiceConnectedPanel`)

- A float of its own, shown only in voice. Padding `8 8 10`, gap `8`.
- **Line:** gap `6`. The clickable part: padding `2 6`, radius `8`, hover `hover`; above, the connection icon at 16 then `Voice connected` 13/600 (`6` apart); below, `1` under, `General / Opencord Dev` 12.5 `textMuted`. Disconnect: compact button, icon 20.
- **Toggles:** equal widths, `8` apart, height `36`, radius `10`, background `hover`, icon 20 `textSecondary`; hover `selected`/`text`; on `accent`/`onAccent`.
- **Appears and leaves by height:** `pane` opening, `paneOut` leaving, clipped to radius 16. Its gap appears with it, so nothing jumps. While leaving it keeps showing the last channel's name.

### 5.6 User panel (`UserPanel`)

- A float, `62` tall (`max(resting composer, 62)`, so it stays level with the composer), padding `0 6 0 8`.
- **Who:** padding `4`, radius `10`, hover `hover`, gap `8`: avatar 32 (initials 12), name 13/600, status 11 `textMuted`.
- **Buttons:** compact (28, icon 18), active `selected`/`text`. Chevrons: `14` × `28`, radius `6`, `expandMore` at 12, weight 600, `textMuted`; hover `selected`/`text`.
- The cog still opens User settings. The prototype's small theme menu there was only for trying themes.

### 5.7 Splitter (`ShellSplitter`, new)

- In the `8` gap between the left column and main, inline layouts only. Hit area: the gap plus `4` on each side. Cursor: resize column.
- **Drag** changes the left column's width live, with no animation: minimum `232`, maximum `min(420, window − 62 − 4·gap − side − 380)`. If the window or the pane shrinks past the maximum, the column follows on `pane`, and returns to the chosen width when there's room.
- **Pill while hovered, focused or dragging:** `3` × `40`, radius `2`, `textSecondary`, centred in the gap: opacity 100 ms, height from half to full, 120 ms `easeOut`.
- **Keyboard:** focusable; ←/→ steps `16`, Home resets. **Double-click resets** to the class's default, on `pane`.
- **Remembered** across launches: key `ui.sidebarWidth`.
- **Screen readers:** a slider, `Resize the channel list`, 232–420.
- The head, channels, voice card and user panel all follow the width.

### 5.8 Main float and header (`ChatArea`, `ChatHeader`; `PanelHeader`, new)

- **Main float:** background `chat`, the notch (§5.9) while the side slot is empty.
- **`PanelHeader`:** height `52` including a 1 px `border` line at the bottom, background `sidebar`, padding `0 8 0 16`, gap `10`. Doesn't drag.
  - Title 15/600 `text`, line height 1.25; subtitle 12.5 `textSecondary`, line height 1.25, including the typing line. Both one line, ellipsis.
  - Buttons: normal (36, icon 20), `2` apart.
  - **Compact only:** a hamburger first, margin-left `−8`.
- **Text channel:** search, pinned messages, More.
- **Voice channel:** hide-non-video, More. (The chat button stays where it appears today.)
- **Right inset of the buttons:** `8 + 54·(notch/52)`, so `62` beside the pane button and `8` once it has gone.
- `ConnectionBanner` and `AudioPlayerBar` stay under the header, inside the float.

### 5.9 The pane button and the notch (`NotchedFloat`, `PaneButton`, new)

- One button replaces the Members and Music buttons: `dockToLeft`, tooltip `Open the right pane`.
- **Notch:** while the side slot is shut, a `52` square is cut from main's top right corner. The page's outline runs around it with radius `16` at all three corners: convex where it meets the top edge, concave inside, convex where it meets the right edge. The bottom of the notch lies on the header's bottom line.
- **Button:** `44` square in the notch, **`4` from its top, right, left and bottom** (the owner's exact choice), background `sidebar`, 1 px `border`, the float shadow, radii top-left `10`, top-right `16`, bottom-right `10`, bottom-left `16`. Icon 20 `textSecondary`; hover `selected`/`text`; press `0.97`.
- **Click:**
  - button: scale to `0.6` towards its top right corner, opacity to 0, both 130 ms `easeIn`, then hidden;
  - after **50 ms**, the notch closes from 52 to 0 on `paneOut`; the header's buttons slide right with it;
  - at once, the side slot opens on `pane`.
- **The pane's X:**
  - side slot closes on `paneOut`;
  - notch opens on `pane`;
  - button: scale `0.6` → 1 on `pop`, opacity 100 ms `easeOut`.
- **Drawing (better than the prototype):** the notched outline is one path, used for the background, the border, the shadow and the clip, so the shadow follows the notch. At notch `n`: joints radius `min(16, n/2)`; the path is intersected with the plain rounded rectangle, so as `n` reaches 0 the shape is exactly the plain float.
- Shown in server views only. Private messages has no side slot, no notch and no button.
- **What's in the slot:** People, or Music where it's offered (§5.13). `Ctrl+Shift+U` still toggles People. The `/music` command still opens Music.

### 5.10 Pinned float (`PinnedBar`)

- Over the messages: `8` under the header, left and right `12`, above the list, background `sidebar`, 1 px `border`, radius `12`, the float shadow, padding `6 6 6 12`, gap `10`.
- Bar `2` × `30` `text`, radius `1`. `Pinned message 2 of 2` 12.5/600 `text`; preview 12.5 `textSecondary`, one line. Close: compact (28, icon 18).
- Behaviour as today. **The messages' top padding becomes `68` while it shows**, and the day pill moves to `64` from the top.

### 5.11 Messages (`MessageList`, `Bubble`)

**Chats start from the bottom,** as the real list already does. Don't change that.

- **Column:** at most `880` including its `16` side padding, centred; top and bottom padding `12`.
- **Rows:** `6` apart, `2` when grouped. Avatar 36 (initials 12.5), gap `10`, at the group's last row.
- **Bubble:** at most `560` wide, padding `7 12 8`, radius `16`, `5` on grouped edges, 1 px `border` on both kinds, `bubbleIn`/`bubbleOut`. A mention adds a `2` `text` left edge.
- **Name line:** 13, name 600 `text`, role `· Maintainer` `textMuted`, `1` below.
- **Time:** 11 `textMuted`, floating at the end of the last line with `10` before it. Own messages add the double tick at 13.
- **Reply:** margin `2 0 4`, padding `3 8`, `2` `textSecondary` left edge, radius `6`, background `hover`, 12.5.
- **Quote:** margin `2 0 3`, left padding `8`, `2` `textMuted` left edge, `textSecondary`.
- **Inline:** mention padding `0 3`, radius `4`, `mentionBg`, 600. Code: padding `1 4`, radius `4`, `mentionBg`, mono 12.5.
- **Reactions:** `5` under, `4` apart, height `24`, padding `0 8`, radius `12`, background `selected`, 12.5/500; own inverted.
- **Unread line:** margin `6 0`, gap `12`, 12.5/600 `textSecondary`, lines `selected`.
- **System pill:** padding `3 10`, radius `12`, background `hover`, 12.5 `textSecondary`, gap `6`, icon 16.
- The compact message style stays as it is.

### 5.12 Composer float (`ComposerFrame`)

- A float of its own under main, `8` apart, **`62` tall at rest** at the default text size, `sidebar`, level with the user panel. It grows upward with more lines, a reply, or staged files; those sit inside it.
- Row padding `0 8`, gap `2`. Buttons normal (36, icon 20), centred at rest. Field padding: `8` sideways, `20.5` vertically; hint in `textMuted`.
- The private composer and every bar that takes its place (`RequestBar`, blocked, no shared server, not reachable, no connected server) are floats in the same spot, the same way.

### 5.13 Side slot (`SideSlot`, new; `MemberPanel`, `MusicPanel`)

- A float, `264` or `340` wide, full height. The width changes on `pane` when opening or growing, on `paneOut` when closing.
- **Content keeps its width while the slot opens**, so it slides in from behind the window's edge rather than being squeezed.
- **Header:** `52`, 1 px `border` line, padding `0 8 0 16`, gap `6`. Close: compact (28, icon 18), tooltip `Close`.
  - Where music is offered: the switch. Otherwise `Members` 15/600 and the count 13 `textMuted`.
- **Switch** (replaces the old buttons): padding `2`, radius `10`, background `hover`, gap `2`; each `People`/`Music`: height `28`, padding `0 12`, radius `8`, 13/500 `textSecondary`; on: `accent`/`onAccent`.
- Swapping panes: new content fades in, 120 ms.
- **People:**
  - search: margin `8`, height `36`, padding `0 12`, gap `8`, radius `18`, background `hover`, icon 18, 13.5, `textMuted`;
  - section: padding `10 16 4`, 11.5/700 upper case `textMuted`; `In this voice channel — 5` first when there;
  - row: min height `44`, margin `0 8`, padding `4 8`, radius `10`, hover `hover`, gap `10`, avatar 34 (initials 12);
  - name 14/500 (star 16 `textMuted`, gap `4`); activity 12.5 `textMuted`; offline rows at 0.55;
  - list bottom padding `8`.
  - `NowPlayingCard` stays under the header, as today.
- **Music,** padding `12`:
  - art: full width, `172` tall, radius `14`, `surface`, 1 px `border`, the music note at 48 `textMuted` when there's no artwork;
  - title 15/600, `12` below; artist 12.5 `textSecondary`; `Added by …` 11 `textMuted`, `2` below;
  - progress `4` tall, `12` below, radius `2`, `selected` under `text`; times 11 `textMuted`, `4` below;
  - transport: centred, `10` apart, margins `6 0 12`; previous and next normal buttons; play/pause `48` inverted while playing;
  - search as People's, margin `0 0 4`, placeholder `Search, or drop song files here`;
  - `Up next` section; rows: min height `44`, padding `4 8`, radius `10`, hover `hover`, gap `10`; art tile `32`, radius `8`; length 11 `textMuted`.
  - Every other real control (`MusicTransport`'s modes, volume, the ⋯ menu, uploads) stays, in the transport row or the header; see §8.

### 5.14 Voice page (`VoiceView`, `VoiceTileView`, `VoiceDock`, new)

- The main float, no composer. **Stage** padding `12`, tiles `8` apart.
- **Grid:** `gridColumns` stays. Tile shape **16:10**. The group is at most `1040` wide, or `520` with fewer than 3 tiles, centred.
- **Tile:** radius `14`, `surface`, 1 px `border`; speaking: a `2` `text` border drawn over it; a screen: `chat`. Avatar `72` (initials 24), at most `0.61` of the height inside its border (§9).
- **Name pill:** left and bottom `8`, padding `3 9`, radius `12`, background `sidebar`, 12.5/600 `text`, gap `5`, icon 16.
- **`LIVE`** at top and right `10`.
- **Not watched yet:** `Tomás Ruiz is live` 14/600, gap `8`, `Watch`: height `32`, padding `0 16`, radius `10`, `accent`/`onAccent`, 14/600, press `0.97`.
- **Focused** (a click, or Watch): stage padding `8`; the tile fills everything above a `76` strip, `8` between; the video fits (`contain`) in a focused tile. **Strip:** the others share the width, each at least `96`, at most `200`; scrolls sideways past that. In the strip: avatar 36, name pill at `4,4`, padding `1 6`, 11; no `LIVE`, no watch prompt.
- **Motion:**
  - a tile's move or resize runs on `move`, as translate and scale from top left;
  - when scale changes by more than 5 %, what's inside fades back in, 140 ms `easeOut` after 70 ms, so it's never seen stretched;
  - tiles arriving: `translateY(6)`, scale `0.97` → in on `tile`, opacity 120 ms `easeOut`, staggered `16` ms each, at most `80` ms. This runs on joining, on opening the view, and for a new tile.
  - A tile keeps its state across all of it: no video restarts.
- **The dock:** inside the page, not in a row of its own.
  - Centred, `14` above the page's bottom edge, at most page width − 24. Scrolls sideways if it must.
  - Padding `7`, radius `31`, background `sidebar`, 1 px `border`, the menu shadow, gap `10`.
  - Round buttons `48`, `surface`, 1 px `border`, icon 20 `text`; hover `selected`; on: `accent`/`onAccent`/`accent` border; press `0.97`.
  - Chat button first where it's on, then a `1` × `28` `border` line. Then mic, camera, screen, soundboard, deafen.
  - Disconnect: height `48`, padding `0 18 0 14`, radius `24`, `accent`/`onAccent`, gap `8`, 14/600, margin-left `6`; hover at 0.88; press `0.97`.
- **It hides:**
  - moving the pointer anywhere over the main float shows it and restarts a **2.5 s** timer;
  - the timer running out, or the pointer leaving the main float, hides it;
  - **it never hides** while the pointer is on it, a control in it has keyboard focus, or a popup it opened is open (soundboard, screen picker, camera menu, audio menus);
  - showing: `translateY(16)` → 0 on `pop`, opacity 100 ms `easeOut`;
  - hiding: 130 ms `easeIn` for both; hidden, it takes no clicks.
  - Opening the page counts as a move, so it shows first and then hides.
- **Before joining (`_JoinPrompt`):** rises `6` on `pop`, with a fade. Glyph `64`, radius `20`, `selected`, `volumeUp` at 20. Name 17/600; `5 connected` 12.5 `textSecondary`; the faces as today; `Join voice`: height `36`, padding `0 18`, radius `10`.

### 5.15 A voice channel's chat

Not in the prototype; my reading of it, for the owner to check.

- **Room for both** (main without a pane, minus 380 + gap, still ≥ 600): the chat goes in the side slot, `380` wide, on the same springs: a float with a `52` header (`Chat · General`, close) over the messages, then the composer float under it. The notch is shut while it's there.
- **No room:** as today, the chat replaces the tiles in the main float, with the composer float under it.

### 5.16 Private messages (`HomeSidebar`, `PrivateHeader`, `FriendsPage`, `RequestsPage`, new)

- **Left column:** the head float (§5.3), a float with the list, the voice card, the user panel.
- **Two tiles** at the top of the list, in a two-column grid, gap `8`, padding `8 8 4`:
  - tile: min height `92`, padding `10`, radius `12`, background `hover`, 1 px `border`, gap `2`; hover `selected`; press `0.97`;
  - top row, `6` above the name: icon tile `32`, radius `10`, background `selected`, icon 20, and the badge at the right;
  - name 13/600, line height 1.2; line 2 11.5 `textMuted`;
  - open: `accent`/`onAccent`, icon tile and badge inverted, line 2 at 0.7.
  - **Friends:** `2 online`, badge = incoming friend requests. **Message requests:** `2 waiting` or `All answered`, badge = requests.
- **Direct messages:** section padding `10 16 4`. Row: margin `0 8 2`, height `44`, padding `0 12`, gap `12`, radius `10`, 14/500 `textSecondary`; open `selected`/`text`; hover `hover`; avatar 32. Unread, close and the right-click menu as today.
- **A chat's header:** `PanelHeader`: avatar 36, gap `10`, name and presence; lock button; relay chip: height `30`, padding `0 8`, 1 px `border`, radius `8`, 12.5 `textSecondary`, gap `6`, chevron 16. Server avatar stays in the chip.
- **Friends page:** `PanelHeader` with `Friends` 15/600, and the tabs at the header's right.
  - Tabs: `Online 2`, `All 3`, `Pending 3`, `Blocked 1`: margin-left `8`, `4` apart; height `30`, padding `0 12`, radius `15`, 13/500 `textSecondary`, hover `hover`; count 11.5 at 0.65, gap `6`; on: `accent`/`onAccent`.
  - Page padding `12`; column at most `760`, centred; fades in, 100 ms.
  - Section: padding `12 10 6`.
  - **Person card:** min height `60`, padding `8 10`, radius `12`, background `sidebar`, 1 px `border`, `6` below, gap `12`; hover `hover`; avatar 40 (initials 13.5); name 14/600; line 12.5 `textMuted`; offline: avatar and text at 0.55.
  - **Friends:** round `36` buttons, `selected`, icon 18, `6` apart, hover at 0.8: Message, More options.
  - **Incoming:** `Accept` and `Decline`. **Sent:** `Cancel`. **Blocked:** `Unblock`. Buttons: height `32`, padding `0 14`, radius `10`, `selected`, 13/600, hover at 0.88, press `0.97`; primary `accent`/`onAccent`.
  - Pending splits into `Incoming — 2` and `Sent — 1`; the Spam fold stays under them.
  - Empty: icon 32 `textMuted`, `8`, the text `textSecondary`; padding `64 16`.
- **Requests page:** `PanelHeader`: `Message requests`, subtitle `They wrote first. Nothing reaches your chats until you accept.`. Page as above.
  - Cards in a grid: each at least `280`, as many columns as fit, gap `8`.
  - Card: padding `12`, radius `14`, background `sidebar`, 1 px `border`, gap `10`.
  - Top row, gap `10`: avatar 40, name 14/600 over `via Opencord Dev` 12 `textMuted`, time 11 `textMuted` at the right.
  - Message: padding `8 12`, radii `16 16 16 5`, `bubbleIn`, 1 px `border`.
  - Buttons, gap `6`, equal widths: `Accept` opens the chat. `Decline` asks first, with the existing `Ignore request?` dialog, then removes it. A click elsewhere on the card opens the chat, where Block lives.
  - Empty: `mail` icon and `No one is waiting for an answer`.

### 5.17 Menus and popovers (`OcMenuPanel`, `PopoverPanel`, `OcPopupRoute`)

Everywhere: right-click menus, dropdowns, popovers.

- **Surface:** padding `4`, radius `10`, `elevated`, 1 px `border`, the menu shadow. Width between `190` and `300`, unless the caller fixes it.
- **Item:** height `32`, padding `0 10`, radius `6`, 13.5 (`OcText.body` at 13.5) `text`; hover or focus `selected`. Icon 18 `textSecondary`, gap `10`. Shortcut, check mark and submenu arrow as today.
- **Divider:** `1` tall, margin `4 6`.
- **Opening:** `pop`, §4.3's table: scale `0.97` → 1 from the corner nearest the trigger, offset `−2` → 0, opacity on the same curve.
- **Closing:** at once, no animation. The chosen action still runs after it's gone.
- **Dialogs** (`showOcDialog`): `pop`, scale `0.97`, fade. Closing 100 ms fade. My call.
- **Tooltips and toasts:** unchanged.

### 5.18 Drawers (`ShellDrawer`, from `_Drawer`)

- **Scrim:** from the title bar's bottom to the window's bottom, full width, over the rail too: `scrim` at 0.6 of its alpha, fade in 120 ms, gone as soon as the drawer starts to close. A click or Esc closes.
- **Right drawer:** floats `8` from the right and bottom, under the title bar: at 1100 × 800, `828,40,264,752`. Enters from `100 % + gap + 24` to the right on `pane`; leaves on `paneOut`. Music: `340`, at most window − 16.
- **Left drawer** (compact): head, channels, voice card and user panel, at x = `gap·2 + 62` = 78, width `min(leftWidth, window − 62 − 32)`: at 759, `78,40,285,652`. Enters from `−24` with a fade (100 ms) on `pane`; leaves in 120 ms `easeIn`.
- The notch closes while the right drawer is open, as inline.
- **Going anywhere closes them:** opening a channel, a chat or Friends shuts whichever drawer is open, on its way out.
- A panel's own X closes the drawer the same way as Esc, on its curve.
- **Home view too:** a compact window gets the hamburger in its headers. (Today it has no way to open the list.)

### 5.19 Everything else

- **`VoiceReconnectBar`:** a float, under the title row, `40` tall, margins `0 8 8`, radius `16`, `elevated`, content as today. Appears and leaves by height on `pane`/`paneOut`. My call.
- **Onboarding and the keyring screen:** `AppTitleBar` without arrows or crumb, over their current content on `chat`.
- **`NoServersView`:** inside the main float, no notch.
- **`OcSize.header` (60)** stays only for the settings dialog's own header.
- **Presence badge:** real shapes stay; a `12` disc whose edge reaches `2` past the avatar's, ringed `2` outside that.
- **Avatar initials:** weight 600; font size from the prototype's table: avatar 20, 22, 26, 32, 34, 36, 40, 72 take 8, 8.5, 9.5, 12, 12, 12.5, 13.5, 24. Any other size takes `0.31 · size + 1.7`, the line through them.
- **`LinuxWindowButtons`** take an `extent`; the media viewer keeps its `36`.
- **Empty panes, errors, tooltips, toasts, settings pages:** unchanged, inside whichever float holds them.

## 6. Keys, for the checks

`ShellKeys` names each frame; the review test writes them under these names, the same as `measure.json`:

`titlebar`, `crumb`, `rail`, `head`, `channels`, `vcard`, `me`, `left`, `splitter`, `main`, `hd`, `acts`, `pin`, `composer`, `side`, `panel`, `notch`, `paneTab`, `dock`, `tiles`, `stage`, `search`, `memSearch`, `railPop`, `srvMenu`, `scrim`.

## 7. Docs

`docs/plans/desktop-ui.md` is the standing description of the app, and this reverses parts of it. §1.7 (motion), §2.3, §2.4, §3, §3.1's "unified headers", §4.1, §4.2, §4.3, §4.4, §4.6, §4.7, §4.10 and §6.2 must say what the app now does.

## 8. What stays from the real app

The prototype shows less than the app does. All of these stay, at the prototype's sizes for the row or surface they sit on:

- Menus' icons, shortcuts, submenus and permission rules. Every right-click menu's entries.
- Rail reordering by dragging; channel, category and voice-person dragging; moving people between voice channels.
- Presence shapes; lock rules; muted badges; `Soon` for voice-less servers.
- Reply, edit, staged files, voice recording, commands, mentions and emoji in the composer. Message hover bar and actions. Compact message mode. Day pill, jump button.
- `MusicStage` (expanded music), uploads, modes, volume, the ⋯ menu, `Music isn't set up`.
- Viewers pill, Stop watching, the soundboard ring and emoji on tiles, per-user audio menus, `No one has video on`.
- The Spam fold, the `Blocked` tab, key banners, the typing line, `OwnDeviceNotice`.
- Keyboard: every shortcut, `Hoverable`'s focus ring and context-menu keys.
- The cog opens User settings; the avatar opens the presence menu.

The prototype's stand-ins that are **not** built: its cog theme menu, the "Pane button" tuner, its Tweaks, its icons, its mock data, `+` that does nothing.

## 9. Claude's calls

Not in the prototype, or a small departure from it. The owner read this list and approved the plan with it on 2026-11-04. Each is one line to change.

1. **Mouse side buttons and Alt+←/→** for back and forward.
2. **In focus mode the dock covers the strip's middle while it shows**, as in the prototype. It hides after 2.5 s. Say if the strip should move up instead.
3. **Decline asks first.** The prototype removed at once, but today's `Ignore` removes the conversation, so its confirmation stays.
4. **Dialogs open on `pop`** like menus, and close with a 100 ms fade.
5. **A voice channel's chat** in the side slot (§5.15).
6. **The reconnect bar as a float** (§5.19).
7. **Strip tiles at most `200` wide.** The prototype stretched them without limit, which looks wrong with one or two people.
8. **`gridColumns` stays.** 4 tiles are 2 × 2 here; the prototype's stand-in grid gave 3 + 1.
9. **The voice card's clip follows its slot** (added at CP3, not yet read by the owner). The prototype animates `grid-template-rows` below `1fr`, where CSS sizes the row twice: the slot follows §4.3, but the card inside is cut to about the slot's height × the slot's fraction, so ground shows under the card as it goes (41 of 66.5 at 40 ms). Here the card fills its slot all the way. Say if it should copy the prototype.
10. **Tiles at the narrow widths** (added in Task 15, not yet read by the owner). `gridColumns` gives the most columns, and the prototype's `190` minimum takes one away where they don't fit, so 6 tiles are 2 × 3 at 1180, 980 and 940 as in the prototype. The grid never scrolls, as today's doesn't: at `narrow-940`, where the prototype's 3 rows run 96 past the stage, its tiles shrink to fit, a group of 453.9 × 434 instead of 556 × 529.8. A short last row starts at the left, as in the prototype; today's was centred.
11. **The avatar's limit** (added in Task 15, not yet read by the owner). This section once said at most half the tile's height, which would make it 60 at 1280, where the prototype draws 72 on a tile 121.7 tall. `0.61` keeps the prototype's 72 at every width and still shrinks it on smaller tiles.
12. **No hover icon or tooltip on a tile** (added in Task 15, not yet read by the owner). The expand icon that showed on hover isn't in the prototype or §8. The prototype's `title` tips, `Focus on …` and `Back to the grid`, aren't built either: a Flutter tooltip sat over the name pill or in the middle of a video for as long as the pointer rested there.
13. **A request's name is 14/600 in `text`** (added at CP5, not yet read by the owner), as §5.16 says. The prototype draws it 12/600 in `muted`, dimmer than nothing around it should be: `.req-who span`, meant for the `via` line, seems to catch the span its template puts around the name. Say if it should copy the prototype.
14. **The private composer has no attach or microphone button** (added at CP5, not yet read by the owner). The prototype draws both, but private messages can't carry files or voice yet, and this plan changes no backend. So the hint starts at the left and Send stays at the right.
15. **The rail's private badge doesn't count message requests** (added at CP5, not yet read by the owner). The prototype's does, but `docs/private-messages.md` says a request makes no sound and puts no badge on the rail, and the Message requests tile already shows the count. A spam request shows no badge anywhere, for the same reason. Say if requests should count.
