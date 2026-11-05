# The Windows checks that are left, by hand

A companion to [check-windows-report.md](check-windows-report.md): one recipe
per box that is still open or only partly done, written to be followed at the
PC without reading the rest of the report first.

Tick the boxes in the **report**, not here. When something fails, write down
what you saw and what you expected — a screenshot of the window and the
session log (**Settings → About → Logs → Open logs folder**) are worth more
than a description.

## Before you start

**The rig is already built and survives restarts.** You only need to start
things again if they are not running:

```powershell
# the server (leave this window open; it prints its claim token at start)
C:\oc\server\opencord-server\opencord-server.exe --config C:\oc\test-server\opencord.toml

# the two clients, each from its own PowerShell window
$env:OPENCORD_PROFILE = "alice"; & C:\oc\<build>\opencord\opencord.exe
$env:OPENCORD_PROFILE = "bob";   & C:\oc\<build>\opencord\opencord.exe
```

`alice` owns the server, `bob` is a member, they share a private chat of 113
numbered messages, and `#general` holds photos, videos, a `.wav` and a
`README.txt`. Test files to send are in `C:\oc\material\`.

**`<build>` matters.** Each section is checked against its own pull request's
build:

| Section | Build folder under `C:\oc\` |
|---|---|
| 1 | `private-messages-titlebar` (#21) and `custom-titlebar-everywhere` (#22) |
| 2 | `tray-background` (#24) |
| 3 | `voice-reconnect-bar` (#27) |
| 4 | `private-message-keys` (#23) |
| 5, 6, 7 | `media-viewer-player` (#25, #26) |
| 8 | `compact-full-width` (#28), `private-chat-scroll` (#29) |

**Five things that will waste your time if you do not know them:**

1. **Settings do not apply until you save.** Changing a switch raises a
   *Careful — you have unsaved changes!* bar at the bottom. Click **Save
   changes** or nothing happened.
2. **The Appearance page scrolls.** The Window frame section is below the
   fold at most window sizes. Scroll before deciding something is absent.
3. **The right half of the reconnect bar's Reconnect button is dead** — it
   maximises the window instead. Click its **left** half. That is a known
   bug (§C), not something you are doing wrong.
4. **The media viewer takes no keyboard input.** Esc, Alt+Enter and the
   arrows do nothing in it. Known (A12); use the mouse.
5. **Tray icons live behind the `^` chevron**, not on the taskbar itself.

---

# §1 The title bar — 1 open, 6 partly

Use **#22's** build unless a step says #21.

### Scaling (open)

1. **Settings → System → Display → Scale** → 125 %. Sign out and in if
   Windows asks; the app must be restarted either way.
2. Start the app. Look at the three caption buttons at the top right.
3. Repeat at 150 %, then put it back to 100 %.

**Pass if** at each scale the glyphs are sharp (not blurred or doubled), the
three buttons are the same size as each other, and their row lines up with
the app's own header rather than sitting high or low in it.

### No system title bar — the two states left (partly)

Only "at start" is proven.

1. Minimise the window, then restore it from the taskbar. Look at the top
   edge.
2. Maximise it, then un-maximise it. Look again.

**Pass if** neither state brings back a Windows title bar — no second strip
above the app's own header, no window text, no second set of buttons.

### Caption buttons on every screen — a private chat (partly)

On **#21's** build (they are absent on #22's, which is expected).

1. **Private** (top left) → the chat with **Bob**.
2. Look at the top right.

**Pass if** minimise, maximise and close are there, in that order.

### Dragging the header over a title (partly)

1. Open `#general`.
2. Press on the header **on the channel's name itself** and drag 100 px.
3. Do the same in the private chat with Bob, on his name.

**Pass if** the window moves both times. **Fail** if the name is selected or
nothing moves.

### Aero Snap by dragging (partly)

The keyboard half passes already.

1. Drag the window by its header to the **top edge** of the screen and
   **hold it there for about a second** before releasing.
2. Then drag it to the **left edge**, hold, release.

**Pass if** the top gives a maximised window and the side gives a half-screen
one. Releasing too quickly only moves the window — that is the drag, not the
app.

### Edges and corners resize (partly)

Only the right edge is proven.

1. Drag the **left**, **top** and **bottom** edges in turn.
2. Drag each of the **four corners**.

**Pass if** each one resizes, and the corners change both dimensions at once.

### Private messages (#21) — the rest (partly)

On **#21's** build.

1. Open **Friends**. Press on the **empty space to the right of the tabs**
   (Online, All, Pending…) and drag.
2. Narrow the window until the sidebar stops being inline (about 700 px).
3. Look at the top right.

**Pass if** the empty tab-row space drags the window, the caption buttons are
still there when narrow, and nothing overlaps them.

---

# §2 The tray — 1 open, 2 partly

Build: `tray-background` (#24). Open the `^` chevron to see the icons.

### Start minimized with Launch at login (open)

**This signs you out of Windows — close everything else first.**

1. **Settings → Windows & behavior** → turn on **Start minimized** *and*
   **Launch at login** → **Save changes**.
2. Quit the app from the tray.
3. Sign out of Windows and sign back in.

**Pass if** Opencord is running (its icon is in the tray) with **no window
on screen**, and a click on the icon opens the window.

Afterwards turn both switches off again and save.

### The icon's sharpness (partly)

1. At 100 % scaling, open the chevron and look at the icon closely.
2. Set scaling to 150 %, restart the app, look again.
3. Restart the app once more at 150 % and look a third time.

**Pass if** the icon is crisp at both scales and after the restart — not
blurred, not a scaled-up small version.

### The unread dot in every voice state (partly)

The four shapes are proven; the dot on top of them is not.

1. From `bob`, send `alice` two messages, and leave `alice`'s window
   minimised so they stay unread.
2. With the unread showing, take `alice` through: join `General` voice, talk,
   mute, then disconnect.
3. At each of those four points, open the chevron and look at `alice`'s icon.

**Pass if** the unread dot is visible in all four states, on top of the ring,
the filled circle, the slashed ring and the bubble.

---

# §3 The reconnect bar — 3 open, 1 partly

Build: `voice-reconnect-bar` (#27). **Click Reconnect on its left half.**

### Another device took the call (open)

1. On `alice`: **Settings → Identity & keys → Backup → Export identity
   backup**. Keep the text.
2. Start a third client: `$env:OPENCORD_PROFILE = "alice2"` and the same
   build. On its welcome screen choose **I have a backup** → **Import
   identity** and paste.
3. `alice` joins `General` voice. Then `alice2` joins the same channel.
4. Look at `alice`.

**Pass if** `alice` shows *"Your call in General moved to another device."*,
and **Reconnect** (left half) takes the call back and drops `alice2` out of
it.

### Getting back in was refused (open)

Needs the network cut. The clean way that only cuts the app: put the server
on Tailscale and cut the Tailscale adapter, or cut Wi-Fi if the server is
reached over it. `Disable-NetAdapter -Name "<adapter>"` as administrator,
`Enable-NetAdapter` to put it back.

1. As owner, open `General`'s settings and set **User limit** to 1. Remove
   **Move Members** from `alice`'s roles.
2. `alice` joins `General`.
3. Cut the network. Wait until `bob` sees `alice` leave the channel.
4. `bob` joins `General` (the owner may pass the limit).
5. Bring the network back.

**Pass if** `alice` shows *"Couldn't get back into General: &lt;reason&gt;"*;
**Reconnect** while it is still full keeps the bar and shows the reason in a
toast; and once `bob` leaves, **Reconnect** works.

### The volume bug (open) — reported, never reproduced

1. `alice` and `bob` in a call. In `alice`'s member menu set `bob`'s **User
   volume** to 40 %.
2. Cut and restore the network so `alice` drops and rejoins by itself (the
   voice panel shows reconnecting, then connected).
3. Check the slider and listen.
4. Now kill `alice` in the call and use **Reconnect**. Check again.

**Pass if** it reads 40 % both times and `bob` sounds as quiet as before. If
it is wrong at any point, write down which of the two cases, what the slider
says and what you hear, and keep the session log.

### No bar after — the three left (partly)

1. Give `bob` a role with **Move Members**, as owner.
2. `alice` joins `General`. From `bob`, open `alice`'s member menu and
   **Disconnect** her. Restart `alice`.
3. Make a second voice channel. `alice` joins one; `bob` **moves** her to the
   other. Restart `alice`.
4. The two tray steps (**Quit Opencord** from the tray in a call, and Close
   with close-to-tray off in a call) **cannot be done on this build** — #27
   has no tray. They need a build with #24 and #27 together, so leave them
   until after the merge.

**Pass if** no bar appears after a restart in 2 and 3.

---

# §4 Private message keys — 5 open

Build: `private-message-keys` (#23). `alice` and `bob` already have a private
chat, which is what these need.

**Making a profile that carries `alice`'s identity**, used twice below:

1. On `alice`: **Settings → Identity & keys → Backup → Export identity
   backup**. Copy the text.
2. Start a client with a new profile name
   (`$env:OPENCORD_PROFILE = "alice-laptop"`).
3. On its welcome screen: **I have a backup** → **Import identity** → paste.

### Scenario 3: a second device of one identity (open)

1. Make `alice-laptop` as above. **Do not approve it yet.**
2. From `bob`, write to `alice`.
3. On `alice`, click **Approve**. `alice-laptop` shows 16 words under
   **Compare words**; `alice` shows the same 16. Click **They match** on both.
4. From `bob`, write again.
5. Reply from `alice-laptop`.

**Pass if** the 16 words match exactly; after step 4 both of `alice`'s
profiles show the message and `bob`'s chat says *"Alice signed in on a new
device."*; and after step 5 the reply shows on both `alice` profiles as sent,
and on `bob`'s.

### Scenario 3b: comparing words, and a stolen identity (open)

1. In `alice`'s chat with `bob`, click the **lock** in the chat header. Do
   the same in `bob`'s chat with `alice`. Compare the 16 words.
2. Click **They match** on both.
3. Make a `thief` profile with `alice`'s identity (same recipe) and **do not
   approve it**.
4. From `bob`, write to `alice`.
5. From `thief`, write to `bob`.

**Pass if** the two word lists in step 1 are identical, **They match** turns
the lock into a **shield** on both sides, and in step 5 `bob` sees *"A
message from keys you haven't accepted."* and **not** the text.

### Restart (open)

1. With the approvals and shield from the two scenarios in place, quit and
   restart **every** profile: `alice`, `bob`, `alice-laptop`.
2. Open each chat. Then send a new message each way.

**Pass if** the shield is still there, the approved device is still approved,
nothing asks to be compared again, and the new messages decrypt and read
normally.

### Fresh start (open)

This needs a profile that had private chats **before** this build — `alice`
and `bob` qualify, since their chat predates it.

1. Start `alice` on the `private-message-keys` build for the first time.
2. Read the notice. Then restart `alice` again.
3. Check **Friends**, **Message requests**, **Blocked**, and
   **Settings → Appearance**.

**Pass if** it says once that old private chats were removed, that notice
does **not** come back on the second start, and friends, requests, blocks and
settings are all still as they were.

### Across machines (open) — held for the Mac

`alice` here, `alice-laptop` on the Mac, same two scenarios. Leave it until
the Mac pass.

---

# §5 Media uploads — 4 open

Build: `media-viewer-player`. Files are in `C:\oc\material\`.

### Sending (open)

1. Open `#general`. Drag `photo1.jpg`, `short_video.mp4` and `document.pdf`
   onto the composer from Explorer (drop, do not use the paperclip — the drop
   path is what this checks).
2. Watch the upload bar. Start a large one (`speech_60s.mp4`, 34 MB) and hit
   **Cancel** part-way.
3. Send the photo and look at the grid.
4. Look at the video tile.

**Pass if** all three drop and attach; the bar actually moves rather than
jumping 0 → 100; Cancel stops that upload and leaves the others; the sent
photo appears at its own proportions with no layout jump as it loads; and the
video tile shows its first frame with its length in the corner.

### Playing (open)

1. Click the video in the chat and let it play. Listen.
2. Open the same video in the viewer and play it there.
3. Drag the seek bar backwards and forwards.
4. In the composer, hold the microphone button, say something, send it.
   Play it back.
5. Start the video playing, then play the voice message.

**Pass if** sound comes out in both places, seeking lands where you put it
and keeps playing, the voice message records and plays back, and starting the
voice message **pauses** the video rather than both playing at once.

### Saving (open)

1. On the `README.txt` or `document.pdf` card, click **Download**.
2. Select several files in one message and use **Save all**.

**Pass if** Download asks where to save and the file is there afterwards, and
**Save all** puts them all in the one folder you chose, alongside whatever
was already in it.

> Note the report's §6 Download box expects the *viewer's* Download to save
> to Downloads **without** asking. A file card asking is not a contradiction;
> they are different buttons.

### The clipboard (open) — only after #31 merges

Until `fix/windows-clipboard` is in, Windows says *"this system's clipboard
isn't supported"* and there is nothing to check.

1. Press **Win+Shift+S**, grab part of the screen, click the composer and
   press **Ctrl+V**.
2. Right-click a photo in the chat → **Copy image**. Paste it into **Paint**,
   then into a browser's address bar or a web text box.

**Pass if** the screenshot appears in the composer as an attachment and
sends; and the copied image pastes into both Paint and the browser, right
way up and not transparent-turned-black.

---

# §6 The media viewer — 6 open, 3 partly

Build: `media-viewer-player`. **The viewer takes no keyboard** (A12), so use
the mouse for everything except where a box is about a key. Click a photo in
`#general` to open it.

### Zoom (open)

1. Open a photo. Scroll the wheel up over it, or use a pinch if you have a
   touchpad.
2. Zoom well in, then drag the picture around.
3. Open `photo4_4k.jpg` (a big one) and zoom in on a corner.

**Pass if** zoom keeps going past the size the photo first appeared at and
uses the whole screen, panning follows the mouse, and the 4K one stays sharp
as you go in rather than showing the first-size pixels blown up.

### The video (open)

1. Click `short_video.mp4` in the chat to open it in the viewer. Play it.
2. Look at the bottom of the screen, and at the bottom left.

**Pass if** it plays with its own controls, and the *"Photo n of m"* /
*"name · date at time"* text is **not** drawn on top of those controls.

### Download (open)

1. Open a photo in the viewer. Click the **download** icon, bottom right.
2. Read the note that appears, and click it.
3. Download the **same** photo a second time.

**Pass if** it saves to **Downloads without asking**, the note says *"Saved
to Downloads"*, clicking it opens the folder with the file selected, and the
second save is named with ` (2)`.

### Opening again (open)

1. Open a photo. Leave the viewer open.
2. Click the main window to bring it forward, then click a **different**
   photo in the chat.

**Pass if** the new photo appears in the **same** viewer window — no second
window — and that window comes to the front.

### The tray (open)

Needs a build with **both** #24 and #25, so this one waits for the merge. The
steps, for then: with the viewer open, close the main window to the tray (the
viewer stays); close the viewer (the app keeps running in the tray); **Quit
Opencord** from the tray with the viewer open (everything exits).

### Display settings (open)

1. At 100 % scaling, open the viewer, then at 150 % (restart the app after
   changing).
2. With two monitors, put the main window on the second one and open a photo.
3. **Known risk, check whether it bites:** set the two monitors to
   *different* scalings, leave the viewer in a normal window on one, then
   open a photo from the main window on the other.
4. **Known risk:** leave a normal viewer window overlapping a monitor's edge,
   unplug that monitor, open a photo.

**Pass if** 1 and 2 behave — full screen opens on the monitor the main window
is on. For 3 and 4, write down what you get: the expected trouble is a
remembered window converted with the wrong scaling (so it reopens too big or
too small) and a remembered window reopening nearly off-screen.

### Modes — Alt+Enter (partly)

Everything else in that box passes. Alt+Enter is dead because of A12; retest
it only after the keyboard bug is fixed.

### Closing — Esc (partly)

Same: Esc is dead for the A12 reason, not the crash. The other two parts pass
once #39 is in.

### Moving around — the arrow keys (partly)

Same again: the side strips work, the arrows do not. A12.

---

# §7 The viewer's player and picture-in-picture — 5 open

Build: `media-viewer-player` (#26). Open `short_video.mp4` from `#general` in
the viewer. None of this has been touched yet, so expect to find things.

### Rotate (open)

1. Open a photo. Find the **rotate** icon beside Download, bottom right.
2. Click it four times, watching each step.
3. Step to the next photo with the side strip.
4. Do the same on a video.

**Pass if** each click turns a quarter **anticlockwise**, four clicks come
back to the start, the next item opens **unturned**, and it works for videos
as well as photos.

### The bar (open)

1. Open a video and look at the control bar.
2. Leave the mouse still for a couple of seconds.

**Pass if** the bar is narrow, centred at the bottom, see-through, and never
covers the picture; the **first row** has volume, play, full screen,
picture-in-picture and the speed; the **second row** has time played, the
seek bar and time left; and it fades with the rest of the controls when the
mouse rests.

### Speed (open)

1. Click the speed control repeatedly: 1x → 1.5x → 2x → 3x → 1x.
2. At 2x, listen to a voice (`speech_60s.mp4` counts seconds aloud — use it).
3. Watch the two times while the speed changes.

**Pass if** it cycles those four and wraps; the voice keeps its **pitch**
(faster, not chipmunked); and the times stay in the **video's own** time —
a 60-second video still says 60 seconds at 2x, it just gets there sooner.
`speech_60s.mp4` makes this easy: the spoken count and the displayed time
should agree.

### Full screen (open)

1. In the viewer, click **full screen** on the bar.
2. Leave the mouse still.
3. Press **Esc** once, then again.
4. Now from a **normal** viewer window, go full screen and come back; then
   close the viewer and open another photo.

**Pass if** the video fills the screen at its own shape (black bars, not
stretched) with only the bar and the title buttons left, both fading; the
**first** Esc leaves full screen and the **second** closes the viewer; and
after 4 the window returns normal and the next viewer still opens normal.

> Esc may do nothing here for the A12 keyboard reason. If so, note it against
> A12 rather than as a separate player bug, and check the rest with the mouse.

### Picture-in-picture (open)

1. Click the **picture-in-picture** button on the bar.
2. Click another application so Opencord is not the active one.
3. Look at its controls, then leave the mouse still.
4. Drag it; then drag one of its edges.
5. Click **Back**, then try a **double click** instead.
6. Go back to picture-in-picture and press its **close**.
7. Open another photo or video from the chat.
8. Move picture-in-picture somewhere else, resize it, close it, then open
   picture-in-picture again.
9. With a **normal** viewer window remembered, close from picture-in-picture,
   then open a photo.

**Pass if**: it becomes a small window about **320 wide** with the video's
shape, at the **bottom right**, still playing without restarting; it stays
**above** other windows even when another app is active; its controls are
volume top left, back and close top right, play in the middle, times and
seek at the bottom, all fading; a drag moves it and its edges resize it
**keeping the shape**; Back and a double click both return it to the window
it came from, still playing, while close stops the video; opening another
item brings the viewer back with it; reopening picture-in-picture puts it
back where and how big you left it; and in 9 you get a **normal window with
its borders laid out as the viewer**, not a picture-in-picture-shaped one.

---

# §8 Scrolling — 1 partly

Build: `private-chat-scroll` (#29). The private-chat half passes already,
across navigation and across a restart.

### Remembering the place — the rest (partly)

1. **A long channel.** `#general` is too short to scroll. Send 100 or so
   messages to it first (or to a new channel), scroll half way up, go to the
   voice channel or another text channel, and come back.
2. **Each chat keeps its own place.** Needs a **second** private chat, so a
   third identity: make a `carol` profile, have her join the server with an
   invite, and start a chat with `alice`. Scroll `alice`'s chat with `bob` to
   one place and her chat with `carol` to another, switch between them twice.
3. **Quit from the tray.** Not possible on #29's build — no tray. Leave it
   for a build with #24 and #29 together.

**Pass if** the channel comes back where you left it in 1, and in 2 each chat
returns to **its own** place rather than sharing one.

---

# After the merge

These are not failures and not skips — there is nothing on any single build
to drive them. They need the batch merged:

- §1 caption buttons on every screen — #21 **and** #22;
- §3 the two tray steps of *No bar after* — #24 and #27;
- §6 the viewer's tray box — #24 and #25;
- §8 quit-from-the-tray — #24 and #29.

# Waiting on a fix

- **§6 Modes (Alt+Enter), Closing (Esc), Moving around (arrows)** — all one
  bug, A12: no key reaches the viewer. Retest all three together once it is
  fixed; the rest of those boxes already passes.
- **§5 The clipboard** — after #31 merges.
- **§3 Reconnect** — usable today if you click the button's **left** half;
  the right half is the known bug in §C.

---

# §9 Floating UI hand checks (`feat/floating-ui`)

### Windows-specific checks
- [ ] **Caption buttons on new row:** Minimize, Maximize/Restore, Close sit in the 40 px top row, flush with the top-right corner, 46 × 32 px.
- [ ] **Snap layouts:** Hovering over the Maximize button shows Windows 11 snap layouts menu (`WM_NCHITTEST` returns `HTMAXBUTTON`).
- [ ] **Dragging by row:** Dragging by empty space in the 40 px title row moves the window.
- [ ] **Double-click:** Double-clicking empty space in the title row toggles maximize/restore.
- [ ] **Splitter cursor:** Hovering the 8 px splitter between the left column and main page shows the horizontal resize column cursor.
- [ ] **Crumb text at display scale:** Text in the breadcrumb is crisp and properly sized at 125 % and 150 % Windows display scaling.

### Linux-specific checks
- [ ] **GNOME left-hand buttons:** When `org.gnome.desktop.wm.preferences button-layout` places window controls on the left, caption buttons appear at the top-left of the row.
- [ ] **KDE Plasma:** Window controls match desktop conventions and decoration settings.
- [ ] **Tiling window:** A tiled window has no rounded corners, no shadow margin, and meets screen edges cleanly.

### Cross-platform checks (All)
- [ ] **Mouse side buttons:** Mouse button 4 (back) and button 5 (forward) navigate channel history.
- [ ] **Video and screen share:** A real video and a real screen share transition through focus mode (click to focus, strip underneath) and return to grid.
- [ ] **Two-person call with dock hiding:** In a call, voice dock appears on mouse movement over the stage, auto-hides after 2.5 s of inactivity or pointer departure, and never hides while hovering the dock or with popups/menus open.
