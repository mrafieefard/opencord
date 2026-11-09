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
| `register_activities` | Register voice and channel activities (milestone 2). |
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
  - `http.request({ url, method, headers, body, timeout_s }): HttpResponse` (requires `internet`)
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
Each plugin has a dedicated SQLite database located in `data/plugins/{id}/db.sqlite`. The database runs in WAL mode with a 100 MB quota. Tables and indices are managed directly by your script using `db.exec`.

### File Storage
Binary files are stored in `data/plugins/{id}/files/` (up to 500 MB total quota). Paths must be relative and cannot escape the folder. Files with `public = true` can be served to clients via media URLs.

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

## Runtime Limits and Quotas

| Resource | Limit |
|---|---|
| Unpacked zip files | Max 10,000 files, max 200 MB total |
| Single package file | Max 50 MB |
| Luau VM memory | 128 MB per plugin |
| CPU execution budget | 100,000,000 VM instructions per event handler (~100 ms) |
| SQLite database | 100 MB per plugin |
| File storage | 500 MB per plugin |
| Web hook payload | Max 1 MB request body |
| HTTP request timeout | Max 30 seconds |
| CPU share | 25 % of one core, averaged over 10 seconds; past it, events wait in the queue. Plugin threads run at a lower priority. |
| Registered commands | Max 100 commands per plugin |
| Commands thinking at once | Max 100 per plugin |
| Active timers | Max 50 active timers per plugin |
| Log history | Kept up to 5,000 lines in memory and disk per plugin |

---

## Protecting Your Work

Code executed on Opencord cannot be hidden from the machines running it: server scripts run on the server administrator's hardware, and client scripts run on viewers' devices. Opencord does not support bytecode obfuscation or encryption.

To protect intellectual property and commercial offerings:
1. **License & Terms:** State your terms clearly in the `license` field. Administrators must read and accept this license when installing the plugin.
2. **License Keys & Secrets:** Use `kind = "secret"` in your manifest configuration. Secrets are stored securely and never broadcast to client apps.
3. **External Services:** Keep core business logic or proprietary algorithms on your own external backend service. Use the `internet` permission to query your service with authentication tokens supplied through secret configuration fields.
