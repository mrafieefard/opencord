# Opencord Plugins

Opencord plugins extend community servers with bots, custom commands, moderation rules, web hooks, background tasks, and interactive features.

Plugins are written in [Luau](https://luau.org), a fast, embeddable scripting language derived from Lua 5.1 with gradual type checking and sandboxing.

---

## What a plugin is

A plugin is an archive or directory containing a manifest, scripts, and static assets. Each plugin has an isolated virtual machine, its own SQLite database, dedicated file storage, and a managed server role and bot user identity.

Plugins can:
- Listen to server events (messages, members, channels, voice, etc.)
- Register slash commands with autocomplete
- Send, edit, and moderate messages
- Expose incoming web hooks
- Schedule one-shot or recurring timers
- Store structured data in SQLite and binary files in dedicated storage
- Communicate with external services over HTTPS (with permission)

---

## The package structure

A plugin is distributed as a standard zip archive (maximum 50 MB, deflated without encryption or symlinks) with files arranged in three folders plus the root files:

```
manifest.lua          -- required manifest at root
SIGNATURE             -- optional signature file created by opencord-plugin pack
KEY-ROTATION          -- optional key rotation statement created by opencord-plugin rotate-key
server/               -- server Luau scripts (.lua or .luau)
client/               -- client Luau scripts (.lua or .luau, future milestones)
assets/               -- images (png, jpg, webp, gif), fonts (ttf, otf), sounds (ogg, mp3, wav)
```

No files other than `manifest.lua`, `SIGNATURE`, and `KEY-ROTATION` may be placed at the root of the package. Development files such as `opencord.d.luau`, `.luaurc`, and `.gitignore` are used for editor tooling and are automatically excluded when packing.

---

## The manifest (`manifest.lua`)

`manifest.lua` must return a table describing the plugin's metadata, permissions, configuration, and entry points.

```lua
return {
  id = "welcome-bot",             -- [a-z0-9-], 3–40 characters, unique per server
  name = "Welcome Bot",           -- 1–60 characters display name
  version = "1.0.0",              -- semantic version (semver)
  author = "Opencord",            -- 1–60 characters author name
  description = "Greets members, moderates banned words, and rolls dice.",
  github = "https://github.com/opencord/welcome-bot", -- optional
  license = "MIT",                -- SPDX id, "All rights reserved", or URL
  api = 1,                        -- plugin API version (currently 1)
  icon = "assets/icon.png",       -- optional icon under assets/

  public_key = "ed25519:…",       -- optional; added automatically by `opencord-plugin pack`

  permissions = {
    "read_members",
    "send_messages",
    "read_messages",
    "manage_messages",
    "register_commands",
  },
  domains = { "api.example.com" }, -- required only with "internet" permission

  config = {
    { key = "welcome_channel", label = "Welcome channel", kind = "channel", required = true },
    { key = "banned_words", label = "Banned words", kind = "text" },
  },

  server = { "server/main.luau" }, -- scripts executed sequentially at startup
  client = "client/main.luau",     -- entry point for client views
}
```

### Manifest fields

| Field | Type | Required | Description |
|---|---|---|---|
| `id` | `string` | Yes | 3–40 lowercase ASCII letters, digits, and hyphens. Must begin and end with an alphanumeric character. |
| `name` | `string` | Yes | 1–60 characters human-readable name. |
| `version` | `string` | Yes | Semantic version string (`MAJOR.MINOR.PATCH`). |
| `author` | `string` | Yes | 1–60 characters author name. |
| `description` | `string` | No | Short description (up to 280 characters). |
| `github` | `string` | No | URL of the form `https://github.com/<owner>/<repo>`. |
| `license` | `string` | Yes | SPDX identifier (e.g. `MIT`, `Apache-2.0`), `"All rights reserved"`, or a URL. |
| `api` | `number` | Yes | Plugin API version (`1`). |
| `icon` | `string` | No | Relative path to a `.png`, `.jpg`, `.jpeg`, or `.webp` file under `assets/`. |
| `public_key` | `string` | No | Public signing key (`ed25519:...`), added by `opencord-plugin pack`. |
| `permissions` | `{ string }` | No | Array of permission names requested by the plugin. |
| `domains` | `{ string }` | No | Array of allowed hostnames when requesting the `internet` permission. |
| `config` | `{ table }` | No | Configuration fields for server admins to provide. |
| `server` | `{ string }` | No | Array of script paths under `server/` to execute on start. |
| `client` | `string` | No | Client script path under `client/` (future milestone). |

### Config fields

Admin settings are defined under `config`:

- `key`: 1–40 lowercase characters, digits, and underscores.
- `label`: 1–60 characters.
- `kind`: one of `"text"`, `"secret"`, `"number"`, `"boolean"`, `"choice"`, `"role"`, `"channel"`, or `"user"`.
- `required`: boolean (default `false`).
- `choices`: array of 1–50 strings (only when `kind = "choice"`).

---

## Permissions

All sensitive operations require explicit permissions declared in the manifest and granted by an administrator during review.

| Permission | Description |
|---|---|
| `read_messages` | Read message history and receive message creation/edit/delete events. |
| `send_messages` | Send and edit messages in channels where the plugin's role has permission. |
| `manage_messages` | Delete messages sent by other members. |
| `attach_files` | Send attachments, images, and files with messages. |
| `read_channels` | Read channel lists and details. |
| `manage_channels` | Create, update, reorder, and delete channels. |
| `create_channel_types` | Create custom plugin channels. |
| `register_activities` | Register activities that run in voice calls. |
| `register_commands` | Register slash commands and handle autocomplete. |
| `read_members` | Read the server member list and receive member join/leave/update events. |
| `read_roles` | Read role names, colors, and permissions. |
| `manage_roles` | Create, update, and manage roles ranked below the plugin's role. |
| `manage_nicknames` | Change member nicknames. |
| `kick_members` | Kick members ranked below the plugin's managed role. |
| `ban_members` | Ban and unban members ranked below the plugin's managed role. |
| `read_invites` | List server invite links. |
| `create_invites` | Create server invite links. |
| `read_server` | Read server name, description, and settings. |
| `read_voice` | View voice states in channels. |
| `move_members` | Move or disconnect members in voice channels. |
| `mute_members` | Server-mute members in voice channels. |
| `deafen_members` | Server-deafen members in voice channels. |
| `play_soundboard` | Play soundboard sounds into voice channels. |
| `read_music` | Read current music playback and queue state. |
| `control_music` | Add tracks, skip, pause, and resume music. |
| `internet` | Make outbound HTTPS requests to approved domains. |
| `receive_web_requests` | Expose incoming HTTP web hooks under `/plugins/{id}/hooks`. |
| `accept_uploads` | Accept files through interactive blocks. |
| `talk_to_plugins` | Emit and listen to inter-plugin bus events. |

### Member Permissions

Starting voice activities requires the server member permission:
- `START_ACTIVITIES` (bit 33): start an activity in a voice call one is in. On for `@everyone` by default; can be overridden per channel with permission overwrites.

---

## Server API and Events

All API functions and type signatures are defined in `crates/opencord-plugins/api/opencord.d.luau`.

### Core Globals

- `opencord`:
  - `opencord.id: string`
  - `opencord.version: string`
  - `opencord.on(event: string, handler: function)`
  - `opencord.off(event: string, handler: function)`
  - `opencord.has_permission(permission: string): boolean`
- `log`:
  - `log.info(...)`, `log.warn(...)`, `log.error(...)`
- `messages`:
  - `messages.send(channel_id, content | { content })` *(file attachments arrive in a later milestone)*
  - `messages.edit(message_id, content)`
  - `messages.delete(message_id)`
  - `messages.fetch(channel_id, { before, limit })`
- `typing`:
  - `typing.start(channel_id)`
- `commands`:
  - `commands.register({ name, description, options }, function(cmd) ... end)`: the function runs when the command does. Registering a name again replaces it.
  - An option's `autocomplete = function(ac) ... end` suggests values with `ac:suggest({ { name, value }, ... })`, within 1 second; later it returns `false`.
  - `cmd.name`, `cmd.options`, `cmd.user`, `cmd.channel_id`.
  - `cmd:reply(content, { only_you? })` answers within 3 seconds. Later it raises `TIMEOUT: too late to reply` and writes a warning to the plugin's log.
  - `cmd:defer({ only_you? })` shows "is thinking…" and gives 15 minutes to reply. It decides where the reply goes.
  - `commands.unregister(name)`
  - Two plugins may use one name, and a plugin may use a built-in's: people pick from the list. A command's function that raises tells the member "{plugin name} ran into an error" at once.
- `channels`:
  - `channels.list()`, `channels.get(id)`, `channels.create(spec)`, `channels.update(id, spec)`, `channels.delete(id)`, `channels.reorder(order)`, `channels.set_overwrite(channel_id, spec)`
- `roles`:
  - `roles.list()`, `roles.create(spec)`, `roles.update(id, spec)`, `roles.delete(id)`, `roles.reorder(order)`
- `members`:
  - `members.list()`, `members.get(id)`, `members.add_role(user_id, role_id)`, `members.remove_role(user_id, role_id)`, `members.set_nickname(user_id, nickname)`, `members.kick(user_id, reason)`, `members.ban(user_id, reason)`, `members.unban(user_id)`
- `server`:
  - `server.get()`
- `bans`:
  - `bans.list()`
- `invites`:
  - `invites.list()`, `invites.create(spec)`
- `voice`:
  - `voice.states(channel_id?)`, `voice.move(user_id, channel_id)`, `voice.disconnect(user_id)`, `voice.mute(user_id, mute)`, `voice.deafen(user_id, deafen)`
- `soundboard`:
  - `soundboard.play(channel_id, sound_id)`
- `music`:
  - `music.state(channel_id)`, `music.add(channel_id, query)`, `music.skip(channel_id)`, `music.pause(channel_id)`, `music.resume(channel_id)`
- `db`:
  - `db.exec(sql, ...)`: executes DDL/DML, returns `{ changes: number, last_id: string }`
  - `db.query(sql, ...)`: returns `{ { [column]: value } }`
  - `db.transaction(function(tx) ... end)`
- `files`:
  - `files.put(path, data, { public = false })`
  - `files.get(path): buffer?`
  - `files.delete(path)`
  - `files.list(prefix?): { FileInfo }`
- `http`:
  - `http.request({ url, method, headers, body, timeout }): HttpResponse` (requires `internet`)
- `hooks`:
  - `hooks.on(path, function(request) return { status = 200, body = "ok" } end)` (requires `receive_web_requests`)
- `timer`:
  - `timer.after(seconds, fn): number`
  - `timer.every(seconds, fn): number`
  - `timer.cron(expression, fn): number`
  - `timer.cancel(timer_id)`
- `plugins`:
  - `plugins.emit(name, data)`
  - `plugins.on(plugin_id, name, handler)`
- `config`:
  - `config.get(key): any`

### IDs as Strings and Errors

All Opencord IDs (user IDs, channel IDs, message IDs, role IDs) are represented as decimal strings in Luau (e.g. `"14242352352352"`). Luau numbers are 64-bit IEEE-754 floats which cannot represent 64-bit integers without precision loss. Passing a number where an ID is expected produces an error.

API errors throw Luau runtime errors. Use `pcall(fn)` to catch errors where a failure is expected or recoverable.

---

## Storage, Files, Network, Web Hooks, and Timers

### SQLite Database
Each plugin has a dedicated SQLite database located in `data/plugins/{id}/db.sqlite`. The database runs in WAL mode with a 1 GB quota. Tables and indices are managed directly by your script using `db.exec`.

### File Storage
Binary files are stored in `data/plugins/{id}/files/` (up to 5 GB total quota). Paths must be relative and cannot escape the folder. Files with `public = true` can be served to clients via media URLs.

### Outbound Network
With the `internet` permission, plugins can perform outbound HTTPS requests via `http.request`. The target host must be explicitly listed in the manifest `domains` table. IP literals, internal private networks, and localhost are strictly blocked to prevent Server-Side Request Forgery (SSRF).

### Incoming Web Hooks
With `receive_web_requests`, plugins can listen for HTTP requests under `/plugins/{id}/hooks` and `/plugins/{id}/hooks/{path}`. Request bodies are limited to 1 MB. Framing headers (`Content-Length`, `Transfer-Encoding`, `Connection`) and cookies are automatically managed and sanitized by the server.

### Timers
Timers run asynchronously within the plugin's VM:
- `timer.after(secs, fn)`: runs once after delay (up to 365 days).
- `timer.every(secs, fn)`: runs periodically (interval at least 1 second).
- `timer.cron(expr, fn)`: runs according to standard 5-field cron syntax (`minute hour day month day-of-week`).

All timers automatically stop when the plugin is stopped or uninstalled.

---

## Developer Tooling (`opencord-plugin`)

The `opencord-plugin` command-line tool assists in creating, testing, packaging, signing, and updating plugins.

### 1. Initialize a starter plugin
```bash
opencord-plugin init my-bot
```
Creates a new directory `my-bot/` with `manifest.lua`, `server/main.luau`, `opencord.d.luau`, `.luaurc`, and `.gitignore`.

### 2. Generate a signing key
```bash
opencord-plugin keygen [--out <path>]
```
Generates an Ed25519 signing key (defaulting to `~/.config/opencord-plugin/signing.key`). Prints the public key and fingerprint. The private key remains secure on your machine and is never shared or included in packages.

### 3. Pack and sign a package
```bash
opencord-plugin pack [<folder>] --key <path-to-key>
```
Embeds the public key into `manifest.lua`, computes SHA-256 digests over all files, signs the canonical listing, and outputs `<id>-<version>.zip`.

To pack without signing:
```bash
opencord-plugin pack [<folder>]
```

### 4. Key rotation
```bash
opencord-plugin rotate-key --old <old-key> --new <new-key> [<folder>]
```
Creates a `KEY-ROTATION` statement signed by the old key proving ownership transfer to the new key. When packed with `--key <new-key>`, servers pinned to the old key will verify the rotation and accept updates signed by the new key.

### 5. Live Development Mode (`dev`)
```bash
opencord-plugin dev --server <host:port> --token <ocdev_...> [--fingerprint <hex>] [<folder>]
```
Watches the plugin directory for file changes (polling every 300 ms). On start and on every save, it creates an unsigned package and pushes it directly to the server. Pushes asking for new permissions wait for admin approval on the Plugins page; pushes with existing permissions hot-reload immediately without losing stored data. It connects to the live log stream and outputs timestamped logs in real time, automatically reconnecting if the server restarts.

---

## Client Scripts and Custom Channels

Plugins can register custom channel types and provide interactive user interfaces using client-side scripts. Client scripts run on member devices inside Opencord's core (`opencord-core`), executing in dedicated Luau virtual machines sandboxed with strict memory and CPU limits.

### 1. The Redraw Model

- **One VM per View:** Opening a plugin channel starts an isolated Luau VM for that view.
- **Initialization:** The script defines `init(ctx)`, which receives the view context (`ctx.viewer`, `ctx.place`, `ctx.size`, `ctx.theme`, `ctx.platform`) and returns the view's initial state table.
- **Rendering:** `render(state)` returns a declarative tree of blocks created with `ui.<block>(props, children?)`. The core reconciles the returned tree against the previous tree and emits compact diff batches to the host renderer.
- **State Updates:** Calling `app.set(function(state) ... end)` modifies the state and marks the view dirty. The core re-renders the view after the current turn. Calling `app.set` directly inside `render` is forbidden.
- **Hot Reloading:** In live development mode, updated client scripts preserve state across reloads. If the script defines `migrate(old_state)`, its return value becomes the new state; otherwise, the existing state is preserved as-is. Note that state must consist of plain JSON-compatible data (booleans, numbers, strings, tables)—functions and userdata cannot cross reload boundaries.

### 2. Client API (`app`)

Client scripts access view state, server messaging, device storage, timers, and dialogs via the global `app` table:

#### Methods
- `app.set(fn)`: updates the view's state table and queues a re-render.
- `app.send(name, data)`: sends a view event (up to 64 KiB JSON) to the plugin's server script, delivered to `ui.on(name, handler)`.
- `app.on(name, handler)`: registers a handler for events sent by the server via `view:send(name, data)`.
- `app.listen(event_name, handler)`: listens for view lifecycle and input events (`"close"`, `"pause"`, `"resume"`, `"resize"`, `"theme_change"`, `"connection_change"`, `"focus"`, `"blur"`, `"key_down"`, `"key_up"`).
- `app.copy(text)`: copies text to the system clipboard (allowed only during press handlers).
- `app.on_back(handler)`: on the phone, the system back button or gesture first closes the plugin's top modal, sheet or dialog; with none open it calls `handler`. Return `true` when the plugin went back itself (a detail screen back to its list); anything else, an error, or no answer within half a second leaves the plugin, so `handler` must answer at once: it can't wait on `app.confirm` or another dialog. To ask before leaving, open your own modal and return `true`. The desktop never calls it.
- `app.open_link(url)`: prompts the user to open an external HTTP or HTTPS URL.
- `app.open_channel(channel_id)`: navigates to a channel.
- `app.play_sound(asset, options?)`: plays a local audio asset from `assets/` with optional volume and `on_end` callback.
- `app.confirm(text)`: displays a confirmation dialog and pauses the handler coroutine until answered (`true` or `false`).
- `app.alert(text)`: displays an alert dialog and pauses until dismissed.
- `app.prompt(text, default?)`: prompts for text input and returns the typed string or `nil`.
- `app.after(seconds, handler)`: schedules a one-shot timer (up to 100 active timers per view).
- `app.every(seconds, handler)`: schedules a repeating timer (minimum 100 ms interval).
- `app.cancel(timer_id)`: cancels an active timer.
- `app.on_frame(handler)`: runs a handler on each animation frame (`dt` in seconds) within an 8 ms budget; pauses when hidden.
- `app.off_frame()`: stops the frame loop.
- `app.modal.open(name, props?)`: opens a modal registered in `app.modals` and returns a handle with `:close(result?)` and `:update(props)`.
- `app.modal.ask(name, props?)`: opens a modal and pauses until the modal closes with a result.
- `app.modal.close(name_or_result?)`: closes the named modal or the topmost modal.
- `app.modal.close_all()`: closes all open modals.
- `app.modal.is_open(name)`: returns whether a modal is currently open.
- `app.modals`: table of modal rendering functions `app.modals[name] = function(state, props) ... end`.
- `app.storage.get(key)`: retrieves plain data from device storage.
- `app.storage.set(key, value)`: stores plain data in device storage (up to 1 MB per plugin).
- `app.storage.delete(key)`: removes a key from device storage.

#### Context Fields
- `app.state`: read-only access to the current state table.
- `app.viewer`: `{ id: string, name: string, roles: { string } }`.
- `app.place`: `{ kind: "channel", channel_id: string }`.
- `app.platform`: `"desktop"` or `"phone"`; `init`'s `ctx.platform` and the server's `view.platform` say the same.
- `app.size`: `{ width: number, height: number }`.
- `app.safe_area`: `{ top: number, right: number, bottom: number, left: number }`.
- `app.theme`: `"dark"` or `"light"`.
- `app.breakpoint`: `"compact"` (< 600px), `"medium"` (600–840px), or `"expanded"` (> 840px).
- `app.input`: `"mouse"` or `"touch"`.
- `app.text_scale`: system font scaling factor.

### 3. Block Catalog

Blocks are organized into six functional groups:

#### Layout
| Block | Props | Events | Description |
|---|---|---|---|
| `row` | `gap`, `align`, `cross`, `wrap` | — | Flex row container |
| `column` | `gap`, `align`, `cross`, `wrap` | — | Flex column container |
| `stack` | — | — | Layered container; children position with `x`, `y`, `right`, `bottom` |
| `grid` | `columns`, `min_column_width`, `gap` | — | Multi-column grid |
| `scroll` | `direction` (`"vertical"` / `"horizontal"`) | `on_scroll`, `on_reach_start`, `on_reach_end` | Scrollable viewport |
| `box` | — | `on_press`, `on_double_press`, `on_long_press`, `on_secondary_press`, `on_hover_enter`, `on_hover_exit` | Tappable styled container |
| `spacer` | `size` | — | Flexible or fixed spacing |
| `divider` | `color`, `thickness` | — | Horizontal or vertical separator |

Containers (`row`, `column`, `stack`, `grid`, `scroll`, `box`, `list`) accept `background`, `border`, `radius`, and `shadow`. All blocks accept common layout props (`width`, `height`, `min_width`, `max_width`, `min_height`, `max_height`, `flex`, `padding`, `opacity`, `visible`, `tooltip`, `label`, `animate`, `enter`, `exit`, `key`) and `on_animation_end`.

#### Lists
| Block | Props | Events | Description |
|---|---|---|---|
| `list` | `count`, `reverse`, `gap`, `item_height`, `item(index)` | `on_scroll`, `on_reach_start`, `on_reach_end` | Virtualized lazy list |

#### Content
| Block | Props | Events | Description |
|---|---|---|---|
| `text` | `text`, `size`, `weight`, `italic`, `color`, `align`, `max_lines`, `selectable`, `font` | — | Formatted text |
| `rich_text` | `text` | — | Markdown text with emoji, mentions, links |
| `image` | `asset`, `file`, `file_token`, `url`, `attachment`, `bytes`, `fit`, `radius`, `placeholder`, `version` | `on_load`, `on_error` | Image renderer |
| `icon` | `name`, `size`, `color` | — | System icon |
| `user` | `id`, `size`, `show_name` | — | User avatar and display name |
| `badge` | `text`, `tone` (`"neutral"`, `"accent"`, `"danger"`) | — | Status badge |
| `progress` | `value` (0.0 to 1.0), `style` (`"bar"`, `"ring"`) | — | Progress indicator |
| `spinner` | `size` | — | Activity spinner |

#### Controls
| Block | Props | Events | Description |
|---|---|---|---|
| `button` | `text`, `variant` (`"primary"`, `"secondary"`, `"danger"`, `"plain"`), `icon`, `loading`, `disabled` | `on_press`, `on_long_press` | Push button |
| `icon_button` | `icon`, `disabled` | `on_press` | Icon-only button |
| `checkbox` | `value`, `text`, `disabled` | `on_change` | Checkbox toggle |
| `switch` | `value`, `text`, `disabled` | `on_change` | Toggle switch |
| `radio` | `value`, `text`, `disabled` | `on_change` | Radio option |
| `slider` | `value`, `min`, `max`, `step`, `disabled` | `on_change`, `on_change_end` | Range slider |
| `select` | `value`, `options`, `placeholder`, `disabled` | `on_change` | Dropdown picker |
| `tabs` | `value`, `options` | `on_change` | Tab bar |
| `segmented` | `value`, `options` | `on_change` | Segmented control |
| `text_input` | `value`, `placeholder`, `password`, `max_length`, `lines`, `disabled` | `on_change`, `on_submit`, `on_focus`, `on_blur` | Text input field |
| `image_picker` | `text` | `on_upload`, `on_upload_error` | Image upload button |
| `file_picker` | `text`, `accept` | `on_upload`, `on_upload_error` | File upload button |

#### Overlays
| Block | Props | Events | Description |
|---|---|---|---|
| `modal` | `open`, `size` (`"small"`, `"medium"`, `"large"`, `"full"`), `sheet`, `dismissible`, `title` | `on_dismiss` | Modal dialog within plugin area |
| `sheet` | `open`, `dismissible` | `on_dismiss` | Bottom sheet within plugin area |
| `menu` | `items` | `on_select` | Context menu |
| `tooltip` | `text` | — | Hover tooltip |
| `toast` | `text`, `open`, `duration` | `on_dismiss` | Floating notification |

#### Drawing
| Block | Props | Events | Description |
|---|---|---|---|
| `canvas` | `items`, `ink` | `on_pointer_down`, `on_pointer_move`, `on_pointer_up`, `on_pointer_cancel`, `on_wheel`, `on_pan`, `on_scale`, `on_stroke` | Retained vector canvas with freehand ink |

#### Event Data Shapes

Event handlers receive a single data table matching the event's wire shape:

| Event | Data shape | Description |
|---|---|---|
| `on_change` | `{ value: T }` | Value of input/control (`boolean` for checkbox/switch/radio, `number` for slider, `string` for text_input, `any` for select/tabs/segmented) |
| `on_change_end` | `{ value: number }` | Slider final value when drag completes |
| `on_submit` | `{ value: string }` | Text input submit / Enter pressed |
| `on_scroll` | `{ offset: number, extent: number }` | Scroll position and maximum scroll extent in pixels |
| `on_reach_start` | `{}` | Reached beginning of scroll viewport |
| `on_reach_end` | `{}` | Reached end of scroll viewport |
| `on_press` | `{}` | Tap or click |
| `on_double_press` | `{}` | Double tap or double click |
| `on_long_press` | `{}` | Long press |
| `on_secondary_press` | `{ x: number, y: number }` | Secondary click / right click coordinates |
| `on_hover_enter` | `{}` | Pointer entered bounding box |
| `on_hover_exit` | `{}` | Pointer exited bounding box |
| `on_load` | `{ width: number, height: number }` | Image loaded with dimensions |
| `on_error` | `{ message: string }` | Image failed to load |
| `on_upload` | `{ name: string, size: number }` | File picked and uploaded to server |
| `on_upload_error` | `{ message: string }` | File upload failed with error reason |
| `on_select` | `{ value: any }` | Menu item selected |
| `on_dismiss` | `{}` | Modal, sheet, or toast dismissed |
| `on_pointer_down` | `{ x, y, pointer, kind, buttons, pressure? }` | Canvas pointer down |
| `on_pointer_move` | `{ x, y, pointer, kind, buttons, pressure? }` | Canvas pointer move |
| `on_pointer_up` | `{ x, y, pointer, kind, buttons, pressure? }` | Canvas pointer up |
| `on_pointer_cancel` | `{ x, y, pointer, kind, buttons, pressure? }` | Canvas pointer cancel |
| `on_wheel` | `{ x, y, dx, dy }` | Canvas mouse wheel / touchpad scroll |
| `on_pan` | `{ x, y, dx, dy, phase }` | Canvas pan gesture (`"start"`, `"update"`, `"end"`) |
| `on_scale` | `{ x, y, scale, phase }` | Canvas pinch/scale gesture (`"start"`, `"update"`, `"end"`) |
| `on_stroke` | `{ stroke, points, done }` | Freehand ink stroke batch with coordinates `[x, y, pressure, ...]` |
| `on_animation_end` | `{}` | Entry, exit, or layout animation finished |

Example reading `data.value`:
```luau
ui.text_input({
    value = state.title,
    placeholder = "Ticket title",
    on_change = function(data)
        app.set(function(s)
            s.title = data.value
        end)
    end,
})
```

### 4. Images and Uploads

The `image` block supports five distinct sources:
1. `asset = "assets/logo.png"`: bundled static asset.
2. `file = "avatars/1.png"` or `file_token = token`: file stored in the plugin's dedicated files quota (optionally loaded with an authenticated token generated via server `files.token(path, view)`).
3. `url = "https://..."`: external web image fetched through the server proxy.
4. `attachment = id`: Opencord channel attachment image, fetched by its attachment ID.
5. `bytes = data`: inline image raw bytes (base64 or buffer, capped at 256 KB).

#### Uploads Round Trip

When a user selects a file through `image_picker` or `file_picker`:
1. The client app displays the file selector and uploads the selected file to the Opencord server.
2. The client script receives the picker's `on_upload` event with `{ name = string, size = number }`.
3. The server plugin receives the `file_uploaded` event via `opencord.on("file_uploaded", function(event) ... end)` with `event.view` (`{ id, place, user }`) and `event.file` (`{ id, name, size, content_type }`). Handling uploads requires the `accept_uploads` permission in the plugin manifest.
4. The server script persists the file into plugin storage using `files.put(path, event.file)`.

```lua
opencord.on("file_uploaded", function(event)
    -- Requires 'accept_uploads' permission in manifest.lua
    local dest_path = "uploads/" .. event.file.name
    files.put(dest_path, event.file)
    log.info("Saved upload " .. event.file.name .. " to " .. dest_path)
end)
```

### 5. Available Icons

The `icon` block and button controls support 43 standard icon names:
`add`, `arrow_back`, `arrow_forward`, `arrow_up`, `arrow_down`, `check`, `close`, `delete`, `edit`, `search`, `settings`, `info`, `warning`, `error`, `star`, `person`, `group`, `chat`, `send`, `image`, `link`, `lock`, `lock_open`, `refresh`, `play`, `pause`, `stop`, `music`, `mic`, `volume`, `schedule`, `history`, `pin`, `tag`, `copy`, `download`, `upload`, `more`, `menu`, `visibility`, `visibility_off`, `palette`, `extension`.

---

### 6. On the phone

The phone apps draw plugin channels and activities with the same renderer and the same blocks as the desktop, from the same client script. What differs:

- `app.platform` is `"phone"`. `app.input` is `"touch"`, and a phone's area is usually narrower than 600 px, so `app.breakpoint` is `"compact"`. Lay out for it.
- A modal is a sheet from the bottom at compact width, unless it sets `sheet = false`.
- Nothing hovers: `on_hover_enter` and `on_hover_exit` never fire, and a `tooltip` doesn't show.
- Controls get a touch target at least 44 points high and wide around what they draw. A small button looks the same but is easier to hit.
- The system back closes your top dialog, modal or sheet first (a modal that isn't dismissible keeps it), then calls `app.on_back`.
- An opened activity takes the call's whole stage, with the people in a strip above it.
- `rich_text` shows code blocks in a monospace box, without syntax colours.
- `app.on("pause")` is heard when the screen goes out of sight: another channel, the call sheet closed, the app in the background. `resume` follows when it's back. The script keeps its state in between.

## Activities

An activity is an interactive, collaborative application that members run together inside an active voice call (such as a shared whiteboard, a game, or collaborative notes).

### 1. Starting an Activity

Any member connected to a voice channel who holds the `START_ACTIVITIES` permission (member permission bit 33, enabled for `@everyone` by default, or configured via channel permission overwrites) can start an activity from the call bar's Activities button. Starting an activity launches a session in that call; all participants in the voice call see its tile on the call's stage and can open its interactive view. Up to 4 activity sessions can run simultaneously in one call.

### 2. Registering Activities

Server scripts register activities during initialization:

```luau
ui.register_activity(key: string, spec: ActivitySpec)
```

- `key`: 1 to 40 characters (`[a-z0-9_]`), unique per plugin.
- `spec: ActivitySpec = { name: string, description: string?, icon: string? }`:
  - `name`: 1 to 60 characters display name shown in the picker and on the stage tile.
  - `description`: optional short description (up to 200 characters) shown in the picker.
  - `icon`: optional path to an image under `assets/` (e.g. `assets/paint.png`).

Registering activities requires the `register_activities` permission in `manifest.lua`. Each plugin may register up to 20 activities.

### 3. Activity Lifecycle Events

Server scripts handle activity sessions by listening for lifecycle events:

```luau
opencord.on("activity_start", function(event: ActivityStartEvent)
    -- event.session: ActivitySession
    -- event.starter: User
    -- event.call: { channel_id: string, member_ids: { string } }
end)

opencord.on("activity_end", function(event: ActivityEndEvent)
    -- event.session: ActivitySession
    -- event.reason: "ended" | "plugin" | "call_empty" | "plugin_stopping" | "unavailable"
end)
```

- `ActivitySession`:
  - `id: string`: unique decimal session ID.
  - `activity: string`: the activity key.
  - `channel_id: string`: ID of the voice channel where the session runs.
  - `starter_id: string`: user ID of the member who started it.
  - `started_at_ms: number`: creation timestamp in milliseconds.
- `ActivityStartEvent`:
  - `session: ActivitySession`: the newly started session.
  - `starter: User`: user who initiated the activity (`id`, `name`, `bot`).
  - `call: { channel_id: string, member_ids: { string } }`: voice channel ID and sorted user IDs currently in the call.
- `ActivityEndEvent`:
  - `session: ActivitySession`: the session that ended.
  - `reason: string`: one of:
    - `"ended"`: ended by the session starter or a channel manager (`manage_channels`).
    - `"plugin"`: ended by the plugin via `activity.finish`.
    - `"call_empty"`: all members left the voice call.
    - `"plugin_stopping"`: the plugin was stopped, turned off, uninstalled, or failed to start.
    - `"unavailable"`: the plugin restarted or updated without registering this activity again, or the activity became unavailable.

### 4. Ending an Activity from Script

A plugin can end its own running activity session at any time:

```luau
activity.finish(session: ActivitySession | string) -> boolean
```

Requires the `register_activities` manifest permission. Takes an `ActivitySession` table or its string `id`. Returns `true` if the session was active and was ended, or `false` otherwise. The end is broadcast to callers and dispatches `activity_end` with reason `"plugin"`.

### 5. Activity View Places and Messaging

When a member clicks **Open** on an activity tile, a view opens with an activity place:

```luau
-- ViewPlace:
{
    kind = "activity",
    session_id = "<session_id>",
    channel_id = "<voice_channel_id>",
    activity = "<activity_key>",
}
```

Server scripts query and broadcast to activity views using `ViewPlaceSelector` (`{ session_id: string } | { channel_id: string } | ViewPlace`):
- `ui.views(place?: ViewPlaceSelector): { View }`: passing `{ session_id = session.id }` returns all open views for that session.
- `ui.send_place(place: ViewPlaceSelector, name: string, data: any): number`: passing `{ session_id = session.id }` broadcasts the event `name` with `data` payload to all members viewing that activity session.

### 6. Client API: Call State and Presence

Client scripts running in an activity view access call participant and speaker information through `app.call`:

```luau
app.call: ViewCall?
-- {
--     channel_id: string,
--     members: { string },
--     speaking: { string },
-- }
```

In an activity view, `app.call` provides:
- `channel_id`: the voice channel ID.
- `members`: sorted decimal string user IDs of everyone in the call.
- `speaking`: sorted decimal string user IDs of members currently speaking.
(In a custom channel view, `app.call` is `nil`.)

Client scripts listen for call changes with `app.listen`:
- `app.listen("call_members_change", function(change: CallMembersChange) ... end)`:
  - `change.members`: full sorted list of current participant IDs.
  - `change.joined`: list of IDs who joined the call since the last update.
  - `change.left`: list of IDs who left the call since the last update.
- `app.listen("speaking_change", function(change: SpeakingChange) ... end)`:
  - `change.speaking`: full sorted list of participant IDs currently speaking. Throttled to at most once per 100 ms.

### 7. When a Session Ends

An activity session ends when:
1. The starter or a member with `manage_channels` in the voice channel chooses **End activity** from the tile's menu (`"ended"`).
2. The plugin server script calls `activity.finish(session)` (`"plugin"`).
3. The last participant disconnects from the voice channel (`"call_empty"`).
4. The plugin is stopped, turned off, uninstalled, or fails to start (`"plugin_stopping"`). A restart or update that registers the activity again keeps the session.
5. The plugin restarts or updates without registering the activity again (`"unavailable"`).

When ended, the activity's tile is removed from the stage, any open views receive a `closed` event with reason `"activity_ended"`, and `activity_end` fires on the server.

### 8. View Pausing and Resuming

An open activity view is paused (`app.listen("pause", ...)` and lifecycle event `pause`) whenever its area is not visible to the user:
- The activity tile is collapsed into the stage's side strip because another tile is focused.
- The user navigates to another channel, server, or settings screen.
- The desktop window is minimized or hidden.

When the view returns to the screen, it is resumed (`app.listen("resume", ...)` and lifecycle event `resume`). The view's memory, state, and timers are preserved across pauses.

### 9. Activity Limits

| Limit | Value | Description |
|---|---|---|
| Activities per plugin | 20 | Maximum activities registered per plugin (`ACTIVITIES_PER_PLUGIN`) |
| Activity sessions per call | 4 | Maximum active sessions in a single voice call (`ACTIVITY_SESSIONS_PER_CALL`) |
| Call input IDs | 1,000 | Maximum IDs in members/speaking lists (`CALL_INPUT_MAX_IDS`) |
| Activity key | 1–40 chars | ASCII letters, numbers, and underscores (`[a-z0-9_]`) |
| Activity name | 1–60 chars | Display name in picker and stage tile |
| Activity description | Up to 200 chars | Optional description in picker |

### 10. Example: Shared Paint Activity

A complete shared whiteboard activity is located in `examples/plugins/paint/`:
- `examples/plugins/paint/manifest.lua`: requests the `register_activities` permission.
- `examples/plugins/paint/server/main.luau`: registers the `paint` activity with `ui.register_activity`, receives drawing strokes via `ui.on("stroke", ...)`, validates point coordinates and color choices, stores up to 300 strokes per session in memory, sends canvas history to new viewers in batches of 30 strokes, and broadcasts strokes with `ui.send_place({ session_id = session.id }, ...)`.
- `examples/plugins/paint/client/main.luau`: renders an interactive 1600 × 1000 canvas with color palette buttons, stroke width selectors, a Clear button, touch/mouse path drawing, stroke thinning, and participant counters using `app.call.members` and `app.listen("call_members_change", ...)`.

---

## Runtime Limits and Quotas

| Resource | Limit |
|---|---|
| Unpacked zip files | Max 2,000 files, max 200 MB total |
| Single package file | Max 50 MB |
| Luau VM memory | 128 MB (server), 32 MB (client) |
| CPU execution budget | 100 ms per event handler (~100,000,000 instructions) |
| SQLite database | 1 GB per plugin |
| File storage | 5 GB per plugin |
| Web hook payload | Max 1 MB request body |
| HTTP request timeout | Max 10 s (`timeout` option) |
| CPU share | 25 % of one core, averaged over 10 seconds; past it, events wait in the queue. Plugin threads run at a lower priority. |
| Registered commands | Max 100 commands per plugin |
| Commands thinking at once | Max 100 per plugin |
| Active timers | Max 1,000 active timers per plugin (server), max 100 active timers per view (client) |
| Log history | Kept up to 5,000 lines in memory and disk per plugin |
| Client handler CPU time | 50 ms per handler |
| Frame budget (`on_frame`) | 8 ms per frame |
| Tree size | Max 20,000 blocks and max 64 deep per view |
| Tree props total | Max 16 MiB per tree |
| String props | Max 10,000 characters per string prop (`text` and `rich_text` text max 20,000) |
| Canvas items | Max 20,000 items, max 10,000 points per path |
| List range | Max 200 items requested per lazy list range |
| Open views | Max 4 views open per session |
| View bandwidth | Max 512 KiB/s to view, max 128 KiB/s from view |
| View events rate | Max 60 to view / max 30 from view per second |
| External image fetch | Max 10 MiB, max 40 megapixels |
| File tokens | Good for 1 hour |
| Custom channel types | Max 20 channel types per plugin |
| Activities per plugin | Max 20 activities per plugin |
| Activity sessions per call | Max 4 activity sessions running in one call |
| Call input IDs | Max 1,000 member / speaker IDs in call input |
| Dialog text | Max 2,000 characters for confirm/alert/prompt |
| Error reports | At most 1 per view per 10 seconds, max 2,000 characters |
| Raw image bytes | Max 256 KB |
| Device storage | 1 MB per plugin |

---

## Example Plugins

Three complete, real-world examples are included in the repository under `examples/plugins/`:

- **Welcome Bot** (`examples/plugins/welcome-bot/`): A server bot demonstrating slash commands, event handlers (member joins, message moderation), background statistics tracking with SQLite, and configuration options.
- **Support Tickets** (`examples/plugins/tickets/`): A custom channel type and interactive view demonstrating custom channels (`ui.register_channel_type`), interactive UI with declarative client blocks, named modals (`app.modals.new_ticket`), responsive layouts (`app.breakpoint`), SQLite persistence for tickets and replies, and role-based permissions (`ui.send_role`).
- **Shared Paint** (`examples/plugins/paint/`): A collaborative whiteboard activity for voice calls demonstrating `ui.register_activity`, activity sessions, `ui.send_place` by session, canvas block rendering, touch/mouse drawing synchronization across callers, and live call participant tracking with `app.call` and `call_members_change`.

---

## Protecting Your Work

Code executed on Opencord cannot be hidden from the machines running it: server scripts run on the server administrator's hardware, and client scripts run on viewers' devices. Opencord does not support bytecode obfuscation or encryption.

To protect intellectual property and commercial offerings:
1. **License & Terms:** State your terms clearly in the `license` field. Administrators must read and accept this license when installing the plugin.
2. **License Keys & Secrets:** Use `kind = "secret"` in your manifest configuration. Secrets are stored securely and never broadcast to client apps.
3. **External Services:** Keep core business logic or proprietary algorithms on your own external backend service. Use the `internet` permission to query your service with authentication tokens supplied through secret configuration fields.
