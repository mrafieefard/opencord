# Opencord — Phase 1 Plan: Initialization, Foundations & Text Chat

> Open-source, self-hostable voice/video/text chat. A mix of Discord (UX, roles, text channels) and TeamSpeak (anyone runs their own server, the client connects to many servers, identity is a local keypair).

This document is the implementation plan for **Phase 1**. It is written to be handed to Claude Code. Work through the milestones **in order**, finish each milestone's acceptance criteria before moving on, and keep the repository building and passing checks after every milestone.

---

## 0. Instructions for Claude Code

- Work milestone by milestone (M0 → M7). Do not start voice/video work in this phase.
- After every milestone run: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and in `app/`: `flutter analyze` and `flutter test`.
- Use the **latest stable versions** of every crate and package; check them at the time of writing rather than trusting versions from memory.
- The dev machine is **Arch Linux on Hyprland (Wayland)**. The Linux desktop target is the first-class target for Phase 1; Android/iOS must *compile* but don't need polish yet.
- Terminal commands in docs and READMEs must not contain inline comments.
- Prefer small, reviewable commits with clear messages (Conventional Commits style).
- When a decision isn't covered here, choose the simplest option that doesn't block later phases, and record it in `docs/decisions.md`.

---

## 1. Product model (decided)

| Concept | Decision |
|---|---|
| Hosting model | **TeamSpeak model.** One `opencord-server` instance = one community ("server"). Anyone can host one. No federation in Phase 1. |
| Client | Connects to **many servers at once**; the sidebar lists them like Discord's server rail. |
| Identity | **Ed25519 keypair generated on the client.** The public key *is* the user's identity on every server. No central account service, no passwords. |
| Server ownership | On first start the server prints a one-time **owner claim token** to its log. The first client that connects with it becomes owner. |
| Joining | Via **invite links** (or open servers if the owner enables it). |
| Transport security | TLS always. Server auto-generates a self-signed cert on first start; clients use **TOFU pinning** of the SHA-256 cert fingerprint (like SSH). Proper CA certs (e.g. behind a domain) also work. |

---

## 2. Tech stack

### Server (Rust)
| Concern | Choice |
|---|---|
| Runtime | `tokio` |
| HTTP + WebSocket | `axum` (WebSocket gateway + a few HTTP endpoints for health/info) |
| TLS | `rustls` via `axum-server` or `tokio-rustls`; `rcgen` for self-signed cert generation |
| Database | **SQLite** via `sqlx` (with compile-time checked queries and migrations). Keep queries portable so PostgreSQL can be added later. |
| Serialization | **Protobuf** via `prost` / `prost-build` |
| Crypto | `ed25519-dalek`, `sha2`, `rand` |
| IDs | 64-bit **Snowflake** IDs (timestamp + worker + sequence), sortable by time |
| Config | TOML file (`opencord.toml`) via `serde` + `toml`, overridable by env vars (`OPENCORD_*`) |
| Rate limiting | `governor` |
| Logging/tracing | `tracing` + `tracing-subscriber` |
| Errors | `thiserror` in libraries, `anyhow` in binaries |

### Client
| Concern | Choice |
|---|---|
| UI | **Flutter** (Dart), targets: Linux, Windows, macOS, Android, iOS |
| Client core | **Rust crate `opencord-core`** — networking, protocol, identity, connection management, reconnection |
| Bridge | **`flutter_rust_bridge` v2** (Dart ⇄ Rust bindings, streams for events) |
| WebSocket client (in Rust core) | `tokio-tungstenite` with `rustls` and a custom cert verifier for TOFU pinning |
| State management | `flutter_riverpod` |
| Routing | `go_router` |
| Secure storage of identity key | `flutter_secure_storage` (secret key bytes are passed into the Rust core at startup) |
| Local persistence (non-secret) | Server list, pinned fingerprints, settings: JSON file in the app support dir via the Rust core (Phase 1). Message cache is out of scope. |

**Key principle:** Dart never sees protobuf or sockets. Dart calls typed Rust functions and receives typed events from a stream. The protocol is shared between `opencord-core` and `opencord-server` through the `opencord-proto` crate.

### Future phases (for awareness, do NOT implement now)
- Voice: SFU in Rust (`str0m`) on the server; `flutter_webrtc` (libwebrtc) on the client; Opus with DTX + FEC.
- Screenshare: PipeWire via `xdg-desktop-portal` on Linux (Hyprland: `xdg-desktop-portal-hyprland`); hardware encoding.
- Camera with simulcast; AV1 → VP9/H.264 fallback.
- Attachments (local disk / S3-compatible), Tantivy search, QUIC transport (`quinn`), MLS E2EE (`openmls`), PostgreSQL backend.

Design Phase 1 so these slot in cleanly: e.g. channels already have a `kind` (`text` | `voice`), permissions already reserve voice bits, and the gateway protocol has room for signaling messages.

---

## 3. Repository layout

```
opencord/
├── Cargo.toml                 # Rust workspace
├── rust-toolchain.toml        # pin stable
├── proto/
│   └── opencord/v1/
│       ├── gateway.proto      # Envelope, handshake, heartbeat, resume
│       ├── models.proto       # User, Member, Role, Channel, Message, Invite
│       ├── requests.proto     # client → server requests + responses
│       └── events.proto       # server → client events
├── crates/
│   ├── opencord-proto/        # prost-generated types (build.rs)
│   ├── opencord-common/       # Snowflake, permissions bitflags + resolver, auth payload, validation limits
│   ├── opencord-server/       # server binary
│   │   ├── migrations/
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── tls.rs
│   │       ├── db/            # repositories per entity
│   │       ├── gateway/       # ws handshake, sessions, dispatch, heartbeat, resume
│   │       ├── handlers/      # one module per request group
│   │       ├── permissions.rs # server-side checks using opencord-common
│   │       └── state.rs       # AppState, broadcast hub
│   └── opencord-core/         # client core used by Flutter
│       └── src/
│           ├── api/           # functions exposed to Dart via flutter_rust_bridge
│           ├── identity.rs
│           ├── connection.rs  # one task per server, reconnect + resume
│           ├── tofu.rs        # cert fingerprint pinning
│           ├── store.rs       # servers list + settings on disk
│           └── events.rs      # mapping proto events → Dart-friendly structs
├── app/                       # Flutter app (flutter_rust_bridge integrated)
│   ├── rust_builder/          # generated by frb
│   └── lib/
│       ├── main.dart
│       ├── src/rust/          # generated bindings (do not edit)
│       ├── core/              # providers wrapping the Rust API
│       ├── features/
│       │   ├── onboarding/
│       │   ├── servers/
│       │   ├── channels/
│       │   ├── chat/
│       │   ├── members/
│       │   └── settings/
│       └── ui/                # theme, shared widgets
├── deploy/
│   ├── Dockerfile
│   ├── docker-compose.yml
│   └── opencord.service       # systemd unit
├── docs/
│   ├── architecture.md
│   ├── protocol.md
│   └── decisions.md
├── .github/workflows/ci.yml
├── LICENSE                    # AGPL-3.0 for server, see decision note
└── README.md
```

**License decision (record in `docs/decisions.md`):** default to **AGPL-3.0** for the server and **GPL-3.0 or MPL-2.0** for the client. Ask the owner if unsure; leave a TODO rather than guessing silently.

---

## 4. Dev environment (Arch Linux)

```bash
sudo pacman -S --needed rustup protobuf clang cmake ninja gtk3 pkgconf sqlite git openssl
rustup default stable
rustup component add clippy rustfmt
cargo install sqlx-cli --no-default-features --features sqlite,rustls
cargo install flutter_rust_bridge_codegen
git clone https://github.com/flutter/flutter.git -b stable ~/.local/share/flutter
echo 'export PATH="$HOME/.local/share/flutter/bin:$PATH"' >> ~/.bashrc
flutter config --enable-linux-desktop
flutter doctor
```

Document these in `README.md` (adapt the PATH line for the user's shell if needed). Android/iOS toolchains are not required in Phase 1.

---

## 5. Protocol

### 5.1 Framing
- One WebSocket connection per client per server at `wss://host:port/gateway`.
- **Binary frames only**, each frame is one protobuf `Envelope`.
- Max frame size: 1 MiB (reject larger).

```proto
message Envelope {
  uint64 seq = 1;          // server → client: monotonically increasing per session (for resume)
  uint64 request_id = 2;   // client → server: set by client; echoed in the matching Response
  oneof payload {
    Hello hello = 10;
    Identify identify = 11;
    Resume resume = 12;
    Ready ready = 13;
    Resumed resumed = 14;
    Heartbeat heartbeat = 15;
    HeartbeatAck heartbeat_ack = 16;
    Request request = 20;
    Response response = 21;
    Event event = 22;
    Error error = 30;
  }
}
```

`Request` / `Response` / `Event` are themselves `oneof` wrappers defined in `requests.proto` and `events.proto`. Every `Response` carries either a result or an `Error { code, message }`.

### 5.2 Handshake & authentication

```
Client                                         Server
  |  ---- TLS (TOFU pinned fingerprint) ---->   |
  |  <------------- Hello -------------------   |  { server_id, server_name, nonce(32B),
  |                                             |    heartbeat_interval_ms, protocol_version }
  |  -------------- Identify --------------->   |  { public_key, signature, timestamp_ms,
  |                                             |    display_name, invite_code?, claim_token? }
  |                                             |  verify signature, membership, bans
  |  <------------- Ready -------------------   |  { session_id, resume_token, self_user,
  |                                             |    server_info, channels, roles, members,
  |                                             |    my_permissions_per_channel }
```

- **Signed payload** (domain-separated):
  `"opencord-auth-v1" || server_id (16B) || nonce (32B) || timestamp_ms (u64 BE)`
- Server rejects if: signature invalid, `|now - timestamp_ms| > 60s`, nonce reused, user banned, or user is not a member and no valid invite / server not open.
- Unknown public key + valid invite → create user + member, consume one invite use.
- Valid `claim_token` (and no owner yet) → user becomes **owner**; token is invalidated.
- `Hello` must be answered within 10s or the connection is closed.

### 5.3 Heartbeat & resume
- Client sends `Heartbeat` every `heartbeat_interval_ms` (default 30 000). Server closes the session after 2 missed intervals.
- Server keeps a **replay buffer** of the last 1 000 events (or 60s) per session.
- On reconnect the client sends `Resume { session_id, resume_token, last_seq }`. If possible the server replays missed events and sends `Resumed`; otherwise it sends `Error(INVALID_SESSION)` and the client performs a full `Identify`.

### 5.4 Requests (Phase 1)
| Group | Requests |
|---|---|
| Messages | `SendMessage`, `EditMessage`, `DeleteMessage`, `FetchMessages { channel_id, before?, limit ≤ 100 }`, `StartTyping` |
| Channels | `CreateChannel`, `UpdateChannel`, `DeleteChannel`, `ReorderChannels`, `SetChannelOverwrite`, `DeleteChannelOverwrite` |
| Roles | `CreateRole`, `UpdateRole`, `DeleteRole`, `ReorderRoles`, `AddMemberRole`, `RemoveMemberRole` |
| Members | `KickMember`, `BanMember`, `UnbanMember`, `FetchBans`, `UpdateNickname` |
| Profile | `UpdateProfile { display_name }` |
| Invites | `CreateInvite { max_uses?, expires_in_s? }`, `FetchInvites`, `RevokeInvite` |
| Server | `UpdateServer { name, description, open_join }` |

### 5.5 Events (Phase 1)
`MessageCreate`, `MessageUpdate`, `MessageDelete`, `ChannelCreate`, `ChannelUpdate`, `ChannelDelete`, `RoleCreate`, `RoleUpdate`, `RoleDelete`, `MemberJoin`, `MemberLeave`, `MemberUpdate`, `PresenceUpdate { user_id, status: online|idle|dnd|offline }`, `TypingStart { channel_id, user_id }`, `ServerUpdate`.

**Fan-out rule:** an event about a channel is only sent to sessions whose resolved permissions include `VIEW_CHANNEL` for that channel. When roles/overwrites change, recompute affected sessions and send `ChannelCreate`/`ChannelDelete` as visibility changes.

### 5.6 Error codes
`UNAUTHORIZED`, `FORBIDDEN`, `NOT_FOUND`, `INVALID_ARGUMENT`, `RATE_LIMITED { retry_after_ms }`, `INVALID_SESSION`, `CONFLICT`, `INTERNAL`.

Write all of the above into `docs/protocol.md` as the source of truth.

---

## 6. Permissions & roles

### 6.1 Bits (`u64`, in `opencord-common`, use `bitflags`)
| Bit | Name |
|---|---|
| 0 | `VIEW_CHANNEL` |
| 1 | `SEND_MESSAGES` |
| 2 | `READ_HISTORY` |
| 3 | `MANAGE_MESSAGES` |
| 4 | `MANAGE_CHANNELS` |
| 5 | `MANAGE_ROLES` |
| 6 | `KICK_MEMBERS` |
| 7 | `BAN_MEMBERS` |
| 8 | `CREATE_INVITE` |
| 9 | `MANAGE_SERVER` |
| 10 | `ATTACH_FILES` *(reserved, Phase 2)* |
| 11 | `MENTION_EVERYONE` |
| 12 | `CHANGE_NICKNAME` |
| 13 | `MANAGE_NICKNAMES` |
| 16 | `CONNECT` *(voice, reserved)* |
| 17 | `SPEAK` *(reserved)* |
| 18 | `VIDEO` *(reserved)* |
| 19 | `SCREENSHARE` *(reserved)* |
| 20 | `MUTE_MEMBERS` *(reserved)* |
| 21 | `DEAFEN_MEMBERS` *(reserved)* |
| 22 | `MOVE_MEMBERS` *(reserved)* |
| 23 | `PRIORITY_SPEAKER` *(reserved)* |
| 63 | `ADMINISTRATOR` |

### 6.2 Resolution algorithm (pure function, heavily unit-tested)
1. If member is the **owner** → all permissions.
2. `base = @everyone.permissions | OR(member's role permissions)`.
3. If `base` has `ADMINISTRATOR` → all permissions.
4. Channel overwrites, in order:
   1. `@everyone` overwrite: `base = (base & !deny) | allow`
   2. All role overwrites for member's roles: aggregate `allow_r` and `deny_r`, then `base = (base & !deny_r) | allow_r`
   3. Member-specific overwrite: `base = (base & !deny) | allow`
5. If `VIEW_CHANNEL` is missing → treat all channel permissions as missing.

### 6.3 Hierarchy
- Roles have a `position`. `@everyone` is position 0 and cannot be deleted.
- A member can only manage roles, kick, or ban targets whose **highest role position is below their own highest role**. Owner bypasses this.
- A member can never grant permissions they don't have themselves.

### 6.4 Defaults on first start
- `@everyone`: `VIEW_CHANNEL | SEND_MESSAGES | READ_HISTORY | CREATE_INVITE | CHANGE_NICKNAME | CONNECT | SPEAK | VIDEO | SCREENSHARE`
- Channels: `#general` (text), `General` (voice, inert in Phase 1)

---

## 7. Database schema (SQLite, sqlx migrations)

All IDs are Snowflake `INTEGER`. Timestamps are Unix ms `INTEGER`.

```sql
server_meta      (key TEXT PRIMARY KEY, value TEXT NOT NULL)
                 -- server_id, name, description, owner_user_id, open_join, claim_token_hash

users            (id INTEGER PK, public_key BLOB UNIQUE NOT NULL, display_name TEXT NOT NULL,
                  created_at INTEGER NOT NULL)

members          (user_id INTEGER PK REFERENCES users, nickname TEXT,
                  joined_at INTEGER NOT NULL)

roles            (id INTEGER PK, name TEXT NOT NULL, color INTEGER, position INTEGER NOT NULL,
                  permissions INTEGER NOT NULL, hoist INTEGER NOT NULL DEFAULT 0,
                  mentionable INTEGER NOT NULL DEFAULT 0)

member_roles     (user_id INTEGER, role_id INTEGER, PRIMARY KEY (user_id, role_id))

channels         (id INTEGER PK, kind TEXT NOT NULL CHECK (kind IN ('text','voice','category')),
                  name TEXT NOT NULL, topic TEXT, parent_id INTEGER REFERENCES channels,
                  position INTEGER NOT NULL, created_at INTEGER NOT NULL)

channel_overwrites (channel_id INTEGER, target_kind TEXT CHECK (target_kind IN ('role','member')),
                  target_id INTEGER, allow INTEGER NOT NULL, deny INTEGER NOT NULL,
                  PRIMARY KEY (channel_id, target_kind, target_id))

messages         (id INTEGER PK, channel_id INTEGER NOT NULL, author_id INTEGER NOT NULL,
                  content TEXT NOT NULL, edited_at INTEGER, deleted INTEGER NOT NULL DEFAULT 0)
                 INDEX (channel_id, id DESC)

invites          (code TEXT PK, created_by INTEGER, created_at INTEGER, max_uses INTEGER,
                  uses INTEGER NOT NULL DEFAULT 0, expires_at INTEGER)

bans             (user_id INTEGER PK, reason TEXT, banned_by INTEGER, created_at INTEGER)
```

- Enable WAL mode, `foreign_keys=ON`, `synchronous=NORMAL`.
- Message `created_at` is derived from the Snowflake ID.
- Store only a **hash** of the owner claim token.

---

## 8. Limits & validation (in `opencord-common`)
| Item | Limit |
|---|---|
| Message content | 1–4 000 chars (Unicode scalar values), trimmed |
| Display name / nickname | 1–32 chars |
| Channel name | 1–100 chars; text channels normalized to lowercase-kebab |
| Role name | 1–100 chars |
| Roles per server | 250 |
| Channels per server | 500 |
| Fetch limit | ≤ 100 |
| Rate limits (per user) | Messages: 5 / 5s per channel; requests overall: 50 / 10s; identify: 5 / min per IP |

---

## 9. Client app flow

### 9.1 First launch
1. App starts → Rust core initialized → check `flutter_secure_storage` for identity secret.
2. None → **Onboarding**: generate identity (Rust), user picks display name, show public key fingerprint (short, human-readable) and an **"export identity backup"** button (file containing the secret key, with a clear warning).
3. Optionally import an existing identity backup instead.

### 9.2 Adding a server
1. User pastes an invite link: `opencord://host:port/invite/CODE#fp=SHA256HEX`, or enters `host:port` manually.
2. Rust core connects. If the link contains a fingerprint → pin it silently when it matches. Otherwise → show the **TOFU dialog** with the fingerprint ("Trust this server?").
3. Fingerprint mismatch on later connections → hard error screen ("Server identity changed"), never silent.
4. Handshake → `Ready` → server added to the persisted list and shown in the server rail.
5. A text field in "Add server" also accepts an owner **claim token**.

### 9.3 Main layout (desktop; responsive collapse on narrow/mobile)
```
┌────┬──────────────┬──────────────────────────────┬──────────────┐
│ S  │ Server name  │ # channel-name   topic       │ Members      │
│ e  │──────────────│──────────────────────────────│  Role A      │
│ r  │ ▾ TEXT       │  messages (virtualized,      │   user1 ●    │
│ v  │  # general   │  reverse list, load older    │  Online      │
│ e  │  # memes     │  on scroll up)               │   user2 ●    │
│ r  │ ▾ VOICE      │                              │  Offline     │
│ s  │  🔊 General  │  [typing indicator]          │   user3 ○    │
│    │──────────────│  ┌────────────────────────┐  │              │
│ +  │ me · status  │  │ message composer        │  │              │
└────┴──────────────┴──┴────────────────────────┴──┴──────────────┘
```
- **Server rail:** one icon per server, connection status dot, `+` to add.
- **Channel list:** grouped by category, only channels with `VIEW_CHANNEL`. Voice channels shown but disabled ("Coming soon").
- **Chat:** virtualized list, grouped consecutive messages per author, plain text + minimal markdown (bold, italic, code, code blocks, links). Edit/delete own messages; delete others with `MANAGE_MESSAGES`.
- **Composer:** Enter to send, Shift+Enter for newline, disabled with a reason when missing `SEND_MESSAGES`.
- **Members:** grouped by highest hoisted role, then online/offline.
- **Server settings** (visible based on permissions): overview, roles editor (permissions checklist, reorder by drag), channel permission overwrites editor, invites, bans.
- **User settings:** display name, identity export, theme (dark default), trusted servers & fingerprints.

### 9.4 Connection behavior
- Each server = an independent Rust task. One server going down never affects the others.
- Reconnect with exponential backoff (1s → 30s cap, jitter); try `Resume` first.
- UI shows per-server state: `connecting`, `connected`, `reconnecting`, `failed(reason)`.
- Optimistic message sending: show as pending, then reconcile on the `Response`/`MessageCreate`.

### 9.5 Rust core API exposed to Dart (sketch)
```rust
pub fn init(app_data_dir: String) -> Result<()>;
pub fn identity_generate() -> IdentitySecret;
pub fn identity_load(secret: Vec<u8>) -> Result<IdentityInfo>;
pub fn event_stream(sink: StreamSink<CoreEvent>);
pub async fn server_add(link_or_address: String, claim_token: Option<String>) -> Result<AddServerOutcome>;
pub async fn server_trust_fingerprint(address: String, fingerprint: String) -> Result<()>;
pub async fn server_remove(server_key: String) -> Result<()>;
pub fn servers_list() -> Vec<SavedServer>;
pub async fn send_message(server_key: String, channel_id: i64, content: String) -> Result<Message>;
pub async fn fetch_messages(server_key: String, channel_id: i64, before: Option<i64>, limit: u32) -> Result<Vec<Message>>;
```
`CoreEvent` is an enum tagged with `server_key` wrapping connection state changes and every gateway event. Riverpod providers subscribe to this single stream and maintain per-server state in Dart.

---

## 10. Server operations
- `opencord-server` with no config file → creates `opencord.toml` with sane defaults, a data directory, the SQLite DB, a self-signed cert, and logs:
  - the server's **cert fingerprint**
  - the **owner claim token** (only while no owner exists)
  - a ready-to-share invite link once an owner exists
- Default bind: `0.0.0.0:7710`.
- CLI subcommands (via `clap`): `run` (default), `fingerprint`, `invite create`, `reset-claim-token`.
- HTTP endpoints: `GET /health`, `GET /info` (name, version, protocol version, member count).
- `deploy/` contains a multi-stage Dockerfile (static musl build if feasible), a compose file with a volume for data, and a systemd unit.
- Graceful shutdown on SIGTERM: stop accepting, notify sessions, flush DB.

---

## 11. Milestones

### M0 — Repository initialization
- Create the workspace, crates, `app/` Flutter project (`flutter create --platforms=linux,windows,macos,android,ios app`), integrate `flutter_rust_bridge` with `opencord-core`.
- Add `rust-toolchain.toml`, `.gitignore`, `.editorconfig`, README with setup, empty `docs/` files.
- CI (GitHub Actions): fmt, clippy, test for Rust; analyze + test + Linux build for Flutter.
- **Accept:** `cargo build --workspace` passes; `flutter run -d linux` shows a placeholder screen that calls a trivial Rust function (e.g. `core_version()`) and displays the result.

### M1 — Shared crates
- `.proto` files for everything in §5; `opencord-proto` builds them.
- `opencord-common`: Snowflake generator, permission bitflags + resolver (§6.2) + hierarchy checks, auth payload builder/verifier, validation limits.
- **Accept:** unit tests for resolver (owner, admin, everyone overwrite, role aggregation, member overwrite, missing VIEW_CHANNEL), Snowflake ordering/uniqueness, signature verify (valid, wrong key, expired timestamp).

### M2 — Server skeleton
- Config loading, data dir, TLS (self-signed generation + fingerprint), migrations, first-start bootstrap (server_id, claim token, defaults from §6.4), `/health`, `/info`, CLI.
- Gateway: Hello → Identify → Ready, heartbeat, session registry, broadcast hub.
- **Accept:** integration test spins up the server on a random port with a temp dir, connects with a test client, claims ownership, receives `Ready` with default channels and roles.

### M3 — Server features
- All requests in §5.4 with permission + hierarchy checks; events in §5.5 with visibility-aware fan-out; invites, bans, kicks (kick closes the target's sessions); presence and typing; rate limits; resume with replay buffer.
- **Accept:** integration tests for: two users chatting; user without `VIEW_CHANNEL` receives no events from that channel; role change revealing/hiding a channel; banned user cannot identify; invite max uses enforced; resume replays missed messages; rate limit returns `RATE_LIMITED`.

### M4 — Client core (Rust)
- Identity, TOFU verifier, persisted server list, per-server connection task with reconnect/resume, typed request API, unified event stream, flutter_rust_bridge API (§9.5).
- **Accept:** Rust integration test using `opencord-core` against a real test server: add server via invite link, send/receive messages, survive a forced disconnect via resume.

### M5 — Flutter app: onboarding & servers
- Onboarding (generate/import identity, display name, backup export), add-server flow with TOFU dialog, server rail with status, persisted across restarts.
- **Accept:** fresh install → create identity → join a local server via invite → restart app → still connected.

### M6 — Flutter app: chat & management
- Channel list, chat view (virtualized, pagination, grouping, minimal markdown, edit/delete, typing indicator, optimistic send), member list, server settings (roles editor, overwrites, invites, bans, kick), user settings, responsive layout.
- **Accept:** two app instances (two identities) chat in real time; owner creates a role, restricts a channel, and the other user's UI updates live without reconnecting.

### M7 — Hardening & packaging
- Docker/compose/systemd, graceful shutdown, structured logs, `docs/architecture.md` with a diagram, finalize `docs/protocol.md`.
- Basic load test (e.g. a Rust bench client: 500 concurrent connections, 50 msg/s) and record results in `docs/`.
- **Accept:** `docker compose up` gives a working server; README covers hosting a server and connecting with the app.

---

## 12. Out of scope for Phase 1
Voice/video/screenshare media, file attachments, embeds/link previews, emoji reactions, DMs, threads, search, notifications/push, message cache on the client, E2EE, federation, PostgreSQL backend, web client. Keep data models and protocol extensible for all of them.

## 13. Phase 2 preview (voice)
SFU in `opencord-server` built on `str0m`; signaling over the existing gateway (`VoiceJoin`, `VoiceLeave`, SDP offer/answer, ICE candidates as new requests/events); `flutter_webrtc` on the client; Opus with DTX + FEC; voice state events (mute/deafen/speaking); `CONNECT`/`SPEAK` permissions enforced. Then Phase 3: screenshare (PipeWire portal on Linux, hardware encoding) and camera with simulcast.
