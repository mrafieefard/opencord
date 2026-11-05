# After the floating UI: nine changes

What the owner asked for on 2026-11-05, once the floating UI was built. The earlier design, `2026-11-04-floating-ui-design.md`, still holds for everything not named here. Numbers are logical pixels, colours are `OcColors` tokens by name, and every quoted string is exact.

## 1. Goals

1. The right pane resizes by dragging, as the left column does.
2. Music volume is on the panel, with no popover.
3. Compact chat mode is gone, and loading still looks right.
4. The Invisible status is gone.
5. A keybind can be a mouse button, alone or with keyboard modifiers.
6. Code fences work in the formats people type, and the language highlights the code.
7. The voice card has a noise suppression chip that opens a popover with a Test button.
8. The same Test is in Voice settings.
9. Hovering the voice card's signal bars shows the ping.

**Not in scope:** the server, the protocol, and the phone apps, apart from the fence parser in §7.1 so that a message reads the same on a phone.

## 2. The right pane resizes

A second `ShellSplitter`, in the `8` gap between main and the side slot. Everything §5.7 of the earlier design says about the left splitter holds: hit area, cursor, pill, live drag with no animation, double-click and Home reset on `pane`, ←/→ step `16`, remembered across launches.

| Content | Default | Minimum | Key |
|---|---|---|---|
| People | `264` | `232` | `ui.sideWidth.people` |
| Music | `340` | `300` | `ui.sideWidth.music` |
| A voice channel's chat | `380` | `300` | `ui.sideWidth.voiceChat` |

- **Each content keeps its own width.** Switching People → Music still moves on `pane`, to the width Music was left at.
- **Maximum:** `min(480, window − 62 − 4·gap − left − mainMin)`, never below the content's minimum. `mainMin` is `380`, or `600` for a voice channel's chat, the stage's minimum today. `left` is the left column's clamped target width.
- **Who gives way when the window shrinks:** the pane first, down to its minimum, then the left column. Both follow on `pane`, and return to their chosen widths when there's room.
- **Whether a voice chat fits beside the stage** stays decided by the default `380`, so dragging never flips it to the full-page chat.
- **Inline only.** Below `1180` the pane is a drawer at its default width, and has no splitter.
- **The splitter shows only while the slot is open.** It sits at the pane's resting edge, so it can't be grabbed mid-spring.
- **Keyboard direction follows the edge:** ← moves the edge left, so the pane grows.
- **Screen readers:** a slider, `Resize the right pane`.
- **Text:** the message list rewraps live during a drag, as it does for the left splitter. No-rewrap (§4.4) still holds for opening and closing.

## 3. Music volume

`MusicVolumeButton` and its popover go. Under the transport row, a row of its own:

- **Row:** height `32`, gap `8`, the same side margins as `MusicProgress`.
- **Speaker:** the existing round icon button, icon `20`: `volumeUp`, or `volumeOff` when muted or at `0`. Tooltip `Mute music` or `Unmute music`. A click toggles mute.
- **Slider:** fills the row, 0–200 in steps of 1. The same look as `MusicProgress`: track `4`, `text` on `selected`, thumb radius `6` on hover and drag, none at rest. While muted, the active track is `textMuted`.
- **Value:** 11, tabular, `textMuted`, right-aligned in `36`: `100%`, or `Muted`.
- **Dragging while muted unmutes.** The volume applies live, as it does today.
- **Screen readers:** `Your music volume`, value as shown.

The transport stays centred in its row. `MusicVolumeControls` stays in the now-playing card's right-click menu, which isn't this popover.

## 4. Bubble chat mode goes

Removed entirely: `MessageDensity`, `AppSettings.density`, the `Message density` section and `DensityMiniature` in Appearance, `MessageBubble` and `MessageLine`, every `comfortable` / bubble path in the channel and private message lists. Messages are shown in compact mode only.

- **An old saved choice:** the `density` key is ignored and never written again. Someone on Comfortable opens on compact.
- **Loading:** `ChatSkeleton` stays, and is the one loading look. It draws compact rows (small round avatars, author and text lines in `hover`). **The private chat's first load changes from its spinner to `ChatSkeleton`,** so both chats load alike.

## 5. Invisible goes

`SelfPresence` keeps `online`, `idle` and `doNotDisturb`. This reaches the settings cards, the user panel's menu and the tray menu, which all list the enum.

- **Someone saved as Invisible** opens as Online, through `SelfPresenceNotifier`'s existing fallback, and is announced as online. The docs say so.
- **Others can still be offline:** `Presence.offline` and `core.PresenceStatus.offline` stay.

## 6. Keybinds with the mouse

### What can be bound

`Mouse3` (middle), `Mouse4` (back) and `Mouse5` (forward), alone or with any of `CTRL`, `ALT`, `SHIFT` and `LOGO`: `Mouse4`, `CTRL+Mouse4`, `CTRL+SHIFT+Mouse5`.

- **Left and right buttons can't be bound.** They'd fire on every click.
- **Higher buttons can't be bound.** Windows has no messages for them, and gaming mice usually send keys for them anyway.
- **A key with a mouse button that isn't a modifier** (`V` + Mouse 4) isn't a keybind: a non-modifier key records as soon as it goes down, as today.
- **Names for people:** `Middle Mouse`, `Mouse 4`, `Mouse 5`. With modifiers: `Ctrl + Mouse 4`, or `⌃Mouse 4` on macOS.

### How it behaves

- **Listening never takes the click from anyone,** as with keys. Mouse 4 still goes back in a browser.
- **Inside Opencord,** a bound plain Mouse 4 or Mouse 5 no longer steps back or forward while its modifiers are held. Alt+←/→ still do.
- **Hold to talk works,** with press and release, like a key.
- **Several keybinds on one button,** and one action with a key and a button, work as they do for keys (`Held`).

### Per system

| System | How | Note |
|---|---|---|
| X11 | XInput 2 raw button events, on the existing thread | Details 2, 8 and 9 |
| Windows | `WH_MOUSE_LL` on the hook thread, only while a binding uses a button | A slow mouse hook lags the whole system, so it does no more than match and send |
| macOS | `otherMouseDown` and `otherMouseUp` on the existing listen-only tap | Button numbers 2, 3 and 4 |
| Wayland | **Not possible.** The GlobalShortcuts portal takes keyboard triggers only | Mouse keybinds work only while Opencord is focused |
| No global hotkeys at all | In the app, while focused | As keys do today |

`HotkeySupport::Global` gains `mouse: bool`. It's false for the portal, whose bindings exclude mouse ones. The app then handles mouse keybinds itself while focused, and the Keybinds page adds, when a mouse keybind exists: `Mouse buttons work only while Opencord is focused on this desktop.`

### Recording

- While recording, pressing Middle, Mouse 4 or Mouse 5 anywhere records it with the modifiers held at that moment.
- A left or right click stops recording with no change.
- The recorder reads `Press keys or a mouse button…`.
- The page's empty row reads `Add one to push to talk, mute or deafen with a key or mouse button, even from another app.`

## 7. Code blocks

### 7.1 The format

One small scanner, `findFences`, used by the markdown parser, the previews and the composers, with the same rules on the phone.

- **Opening:** ` ``` ` at the start of a line, after optional indentation.
- **Language:** the rest of the opening line if it's `[A-Za-z0-9_+#.-]{1,32}` and a newline follows. Otherwise there's no language, and that text is the code's first line.
- **Closing:** the next ` ``` ` anywhere, including at the end of a code line. Text after it on the same line starts a new paragraph.
- **The code:** blank lines at its start and end are dropped; indentation and blank lines inside are kept.
- **Not a block:** no closing fence, nothing between the fences, or an opening fence in the middle of a line. These stay ordinary text, as today.

| Typed | Result |
|---|---|
| ` ```py⏎⏎CODE``` ` | code `CODE`, language `py` |
| ` ```js⏎Code⏎``` ` | code `Code`, language `js` |
| ` ```one line``` ` | code `one line`, no language |
| ` ```py code``` ` | code `py code`, no language |
| ` ```⏎b``` after ` | code `b`, then paragraph `after` |
| ` ```⏎no end ` | ordinary text |

### 7.2 Typing it

Enter sends a message, so a fence could never be typed line by line. **While the text before the cursor is inside an open fence, Enter adds a line** in both composers. Once the fence is closed, Enter sends again. A suggestion list still takes Enter first, and the send button always sends what's there.

### 7.3 Highlighting

Shades of grey, not colours: the app stays monochrome (the owner's call). Through the `re_highlight` package (MIT). Only these languages are registered:

| Language | Also written |
|---|---|
| `python` | `py` |
| `javascript` | `js`, `jsx` |
| `typescript` | `ts`, `tsx` |
| `rust` | `rs` |
| `dart`, `go`, `java`, `kotlin`, `swift`, `lua`, `php`, `ruby`, `sql`, `css`, `json`, `yaml`, `diff`, `markdown` | `golang`, `kt`, `rb`, `yml`, `md` |
| `c`, `cpp`, `csharp` | `h`, `c++`, `cs` |
| `bash`, `shell` | `sh`, `zsh` |
| `xml` | `html` |
| `ini` | `toml` |

| Looks | Token classes |
|---|---|
| `text`, weight 700 | `keyword`, `selector-tag`, `section`, `doctag` |
| `text`, weight 500 | `title`, `built_in`, `type`, `tag`, `name`, `attr` |
| `textSecondary` | `string`, `regexp`, `number`, `literal`, `symbol`, `meta`, `attribute` |
| `textMuted` | `comment`, `quote` |
| `text` | everything else |

- **Plain text, same look as today:** an unknown language, no language, or more than `20000` characters.
- **Results are cached** (the last 64 blocks), because scrolling rebuilds rows.
- **Unchanged:** the language label shows what was typed, and Copy gives the code as typed.
- **On the phone:** the parser only, not the highlighting.

## 8. Noise suppression and the Test

### 8.1 The chip

In the voice card's first row, left of Disconnect: `[status] 6 [chip] 6 [Disconnect]`.

- **Chip:** height `24`, padding `0 8`, radius `12`, background `hover`, `selected` on hover or while its popover is open. Text 11.5/600: `Standard` or `High` in `textSecondary`, `Off` in `textMuted`; `text` on hover.
- **Tooltip:** `Noise suppression`. **Screen readers:** `Noise suppression: Standard`.
- **A click** opens the popover to the right of the chip, like the connection graph's.
- The status text ellipsizes to make room. At the `232` column nothing overflows.

### 8.2 The popover

Width `264`, from top to bottom:

1. **Title row:** `Noise suppression` 14/600 `text`, and an `OcSwitch` at the end: on unless the mode is Off.
2. **`8` below, the switch:** `Standard` and `High`, the same look as the People and Music switch. While Off, neither is selected, and tapping one turns suppression on at that mode.
3. **`6` below, one line,** 12.5 `textSecondary`, copied from Settings: `Removes steady noise; very light`, `Also keyboards and dogs; heavier`, or `Sends the microphone as it is`.
4. **`12` below, a `1` line in `border`,** then `12` below it **`MicTestPanel`** (§8.3).

- **Turning the switch on** goes back to the mode it was on before, within this opening; otherwise `Standard`.
- **Changes apply at once,** through `audioSettingsProvider`, so a running test hears them.
- **If High falls back to Standard,** the existing setting change and toast happen, and the popover follows.
- **Closing the popover ends the test.** Someone can't be left deafened with nothing on screen saying why.

### 8.3 `MicTestPanel`

The same widget in the popover and in Settings:

- **Note,** 12.5 `textSecondary`. In a call: `Hear what Opencord sends. You're deafened until you stop. Use headphones, or you may hear an echo.` Otherwise the same without the middle sentence.
- **`InputLevelMeter`** while testing, `8` below the note.
- **`8` below, a full-width `OcButton`:** `Test`, or `Stop test`, in its primary look, while testing.

### 8.4 What the test does

"What Opencord sends" means after noise suppression, input volume and the voice gate.

- **In a call:** starting deafens this person, if they aren't already. Everyone else hears nothing from them and sees them deafened, as with any deafen. They hear only themselves: no people, music or soundboard.
- **Not in a call:** nothing else changes. The core already plays it through an engine of its own.
- **Push to talk:** the test works as if the key were held, or it would be silent.
- **Voice activity:** the gate stays on, so the test sounds as a call would, cuts included.
- **It stops** when `Stop test` is pressed, the popover or the Settings page closes, the person leaves or joins a call, or they undeafen themselves any other way. A hotkey that undeafens ends the test.
- **Stopping restores the state from before:** undeafened only if the test was what deafened them.
- **Nothing is sent while muted or deafened.** No audio frames, no talking indicator, and no "You're muted" reminder during the test.
- **Never remembered.** It always starts off.

## 9. The test in Settings

Voice settings gets a section after `Noise suppression`: title `Microphone test`, holding `MicTestPanel`.

- **It hears what the page shows,** unsaved changes included: while testing, the draft's audio settings go to the engine. On stopping, the saved ones go back unless the draft was saved.
- **Leaving the page or closing Settings ends the test.**

## 10. Ping on the signal bars

Hovering the signal bars' icon in the voice card shows a tooltip with the ping: `42 ms`.

- The theme's tooltip look, no wait before it shows. It updates with each report while open.
- **No ping known yet, or no bars showing:** no tooltip.
- A click still opens the connection graph. **Screen readers** hear the ping in the row's label.

## 11. Hand checks owed

These can't be proved here, and go in the check documents:

- **Mouse keybinds:** X11, Windows and macOS, in another app; hold to talk; that the mouse doesn't lag on Windows; macOS's Input Monitoring prompt covering the mouse.
- **Wayland:** the footer shows, and mouse keybinds work while focused.
- **The test:** on headphones and on speakers, in and out of a call; a second person confirms they hear nothing; push to talk.

## 12. Claude's calls

Not in the owner's words, or a choice between readings. Each is one line to change.

1. **The Test runs only while its popover or page is open** (§8.2), so nobody stays deafened without seeing why.
2. **"Remember the last feature" means the same Test in Settings** (the owner's answer, 2026-11-05), not a saved on/off.
3. **An on/off switch as well as Standard and High** (§8.2), from "turn on and off".
4. **Mouse 3, 4 and 5 only** (§6).
5. **Enter inside an open fence adds a line** (§7.2), or the requested format couldn't be typed.
6. **Maximum pane width `480`, and these minimums** (§2).
7. **The private chat's first load uses the skeleton too** (§4).
8. **The phone's parser follows the new fence rules** (§7.1).
