# Decision log

Decisions not covered by [the Phase 1 plan](plans/phase-1.md) or [the desktop UI plan](plans/desktop-ui.md), and the reasons for them. Newest last.

## D1: Licensing (2026-10-06)

- **Server** (`crates/opencord-server`, `deploy/`, and anything not listed below): AGPL-3.0-or-later, in the root `LICENSE`.
- **Client and shared code** (`app/`, `crates/opencord-core`, `crates/opencord-common`, `crates/opencord-proto`, `proto/`): MPL-2.0, with a `LICENSE` file in each directory.

Why:
- AGPL keeps modifications to hosted servers open.
- The shared crates are compiled into both server and client, so they cannot be stricter than the client license.
- MPL-2.0 is file-level copyleft. Changes to Opencord's own files stay open, but client forks and app-store builds remain possible.
- MPL-2.0 is compatible with the AGPL, so the server can depend on the shared crates.
- "or later" lets the project move to a future AGPL version without asking every contributor.

## D2: flutter_rust_bridge 2.13.0 with cargokit (2026-10-06)

- `flutter_rust_bridge` is pinned to exactly 2.13.0 in `Cargo.toml` (`=2.13.0`), `app/pubspec.yaml` and the codegen install command, and the three must always match. 2.13.0 was the latest stable release; 2.14 was still in beta.
- The integration backend is cargokit (frb's default), which produces the `app/rust_builder/` layout from the plan. frb also offers a Dart native-assets backend, which could be worth revisiting later.
- Consequence: cargokit builds the core with `rustup run stable cargo build`, so building the app requires rustup.

## D3: The `opencord-core` package is named `opencord_core` (2026-10-06)

cargokit derives the native library filename from the Cargo package name, so the package uses an underscore. The directory stays `crates/opencord-core`, and Rust code refers to the crate as `opencord_core` either way.

## D4: App identifiers (2026-10-06)

The Dart package is `opencord` and the org is `dev.opencord`, which makes the application/bundle id `dev.opencord.opencord`.

**Provisional:** before any store release, replace it with a reverse-DNS name under a domain the project controls.

## D5: Rust edition 2024; generated bindings excluded from rustfmt (2026-10-06)

- The workspace uses edition 2024 with resolver 3.
- `frb_generated.rs` is marked `#[rustfmt::skip]`. The codegen formats it in 2021 style, and reformatting it would put a diff on every regeneration.

## D6: Generated bindings are committed (2026-10-06)

`app/lib/src/rust/` and `crates/opencord-core/src/frb_generated.rs` are committed, so the app builds without the codegen installed.

Nothing catches stale bindings automatically. frb's startup content-hash check only compares the Dart and Rust halves of the generated code with each other, and both are written in the same codegen run. After changing `crates/opencord-core/src/api/`, run `flutter_rust_bridge_codegen generate`:

- A changed signature that wasn't regenerated usually breaks the Rust build.
- A new function that wasn't regenerated stays missing on the Dart side until Dart code calls it.

A CI check (regenerate, then `git diff --exit-code`) can be added later.

## D7: `.proto` files are compiled with protox, not protoc (2026-10-06)

`crates/opencord-proto/build.rs` uses [protox](https://crates.io/crates/protox), a protobuf compiler written in Rust, so building no longer needs a system `protoc`. Once the client core depends on the proto crate, it gets built inside Xcode (iOS/macOS) and on Windows, where `protoc` is rarely on PATH.

The generated `Envelope` boxes its `Ready` payload, which is many times larger than every other variant.

## D8: Protocol details beyond the plan (2026-10-06)

[protocol.md](protocol.md) is the full specification. These details were not in the plan:

- `Identify` carries the client's `protocol_version`; a mismatch is rejected.
- `Heartbeat` carries `last_seq`.
- After a failed `Resume`, the server keeps the connection open so the client can `Identify` right away, without reconnecting.
- `SendMessage` carries a client `nonce`. It is echoed on the response and on `MessageCreate`, so optimistic sends can be matched whichever arrives first.
- Added an `UpdatePresence` request, since the plan's presence statuses (idle, dnd) need a way to be set. `OFFLINE` makes a user appear offline.
- `ReorderRoles` takes the listed roles in their new order and rearranges them among the positions they already hold, so a member can never move a role above their own.
- Role color 0 means "no color".
- Per-request permission requirements, event audiences, and WebSocket close codes are defined in protocol.md.
- Limits the plan doesn't give: topic 1 024 characters, server description 1 000, kick/ban reason 512. `FetchMessages` limit 0 means 50, and larger values are capped at 100.

## D9: Owner claim tokens and the startup invite (2026-10-07)

- Only the SHA-256 of the claim token is stored, so it can't be printed again. Instead, every start of a server that has no owner issues a new token, invalidating the previous one.
- Using the token makes that user the owner and clears the token.
- `reset-claim-token` issues a token even when the server already has an owner. Whoever uses it takes over ownership. This lets an operator with shell access recover a server whose owner lost their identity.
- Once the server has an owner, it keeps one permanent, unlimited invite and logs its link at startup. It is created the first time it is needed and reused for as long as it remains valid.

## D10: Server internals (2026-10-07)

- **Guild cache:** roles, channels with overwrites, and members are cached in memory. Permission checks and event fan-out never touch the database. Changes go to the database first, then to the cache, then out as events, all under one write lock so the three stay in step.
- **Messages** do not take that lock; they never change the cache.
- **Ready first:** a new session is registered only after its `Ready` is queued, under the write lock, so no event can arrive before `Ready`.
- **Snowflake worker id** is always 0, since a server runs as a single process.

## D11: Client core API (2026-10-07)

- **Dart-facing types:** Dart sees only the types in `crates/opencord-core/src/api/types.rs`, never protobuf. The Rust `Client` (`src/client.rs`) uses the same types, so the integration tests exercise exactly what the app gets.
- **Permission bits are `i64`:** they reach Dart as a plain `int` instead of the `BigInt` frb uses for `u64`. The bits are unchanged, so `ADMINISTRATOR` (bit 63) makes the value negative; test bits with `&`.
- **Data-carrying enums use freezed:** enums like `CoreEventPayload`, `ConnectionState`, `AddServerOutcome` and `CoreError` become Dart sealed classes through `freezed`, which is frb's standard approach. `flutter_rust_bridge_codegen generate` runs `build_runner` itself, and the generated `*.freezed.dart` files are committed with the other bindings.
- **Own runtime:** the core runs a dedicated tokio runtime for connection tasks. Async API calls are handed to it, so they don't depend on frb's executor.
- **Event buffering:** events arriving before Dart opens `event_stream` are buffered, up to 10 000.
- **`identity_load` also takes the display name,** since `Identify` needs it.
- **Owner claims stop at the TOFU prompt:** adding a server by plain `host:port` with a self-signed certificate returns `NeedsTrust` with the fingerprint. The app asks the user, calls `server_trust_fingerprint`, and adds again. An invite link's `#fp=` pins silently when it matches.
- **Certificates from a public CA** are accepted without a pin, as the plan allows.
- **Reconnects** use backoff from 1 s up to 30 s with ±25 % jitter, and try `Resume` first. Kicks, bans, rejected identities and changed certificates stop reconnecting, with a `Failed` state giving the reason.
- **Licensing:** the MPL-2.0 core dev-depends on the AGPL server, to run its integration tests against a real server. Dev-dependencies are not part of anything built or shipped.

## D12: Identify signatures cover the server's certificate (2026-10-07)

The plan's §5.2 payload (`"opencord-auth-v1" || server_id || nonce || timestamp_ms`) lets one server log in as its users elsewhere. Anyone can run a server, so a malicious one can open a connection to a victim server, pass that server's `Hello` through to a connecting user, and replay the user's signed `Identify` there.

The signed payload now also includes the SHA-256 fingerprint of the TLS certificate the client verified, and the server checks it against its own certificate. A relaying server presents its own certificate, so the user's signature never verifies on the victim.

Costs:
- TLS has to end at the Opencord server; a reverse proxy may pass TLS through but not terminate it.
- Renewing the certificate takes effect on restart, as it already did.

The protocol was not released before this change, so the version stays at 1.

## D13: Desktop UI scope (2026-10-07)

- The [desktop UI plan](plans/desktop-ui.md) is the spec for M5 and M6 on desktop. It replaces Phase 1 plan §9.1–9.3 wherever the two differ. For example, voice channels open a voice view on mock data instead of showing "Coming soon".
- The plan was pasted cut off inside §17.1. Everything up to that point is built; the rest of §17 waits for the user.
- Screens are built against a mock repository first (plan §11) and then switched to the Rust core (plan §12, step 15).
- Some UI features have no protocol support in Phase 1: reactions, pins, read state, system messages, announcement channels and voice. They work on mock data. With the real core they are hidden until the protocol supports them, and step 15 records each case.

## D14: Bundled fonts and the icon subset (2026-10-07)

- Inter 4.1 ships as static TTFs for 400, 500, 600 and 700, plus italic at 400 and 600. Flutter does not map `fontWeight` onto a variable font's `wght` axis, so static files keep every `TextStyle` simple.
- JetBrains Mono 2.304 ships at Regular only, because the plan uses mono at a single weight.
- Material Symbols Rounded comes from a pinned upstream commit and is cut down to the icons the app uses by `app/tool/build_icons.dart`. The script uses the `font-subset` tool from the Flutter SDK and keeps all four variation axes. The result is about 260 KiB instead of 15 MB. To add an icon, add it to the list in the script and run it again.
- `uses-material-design` is off, so no widget can fall back to the Material Icons family.
- The fonts' licenses are bundled and appear on the app's license page.

## D15: What "no hue" means (2026-10-07)

The §2.1 greys have a slight cool tint: the red, green and blue channels differ by up to 8 out of 255 (for example `#6E6E76`). §13 asks that every painted color have saturation 0, which the tokens themselves don't meet. The checks therefore treat a color as monochrome when its channels differ by at most 8. That admits the tokens and rejects anything with real color.

## D16: Muted text contrast is below the plan's own minimum (2026-10-07)

§2.1 asks for muted text at 4.5:1 or more. The `textMuted` tokens give 2.7–3.9:1 on the surfaces they sit on: dark `#6E6E76` gets 3.8:1 on `chat`, and light `#8A8A92` gets 3.4:1 on white. §0 asks for the tokens exactly, so they are kept and the contrast test is skipped with a pointer here. Raising muted text to 4.5:1 would bring it within about 1:1 of `textSecondary` and flatten the hierarchy. Changing it means editing two values in `oc_colors.dart`.

## D17: Theme and settings defaults (2026-10-07)

- The theme defaults to System. The Phase 1 plan said dark, but the UI plan lists System first and has a light theme.
- Switching themes takes effect on the next frame with no cross-fade, because §13 asks for an instant switch.
- The font-size slider sets the body size from 12 to 18 px. Every style scales by `size / 14` through `MediaQuery.textScaler`, multiplied by the platform's own scale.
- UI settings are stored as one JSON value, `ui.settings`, in the Rust core's settings file, so the app has a single local data store. A damaged value falls back to the defaults instead of failing.
- `path_provider` locates the app data directory. It is a platform utility, not a UI kit.

## D18: The UI's data layer (2026-10-07)

- Screens see plain Dart view models (`app/lib/core/model/`), never Rust types. Both the mock and the Rust core implement one `OpencordRepository` interface and emit the same `RepoEvent`s (plan §10).
- State is built only from events, by pure reducer functions that can be unit-tested. Riverpod notifiers hold the results at a fine grain: per server, presence, unread activity, typing, voice, speaking, and per channel for messages. A presence or speaking update therefore rebuilds only what shows it.
- An event pump routes each event to the notifiers it concerns. Message events go only to channels whose history is loaded; other channels update their unread counts and previews.
- A fresh session (Ready) increments the server's `epoch`, and loaded histories reload, because messages may have been missed while disconnected.
- Mentions and channel links are written `<@id>` and `<#id>` in message content, a client convention recorded in the protocol doc. Matching by id keeps mentions correct when names collide or change.
- Messages sent by the user appear at once as pending. They are confirmed by the response or by the `MessageCreate` event, whichever arrives first (matched by nonce). A failed send keeps its text and offers Retry (plan §6).
- The mock's channel ids are fixed per server, so remembered channels survive a restart. The reconnecting server shows its last state, then keeps retrying with backoff (plan §11, §4.13).

## D19: Window chrome (2026-10-07)

- **Linux decides at window creation.** GTK settles client-side decorations when the window is realized, which happens before the app's Dart code runs. The app therefore saves the user's choice (auto, custom or system) in `window-chrome` in its data directory, and the runner reads that file when it creates the window. Auto, or no file on the first start, is settled by the runner for the desktop it finds at that start, so switching between, say, Hyprland and GNOME gets the right frame on each. A changed Window frame setting takes effect after a restart (§8.1 allows this), and `configure` reports what the window really has.
- **Linux custom frame.** On a compositing desktop, the visible window sits inside a 14 px transparent margin that carries a soft shadow, with 12 px rounded corners and a 1 px border. The 6 px of margin next to the window resize it, with 12 px corners. The outer margin passes clicks through (through an input shape), and the shadow width tells the compositor where the window really is. Without compositing, the corners are square and the resize strips sit just inside the edges. Maximized, tiled and fullscreen windows have no frame.
- **Native drag, resize and window menu** use the last real button press. Wayland compositors require one.
- **GNOME buttons** are 24 px circles in 36 px hit squares; the plan's "18 px hit-padding" is read as the space around each circle. When the member list's header carries the window buttons, the list's own close button is hidden, so two × marks never sit side by side. The list still toggles from the chat header or with Ctrl+Shift+U.
- **Windows** handles `WM_NCCALCSIZE` and `WM_NCHITTEST` in the runner. The side and bottom borders stay for resizing and the DWM shadow; the top resize strip and the maximize button (so Windows 11 snap layouts appear) are answered by the top-level window, with the Flutter view passing those points through. Over the maximize button the runner reports hover and press to the app. Like everywhere else, a changed frame applies at the next start.
- **macOS** keeps the native traffic lights, centred in a 52 × 76 pt band above the rail and inset 20 pt. The native title bar area is confined to that band, so buttons elsewhere in the header row keep their clicks; the header row moves the window through `performDrag`. A double click on the title bar follows the system's "double-click a window's title bar" setting.
- **Restoring the window** brings back its size and maximized state everywhere. Position comes back too where the platform allows; Wayland does not let windows read or set their position. A saved position on a monitor that is gone falls back to the primary monitor, at the saved size. Windows keeps the position in physical pixels and the size in logical ones, so a window on a monitor with a different scale from the primary comes back there.
- The Windows and macOS runner code could not be built on the development machine. CI builds both.


## D20: Chat conventions (2026-10-07)

- The message list is two slivers around an anchor: older messages grow upwards from it and newer ones downwards. This is the only layout where both loading history and receiving messages leave the view still. The list follows new messages only while the reader is at the end.
- Channel links are `opencord://host:port/c/<channel id>`; message links add `/<message id>`. The UI plan names "Copy link" but gives no format.
- Scroll positions (the newest 200 channels) and drafts are kept across restarts. Reply and edit targets last only for the run, because the message replied to may not be loaded after a restart.
- Emoji data is generated by `app/tool/build_emoji.dart` from iamcal/emoji-data v16.0.0 (MIT, pinned commit, license on the license page). It stops at Unicode 15.0 because Windows' emoji font lacks 15.1 and later. No emoji package fits the UI plan's "no UI kits" rule.

## D21: Desktop integration (2026-10-07)

- **One instance per user session.** On Linux the GtkApplication is unique, and later launches hand their `opencord://` link to the running one over D-Bus and quit. Windows uses a session mutex and `WM_COPYDATA`; macOS apps are single-instance already.
- **The `opencord://` scheme** is registered for the current user on every start on Windows and declared in `Info.plist` on macOS. On Linux, `app/linux/packaging/dev.opencord.opencord.desktop` is for packagers, and the README says how to register a local build.
- **The Linux tray** is a StatusNotifierItem with a com.canonical.dbusmenu menu, written over `package:dbus` in Dart, so there is no libappindicator dependency. Opencord has no logo yet, so the tray draws a provisional monochrome mark in code, and the launcher icons are still Flutter's defaults.
- **Closing** hides the window to the tray only when "close button minimizes to tray" is on and a tray is showing the icon. Otherwise it quits, so the app is never left running with no way to reach it.
- **Linux notifications** go through `org.freedesktop.Notifications`. A call that gets no answer in 5 seconds stops further tries until a notification daemon appears on the bus.
- **Launch at login** is an XDG autostart entry on Linux, the HKCU Run key on Windows and SMAppService on macOS 13+. It is reapplied at every start, so it follows a moved app.
- **Not yet built:** the Windows and macOS trays, notifications and badges. There is no Windows or macOS toolchain on the development machine. `TrayService` and `NotificationService` are where they plug in.
- `dbus` is held at ^0.7.15 because `desktop_drop` 0.8.4 needs 0.7.x.

## D22: The app on the Rust core (2026-10-07)

- **The core is the default.** `OPENCORD_MOCK=1` (or `--mock`) runs the mock repository for UI work. `OPENCORD_PROFILE=<name>` runs a separate copy with its own data folder, keychain entries and single-instance name, so two users can be tried on one machine.
- **The seam.** `RustRepository` calls the generated bindings through a `CoreApi` interface, so it is tested against a fake without the native library.
- **Read state is kept on the device.** Phase 1 servers keep no read state, so read positions are stored per server in the local settings. After each Ready, the last message and the unread and mention counts are rebuilt from the newest 50 messages of each readable text channel, four channels at a time. A channel never read before starts out read. Events that arrive meanwhile are held until that Ready has gone out.
- **The identity** (secret and display name) is kept in the system keychain through `flutter_secure_storage`, under keys namespaced by profile. On Linux the plugin keeps all of an app's keys in one keychain item, so the app deletes only its own keys and never everything.
- **What Phase 1 servers lack is not offered** (D13). There is no Reply (a new `replies` capability), no reactions, no pins and no announcement channel type. Voice channels read "Soon" in Phase 1; since Phase 2 V0 they join (D24), and the camera and screen share controls stay hidden until there is media (`camera` and `screenShare` capabilities).
- **Private channels.** A private channel is made by allowing its maker, then denying @everyone. The Private switch therefore shows only to members with Manage roles.
- **The core gained `server_retry_now`**, which skips the reconnect wait or starts over after a failure. It also reports both certificate fingerprints when a server's identity changes. The app's Retry now button and its "Server identity changed" dialog use these.

## D23: Phase 1 acceptance checks (2026-10-07)

- `app/tool/check_m5.sh` and `app/tool/check_m6.sh` drive the real app (integration tests) against a real server. Each starts its own server on a free port in a temporary folder, with temporary XDG folders and its own profile, so it leaves the developer's server and data alone. The keychain is the exception on Linux: `flutter_secure_storage` keeps every key of the app, whatever the profile, in one Secret Service item named at build time. The checks only add and delete keys under their own profile's prefix, but the plugin rewrites that item whole, so a check racing the developer's own app writing its identity at the same moment could lose that write.
- M5 runs the app twice. The first run creates an identity and claims the server (TOFU). The second is the restart, which must come back connected.
- M6 asks for two app instances. `integration_test` drives only one app process, so the app plays the member, driven through the UI. The owner is a peer program on the same Rust core (`crates/opencord-core/examples/m6_peer.rs`). "Without reconnecting" is checked as exactly one handshake for the whole run. The owner answers the member's first message, so the answer can only arrive live. The owner's settings pages are covered by widget tests; the README describes the two-window run by hand.

## D24: Voice on the main gateway (Phase 2 V0, 2026-10-07)

- **Packages.** `voice.proto` and `internal.proto` sit in `proto/opencord/v1/`, where the plan puts them, but use the packages `opencord.voice.v1` and `opencord.internal.v1`. The plan's voice gateway names (Hello, Identify, Ready, Resume) would otherwise collide with the main gateway's. The protocol version stays 1: every change adds fields or messages.
- **Names.** Screen share requests are verb-first (`CreateStream`, `UpdateStream`, `DeleteStream`, `WatchStream`, `UnwatchStream`), as `CreateChannel` is; the plan's `StreamCreate`, `StreamUpdate` and `StreamDelete` are the events. Ids stay `int64` like every other id. Bitrates travel as bits per second, Discord's unit; the app shows kbps.
- **One settings message.** `VoiceSettings` carries both the voice and video settings (plan §5.2) and the soundboard settings (§11.7); `UpdateVoiceSettings` changes either. They are stored in `server_meta` under `voice.*`.
- **Upgrades.** Migration `0002_voice` gives existing servers' @everyone `USE_VOICE_ACTIVITY`, `USE_SOUNDBOARD` and `USE_EXTERNAL_SOUNDS`, so an upgraded server behaves like a new one instead of forcing everyone to push-to-talk.
- **Moderation** (server mute, deafen, move, disconnect) needs the permission in the target's channel and outranking the target, as kicks and nicknames do, except on yourself. Server mute and deafen are remembered per user while the server runs, across leaving and rejoining, as Discord's member flag is. They are not saved, so a restart lifts them, because the plan keeps voice in memory. A move needs `MOVE_MEMBERS` in both channels, and the target must be able to view and connect to the destination. Moves skip the user limit but not the configured per-channel cap, which binds everyone.
- **Permission changes.** Losing View channel ends a voice state, losing Connect does not, and losing Speak suppresses, as on Discord. This runs after every role, overwrite and membership change.
- **Tokens.** One Ed25519 key in the data folder signs voice tokens and media tokens, under separate domain labels. Voice nodes get only its public half. Media tokens are base64url bearer tokens, valid for 24 hours.
- **The client core** marks each voice state `this_device` rather than handing session ids to Dart. It holds this device's channel across all servers (one at a time) and, after a fresh session such as a server restart, joins it again. When the server refuses, the app is told this device left. When the connection fails, it tries again at the next Ready. `VoiceServerUpdate` and the media token stay in Rust, for the media engine and the soundboard.
- **Server settings** gain a Voice & video page for Manage server. The channel editor gains bitrate, user limit, voice chat and "Push-to-talk required", which sets or clears the @everyone deny of Use voice activity (plan §5.3). The desktop UI plan's §17.4, which describes this, has not arrived, so these follow the existing settings pages. Permissions the app cannot use yet keep the roles editor's "later" note.


## D25: Voice nodes and audio forwarding (Phase 2 V1, 2026-10-08)

- **str0m** 0.24.1 with default features off and the aws-lc-rs backend. The pure-Rust backend was the first choice, but in 0.24.1 it turns on dimpl's `rcgen` feature, which builds aws-lc-rs anyway. It runs in RTP mode with the direct API, without SDP. The node is ICE-lite and the DTLS server; clients control ICE and are the DTLS client. Each client sends on one media and receives each other person on a media named after their SSRC. SSRCs are unique per channel and kept end to end, and sequence numbers stay continuous across packets the node chooses not to forward.
- **Addresses.** The node binds `0.0.0.0` and `::` and tells str0m a fixed documentation address as each packet's destination, one per address family; the kernel does the routing. A candidate with an empty address means "the host you reached the voice gateway with". That is what `public_address = "auto"` sends, for the embedded node and for external ones, so no STUN server is involved.
- **Crates.** `opencord-voice` (AGPL) is a library: the node runtime, the voice gateway, the SFU and the control channel messages. `opencord-media` (MPL) is the client side, so far its voice transport. `opencord-voicebot` (MPL) is the headless test client, with libopus through the `opus` 0.4 crate (bundled and built statically). The `opencord-voice-node` binary belongs to `opencord-server`, to reuse its TLS and config code. The control channel's proof uses `hmac` 0.13.
- **External nodes** connect to the main server at `/internal/voice` on its own port, which exists only when nodes are configured. They prove a shared secret with HMAC-SHA256 over a random challenge. The secret is a file of at least 16 bytes; `opencord-voice-node generate-secret` writes 32 random bytes as hex. A node is known by its configured endpoint. It trusts the main server by the pinned fingerprint, or by the public certificate authorities when none is given. Registrations are limited to 10 per minute per IP address.
- **Choosing a node.** A channel's first participant gets the node with the fewest people, then the fewest channels, and ties go to the embedded node. In embedded mode, configured external nodes take part too. A channel keeps its node until it is empty.
- **Failover.** When a node's control channel closes, or is silent for 15 seconds, the main server gives its channels to other nodes, with fresh tokens. With no node left, voice states stay and wait; they get a node when one registers. Discord signals this with a null endpoint; here nothing is sent until a node exists. The node itself ends every call (close 4015) and lets nobody in until it registers again, so no connection outlives the main server's control over moderation. It reports only voice sessions that expired: endings the main server caused, replaced connections and shutdowns are not reported, since during a failover such a report would end the voice state the main server is moving.
- **Moves** close the old voice connection and send a `VoiceServerUpdate` for the new channel; a node never switches someone's channel in place.
- **Load reports** carry channels, participants, packets per second and bitrates. CPU is reported as 0 until the V10 load tests measure it. `SettingsUpdate` is not sent, since limits travel per channel in `ChannelUpdate`; `SpeakingActivity` waits for AFK moves (V9).
- **Not covered by automated tests:** the 15-second silence limit on either side of the control channel. A test would take 15 seconds, and the code is one timer in each loop.

## D26: The client audio engine (Phase 2 V2, 2026-10-08)

- **Crates.** cpal 0.18 for devices: its own PipeWire host on Linux (ALSA as the fallback), WASAPI and Core Audio elsewhere. rubato 5 resamples devices not at 48 kHz, rtrb 0.4 carries samples between the device callbacks and processing, and audio_thread_priority 0.38 raises the processing thread (RTKit over D-Bus on Linux, MMCSS on Windows; a refusal is logged and audio runs anyway). libopus through the `opus` crate, as for the voicebot.
- **Threads.** Device callbacks only move samples through lock-free rings. One engine thread wakes every 5 ms, keeps 30 ms queued for the speaker and runs the processing tick: microphone ticks through the gate into 20 ms Opus frames, and packets through each person's jitter buffer and decoder into the mixer. The network side is the transport's tokio task. Devices prefer 48 kHz and 10 ms buffers where offered. Measured without devices, the software path from microphone tick to speaker tick takes about 40 ms.
- **Jitter buffer.** Its own, per person, as the plan says: the delay is the 95th percentile of recent arrival jitter plus 10 ms, from 20 to 200 ms, set again at the start of each talk spurt. A lost frame comes back from the next packet's in-band FEC or is concealed; a backlog more than 100 ms over the delay is cut. RTP timestamps follow the sender's capture clock and a talk spurt's first packet is marked, so pauses and new spurts are visible.
- **Sending.** A level gate until V3's sensitivity settings (-45 dBFS, 250 ms hangover, 20 ms of pre-roll), or push-to-talk with a 200 ms release delay. Opus DTX frames are not sent, and nothing is sent while muted, server-muted or suppressed; a silent participant costs about 620 bit/s of keep-alives. Deafen also mutes.
- **Devices.** Settings keep device ids, with the name for while a device is unplugged. A chosen device that goes away falls back to the system default with a toast; the core lists devices every 2 s while in voice and switches back when the chosen one returns. The engine never lists devices itself, since that can take tens of milliseconds.
- **Connection states and recovery** (plan §7.14). The core reports awaiting endpoint, authenticating, RTC connecting, connected, reconnecting, no route and disconnected. A dropped gateway resumes while media flows. A connection that cannot be saved asks the server for a fresh token with the new `RefreshVoiceServer` request. On a network change the client moves media to a new socket as a new ICE candidate. The node follows a client's nomination from a new address only when it is signed with that client's ICE password, so a forged packet moves nothing. Media was back in about 100 ms in the tests.
- **Not in V2.** Per-person volume and local mute, and the push-to-talk key, exist in the engine, core and app API, but have no UI until the desktop UI plan's §17.3 and §17.5 arrive. The impairment shim (plan §15) waits for V4's bandwidth tests; loss and jitter were tested on the audio pipeline over a simulated network.

## D27: Voice processing (Phase 2 V3, 2026-10-08)

- **Echo cancellation and gain control** come from the `aec3` crate 0.4, a pure-Rust port of WebRTC's AudioProcessing (AEC3, AGC2), rather than `webrtc-audio-processing` 2.1, which builds the C++ original. Both were run on the same synthetic recordings: a speech-like far end through a room (50 ms delay, 120 ms tail, a soft-clipping speaker), with near-end talk added later for double talk. At echo levels of -6.8, -20.7 and -32.8 dBFS, aec3 removed 39.7, 30.7 and 24.6 dB of echo, against 45.0, 32.7 and 22.0 dB for the C++ build. During double talk it kept more of the near-end voice at the loudest echo (-12.1 dB against -24.4 dB) and the same at the others (-6.4 against -6.0, -1.4 against -1.4). It costs about 130 µs per 10 ms against 80 µs, about 1.3 % of a core. The quality is comparable, and aec3 builds wherever Rust does. The C++ build needs meson, ninja and clang, and its Windows (MSVC) build could not be checked here. `webrtc-audio-processing` stays the fallback if aec3 falls short on real devices.
- **Steady tones.** Neither build cancels the echo of a steady pure tone for long: both let a 400 Hz tone's echo through after about half a second. AEC3 leaves stationary echo to noise suppression. Speech is what it is for, and the tests use a voice.
- **The high-pass filter** is our own second-order Butterworth at 100 Hz, which is what WebRTC's filter does. aec3 0.4's filter applies its 48 kHz coefficients to the 16 kHz low band, which moves its cutoff to about 33 Hz: it took only 1.6 dB off 50 Hz hum. Ours takes 12 dB off it and leaves 300 Hz and up alone.
- **The chain** follows plan §7.2: the high-pass filter, AEC3, noise suppression, AGC2, then the input volume and the gate. AEC3's own high-pass filter, noise suppressor, gain control and post filter are off in its pipeline. AGC2 runs as a second aec3 graph after noise suppression, so noise is not amplified, and it never touches the operating system's microphone volume. Everything Opencord plays is the echo canceller's reference, taken just before it goes to the speaker.
- **The echo's remains.** After AEC3, a microphone tick 25 dB or more under the loudest tick played in the last 300 ms (when that was above -60 dBFS) is taken for what is left of the echo. Voice activity never opens on it, and gain control skips it. Without this, Bob's default chain (echo cancellation, Standard, gain control, automatic sensitivity) sent 43 frames of echo back to Alice in the test, up to -34 dBFS, with his speaker 10 dB down at his microphone. AEC3 left about 30 dB less than the echo; AGC2 starts at +15 dB, and RNNoise rightly calls the residue speech. With the guard nothing goes back, with Standard or High. Bob talking over Alice still got 84 of 100 frames through (91 without the guard). The cost is that a quiet voice over a loud far end is gated, the trade-off of half-duplex.
- **Standard noise suppression** is RNNoise through `nnnoiseless` 0.5.2 (BSD-3-Clause), fed samples scaled to the 16-bit range. The tests use recordings (`crates/opencord-media/tests/fixtures/README.md`): a LibriVox reader, and public-domain keyboard, fan and street recordings and a CC BY-SA barking dog from Wikimedia Commons. With the noise at -35 dBFS under the voice at -22 dBFS, Standard took 12 dB off the fan, 55 dB off the street and 56 dB off the keyboard, and the voice came out within 0.1 dB. It took under 1 dB off the barking, and RNNoise rates some barks as voice. So the plan's "clearly reduced in both modes" holds for High only where barking is concerned; that is what High is for (plan §7.3).
- **High noise suppression** is DeepFilterNet 3, the network behind DeepFilterNet's real-time PipeWire plugin. DeepFilterNet's Rust library is published only as an old 0.2.5. Its latest commit (0.5.7-pre, October 2024) builds only against tract 0.21.4, from April 2024, because it mixes in ndarray 0.15 types and a field tract renamed in 0.21.6. So its streaming path for one channel is ported into `opencord-media` (`audio/deep_filter`, about 500 lines) on current tract (0.23). On the same audio, the port's output matches upstream libDF's within 1.8e-7 per sample, and its local SNR estimates within 3.8e-5 dB. The model (three ONNX files, 8.6 MB) and DeepFilterNet's MIT and Apache-2.0 licences sit in `crates/opencord-media/models/deepfilternet3`. The repository dual-licenses all its code, and the model files carry no other terms. On the same recordings, High took 85 dB off the fan, 85 dB off the street, 47 dB off the keyboard and 48 dB off the barking. It leaves a clean voice untouched, but trims a voice with noise under it by 3 to 5 dB; gain control, after it, makes the level up. High delays the microphone by 30 ms and needs about 0.3–0.5 ms per 10 ms tick here (3–5 % of a core, i7-13620H), against about 0.09 ms for Standard. Debug builds optimize tract and the FFT crates, so High keeps up while developing.
- **Falling back.** High gives way to Standard when it needs more than 60 % of each tick over the last two seconds (plan §7.3), or when it fails. The engine reports it, and it stays on Standard until High is picked again. With High, RNNoise still finds the voice probability for automatic sensitivity, from DeepFilterNet's output, so it lines up with what is sent.
- **The first-run benchmark** (plan §7.3) runs High for two seconds of audio with every stage on, whatever the local SNR, so it measures the worst case: about 4.6 % of each tick here. High is the default under 20 %.
- **Automatic sensitivity** opens the gate when RNNoise's voice probability for a 10 ms tick is at least 0.5, with the 250 ms hangover and 20 ms pre-roll. RNNoise still runs for the probability when noise suppression is off. Manual sensitivity is a level threshold, -45 dBFS by default. If no probability is available, automatic falls back to that level.
- **Threads.** aec3's graphs cannot move between threads, so the audio processor is now built on the engine thread itself. The engine reports whether it started.
