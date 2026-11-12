# 50 test plugins for the plugin UI

2026-11-11, against `feat/plugins-channels` (milestone 2: custom channels on
desktop).

Each idea is one plugin with its own channel type. Together they reach every
block, every prop and event, every `app.*` call, every server call a view can
use, every state of the plugin area and the channel list, and every limit.
The index at the end says which idea tests what.

**Every plugin needs** `create_channel_types` and a `client` entry in its
manifest. The permissions listed under an idea are the extra ones.

**For every plugin, also check:** dark and light; 360 wide and a full window;
switching the theme while it's open; leaving and coming back (the view
closes, `init` runs again); no unread dot on its channel; two accounts at
once (an admin and a plain member); the phone app doesn't list the channel at
all.

**Not covered, because it isn't built yet:** activities and their call events
(`call_members_change`, `speaking_change`) are milestone 3, the phone is
milestone 4, and `app.on_back` has nothing that calls it on desktop.

---

## A. Layout, look and text

### 1. Block Gallery

A tour of every layout block and look prop, one `tabs` page per group.

- **Tests:** `row` and `column` with each `align`, each `cross`, `wrap` and
  `gap`; `stack`; `grid` with `columns`, then with `min_column_width`;
  `scroll`; `box`; `spacer` fixed and flexible; `divider` across and down
  with `color` and `thickness`. On containers: `background` as a colour name,
  `#rrggbb`, `#rrggbbaa` and a gradient with `angle`; `border`; `radius`;
  every `shadow`. On any block: `width`/`height` as a number, `"fill"` and
  `"fit"`; min and max sizes; `flex`; `padding` as a number, `{x, y}` and
  `{top, right, bottom, left}`; `opacity`; `visible = false`; the `tooltip`
  prop.
- **Try:** every page at 360 and full width, nothing overflows or clips;
  `visible = false` leaves no gap; `wrap` wraps when narrow; tooltips show on
  hover and stay inside the area; a gradient turns the way its `angle` says.

### 2. Theme and Type Lab

Swatches of the 12 colour names, a hex box that recolours a sample, a type
specimen and every icon.

- **Tests:** each colour name in both themes; hex colours typed into a
  `text_input`; `text` with every `size`, `weight`, `italic`, `color`,
  `align`, `max_lines` and `selectable`; a font bundled in `assets/` (`.ttf`
  and `.otf`) through `font`; all 43 `icon` names at several sizes and
  colours; `app.theme` and `app.listen("theme_change")`; a `slider` for the
  size.
- **Try:** switch Opencord's theme: named colours follow at once, hex ones
  don't; a bad hex (`#12345`), a font size of 5 or 201 show the error with
  the block's path instead of breaking the app; `max_lines` cuts with an
  ellipsis; selectable text can be selected and copied; the bundled font
  really draws; note what an unknown font name or icon name does (it must
  not break the view).

### 3. Responsive Dashboard

One column at `compact`, two at `medium`, three at `expanded`, with its
context printed on screen.

- **Tests:** `app.breakpoint`, `app.size`, `app.input`, `app.safe_area`,
  `app.text_scale`, `app.platform`, `app.viewer`, `app.place`;
  `app.listen("resize")`; the same fields in `init(ctx)`.
- **Try:** make the plugin area 599, 600, 839 and 840 wide. It's the area
  that counts, not the window: open and close the floating panels to change
  the area alone. `resize` fires with the new breakpoint each time. Change
  Opencord's text size: `text_scale` follows and text grows. `platform` is
  `"desktop"`; `viewer.roles` lists role ids; on a touchscreen laptop, note
  what `input` says.

### 4. Event Logger

Every block that has events, each writing what it got into a log at the
bottom of the page.

- **Tests:** `box` `on_press`, `on_double_press`, `on_long_press`,
  `on_secondary_press` (`{x, y}`), `on_hover_enter`, `on_hover_exit`;
  `button` press and long press; `icon_button`; every control's `on_change`;
  `text_input` `on_submit`, `on_focus`, `on_blur`; `slider` `on_change_end`;
  `scroll` `on_scroll` (`offset`, `extent`), `on_reach_start`,
  `on_reach_end`; `image` `on_load` (`width`, `height`) and `on_error`
  (`message`); `on_animation_end`; `menu` `on_select`; `on_dismiss` of
  `toast` and `modal`; `app.copy` from each kind of press.
- **Try:** each event fires once per action, with the data above; note
  whether a double press also logs two presses; a right click gives `{x, y}`
  inside the box; `app.copy` works in all four press kinds and shows
  "Copied"; a switch that drops a handler in the next render really stops it
  firing.

### 5. Markdown Notebook

A shared notepad: write on the left, preview on the right (tabs at
`compact`).

- **Tests:** `rich_text` with bold, italic, code, code blocks, links,
  user/role/channel mentions, emoji and the server's own emoji; `text_input`
  with `lines`, `max_length` and `placeholder`; text up to 20,000 characters;
  `db` for the notes; `ui.send_place` so others' previews follow.
- **Try:** everything looks as it does in a message; note what clicking a
  link or a mention does; 20,000 characters pass and 20,001 show the prop
  error; typing stays smooth in a long text; another member's edits arrive
  without moving your cursor, and the echo of your own typing doesn't either.

## B. Controls and forms

### 6. Survey Builder

Admins build a survey, members answer it, results show live as bars.

- **Tests:** `checkbox`, `switch`, `radio` (a group of several), `slider`
  (`min`, `max`, `step`), `select` (option `icon`s, `placeholder`,
  `disabled`), `segmented`, `tabs`, `text_input` (`password` for a secret
  answer), every `button` `variant` with `icon`, `loading` and `disabled`,
  `icon_button`; `progress` bars; `db.transaction`; `ui.send_place`.
- **Try:** a control left untouched still sends the default it shows;
  disabled controls can't be changed by mouse or keyboard; `step` snaps;
  `loading` blocks presses; two members on different pages of the survey at
  once (each view is its own page); results update for everyone after each
  answer; the slider sends `on_change` while dragging and `on_change_end`
  once.

### 7. Settings Mirror

Shows the plugin's own settings live, so an admin sees a change land without
reopening anything.

- **Tests:** a manifest `config` with every kind (`text`, `secret`, `number`,
  `boolean`, `choice`, `role`, `channel`, `user`) and one `required`;
  `config.get`; the `settings_change` event; `ui.send_all`; the Plugins tab's
  Settings page.
- **Try:** change each setting in the Plugins tab: open views update within
  a second; the secret is masked in the Plugins tab and the view shows only
  "set" or "not set" (only the server script can read it); note what happens to the plugin and its channel when a
  required setting is cleared; a deleted role or channel in a setting shows
  sensibly.

### 8. Typing Race

Everyone in the channel races to type the same sentence after a countdown.

- **Tests:** `app.listen("key_down")` and `"key_up"` with `key`, `code`,
  `shift`, `ctrl`, `alt`, `meta`, `repeat`; `focus` and `blur`; `app.every`
  for the countdown; `ui.send_place` for each player's progress; `progress`
  per player.
- **Try:** keys arrive only after clicking into the area; push-to-talk, mute,
  deafen and Opencord's global keys still work while the area has focus and
  never reach the plugin; typing doesn't jump to a message box (type-to-focus
  is off here); holding a key sends `repeat`; Alt+Tab away gives `blur`, back
  gives `focus`; a player who leaves mid-race drops out for the others.

### 9. Room Booking

A week calendar built from `grid` and `box` (there's no date picker block);
click a free slot to book it.

- **Tests:** `grid`; `box` hover and secondary press; `segmented` for day or
  week; `select` for the room; `badge` on your own bookings; a `menu` to
  cancel; `db.transaction`; `ui.send_place`.
- **Try:** hovering highlights a slot, and enter and exit stay paired when
  the mouse moves fast; a right click opens the menu at the pointer, inside
  the area; two members booking the same slot at the same moment: exactly
  one wins, the other sees why; bookings survive a plugin restart and an
  update.

### 10. Accessibility Check

A page of controls with screen reader labels, and a checklist to tick.

- **Tests:** the `label` prop on every block; `tooltip`; keyboard focus order
  (Tab, Shift+Tab, Space, Enter, arrow keys on `slider`, `radio`,
  `segmented` and `tabs`); `app.text_scale`; reduced motion.
- **Try:** with Orca (Linux), Narrator (Windows) and VoiceOver (macOS), each
  control reads its label and state; Tab reaches every control in order; at
  the largest text size nothing overlaps; with the system's reduce motion
  setting on, `animate`, `enter` and `exit` do nothing; `text_muted` on
  `surface` is readable in both themes.

## C. Lists, scrolling and motion

### 11. Infinite Archive

A text channel's whole history in one lazy list.

- **Tests:** `list` with a huge `count`, `item(i)`, `item_height`, `gap`,
  `on_reach_end`; paging with the server's
  `messages.fetch(channel, { before, limit })`.
- **Try:** fling to the end of 100,000 items without a freeze; items not yet
  drawn show blanks of `item_height` (48 without it); only the visible items
  plus 20 on each side are asked for, never past `count`; changing `count`
  while scrolled keeps your place; 0 items is fine; an error thrown in
  `item(i)` shows the error with the list's path.
- **Permissions:** `read_messages`, `read_channels`.

### 12. Side Chat

A small chat living in a plugin channel, newest at the bottom.

- **Tests:** `list` with `reverse = true`; `on_reach_start` for older
  messages; `enter = "slide_up"` on new rows; `text_input` `on_submit`;
  `user` blocks for authors; `db`; `app.send`, `ui.send_place`, `app.on`.
- **Try:** it opens at the newest message; new messages keep it at the bottom
  unless you scrolled up, and then it stays put; two members see each
  other's messages within a second; Enter sends and clears the box; the
  channel never gets an unread dot (by design, §14.5).

### 13. Live Leaderboard

A scoreboard whose rows slide to their new places as scores change every
second.

- **Tests:** `key`ed rows that move; `animate = { duration, curve }` with all
  six curves (`linear`, `ease`, `ease_in`, `ease_out`, `ease_in_out`,
  `spring`); `enter` and `exit` with `fade`, `slide_up`, `slide_down` and
  `scale`; `on_animation_end`; `timer.every` and `ui.send_all` on the server.
- **Try:** a moved row really moves, it isn't the old row with new text;
  rows animate in once and out once; `on_animation_end` fires once per
  animation; durations of 0 and 2,000 work, and note what 2,001 does; reduced
  motion turns all of it off.

### 14. Media Shelf

Rows of posters that scroll sideways, inside a page that scrolls down.

- **Tests:** a horizontal `scroll` inside a vertical one; `on_scroll`;
  `on_reach_start` and `on_reach_end`; a `grid` with `min_column_width` for
  the "all" page; `asset` images.
- **Try:** the mouse wheel scrolls the page, Shift+wheel and a trackpad's
  sideways swipe scroll a row; scroll positions survive redraws (a clock on
  the page changes every second); reaching a row's end loads more; the grid
  drops columns as the area narrows.

### 15. Kanban Board

Columns of cards for a team; cards move between columns.

- **Tests:** a `row` of `column`s, each in a `scroll`; a `menu` on each card
  (icons, a `danger` Delete); a named modal to edit a card; `badge` labels;
  keyed cards changing column; `db`; `ui.send_place` so everyone's board
  matches.
- **Try:** a card moved to another column is still the same card (no other
  card takes its text); menus open inside the area even at the right and
  bottom edges; two members moving the same card at once end with the same
  board; 500 cards stay smooth.

## D. Overlays and dialogs

### 16. Modal Maze

Named modals opened, stacked, updated and closed every way there is.

- **Tests:** `app.modals[name]`; `app.modal.open`, then `handle:update` and
  `handle:close(result)`; `app.modal.ask`; `app.modal.close(name)` and
  `app.modal.close(result)`; `close_all`; `is_open`; modal roots with `size`
  (`small`, `medium`, `large`, `full`), `sheet`, `dismissible` and `title`.
- **Try:** modals stack in order, and a counter inside each keeps ticking
  (they redraw with the state); opening a name already open brings it to the
  top with the new props; `ask` returns what `close` gave, dismissing returns
  `nil`; Esc and the backdrop close only dismissible ones; Tab can't leave
  the top modal; at `compact` a modal becomes a sheet unless `sheet = false`;
  every modal stays inside the plugin area, and the sidebar and title row
  stay usable; the 21st open modal is refused; a hot reload closes them all.

### 17. Quick Dialogs and Toasts

Buttons that ask, warn and prompt, and a shelf of toasts.

- **Tests:** `app.confirm`, `app.alert`, `app.prompt(text, default)`; in-tree
  `ui.modal({ open })` and `ui.sheet({ open })` with `on_dismiss`; `toast`
  with `open`, `duration` and `on_dismiss`.
- **Try:** each dialog pauses its handler until answered; prompt's Cancel
  gives `nil`, an empty OK gives `""`; dialogs work from a press, a timer and
  a server message, and `render` or `init` get `FORBIDDEN`; 2,000 characters
  of text fit, 2,001 are refused; leaving the channel with a dialog up
  leaves nothing behind; a toast goes after its `duration` (3 s by default)
  and fires `on_dismiss` once; two toasts at once.

### 18. File Manager

A browser for the plugin's own file storage.

- **Tests:** the server's `files.list(prefix)`, `files.get`, `files.put` with
  `public`, `files.delete`, `files.token`; a `menu` per file (Rename, Make
  public or private, Delete as `danger`); `tooltip` blocks; a `select` with
  many options; `app.confirm` before deleting.
- **Try:** menus and the `select` list open inside the area at every edge and
  close with Esc; a `select` of 100 options scrolls, 101 are refused with the
  path; a public file shows by `file`, a private one only by `file_token`;
  after a rename the new file shows at once.

### 19. Onboarding Tour

A guided tour with callouts pointing at parts of a page, remembered once
finished.

- **Tests:** `stack` children placed with `x`, `y`, `right` and `bottom`;
  `sheet` steps; `progress` with `style = "ring"`; `app.storage.get`, `set`
  and `delete`.
- **Try:** callouts stay on their targets while resizing; a finished tour
  stays hidden next time, also after restarting Opencord; storage is per
  server and per device (another server running the same plugin shows the
  tour again); "Reset tour" deletes the key; a 201-character key and storage
  over 1 MiB raise their errors.

## E. Drawing and frames

### 20. Whiteboard

A shared board: draw with a mouse or a pen, everyone sees it.

- **Tests:** `canvas` with `ink = { color, width }`; `on_stroke` batches
  (`stroke`, `points`, `done`); strokes sent to the server and drawn for
  others as `path` items; erasing by removing items; `segmented` colours; a
  `slider` for width.
- **Try:** the line follows the pointer with no lag (no Lua runs while
  drawing); others see each stroke; a pen tablet gives pressure (0.5
  without one); a stroke longer than 10,000 points is split and still draws;
  someone who opens the channel later sees the whole board; Clear empties it
  for everyone.

### 21. Shape Lab

Every canvas item type, each with sliders for its look.

- **Tests:** items `line`, `path` (open and `closed`), `rect` (`radius`, and a
  negative `w` or `h`), `ellipse`, `text` (`size`, `color`), `image` (each
  image source); `fill` as a colour and as a gradient; `stroke`; `opacity`;
  `transform` with `translate`, `rotate` and `scale`; bring to front and
  send to back; a canvas with no set size inside a `scroll` (300×150).
- **Try:** a slider changes only its own item; the order buttons change
  what's on top; an item with no `type` or an unknown one shows an error that
  says so; 20,000 items work, 20,001 are refused.

### 22. Pan and Zoom Map

A big grid world you drag around and zoom into.

- **Tests:** `on_pan` and `on_scale` with `phase` (`start`, `update`, `end`);
  `on_wheel` (`dx`, `dy`); `on_pointer_down`, `_move`, `_up`, `_cancel` with
  `buttons`, `kind` and `pressure`; a double press on a `box` around the
  canvas to reset.
- **Try:** mouse drag, wheel, Ctrl+wheel, a trackpad pinch and two-finger
  swipe (macOS), a touchscreen (Windows laptop); right and middle buttons
  show in `buttons`; dragging out of the canvas and letting go ends with
  `pointer_up` or `pointer_cancel`; zooming the map doesn't scroll anything
  else.

### 23. Comet Dodge

A small arcade game on a frame loop: steer a ship around falling comets,
with shared high scores.

- **Tests:** `app.on_frame(dt)` and `app.off_frame()`; `key_down` and
  `key_up`; canvas items replaced every frame; `app.play_sound` on a hit;
  scores in the server's `db`; a "heavy" switch that spends more than 8 ms a
  frame.
- **Try:** smooth, with `dt` near 0.016; minimise the window and the loop
  pauses and CPU use drops to nothing; in heavy mode frames stop and the
  error is reported while the app stays quick; the game never takes
  Opencord's own shortcuts.

### 24. Live Charts

Charts of the server's activity, drawn on a canvas, updated by the server.

- **Tests:** server events `message_create`, `message_update`,
  `message_delete`, `typing_start`, `presence_update`, `voice_state_update`;
  `opencord.off` behind a "stop counting" switch; `timer.every(1)` with
  `ui.send_all`; canvas paths replaced by id; `progress` with no `value`
  (indeterminate) while loading.
- **Try:** charts move every second without flicker; an idle view with
  nothing new uses no CPU; the switch really stops counting; the events'
  payloads match what the plugin writes with `log.info` in the Plugins tab's
  log.
- **Permissions:** `read_messages`, `read_members`, `read_voice`.

## F. Images, files and uploads

### 25. Image Source Zoo

One tile for each way of showing a picture, and tiles that must fail.

- **Tests:** `image` from `asset` (PNG, JPEG, WebP, animated GIF), `file`
  (public), `file_token` (private), `url` (an allowed domain), `attachment`
  (an image posted in a text channel, found with `messages.fetch`) and
  `bytes` (a Lua string and a `buffer`); `fit` (`cover`, `contain`, `fill`,
  `none`); `radius`; `placeholder`; `version`; `on_load` and `on_error`.
- **Try:** each shows, and animated ones move; a `url` off the manifest's
  `domains`, an SVG, a picture over 40 megapixels, a file over 10 MiB and a
  name that resolves to a private address each show the placeholder and fire
  `on_error` (the plugin's log says why for website failures); `bytes` over
  256 KiB, two sources at once and no source each give a tree error; saving
  over a file and raising `version` shows the new picture.
- **Permissions:** `internet` with `domains`, `read_messages`.

### 26. Photo Wall

Members post photos to a shared wall from inside the channel.

- **Tests:** `image_picker` with `on_upload` and `on_upload_error`; the
  server's `file_uploaded` event; `files.put(path, upload, { public = true })`;
  `ui.send_place`; `version`; a `menu` to delete your own photo (moderators,
  any).
- **Try:** an upload appears for everyone; a member without Attach Files in
  this channel is refused; a blocked extension and a file over the server's
  limit show `on_upload_error`; without `accept_uploads` it says "this plugin
  doesn't take uploads"; an upload the plugin never keeps is gone after an
  hour; GIFs move.
- **Permissions:** `accept_uploads`.

### 27. Homework Drop

Members hand in files privately; a teacher role sees them all and can post
one to a text channel.

- **Tests:** `file_picker` with `accept` (say `pdf`, `txt`, `zip`); private
  files with per-view tokens; `messages.send(channel, { content, files })`;
  `ui.send_role` to tell the teachers.
- **Try:** only the listed extensions can be picked; members see only their
  own hand-ins; a posted file is a normal attachment (thumbnail, download,
  the same size checks as a member's); too many files or a blocked one is
  refused; a missing path says "no plugin file at <path>".
- **Permissions:** `accept_uploads`, `send_messages`, `attach_files`.

### 28. Picture Search

Search a public image API and show the results as a grid.

- **Tests:** `http.request` with `method`, `headers` and `timeout`; `url`
  images through the server; a `spinner` while loading; the limit of 20 new
  pictures per member every 10 seconds; the one-hour image cache shared by
  everyone.
- **Try:** two members load the same results while the website sees one
  request per picture; scrolling fast through 100 new pictures hits the
  limit, shows placeholders, then recovers; a domain not in `domains` and a
  raw IP address are refused; a slow API runs into the timeout.
- **Permissions:** `internet`.

## G. Talking to the server

### 29. Ping-Pong Lab

A developer's bench for view events in both directions.

- **Tests:** `app.send` and `app.on`; `ui.on` and `view:send`; event names,
  sizes, rates and JSON shapes.
- **Try:** round-trip time shows live; 30 events a second from the view
  pass, the 31st is refused (`RATE_LIMITED`), and so is more than 128 KiB a
  second; from the server, the 61st event a second to one view raises "too
  many events to this view", and so does more than 512 KiB a second; 64 KiB
  of data passes, one byte more raises "an event is at most 64 KiB"; names
  with `.`, `:`, `-` and `_` work, capitals, spaces and 65 characters raise;
  nested tables, arrays, empty tables, unicode, large numbers and
  `json.null` arrive as sent, or note what changes.

### 30. Broadcast Console

An admin console that sends to one view, this channel, a role or everyone;
the receivers get a toast and a sound.

- **Tests:** `ui.views(place?)` (each view's user and `platform`),
  `view:send`, `ui.send_place`, `ui.send_role`, `ui.send_all` (each returns
  how many it reached); `toast`; `app.play_sound` with `volume` and `on_end`
  (`.ogg`, `.mp3`, `.wav`).
- **Try:** the counts match who has it open; `send_role` reaches only the
  role's holders, also right after the role is given or taken; volumes 0,
  0.5 and 1 sound right and `on_end` fires when a sound ends; a broadcast
  skips a rate-limited view and the log warns once a minute, not per event.
- **Permissions:** `read_roles`.

### 31. Who's Here

Everyone looking at this channel right now, and why each person left.

- **Tests:** `view_open` and `view_close` with every reason (`closed`,
  `disconnected`, `lost_access`, `channel_deleted`, `unavailable`,
  `plugin_stopping`); `user` blocks; `app.listen("close")` sending a goodbye
  within its 500 ms; `app.listen("connection_change")`.
- **Try:** leaving the channel says `closed`; pulling the network says
  `disconnected`; taking someone's access, deleting the channel and stopping
  the plugin each give their reason; one person on two computers shows
  twice; restarting the plugin keeps everyone listed (views stay open and the
  new VM hears `view_open` for each).

### 32. Tic-tac-toe Arena

Two players, any number of spectators, rematches, a sound for the winner.

- **Tests:** each viewer at their own stage (one in the lobby while two
  play); the server as the only truth (`db`, `ui.send_place`); a `grid` of
  `box`es; `animate` on the winning line; `app.play_sound`.
- **Try:** a player who leaves and comes back mid-game gets the board from
  the server; a spectator can't move (the server checks `view.user`, not what
  the client sends); two quick clicks on one square place one mark; winner
  and loser each see their own screen.

### 33. Plugin Bus

Two plugins, A and B, that talk to each other; their views show the
traffic.

- **Tests:** `plugins.emit` and `plugins.on`; both plugins registering the
  same `type_key` (kept apart by plugin id); a command of the same name in
  both.
- **Try:** without `talk_to_plugins` the calls raise; with B stopped, A's
  emits go nowhere without errors; the create channel dialog lists both
  types, each under its plugin's name; the `/` popup shows both commands
  under their own icons.
- **Permissions:** `talk_to_plugins`, `register_commands`.

### 34. Webhook Inbox

A live inbox of web requests sent to the plugin from outside (CI, scripts,
`curl`).

- **Tests:** `hooks.on` for `/hooks` and `/hooks/<path>`; the request's
  `method`, `query`, `headers` and `body`; replies with a status and
  headers; `db`; `ui.send_place`.
- **Try:** a request shows in the view within a second; a body over 1 MB and
  the 61st request in a minute are refused; cookies and framing headers never
  reach the plugin; a body with newlines written with `log.info` doesn't
  forge extra lines in the plugin's log.
- **Permissions:** `receive_web_requests`.

## H. Opencord's own features, from a plugin

### 35. Moderator Desk

Find members and act on them, with reasons and a history.

- **Tests:** `members.list` and `get`; `members.kick`, `ban`, `unban`;
  `bans.list`; `members.set_nickname`, also clearing it; `messages.fetch`
  and `messages.delete` on a member's recent messages; events `member_join`,
  `member_leave`, `member_update`, `member_unban`; `opencord.has_permission`
  and `permission_change`; `app.prompt` for the reason and `app.confirm`
  before a ban; `user` blocks for people who left.
- **Try:** acting on someone ranked above the plugin's role fails with a
  clear message; turning off `kick_members` in the Plugins tab greys the Kick
  buttons at once; a member kicked or banned while looking at the channel
  loses the view at once (`lost_access`); the plugin's log lists every
  action.
- **Permissions:** `read_members`, `kick_members`, `ban_members`,
  `manage_nicknames`, `read_messages`, `manage_messages`.

### 36. Role Picker

Members pick their own roles from a list the admins choose; admins edit the
roles on the same page.

- **Tests:** `roles.list`, `create`, `update`, `delete`, `reorder` (colour
  sliders, permission checkboxes, `hoist`, `mentionable`);
  `members.add_role` and `remove_role`; events `role_create`, `role_update`,
  `role_delete`; `badge`s in role colours; `app.viewer.roles`.
- **Try:** a picked role shows in the member list at once; roles above the
  plugin's role can't be given or edited; picking a role that hides this
  channel closes the view with "You can no longer see this channel"; note
  whether `app.viewer.roles` changes while the view is open or only on
  reopening.
- **Permissions:** `read_roles`, `manage_roles`.

### 37. Channel Architect

The server's channel tree, to create, rename, reorder, lock and delete
channels.

- **Tests:** `channels.list`, `get`, `create`, `update`, `delete`,
  `reorder`; `channels.set_overwrite` for a role and a member, allow and
  deny; a channel of the plugin's own type made from Lua; events
  `channel_create`, `channel_update`, `channel_delete`,
  `plugin_channel_create`, `plugin_channel_delete`; `app.open_channel`.
- **Try:** new text, voice and category channels show in the sidebar at
  once, and "Go there" opens them; a channel of another plugin's type is
  refused; renaming this channel and changing its topic update the header
  live; denying View Channel here to a role closes those members' views;
  deleting it while others look shows them "This channel was deleted";
  Server settings → Channels lists plugin channels too.
- **Permissions:** `read_channels`, `manage_channels`, `manage_roles`.

### 38. Voice Control Room

Every voice channel and who's in it, live, with moderator buttons and a
soundboard.

- **Tests:** `voice.states`, `move`, `disconnect`, `mute`, `deafen`;
  `soundboard.play`; events `voice_state_update`, `stream_create`,
  `stream_update`, `stream_delete`, `stream_viewers_update`,
  `soundboard_play`, `soundboard_sound_create`, `soundboard_sound_update`,
  `soundboard_sound_delete`.
- **Try:** self-mute and server mute look different; camera and screen share
  show; moving someone puts them in the other channel; soundboard sounds play
  in a call the plugin isn't in; every change shows within a second.
- **Permissions:** `read_voice`, `move_members`, `mute_members`,
  `deafen_members`, `play_soundboard`.

### 39. DJ Booth

A remote for the music player: now playing, a progress bar, the queue,
search and add.

- **Tests:** `music.state`, `add`, `skip`, `pause`, `resume`; events
  `music_now_playing`, `music_queue_update`, `music_notice`; the position
  worked out from `position_ms` and `server_time_ms` with `app.every(1)`; a
  `list` for the queue.
- **Try:** the bar stays in step with Opencord's own player through pause
  and resume; adding from Opencord's player updates the booth; uploaded
  songs and live streams (no length) show right; with Lavalink down it says
  it's waiting.
- **Permissions:** `read_music`, `control_music`.

### 40. Server Dashboard

The server at a glance: name, description, icon, counts, invites.

- **Tests:** `server.get`; `invites.list` and `invites.create` (`max_uses`,
  `expires_in_s`); events `server_update`, `invite_create`, `invite_revoke`,
  `voice_settings_update`, `upload_settings_update`; `app.copy` for an invite
  code; `app.open_link` for the server's website.
- **Try:** renaming the server updates the page; Copy shows "Copied";
  copying from a timer raises the press-only error; the link asks "Open this
  link?" with Open and Cancel; `ftp://` and a 2,001-character link are
  refused.
- **Permissions:** `read_server`, `read_invites`, `create_invites`,
  `read_members`, `read_channels`.

### 41. Scheduled Posts

Write announcements now, post them later or on a schedule.

- **Tests:** `messages.send`, `edit`, `delete`; `typing.start` before
  posting; `timer.after`, `every`, `cron`, `cancel`; `db` to keep the
  schedule; a `select` of channels.
- **Try:** a post goes out on its minute, after the typing dots; a cron like
  `*/5 * * * *` repeats; a bad cron shows its error; Cancel stops it; after a
  plugin restart the timers come back from `db`; edit and delete work on
  posted messages; posts show the plugin as author, with its badge.
- **Permissions:** `send_messages`, `read_channels`.

### 42. Command Center

The plugin's slash commands with usage counts, and switches to add or
remove them live.

- **Tests:** `commands.register` and `unregister` while running; options
  with `autocomplete`; `cmd:reply` with `only_you`; `cmd:defer`; a reply
  after 3 seconds; `ui.send_all` each time a command runs.
- **Try:** switching a command off removes it from the `/` popup without a
  restart; a late reply raises "too late to reply" and the log says so; a
  deferred command shows "is thinking…" and a reply minutes later lands; a
  command named like a built-in shows both.
- **Permissions:** `register_commands`.

## I. Lifecycle, errors and the Plugins tab

### 43. Hot Reload Playground

A counter and a half-filled form you keep while editing the plugin with
`opencord-plugin dev`.

- **Tests:** dev mode pushes; client reloads keeping the state;
  `migrate(old_state)`; `require` of a second client file; server restarts
  that keep views open; a push asking for a new permission; dev tokens.
- **Try:** saving a client file updates open views in about a second with
  the count kept; a `migrate` that renames a field works; open modals close
  on reload; a syntax error shows "This plugin hit an error" with file, line
  and trace to you (Manage Plugins) and without them to a plain member;
  fixing it and pressing Reload brings it back; only people with the view
  open get reloads; a push asking for `internet` waits for approval on the
  Plugins tab; a revoked or expired token stops pushing; the Dev badge
  shows.

### 44. Crash Test Dummy

Buttons that break the view and the server script on purpose.

- **Tests:** errors in a press handler, `render`, `init`, `item(i)` and a
  modal; `init` returning no table; `app.set` inside `render`; an endless
  loop in a handler; a memory bomb; a server script that crashes as it
  starts.
- **Try:** a handler error leaves the view running and writes "view error
  for <name>: …" to the plugin's log, at most once every 10 s per view; a
  render error shows the error page and Reload works; an endless loop stops
  at the 50 ms budget and Opencord never freezes; the memory bomb stops only
  this view while chat and voice carry on; a server crash restarts after 1,
  2, 4 and 8 s, the fifth crash in an hour turns the plugin off, and its
  channel greys out meanwhile and comes back.

### 45. Type Shifter

Several versions of one plugin that add, rename and drop channel types.

- **Tests:** `ui.register_channel_type` (key, name, description, icon); the
  20-type limit; registering a key again; the create channel dialog;
  unavailable and type-gone channels; the `start`, `stop` and `update`
  events; update, uninstall and reinstall; signing.
- **Try:** turned off, its channels grey out with "This channel is
  unavailable" on hover and can't be opened by click, keyboard, quick
  switcher or an `opencord://` link (a toast says why), and open views close
  with "This plugin stopped"; turned on, they come back with the same place,
  permissions and data; v2 without type `beta` shows "This plugin no longer
  provides this channel" and admins can delete those channels, while views
  of `alpha` stay open and reload with v2's client; v3 without a `client`
  entry shows the "no screen" error; a new icon or name shows after the
  update (no old cached asset); the 21st type and bad keys, names and icons
  raise; uninstalling with data kept, then reinstalling, brings the data
  back; an unsigned build can't replace a signed one. "Update Opencord to
  open this" can't be reached today: the server refuses `api = 2`, so it
  needs a server built with a newer API.

### 46. Private Vault

Each member's private notes and pictures, that nobody else may see.

- **Tests:** `view.user` as the only identity the server trusts; private
  files through `files.token`; tokens bound to one member and good for an
  hour; `text_input` with `password`.
- **Try:** a client sending someone else's id in `app.send` still gets only
  its own notes; a token copied to another account doesn't load; after an
  hour without reopening, a token stops working; the vault survives an
  update and a restart; error details never show to members without Manage
  Plugins.

### 47. Window Watcher

What the app tells a view about its window and its connection.

- **Tests:** `app.listen` with `pause`, `resume`, `focus`, `blur`,
  `connection_change`; `app.after`, `app.every` and `app.cancel`; frames
  pausing while hidden; `app.on_back` (must do no harm on desktop).
- **Try:** minimising, hiding to the tray and switching virtual desktops
  send `pause` and `resume`, and the on-screen clock stops and starts with
  them; Stop cancels the clock; `focus` and `blur` follow the window;
  turning the network off shows "Reconnecting", then recovers; sleep and
  wake the computer; restart the server with the view open and note what
  the area does (it must not stay blank).

## J. Limits, sandbox and load

### 48. Limit Breaker

One button per limit. Each shows the exact error, and the view keeps
working.

- **Try:** 20,000 blocks pass, 20,001 fail; 64 deep pass, 65 fail; a
  10,000-character prop passes, 10,001 fails (20,000 for `text` and
  `rich_text`); 100 options, then 101; 100 timers, then 101 ("at most 100
  timers"); `app.every(0.1)` works, `0.09` doesn't; NaN, infinity, a size
  over 100,000, `opacity` 1.1, duplicate keys, a child on a `spacer`, two
  children in a `box`; 20 open modals, then 21; 100 handlers waiting on
  dialogs, then 101; 1,000 listeners, then 1,001; device storage over 1 MiB;
  `app.copy` of 20,000 characters, then 20,001; `app.listen("nope")`;
  `app.open_link("ftp://…")`; on the server, 1,000 `ui.on` handlers, then
  1,001, and 10,000 live file tokens.

### 49. Sandbox Escape

A client script that tries every way out of its box.

- **Try:** `http`, `db`, `files`, `hooks`, `io`, `os.execute`, `debug`,
  `loadstring` and `getfenv`/`setfenv` tricks are missing or harmless;
  `require("../server/main")` fails; a plain table dressed up as a block is
  refused ("not a block"); writing to `app.state` directly changes nothing
  on screen; growing a string to 100 MB hits the 32 MiB cap and stops only
  this view; asset paths with `..` load nothing; nothing it does reaches the
  member's identity or private message keys.

### 50. Stress Bench

Many viewers and many changes, to find where it slows down.

- **Try:** 20 members (or test clients) with the channel open while the
  server sends 60 events a second; a 10,000-row list scrolled fast; 1,000
  keyed rows reordered every second; 5,000 canvas items moving on frames; 200
  thumbnails; watch Opencord's CPU and memory and the Plugins tab's CPU and
  memory columns; an idle view uses no CPU; a server script in a hard loop
  waits its turn (25 % of a core) while chat stays quick; if any way exists to
  open five plugin channels at once on one login, the fifth says "too many
  open views".

---

## Index: which idea tests what

**Blocks.** `row`, `column` 1, 3, 15 · `stack` 1, 19 · `grid` 1, 9, 14, 32 ·
`scroll` 1, 4, 14, 15 · `box` 4, 9, 22, 32 · `spacer`, `divider` 1 · `list`
11, 12, 39, 50 · `text` 2 · `rich_text` 5 · `image` 25, 14, 26, 28 · `icon`
2 · `user` 12, 31, 35 · `badge` 9, 15, 36 · `progress` 6, 19, 24 · `spinner`
28 · `button` 6 · `icon_button` 4, 6 · `checkbox`, `switch`, `radio` 6 ·
`slider` 2, 6, 20 · `select` 6, 9, 18, 41 · `tabs` 1, 6 · `segmented` 6, 9,
20 · `text_input` 2, 5, 6, 46 · `image_picker` 26 · `file_picker` 27 · `modal`
16, 17 · `sheet` 17, 19 · `menu` 9, 15, 18, 26 · `tooltip` 18 · `toast` 17, 30
· `canvas` 20–24.

**Common props.** sizes, `flex`, `padding`, `opacity`, `visible`, look props
1 · `tooltip` 1, 10 · `label` 10 · `key` 13, 15 · `animate` 13, 32 · `enter`,
`exit` 12, 13 · `x`, `y`, `right`, `bottom` 19.

**Block events.** presses and hover 4, 9, 22 · inputs 4, 6 · scrolling 4, 11,
14 · image load and error 4, 25 · `on_animation_end` 4, 13 · `on_select` 4,
15 · `on_dismiss` 4, 17 · uploads 26, 27 · canvas pointer, wheel, pan, scale
22, ink strokes 20.

**Client API.** `init`, `render` all · `migrate`, `require` 43 · `app.set`,
`app.state` all, 44, 49 · `app.send`, `app.on` 29 · `app.listen`: `close` 31,
`pause`/`resume` 47, `resize` 3, `theme_change` 2, `connection_change` 31,
47, `focus`/`blur` 8, 47, keys 8, 23 · `app.copy` 4, 40 · `app.open_link` 40,
48 · `app.open_channel` 37 · `app.play_sound` 23, 30, 32 · quick dialogs 17,
18, 35 · `app.modal.*`, `app.modals` 15, 16 · `app.after`, `every`, `cancel`
8, 39, 47 · `app.on_frame`, `off_frame` 23, 47 · `app.storage` 19 ·
`app.on_back` 47 · context fields 3.

**Server side of views.** `ui.register_channel_type` 45 (and every plugin) ·
`ui.on`, `view:send` 29 · `ui.views` 30 · `ui.send_place` 5, 12, 15, 32 ·
`ui.send_role` 27, 30 · `ui.send_all` 7, 13, 24 · `files.token` 18, 25, 46 ·
`file_uploaded` and keeping uploads 26 · files in messages 27 · plugin
channels from Lua 37 · `view_open`, `view_close` 31 · `plugin_channel_create`,
`plugin_channel_delete` 37.

**Server API.** messages 11, 35, 41 · typing 41 · commands 33, 42 · channels
37 · roles 36 · members 35, 36 · bans 35 · invites 40 · server 40 · voice 38 ·
soundboard 38 · music 39 · `db` 9, 12, 15, 32 · files 18 · `http` 28 · hooks
34 · timers 13, 41 · plugin bus 33 · config 7 · log 24, 34, 44 ·
`opencord.on`, `off`, `has_permission` 24, 35.

**Server events.** `start`, `stop`, `update` 45 · `settings_change` 7 ·
`permission_change` 35 · messages and typing 24 · channels 37 · roles 36 ·
members 35 · presence 24 · invites, server, voice and upload settings 40 ·
voice states, streams, soundboard 38 · music 39.

**Plugin area and channel list.** running, all · error and Reload 43, 44 ·
unavailable, type gone 45 · closed: plugin stopped 45, lost access 35, 36,
37, deleted 37 · "no screen" 45 · too old 45 (not reachable yet) · create
dialog 33, 45 · quick switcher and links 45 · Server settings → Channels 37 ·
header and topic 37 · no unread dot 12.

**Plugins tab.** CPU and memory 50 · permissions 35 · settings 7 · log 24, 34,
44 · restart 31, 44 · update, uninstall 45 · dev tokens and reviews 43, 45 · signing
45.

**The app around the area.** theme 2 · text size 3, 10 · reduced motion 10,
13 · screen readers 10 · keyboard and Opencord's shortcuts 8, 23 · window
and tray 47 · network 31, 47 · area size and floating panels 3, 16.

**Limits.** 48, plus view events 29, images 25, 28, uploads 26, web hooks 34,
CPU 44, 50, memory 44, 49, device storage 19, channel types 45.
