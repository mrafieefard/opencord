# Plugins: server scripts, custom channels and activities (design, 2026-11-07)

Status: written 2026-11-07 from a conversation with the owner (2026-11-02 to
2026-11-07), and approved by the owner the same day, including the calls
marked *(Claude's call)* (§25 lists them).

## 1. Why

Server owners want to shape their server beyond what Opencord ships: bots
that answer and moderate, screens of their own inside a channel, and things
people open together in a call. Telegram Mini Apps and Discord Activities do
part of this with web pages. The owner wants more freedom than either, with
no web engine anywhere, drawn natively, on desktop and phone alike.

The model is Roblox and FiveM: a package of Lua scripts, some running on the
server, some on each viewer's device, talking through named events.

## 2. What it is, in one page

- A **plugin** is a zip. An admin drops it into **Server settings →
  Plugins**. Its `manifest.lua` names it and lists the permissions it needs.
- **Server scripts** run on the server, always while the plugin is on. They
  can be a bot (react to messages, members, voice; send, edit, delete;
  register `/commands`), keep data in the plugin's own database, and
  register **channel types** and **activities**.
- **Client scripts** run on a viewer's device while that viewer has the
  plugin's UI open. They draw the UI with our **blocks** and send events to
  the server scripts. Each viewer runs their own copy, like a browser tab;
  viewers can be at different stages. Keeping them in step is the plugin
  developer's job, with server scripts and the database.
- A **custom channel** is a channel whose main area (where chat or the voice
  view normally show) shows the plugin's UI.
- An **activity** is started from the call bar, beside the soundboard,
  camera and screen share. It shows as a tile in the call like a screen
  share; each person taps it to open their own copy, drawn natively.
- The Plugins tab is a resource manager in the spirit of FiveM's: status,
  CPU and memory, start, stop, restart, per-permission switches, settings,
  log, update, uninstall.
- **Dev mode** lets a developer edit a folder on their own computer and see
  every save on a server within a second.

## 3. Goals

- Plugins can do anything a member with the granted permissions could do,
  and react to every event the server has, except the off-limits areas in
  §8.4.
- Custom channels and activities draw natively with Flutter on desktop and
  on the phone (the phone embeds Flutter for this part only, §18).
- Client scripts never reach the network. Everything a viewer's device
  loads comes from its Opencord server.
- A plugin can't do more than its approved permissions allow, and an admin
  can't approve more than they hold.
- A plugin that loops, leaks or crashes affects only itself.
- Signed plugins can only be updated by the same author's key.
- Turning a plugin off and on loses nothing: its channels, their places and
  permissions, and its data come back exactly.
- Developers get a fast loop: dev mode, errors on screen, live log, editor
  autocomplete.

## 4. Non-goals

- **Message components** (blocks inside chat messages). Later, built per
  platform (React Native on the phone).
- **3D.** Later, with `flutter_scene`; on desktop it needs Impeller for the
  whole app, which isn't ready on Windows and Linux.
- **A tile-map block** for large 2D game worlds. Later.
- **Web views** of any kind, now or later.
- **Network access from client scripts.**
- **Hiding plugin code.** Code that runs on someone's machine can be read by
  its owner; Opencord is open source and self-hosted, so encryption would be
  theatre (§9.5).
- **A central plugin store or catalogue.**
- **Plugins touching private messages,** which are end-to-end encrypted.
- **Managing plugins from the phone.** The phone uses plugins (channels,
  activities); installing and the resource manager are desktop only. (The
  owner's decision.)
- **A manifest field limiting a plugin to desktop or phone.** Plugins adapt
  with the responsive context (§11.4) instead. (The owner's decision.)

## 5. Anti-goals (a failure even if everything works)

- A plugin reads private messages, device keys, the server's secrets, or
  another plugin's data.
- A viewer's device contacts any host other than its Opencord server
  because of a plugin.
- A plugin draws outside its own area, or puts up something that passes for
  an Opencord dialog over the rest of the app.
- A plugin does something its approved permissions don't allow, or that
  the admin who approved it couldn't do.
- A looping or crashing plugin slows chat, voice or other plugins, or takes
  the server down.
- Precompiled bytecode is loaded, from anywhere.
- An update from a different key silently replaces a signed plugin.
- Turning a plugin off loses a channel, its position, its permissions or
  the plugin's data.
- A server reached through a plugin's internet access ends up talking to
  the server's own machine or network (SSRF).

## 6. Words used

| Word | Meaning |
|---|---|
| Plugin | One installed package, known by its `id` |
| Manifest | `manifest.lua`, the plugin's description and requests |
| Server VM | The Luau state running a plugin's server scripts |
| View | One viewer's open copy of a plugin UI, on one session: a client VM plus what it draws |
| Place | Where a view was opened: a custom channel, or an activity session in a call |
| Block | One element of the UI vocabulary (`button`, `list`, `canvas`…) |
| Channel type | A kind of custom channel a plugin registered, by `type_key` |
| Activity | A kind of activity a plugin registered, by `activity_key` |
| Activity session | One running activity in one call |
| Dev plugin | A plugin pushed by dev mode: unsigned, reloaded on every save |

## 7. The plugin package

### 7.1 The zip

```
manifest.lua          required, at the root
server/…              server scripts (.lua or .luau)
client/…              client scripts (.lua or .luau)
assets/…              images (png, jpg, webp, gif), fonts (ttf, otf),
                      sounds (ogg, mp3, wav)
SIGNATURE             optional, written by `opencord-plugin pack` (§9)
KEY-ROTATION          optional (§9.3)
```

The server refuses the zip, saying which rule failed, when:

- anything else is in it (no other extensions, no binaries, no folders
  outside the three, no symlinks);
- a path is absolute, contains `..`, or differs only by case from another;
- it exceeds the limits in §21 (zip size, file count, single file size,
  unpacked size, compression ratio);
- the manifest is missing, doesn't return a table, or fails validation.

### 7.2 The manifest

`manifest.lua` is run in an empty Luau VM: no globals beyond `string`,
`table`, `math`, no API, 100 ms and 4 MB. It must return a table:

```lua
return {
  id = "tickets",                 -- [a-z0-9-], 3–40 chars, unique on the server
  name = "Ticket System",
  version = "1.2.0",              -- semver
  author = "Amin",
  description = "Support tickets in a channel of their own.",
  github = "https://github.com/amin/opencord-tickets",   -- optional
  license = "MIT",                -- SPDX id, "All rights reserved", or a URL
  api = 1,                        -- plugin API version it was written for
  icon = "assets/icon.png",
  public_key = "ed25519:…",       -- optional; set by `opencord-plugin pack`

  permissions = {
    "send_messages", "manage_channels", "create_channel_types",
    "register_commands", "internet",
  },
  domains = { "api.example.com" },  -- only with "internet"

  config = {                        -- settings admins fill in (§17.3)
    { key = "support_role", label = "Support role", kind = "role" },
    { key = "api_key", label = "API key", kind = "secret" },
  },

  server = { "server/main.luau" },  -- run in this order at start
  client = "client/main.luau",      -- entry point of every view
}
```

Config kinds: `text`, `secret`, `number`, `boolean`, `choice` (with
`choices`), `role`, `channel`, `user`. Secrets are sealed with the server's
`secrets.key` (like DNS provider tokens), never sent to clients, and shown
masked; an empty value on save keeps the stored one.

`api` lets Opencord grow the API without breaking old plugins, and lets an
app tell that a plugin needs more than it knows (§11.7).

## 8. Permissions

### 8.1 Plugin permissions

A plugin gets nothing it didn't list and the admin didn't approve. Each
plugin permission names the member permission the approving admin must hold
(the owner and `ADMINISTRATOR` hold all).

| Plugin permission | Allows | Admin must hold |
|---|---|---|
| `read_messages` | Message events, fetching history | `READ_HISTORY` |
| `send_messages` | Send messages, reply to commands | `SEND_MESSAGES` |
| `manage_messages` | Edit its own, delete anyone's | `MANAGE_MESSAGES` |
| `attach_files` | Send files from its storage | `ATTACH_FILES` |
| `read_channels` | Channel events and lists | — |
| `manage_channels` | Create, edit, delete, reorder channels | `MANAGE_CHANNELS` |
| `create_channel_types` | Register channel types (§14) | `MANAGE_CHANNELS` |
| `register_activities` | Register activities (§15) | `MANAGE_SERVER` |
| `register_commands` | Register `/commands` | `MANAGE_SERVER` |
| `read_members` | Member and presence events and lists | — |
| `read_roles` | Role events and lists | — |
| `manage_roles` | Roles and member roles | `MANAGE_ROLES` |
| `manage_nicknames` | Change nicknames | `MANAGE_NICKNAMES` |
| `kick_members` | Kick | `KICK_MEMBERS` |
| `ban_members` | Ban, unban, list bans | `BAN_MEMBERS` |
| `read_invites` | Invite events and lists | `MANAGE_SERVER` |
| `create_invites` | Create invites | `CREATE_INVITE` |
| `read_server` | Server, voice and upload settings events | — |
| `read_voice` | Voice states, streams, soundboard events | — |
| `move_members` | Move, disconnect | `MOVE_MEMBERS` |
| `mute_members` / `deafen_members` | Server mute / deafen | `MUTE_MEMBERS` / `DEAFEN_MEMBERS` |
| `play_soundboard` | Play sounds in a call without being in it (§10.9) | `USE_SOUNDBOARD` |
| `read_music` | Music events and queue | — |
| `control_music` | Add, skip, pause… without being in the call (§10.9) | `MUSIC_MANAGE` |
| `internet` | HTTP to the manifest's `domains`; images from them (§13) | `MANAGE_SERVER` |
| `receive_web_requests` | Answer requests on `/plugins/<id>/hooks/…` | `MANAGE_SERVER` |
| `accept_uploads` | The `image_picker`/`file_picker` blocks (§13.3) | `ATTACH_FILES` |
| `talk_to_plugins` | Send and receive other plugins' shared events | `MANAGE_SERVER` |

A plugin can always use its own database, files, config, log, timers and
views, and register handlers for its own UI events.

### 8.2 The plugin's role *(Claude's call)*

Installing a plugin creates a **managed role** "Plugin: <name>", placed just
below the installing admin's highest role (the owner's: at the top). It
carries the member permission bits that match the plugin's granted
permissions. Members can't be given it, and it can't be deleted while the
plugin is installed; admins can move it down.

It gives plugins what Discord's bot roles give: the hierarchy works
unchanged (a plugin can only kick, ban or manage roles below its role), and
admins can hide channels from a plugin with ordinary channel overwrites.
A plugin receives events only from channels its role can see.

### 8.3 Granting and revoking

- The install review lists every requested permission with a sentence on
  what it allows; all start ticked, and the admin can untick any. A plugin
  must handle a missing permission (calls fail with `PERMISSION_DENIED`;
  `opencord.has_permission(name)` checks first).
- Permissions can be switched off and on later on the plugin's page; the
  server VM gets `permission_change` and keeps running.
- Only permissions the acting admin holds can be switched on. This is
  checked when they're granted, not afterwards: the plugin's role then
  belongs to the plugin, as a bot's role does on Discord, and keeps working
  when the admin who approved it loses a role or leaves. The log records
  who granted each permission. *(Claude's call, from the design review.)*
- An update that asks for new permissions shows them; they start unticked
  if the admin doesn't hold them.

### 8.4 Off-limits whatever is granted

- Private messages, friend requests, privacy settings, devices and keys.
- Network, domains, certificates, tunnels, storage settings, the server's
  secrets, and `opencord.toml`.
- Other plugins' data, files and VMs (except events a plugin chooses to
  share, with `talk_to_plugins`).
- Member sessions, tokens and identity keys.

### 8.5 New member permissions

- `MANAGE_PLUGINS`: install, update, configure, start, stop and uninstall
  plugins, create dev tokens. Owner by default.
- `START_ACTIVITIES`: start an activity in a call one is in. On for
  @everyone in new servers and added to @everyone on upgrade, like
  `SCREENSHARE`. *(Claude's call.)*

Opening a custom channel needs `VIEW_CHANNEL`, like any channel. The plan
picks the bits.

## 9. Installing, updating and signing

### 9.1 Install

1. The admin drops a zip on the Plugins tab (or picks one). It goes up by
   HTTPS, like attachments, and is checked (§7.1).
2. The review dialog shows: icon, name, version, author, description,
   GitHub link with its check (§9.4), signature state (Signed / Unsigned /
   Dev), licence, requested permissions with switches, domains, and config
   fields that must be filled.
3. The admin accepts the licence (when it isn't an open-source SPDX id) and
   presses **Install**. The plugin is unpacked to
   `data/plugins/<id>/<version>/`, its role is created, and it starts.

### 9.2 Update

A zip whose `id` is already installed is an update. The review adds:
"This replaces Ticket System 1.2.0 by Amin", the version change, and the
permission changes. Data, files, config, channels and role stay.

| Installed | New zip | Result |
|---|---|---|
| Unsigned | Unsigned | Allowed |
| Unsigned | Signed | Allowed; the plugin is pinned to that key from now on |
| Signed | Same key | Allowed |
| Signed | Key named in a valid `KEY-ROTATION` | Allowed; the pin moves to the new key |
| Signed | Different key or unsigned | Refused: "This isn't from the same author." The admin can only uninstall and install it as new |

The previous version's folder is kept until the new one has started, so a
version that fails to start rolls back. *(Claude's call.)*

### 9.3 Signing

- `opencord-plugin keygen` makes an Ed25519 key pair; the private key stays
  on the developer's machine.
- `opencord-plugin pack` writes `public_key` into the manifest, then a
  `SIGNATURE` file: the signature over a canonical list of every other
  file's path and SHA-256, sorted by path.
- The server checks it on every install and update. Any file added,
  removed or changed after signing fails the check.
- First install of a signed plugin is trust on first use: the server
  records `id → key`.
- **Changing keys:** `opencord-plugin rotate-key` writes `KEY-ROTATION`, a
  statement naming the new public key, signed by the old key. The zip is
  then signed by the new key.
- **A lost key** can't be recovered; the plugin can't be updated in place.

Signing proves who wrote a plugin, not that it's safe. Permissions remain
the protection.

### 9.4 Verified via GitHub *(Claude's call on the file path)*

When the manifest has a `github` link and the plugin is signed, the server
fetches `.opencord/plugin.pub` from the repository's default branch over
`raw.githubusercontent.com`. If it matches `public_key`, the review and the
plugin's page show "Verified: github.com/amin/opencord-tickets". It's
re-checked on each update. "Couldn't check GitHub" is shown when the fetch
fails; it doesn't block installing.

### 9.5 Protecting a developer's work

Code can't be hidden from the machines that run it: server scripts sit on
the admin's disk, client scripts on viewers' devices. Opencord won't
pretend otherwise (no encryption, no bytecode). What we give:

- the `license` field, shown at install and accepted by the admin;
- `secret` config fields, for licence keys or API keys;
- `internet` and `receive_web_requests`, so the valuable part can stay on
  the developer's own service and the plugin calls it;
- documentation saying so, and that client scripts should only draw.

Opencord is AGPL-3.0. Whether closed, paid plugins are clearly allowed may
need an explicit plugin exception to the licence; the owner checks with a
lawyer. Not part of this plan.

### 9.6 Off, on, uninstall

- **Stop** ends the server VM after a `stop` event (1 s to finish), closes
  every view with "This plugin was turned off", greys out its channels
  (§14.4), ends its activity sessions, and removes its commands.
- **Start** runs it again; registrations come back as the scripts run.
- **Uninstall** asks two things: delete the plugin's data and files, or
  keep them; delete its channels, or keep them greyed out. Kept channels
  and data reconnect when a plugin with the same `id` (and, if it was
  signed, the same key) is installed again. The role is deleted.

## 10. Server runtime

### 10.1 The VM

- Luau through `mlua` (feature `luau`), one VM per plugin, on its own
  thread or task so a busy plugin can't block the gateway.
- Sandbox: `mlua`'s Luau sandbox mode; no `io`, `os` (except `os.time`,
  `os.clock`, `os.date`), `debug`, `loadstring`, `getfenv`/`setfenv`, or
  `require` of anything but the plugin's own files. Only source is ever
  compiled; bytecode is never loaded.
- A CPU budget per handler call, enforced with Luau's interrupt; time spent
  waiting on async calls (HTTP, database, files) doesn't count. A memory
  cap per VM. Numbers in §21.
- A handler that errors or runs out of time is logged with its stack trace;
  the plugin keeps running. Running out of memory, or 5 failures in 10
  minutes, restarts the VM with backoff; 5 restarts in an hour turn it off,
  with the reason on its page. *(Claude's numbers.)*
- Each plugin has an event queue; when full, the oldest events are dropped
  and the drop is logged.
- Calls that wait (HTTP, database, files, gateway actions) are async
  functions: the handler's coroutine yields while Rust does the work.

### 10.2 Lua helpers available everywhere

`string`, `table`, `math`, `utf8`, `bit32`, `buffer`, `coroutine`, plus
`json.encode`/`json.decode` and `os.time`/`os.clock`/`os.date`.

### 10.3 Server API

Names are the contract the plan builds and the type definitions describe.

| Area | Calls |
|---|---|
| Events | `opencord.on(event, fn)`, `opencord.off(event, fn)` |
| Plugin | `opencord.id`, `opencord.version`, `opencord.has_permission(p)`, `config.get(key)`, `log.info/warn/error(...)` |
| Messages | `messages.send(channel, { content, files? })`, `messages.edit(id, content)`, `messages.delete(id)`, `messages.fetch(channel, { before?, limit })`, `typing.start(channel)` |
| Commands | `commands.register({ name, description, options })` (option kinds as in `commands.proto`), `commands.unregister(name)`; a command event carries `reply(content, { only_you? })` |
| Channels | `channels.list()`, `channels.get(id)`, `channels.create({ kind, name, topic?, parent?, type_key? })`, `channels.update(id, …)`, `channels.delete(id)`, `channels.reorder(…)`, `channels.set_overwrite(…)` |
| Roles and members | `roles.list/create/update/delete/reorder`, `members.list/get`, `members.add_role/remove_role`, `members.set_nickname`, `members.kick(user, reason?)`, `members.ban/unban`, `bans.list()` |
| Voice | `voice.states(channel?)`, `voice.move/disconnect/mute/deafen(user, …)`, `soundboard.play(channel, sound)` |
| Music | `music.state(channel)`, `music.add/skip/pause/resume(…)` |
| Invites and server | `invites.list/create`, `server.get()` |
| Your UI | `ui.register_channel_type(key, spec)`, `ui.register_activity(key, spec)`, `ui.views(place?)`, `view:send(name, data)`, `ui.send_place(place, name, data)`, `ui.send_role(role, name, data)`, `ui.send_all(name, data)`, `activity.end(session)` |
| Storage | `db.exec(sql, ...)`, `db.query(sql, ...)`, `db.transaction(fn)` |
| Files | `files.put(path, bytes)`, `files.get(path)`, `files.delete(path)`, `files.list(prefix)` |
| Internet | `http.request({ method, url, headers?, body?, timeout? })` |
| Web hooks | `hooks.on(path, fn(request) → response)` |
| Time | `timer.after(seconds, fn)`, `timer.every(seconds, fn)`, `timer.cron("0 9 * * *", fn)`, `timer.cancel(t)` |
| Other plugins | `plugins.emit(name, data)`, `plugins.on(plugin_id, name, fn)` |

Every call checks the plugin's permissions and the role's channel access,
goes through the same server code as a member's request (the same checks,
limits and events), and is rate-limited like a member (§21).

### 10.4 Server events

| Group | Events | Needs |
|---|---|---|
| Plugin | `start`, `stop`, `update` (from, to), `settings_change`, `permission_change` | — |
| Messages | `message_create`, `message_update`, `message_delete`, `typing_start` | `read_messages` |
| Commands | `command`, `command_autocomplete` | `register_commands` |
| Channels | `channel_create`, `channel_update` (moves included), `channel_delete` | `read_channels` |
| Your channel types | `plugin_channel_create`, `plugin_channel_delete` | — |
| Roles | `role_create`, `role_update`, `role_delete` | `read_roles` |
| Members | `member_join` (with the invite used), `member_leave` (left, kicked or banned; by whom), `member_update`, `member_unban`, `presence_update` | `read_members` |
| Invites | `invite_create`, `invite_revoke` | `read_invites` |
| Server | `server_update`, `voice_settings_update`, `upload_settings_update` | `read_server` |
| Voice | `voice_state_update`, `stream_create`, `stream_update`, `stream_delete`, `stream_viewers_update`, `soundboard_play`, `soundboard_sound_create`, `soundboard_sound_update`, `soundboard_sound_delete` | `read_voice` |
| Music | `music_now_playing`, `music_queue_update`, `music_notice` | `read_music` |
| Your UI | `view_open` (view: user, session, place, platform), `view_close` (closed, disconnected, lost access, plugin stopping), and every named client event via `ui.on(name, fn(view, data))` | — |
| Activities | `activity_start` (session, starter, call), `activity_end` | `register_activities` |
| Web | `hooks.on` handlers | `receive_web_requests` |
| Uploads | `file_uploaded` (view, file) | `accept_uploads` |

Every event the gateway has today maps onto one of these except private
messages, network, storage, uploads in progress and the owner-only notices,
which are off-limits (§8.4).

### 10.5 Storage

- Each plugin gets its own SQLite file, `data/plugins/<id>/data.sqlite`,
  with plain SQL and `?` parameters. It can't see Opencord's tables or
  another plugin's. WAL mode, a size quota (§21), and a busy timeout.
- Files are kept on the server's disk, under `data/plugins/<id>/files/`,
  with a quota. *(Claude's call, 2026-11-07, while planning: the storage
  mover and "remove a bucket only when it's empty" only know attachments,
  so plugin files in a bucket could be stranded. Buckets for plugin files
  can come later.)*
- Both are kept when the plugin is stopped, and on uninstall unless the
  admin deletes them.

### 10.6 Messages from plugins

A message a plugin sends shows the plugin's name and icon with a **Plugin**
badge. Behind it, each plugin has a **plugin user**: a user row with no
usable key (nobody can sign in as it), never a member, never in the member
list. Messages, soundboard plays and queued tracks name it as their author.
Every app gets the list of plugin users (`Ready` and an update event) and
shows those authors with the plugin's name, icon and badge. *(Claude's
call; the plugin user was settled while planning.)*

### 10.7 Internet access

`http.request` and server-fetched images (§13) only reach the manifest's
`domains` (exact host, or `*.example.com`). The server resolves the name
itself and refuses loopback, private, link-local, multicast and cloud
metadata addresses (IPv4 and IPv6), after following any redirect (each hop
re-checked). Time, response size and concurrent requests are limited
(§21).

### 10.8 Web hooks

With `receive_web_requests`, requests to
`https://<server>/plugins/<id>/hooks/<path>` reach the plugin's
`hooks.on(path, fn)` handler, which returns `{ status, headers?, body? }`.
Body size, rate and time are limited (§21). No cookies or Opencord sessions
are passed through.

### 10.9 Soundboard and music from a plugin

A plugin is never in a call. With `play_soundboard` it plays a sound in a
voice channel without being connected; the usual checks that need a voice
state are skipped for it, and the rest apply: the sound exists, three at
once per channel, and a cooldown per plugin. With `control_music` its role
carries `MUSIC_MANAGE`, which already lets members control music without
being connected. Plays and tracks show the plugin as who played or added
them. *(Claude's call, from the design review.)*

## 11. Client runtime

### 11.1 Where it runs

In `opencord-core`, the Rust core the desktop app and the phone app share:
Luau through `mlua`, one VM per open view, the same sandbox as §10.1, a
smaller memory cap, and a per-handler CPU budget (§21). The core holds the
VM, runs `render`, compares trees and sends changes to the renderer; the
renderer only draws and reports input (§16, §18).

When a view opens, the core asks the server for the plugin's client
scripts and assets; they're cached on the device by content hash, so a
plugin that didn't change loads from cache.

### 11.2 A client script

```lua
-- client/main.luau
function init(ctx)            -- ctx: viewer, place, size, theme, platform
  return { count = 0 }        -- the view's first state
end

function render(s)
  return ui.column({ gap = 8, padding = 12 }, {
    ui.text({ text = "Clicks: " .. s.count, size = 18 }),
    ui.button({ label = "Click", on_press = function()
      app.set(function(s) s.count += 1 end)
      app.send("clicked")
    end }),
  })
end

app.on("count", function(n) app.set(function(s) s.count = n end) end)
```

### 11.3 Redrawing

React-style, never a loop:

1. A handler runs (a tap, a server event, a timer) and calls `app.set`.
2. The view is marked dirty; several changes in one handler give one
   redraw.
3. On the next frame the core calls `render(state)` and checks the tree
   (unknown blocks or bad props are errors with the block's path).
4. The core compares it with the last tree by `key` (like React; children
   without keys compare by position) and sends only the changes: add,
   remove, move, update props.
5. The renderer applies them to its widgets.

Handlers in the tree are Lua functions; the core gives each an id per
redraw and drops the old ones. Text fields own their text while the viewer
types (`on_change` is throttled; setting `value` replaces it). Scroll
positions belong to the renderer too. Declared animations run in the
renderer. An idle view runs no Lua.

### 11.4 Client API

| Area | Calls |
|---|---|
| State | `app.set(fn)`, `app.state` (read) |
| Server | `app.send(name, data)`, `app.on(name, fn)` |
| Context | `app.viewer` (id, name, roles), `app.place`, `app.size`, `app.theme`, `app.platform` |
| Responsive | `app.breakpoint`: `"compact"` (plugin area under 600 wide), `"medium"` (600–839), `"expanded"` (840 and up), measured on the plugin area, not the window; `app.input`: `"touch"` or `"mouse"`; `app.safe_area` (insets for notches and the home bar when the area reaches the screen's edge); `app.text_scale` (the viewer's font size setting) |
| Back | `app.on_back(fn)`: the phone's back button or gesture first closes the top modal, then calls this; returning `true` means the plugin handled it (detail → list); otherwise back leaves the plugin |
| Modals | `app.modals[name] = fn(state, props)`, `app.modal.open(name, props) → handle`, `handle:close()`, `handle:update(props)`, `app.modal.close(result?)`, `app.modal.close(name)`, `app.modal.close_all()`, `app.modal.is_open(name)`, `app.modal.ask(name, props) → result` |
| Quick dialogs | `app.confirm(text) → bool`, `app.alert(text)`, `app.prompt(text, default?) → string?` |
| Device storage | `app.storage.get/set/delete(key)`: a small key-value store per plugin, per server, on this device |
| Other | `app.play_sound(asset, { volume? })`, `app.copy(text)` (only inside a press handler), `app.open_link(url)` (Opencord asks the viewer first), `app.open_channel(id)` |
| Time | `app.after(seconds, fn)`, `app.every(seconds, fn)` (paused while the view is hidden), `app.cancel(t)` |

`app.modal.ask` and the quick dialogs pause the handler's coroutine until
the dialog closes.

There are no network calls of any kind.

### 11.5 Client events

| Group | Events |
|---|---|
| Lifecycle | `init(ctx)`, `close`, `pause`/`resume` (phone app in the background, activity minimised, window hidden), `back` (through `app.on_back`) |
| From the server | each `app.on(name, fn)` |
| Surroundings | `resize`, `theme_change`, `connection_change` (lost, back), `focus`/`blur` (window) |
| Call (activities) | `call_members_change`, `speaking_change` |
| Pressing | `on_press`, `on_double_press`, `on_long_press`, `on_secondary_press`, `on_hover_enter`, `on_hover_exit` |
| Inputs | `on_change`, `on_submit`, `on_focus`, `on_blur`, slider `on_change_end` |
| Scrolling | `on_scroll`, `on_reach_start`, `on_reach_end` |
| Overlays | `on_dismiss` |
| Media | image `on_load`/`on_error`, `on_animation_end`, `on_sound_end` |
| Canvas | `on_pointer_down`/`_move`/`_up`/`_cancel` (x, y, pointer, kind: mouse/touch/pen, buttons, pressure where reported), `on_wheel`, `on_pan`, `on_scale`, ink `on_stroke` |
| Keyboard | `on_key_down`, `on_key_up`: only while the plugin area has focus |
| Frames | `app.on_frame(fn(dt))`: only while visible, within the frame budget; `app.off_frame()` |

Opencord's own shortcuts (push-to-talk, mute, deafen, the app's global
keys) are handled first and never reach a plugin.

### 11.6 Hot reload state

When dev mode replaces a client script, the view keeps its state: if the
new script defines `migrate(old_state)`, its result becomes the state;
otherwise the old state is kept as is. Then it redraws. *(Claude's call.)*

### 11.7 Older apps

An app that supports a lower plugin API than the manifest's `api`, or
meets a block it doesn't know, shows "Update Opencord to open this" in the
plugin area instead of a broken screen.

Apps from before a milestone don't know plugin channels or activities at
all. So `Identify` gains `features`, the plugin features the app supports
(`"plugin_users"` from M1, `"plugin_channels"` from M2, `"activities"` from
M3). The server sends plugin channels only to sessions that list
`"plugin_channels"`, and activity sessions only to those that list
`"activities"`; other apps simply don't see them. Plugin messages need
nothing: the phone learns to show plugin authors in M1. *(Claude's call,
from the design review.)*

## 12. Blocks

### 12.1 Common props

Every block takes: `key`, `width`/`height` (number, `"fill"`, or
`"fit"`), `min_width`/`max_width`/`min_height`/`max_height`, `flex`,
`padding`, `opacity`, `visible`, `tooltip`, `label` (for screen readers),
and `animate` (§12.4). Containers also take `background` (colour or
gradient), `border`, `radius`, `shadow`.

Layout is a flexbox subset: `row` and `column` with `gap`, `align` (main
axis: start, center, end, space-between, space-around) and `cross` (start,
center, end, stretch), and `wrap`. Sizes are logical pixels.

### 12.2 The blocks

| Group | Block | Notes |
|---|---|---|
| Layout | `row`, `column` | As above |
| | `stack` | Layers; children may set `x`, `y`, `right`, `bottom` |
| | `grid` | `columns` (count) or `min_column_width` |
| | `scroll` | Vertical or horizontal |
| | `box` | One child with padding, background, border, radius, shadow; tappable |
| | `spacer`, `divider` | |
| Lists | `list` | Lazy: `count`, `item(i)`, `reverse` (pinned to the bottom), `on_reach_start`, `on_reach_end` |
| Content | `text` | `size`, `weight`, `color`, `align`, `max_lines`, `selectable`, `font` |
| | `rich_text` | Bold, italic, code, links, mentions, emoji: the same as messages |
| | `image` | Sources in §13; `fit`, `radius`, `placeholder`, `version`, `on_load`, `on_error`; animated GIF and WebP |
| | `icon` | The app's icon set, by name |
| | `user` | Avatar and name from the app's cache, `size`, `show_name` |
| | `badge`, `progress` (bar, ring), `spinner` | |
| Controls | `button` | `variant` (primary, secondary, danger, plain), `icon`, `loading`, `disabled` |
| | `icon_button`, `checkbox`, `switch`, `radio`, `slider`, `select`, `tabs`, `segmented` | |
| | `text_input` | One or several lines, `placeholder`, `password`, `max_length`, `value` |
| | `image_picker`, `file_picker` | With `accept_uploads` (§13.3) |
| Overlays | `modal`, `sheet` | Inside the plugin area only (§12.5) |
| | `menu`, `tooltip`, `toast` | Inside the plugin area only |
| Drawing | `canvas` | §12.6 |

The look is Opencord's: blocks use the viewer's theme (`theme.accent`,
`theme.surface`, `theme.text`… in light and dark) unless the plugin sets
colours. Plugins may bundle fonts and use them by file name.

### 12.3 What isn't a block yet

Charts, a video player, date and colour pickers, drag and drop, 3D, a tile
map, message components (§4).

### 12.4 Animation

`animate = { duration = 200, curve = "ease_out" }` on a block animates
changes to its size, position, opacity and colours. Keyed children of a
`list`, `column` or `row` can set `enter` and `exit` (fade, slide, scale).
The renderer runs them; Lua isn't called per frame.

### 12.5 Modals and sheets

- They cover only the plugin's area (the channel's main area, or the
  activity tile or large view) with a dimmed backdrop. The rest of Opencord
  stays visible and usable.
- Named modals (§11.4) are kept by the platform in a stack: opening one
  puts it on top; each is drawn again with the current state on every
  redraw, so an open modal stays live. `ui.modal({ open = … })` in the tree
  also works for simple cases.
- `size`: small, medium, large, full (fills the plugin area). `sheet`
  slides up from the bottom of the area. At the `compact` breakpoint a
  `modal` shows as a sheet unless it sets `sheet = false`.
- `dismissible` (default true): tapping outside, Esc, and the phone's back
  button or gesture close the top modal; back leaves the plugin only when
  no modal is open.
- Keyboard focus stays inside the top modal. Opening and closing animate.
- Closing the view closes its modals.

### 12.6 Canvas

- **Retained**: the plugin adds and removes drawing items (lines, paths,
  rectangles, ellipses, text, images from §13, with stroke, fill,
  gradient, opacity, transform), each with an id. Nothing loops.
- **Ink mode**: `ink = { color, width }` makes the renderer draw strokes
  under the pointer itself, with no Lua in the loop; `on_stroke` delivers
  the points in small batches while drawing and at the end.
- **Frame loop**: with `app.on_frame`, the plugin replaces the drawing each
  frame within the frame budget; paused while hidden.
- Pointer and gesture events as in §11.5.

Desktop draws it with Flutter's canvas, and so does the phone (the same
renderer, §18).

## 13. Images and files

### 13.1 Five sources, one block

| Source | Block |
|---|---|
| The plugin's assets | `ui.image({ asset = "assets/logo.png" })` |
| The plugin's storage | `ui.image({ file = "tickets/42.png", version = n })` |
| The internet, through the server | `ui.image({ url = "https://…" })` |
| Opencord's own media | `ui.user({ id })`, `ui.image({ attachment = id })` |
| Small raw images | `ui.image({ bytes = data })`, sent in an event (size cap, §21) |

Every image a device shows comes from its Opencord server. For `url`, the
client asks the server; the server fetches it under §10.7's rules (with the
`internet` permission and the manifest's domains), checks the type, file
size and pixel count, drops anything that isn't a PNG, JPEG, WebP or GIF,
and caches it for every viewer. SVG isn't supported.

Plugin images are served under `/plugins/<id>/…` with the viewer's media
token, like other media. The device caches them on disk and decodes them
at the size shown.

Who may load which file *(Claude's call, from the design review)*:

- `files.put(path, bytes, { public = true })` makes a file any member may
  load, like avatars. Files are private by default.
- A private file reaches a viewer only through a token the server script
  makes for that view: `files.token(path, view)` (good for an hour), shown
  with `ui.image({ file_token = t })`. So a changed app can't read other
  members' private files by guessing paths.
- `ui.image({ url })` may ask for any URL on the manifest's domains; the
  admin approved those domains.

### 13.2 Changing images

Saving over a file and raising `version` makes clients load it again.

### 13.3 Uploads from viewers *(Claude's call)*

With `accept_uploads`, `image_picker` and `file_picker` send a file through
the server's existing upload path and checks (blocked extensions, size
limits). Server Lua gets `file_uploaded` with the view and the file, and
keeps it with `files.put` or lets it be deleted after an hour.

## 14. Custom channels

### 14.1 Registering a type

With `create_channel_types`, a server script registers types as it starts:

```lua
ui.register_channel_type("tickets", {
  name = "Tickets", icon = "assets/ticket.png",
  description = "Open and track support tickets",
})
```

`type_key` is `[a-z0-9_]`, up to 40, and must stay the same across versions;
the developer docs say so.

### 14.2 The channel

A new channel kind, `PLUGIN`, with `plugin_id` and `type_key`. It's an
ordinary channel otherwise: id, name, topic, position, category,
overwrites. **Create channel** lists the registered types of running
plugins (with `MANAGE_CHANNELS`); a plugin with `manage_channels` can
create them too. The plugin gets `plugin_channel_create` and
`plugin_channel_delete`.

### 14.3 Opening one

The main area shows the plugin's view, with these states:

| State | Shows |
|---|---|
| Loading | A spinner in the area while scripts load and `init` runs |
| Running | The plugin's UI |
| Error | "This plugin hit an error" and Reload; admins and dev token holders also see the error, file, line and stack trace |
| Unavailable | §14.4 |
| Too old | "Update Opencord to open this" (§11.7) |

Each person who opens it gets their own view; leaving the channel closes
it.

### 14.4 Available or not

A plugin channel is **available** when its plugin is running and has
registered its `type_key`. Otherwise it stays in the channel list in its
place, greyed out, and can't be opened:

- desktop: hovering shows "This channel is unavailable";
- phone: tapping shows the same message briefly;
- if the plugin is running but didn't register the type (a newer version
  dropped or renamed it): "This plugin no longer provides this channel".
  Admins can delete such channels.

Nothing about the channel row changes when the plugin stops or starts.
The server compares `(plugin_id, type_key)` with what the running plugin
registered. The plugin's data for a channel is saved under the channel's
id, so it comes back too. Two plugins can use the same `type_key`; the
`plugin_id` keeps them apart.

### 14.5 Unread dots

Plugin channels have no unread dot or mention count in this plan: unread
state lives on each device and is worked out from messages, which plugin
channels don't have. It needs per-user unread state on the server first.
Later. *(Claude's call, from the design review.)*

## 15. Activities

### 15.1 Registering

With `register_activities`:

```lua
ui.register_activity("paint", {
  name = "Paint together", icon = "assets/paint.png",
  description = "A shared whiteboard",
})
```

### 15.2 Starting one

- An **Activities** button in the call bar, beside the soundboard, camera
  and screen share, opens a picker of the registered activities of running
  plugins. It shows for members in the call with `START_ACTIVITIES`.
- Starting one creates an **activity session** in that voice channel. The
  plugin gets `activity_start`; everyone in the call sees a tile in the
  call grid like a screen share: the activity's icon and name, who started
  it, and who has it open.
- Several activities can run in one call at once, each with its own tile.
  *(Claude's call.)*

### 15.3 Opening it

- Tapping the tile opens the viewer's own view in the tile; it can be made
  large like a screen share. Closing it closes only that viewer's view.
- The view's `place` names the session and the call; client scripts get
  `call_members_change` and `speaking_change`.
- People who join the call later see the tile and can open it.

### 15.4 Ending

The session ends when the starter or an admin ends it from the tile, when
the plugin calls `activity.end(session)`, when the plugin stops, or when
the call empties. *(Claude's call.)* The plugin gets `activity_end`, open
views close with "This activity ended", and the tile goes.

## 16. Rendering on desktop

- A Flutter package, `opencord_blocks`, holds the renderer: one widget per
  block, the patch applier, the modal stack, the canvas, image loading,
  and the plugin area's states. It's used by the desktop app directly and
  by the phone's embedded Flutter module (§18).
- On desktop it gets changes and sends input through
  `flutter_rust_bridge`, like the rest of the app.
- Desktop stays on Skia; nothing here needs Impeller.

## 17. The Plugins tab (desktop)

### 17.1 The list

Server settings → **Plugins**, for members with `MANAGE_PLUGINS`:

- a drop zone (and a button to pick a zip);
- one row per plugin: icon, name, version, author, signature state (Signed,
  Verified, Unsigned, Dev), status (running, stopped, crashed, starting),
  CPU and memory use, and a switch to turn it on or off.

### 17.2 A plugin's page

- **Overview**: description, GitHub link and check, licence, key
  fingerprint, installed date, data and file use against their quotas.
- **Permissions**: one switch each, with what it allows; the domains list.
- **Settings**: the manifest's config fields; secrets masked.
- **Log**: the live log (info, warnings, errors with stack traces, actions
  such as "deleted message 123 in #general"), filterable, the last 5,000
  lines kept on disk.
- **Actions**: restart, update (pick or drop a zip), uninstall.

### 17.3 Dev tokens

A section to create a dev token for one plugin `id`: it's shown once,
expires (7 days by default), can be revoked, and lists when it was last
used.

### 17.4 Reviews

The install and update reviews (§9.1, §9.2) are dialogs on this tab.

## 18. The phone

- The phone embeds the Flutter engine (add-to-app) for plugin areas only:
  a small Flutter module wrapping `opencord_blocks`, built as an Android AAR
  and iOS frameworks, wired into the Expo app with a config plugin. The
  phone builds and CI need the Flutter toolchain.
- **One Rust core.** The Flutter module doesn't link the Rust core (on
  iPhone that would give a second, separate copy). The core runs in the
  React Native app as now; native code passes the core's changes to the
  Flutter view, and the view's input back, over a binary platform channel.
- The engine is warmed up when the viewer joins a call or opens a server
  that has plugins, not when they tap.
- React Native builds the screens around the plugin area: the plugin
  channel screen with its states, the unavailable message, the activity
  tile, the large view, the Activities picker.
- Keyboard, focus and back gestures between React Native and the Flutter
  view are checked on real phones early.

## 19. Dev mode

- `opencord-plugin init <id>` creates a starter plugin: a manifest, a
  server script, a client script, and `opencord.d.luau` (type
  definitions for luau-lsp, generated from the same API description as the
  docs).
- `opencord-plugin dev --server <address> --token <dev token>` watches the
  folder and sends changed files on every save over HTTPS (unzipped,
  unsigned). The first push shows the install review on the Plugins tab;
  pushes that ask for new permissions wait for an admin there.
- Within about a second, server scripts restart (data kept), and open
  views get the new client scripts with their state kept (§11.6).
- The terminal streams the plugin's log live.
- Errors show in the plugin area with file, line and stack trace for
  admins and the token's holder.
- Dev plugins show a **Dev** badge; they can't replace a signed plugin with
  the same `id`.
- The developer can run an Opencord server on their own computer as a
  sandbox; there's no separate simulator.
- Later: `opencord-plugin test`, Luau tests against a fake server.

`opencord-plugin` also has `keygen`, `pack` and `rotate-key` (§9.3).

## 20. Protocol changes (what's needed; the plan names them)

- `ChannelKind` gains `PLUGIN`; `Channel` gains `plugin_id`, `type_key`,
  and whether it's available.
- Plugin users: `Ready` lists them (user id, plugin id, name, icon), and an
  event updates the list; messages, soundboard plays and tracks name them
  as authors.
- `Identify.features` (§11.7).
- Requests: plugin list and details, install and update (the zip goes up
  over HTTPS, then a review is confirmed), set permissions, set config,
  start, stop, restart, uninstall, fetch log, watch log, create and revoke
  dev tokens; open view, close view, client event; start and end an
  activity session.
- Events: plugin status, registered channel types and activities, activity
  session create/update/delete, server events to a view, client script
  update (hot reload), log lines to watchers.
- Commands: plugin commands use the existing command specs, with `feature`
  set to `plugin:<id>`; `CommandsUpdate` goes out when they change.
- Permissions: `MANAGE_PLUGINS`, `START_ACTIVITIES`; the managed role.

## 21. Limits

Defaults *(Claude's numbers)*, changeable in a `[plugins]` section of
`opencord.toml`:

| Limit | Default |
|---|---|
| Plugins per server | 50 |
| Zip size / unpacked size | 50 MB / 200 MB |
| Files in a zip / largest file | 2,000 / 20 MB |
| Server VM memory | 128 MB |
| Server handler CPU time | 100 ms (waiting on async calls doesn't count) |
| Event queue | 10,000 events |
| Database size / file storage | 1 GB / 5 GB per plugin |
| HTTP: concurrent / timeout / response | 10 / 10 s / 10 MB |
| Web hooks: body / rate | 1 MB / 60 per minute |
| Plugin actions | A member's rate limits, per plugin |
| Client VM memory | 32 MB |
| Client handler CPU time | 50 ms |
| Frame budget (`on_frame`) | 8 ms |
| Events to a view / from a view | 60 / 30 per second |
| Event payload | 64 KB |
| Raw image bytes | 256 KB |
| Server-fetched image | 10 MB, 40 megapixels |
| Device storage per plugin | 1 MB |
| Dev token lifetime | 7 days |

## 22. Milestones (order agreed with the owner)

1. **Runtime, resource manager and bots** (desktop). The server VM and
   sandbox, the manifest, permissions and the managed role, install,
   update and signing, the Plugins tab, storage, files, internet, web
   hooks, timers, commands, every server event, the `opencord-plugin` CLI
   with dev mode. Example: a simple moderation and welcome bot.
2. **Custom channels** (desktop). The client VM in the core, the redraw
   pipeline, `opencord_blocks` with every block in §12, images (§13), the
   plugin channel kind and its states. Example: the ticket system.
3. **Activities** (desktop). Registration, the call bar picker, sessions,
   tiles, the large view, call events. Example: a shared paint activity.
4. **The phone.** The embedded Flutter module, the platform channel to the
   core, the React Native screens around it.

The example plugins live in the repository, are documented as samples,
and run in integration tests.

## 23. Testing

- **Sandbox**, each as a test that must fail to escape: `io`/`os`/`debug`
  access, `loadstring`, bytecode, `require` outside the plugin, an endless
  loop (stopped by the budget), a memory bomb (stopped by the cap), a zip
  with `..`, absolute paths, symlinks, case clashes, a zip bomb.
- **Internet**: every refused address range, including after a redirect
  and through DNS answers that change; allowed domains only.
- **Permissions**: every API call with and without its permission; an admin
  granting what they don't hold; the role's hierarchy and channel
  overwrites hiding events.
- **Signing**: tampered file, missing file, extra file, wrong key, rotation,
  unsigned over signed.
- **Off and on**: a channel's id, position, overwrites and plugin data
  identical after stop, start, update and reinstall.
- **Redraw**: the core's tree comparison against fixture trees (keys,
  moves, prop changes), and the renderer's widget and golden tests for
  every block in light and dark, at desktop and narrow widths.
- **Example plugins** as end-to-end tests on a real server.
- **Phone**: real-device checks of keyboard, focus and back gestures
  (human, owed per milestone).

## 24. Left to the builder

- Exact protobuf messages and field numbers, permission bit numbers, error
  codes, crate and module layout, and the SQLite schema for plugin
  records.
- Prop names beyond those shown, icon names, curves.
- How the core and renderer encode patches, as long as one encoding serves
  `flutter_rust_bridge` and the phone's platform channel.
- Fine-tuning the numbers in §21 after measuring.

## 25. Claude's calls (for the owner to check)

1. Luau through `mlua`, on both sides.
2. One SQLite file per plugin, plain SQL.
3. A managed role per plugin for hierarchy and channel overwrites (§8.2).
4. The admin-must-hold mapping for each plugin permission (§8.1).
5. `START_ACTIVITIES` on for @everyone (§8.5).
6. The previous version kept until the update starts, for rollback (§9.2).
7. GitHub verification through `.opencord/plugin.pub` (§9.4).
8. Plugin messages authored by the plugin, with a badge, not a member
   (§10.6).
9. Restart and turn-off thresholds (§10.1) and every number in §21.
10. Hot reload keeps client state, with an optional `migrate` (§11.6).
11. `image_picker` and `file_picker` behind `accept_uploads` (§13.3).
12. Several activities per call; a session ends when the call empties
    (§15.2, §15.4).
13. `os.time`, `os.clock`, `os.date` and JSON helpers available (§10.2).
14. The responsive context, `app.on_back`, and modals turning into sheets at
    `compact` (§11.4, §12.5).
15. From the design review: no `reply_to`, `@everyone` or unread dots for
    plugins until Opencord has them for members (§10.3, §14.5).
16. Permissions are checked against the admin when granted, then belong to
    the plugin (§8.3).
17. Plugins play sounds and control music without being in the call
    (§10.9).
18. `Identify.features`, so older apps don't see what they can't show
    (§11.7).
19. Plugin files private by default, reached through per-view tokens
    (§13.1).
20. Plugin users behind plugin authors (§10.6), and plugin files on the
    server's disk (§10.5), both settled while planning.
