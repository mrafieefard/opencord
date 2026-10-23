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

- **Echo cancellation and gain control** come from the `aec3` crate 0.4, a pure-Rust port of WebRTC's AudioProcessing (AEC3, AGC2), driven directly through its processing modules rather than its graph runtime. The graph cost about 15 % more per tick, which put Standard just over the 2 % budget in `docs/performance.md`. We chose aec3 rather than `webrtc-audio-processing` 2.1, which builds the C++ original. Both were run on the same synthetic recordings: a speech-like far end through a room (50 ms delay, 120 ms tail, a soft-clipping speaker), with near-end talk added later for double talk. At echo levels of -6.8, -20.7 and -32.8 dBFS, aec3 removed 39.7, 30.7 and 24.6 dB of echo, against 45.0, 32.7 and 22.0 dB for the C++ build. During double talk it kept more of the near-end voice at the loudest echo (-12.1 dB against -24.4 dB) and the same at the others (-6.4 against -6.0, -1.4 against -1.4). It costs about 130 µs per 10 ms against 80 µs, about 1.3 % of a core. The quality is comparable, and aec3 builds wherever Rust does. The C++ build needs meson, ninja and clang, and its Windows (MSVC) build could not be checked here. `webrtc-audio-processing` stays the fallback if aec3 falls short on real devices.
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
- **Global hotkeys** (plan §7.13) live in `opencord-media` (`hotkeys`). Push-to-talk and the priority speaker key act on the audio engine straight from the core; the mute and deafen toggles reach the app as events. Bindings are written as the XDG shortcuts specification writes them, such as `CTRL+SHIFT+m`, `F12` or `grave`, with the key an xkb keysym name from a fixed set: letters, digits, F1–F24, numpad digits and a few others. On Wayland they are bound through the GlobalShortcuts portal, with `ashpd` 0.13. The portal wants the app's id, so Opencord registers as `dev.opencord.opencord` through the host app registry. That works only once the app's desktop file is installed (the Linux packaging ships it), so a development build answers "focused only". On X11 they come from XInput 2 raw key events, with `x11rb` 0.14, which see every key without taking it from other apps. This was tested against Xvfb, with keys typed through XTEST. On Windows and macOS, hotkeys work only while Opencord is focused for now. Their code could be neither built nor tried here, and CI builds those apps. Wherever hotkeys are not system-wide, the app handles the same bindings while focused, without swallowing keys. Nothing is bound by default; choosing hotkeys belongs to the desktop UI plan's §17.5, which has not arrived.

## D28: Video transport (Phase 2 V4, 2026-10-08)

- **Wire format.** H.264 on payload type 102 with RTX on 103 (packetization mode 1, profile 42e01f advertised; nothing on the path decodes). Opencord packetizes RFC 6184 itself, single NAL units and FU-A with payloads up to 1080 bytes, over lists of NAL units. str0m's packetizer sits behind its `unversioned` feature and scans for Annex B start codes, which a frame transform could create. Layers and keyframes are marked in an Opencord frame-marking header extension (id 9, one-byte form): a keyframe bit, a first-packet bit and the layer index, plus width and height on a keyframe's first packet. The RTP marker bit ends a frame. The node assigns every SSRC: each layer's, its RTX, and the one receivers get the track on, so a receiver sees one continuous stream across layer switches.
- **Frame transform.** Every encoded frame passes through a `FrameTransform` in the client transport, both ways: whole Opus frames, and each NAL unit after its one-byte header, so codec framing stays intact (the shape DAVE uses). It is the identity for now. An XOR transform and the impairment shim (str0m-netem, by str0m's authors) sit behind `opencord-media`'s `testing` feature. With every payload XORed end to end, audio and video still flowed through the node, layer switches included, since it reads only RTP headers and extensions.
- **Tracks.** A track id is unique within its channel, because `MediaSinkWants` names tracks only by id. Clients pick random ids, and the node refuses one already in use. `PublishTrack` is checked. Ids are 1 to 64 visible ASCII characters. Each participant may have one camera and one screen. A camera is at most 1280×720 at 30 fps, and a screen stays within the server's preset. A track has one to three layers named l, m and h, each taller than the one below and within its ceiling. A missing permission, or cameras turned off, gets FORBIDDEN; going over the limits gets QUALITY_LIMIT; a full channel gets CAMERA_LIMIT. A permission or limit change that no longer allows a track stops it at once. `MediaSinkWants` has its own allowance of 20 a second (plan §14). The client sends at most 10 a second, and the latest wins.
- **Layers for receivers.** Every 200 ms the node gives each receiver a budget: 90 % of its TWCC estimate, less the audio it gets. Everyone gets their lowest layer first. Upgrades then go by priority, screen shares before cameras and taller tiles first. A layer costs its declared maximum, and never less than the layer below. Measuring what each layer actually sent went wrong whenever a sender could not send a layer at its full rate. Going up needs 15 % headroom, and the layer being sent rides out a dip to 85 % of its cost. Switches wait for a keyframe. Keyframe requests are paced at one a second per layer; the plan says per track, which would hold up a second receiver's switch.
- **What senders encode.** The node asks each sender for the layers its receivers would get if every layer were on, plus those being switched to or forwarded. A layer nobody has needed for a second is let go. Asking only for what receivers get now deadlocked: a layer that had been stopped could never fit, so it was never asked for again. When a layer that was asked for has not arrived after a second, the node also asks for the layer below, and keeps asking for both. Within that, the client transport decides what its uplink can carry (plan §7.10) and tells the app with `VoiceEvent::Encode`. The camera's top layer goes first, then its middle layer, then the screen's low layer; then the screen slows down to 5 fps, then shrinks to a quarter of its pixels. A camera's low layer and a screen's main layer are never cut: their encoders get less, because a sender that sends nothing never learns that its uplink came back. Room for voice is kept while the participant has talked in the last 10 s. A plan rides out a dip to 85 % of its cost, and recovers one step a second with 15 % headroom.
- **Congestion control.** str0m's TWCC estimator runs at both ends. The real-time tests showed four things it needed:
  - Forwarded video leaves the node unpaced. str0m's per-stream queue is unbounded and drains at 1.1× the estimate, so after a drop, the new layer's keyframe waited seconds behind the old layer's backlog.
  - A connection's clock is brought to the present before a packet is written for it, and datagrams are stamped as they are read.
  - A receiver whose reports show 20 % loss or more (smoothed) is capped at what got through, without waiting for the estimator. Loss comes from the reports' running totals, because str0m's loss fraction wraps when a retransmission lowers the total.
  - Both ends ask for a probe themselves when they want more than their estimate allows and have been calm for 2 s, at most every 5 s. str0m caps its estimate at 1.5× what is being sent and seldom probes on its own.
- **Limits.** A track sending more than 1.25 times the sum of its layers' maxima for over 5 s is stopped with QUALITY_LIMIT. Each participant's inbound media passes a token bucket at 1.2 × (its tracks' ceilings + 300 kbps for voice) with a second's burst. What exceeds it is dropped.
- **Test pattern.** Until V5 brings an encoder, video is a synthetic pattern of H.264-shaped access units: SPS, PPS and an IDR slice on keyframes, one slice otherwise. Each is sized to its layer's bitrate and carries a frame number and a checksum, which receivers check; nothing decodes them. The voicebot publishes it (`--camera`), publishes it again when its call moves to another node, and watches others (`--watch HEIGHT`).
- **Timing.** With a receiver's downlink capped at 500 kbps, the node moved it to a lower layer within 1.0 to 1.9 s in release builds, and back up once the cap was lifted. That real-time test runs only in release builds, because a debug build's processing delays distort the estimator. The SFU's own tests check the same behaviour on a virtual clock in every build. Tests that depend on estimates run one at a time. On localhost, video takes 4 to 11 s from start to reach the top layer, because the receiver's downlink is probed first and the sender's uplink after it. V10 will try kernel receive timestamps and probing as soon as a layer is asked for.
- **The app** gets the new transport events and ignores the video ones until V5 shows video.

## D29: The camera (Phase 2 V5, 2026-10-09)

- **Linux first.** Encoding, decoding, capture and textures are Linux-only for now. Windows and macOS backends can't be built or run on this machine, and making FFmpeg a dependency there would break their CI builds. On those systems the camera reports "not supported on this platform yet", as V3 did for global hotkeys. The plan's Windows and macOS acceptance waits for those machines.
- **Codecs.** FFmpeg through ffmpeg-next 9.0, the most used binding. It is in maintenance-only mode but builds against FFmpeg 3.4 to 9.0. Encoders are tried in order: VA-API, then NVENC, then OpenH264 (Cisco's library, loaded at run time; plan §7.9). libx264 is never used, because it is GPL. The VA-API driver is chosen per render node from its kernel driver (i915 or xe gives iHD, amdgpu gives radeonsi) rather than read from `LIBVA_DRIVER_NAME`. Hybrid laptops often set that to NVIDIA's decode-only driver, as this one does. Video is Constrained High (High without B-frames, profile-level-id 640c1f). Keyframes come only on request: the GOP is an hour long. Decoding uses FFmpeg's H.264 decoder in low-delay mode on one thread. Layers taller than 360 lines decode on VA-API, smaller ones in software.
- **Capture.** Cameras come through the Camera portal and PipeWire (pipewire 0.10.1, the version cpal already brings; ashpd 0.13 for the portal). V4L2 is the fallback, but never after the user refused access. Opencord lists a camera's modes itself and asks for exactly one: at least 30 fps, then the smallest size reaching 1280×720, then raw formats over MJPEG. Leaving it to PipeWire's negotiation would make the result depend on the camera's format order. Ids are `pipewire:<node.name>` or `v4l2:<by-id path>`, both stable across restarts.
- **Sending.** Layers are 180, 360 and 720 lines at 15, 30 and 30 fps, with 150 k, 500 k and 1.5 Mbit/s scaled by pixel count. Each layer keeps the camera's shape. Each track has one thread, and when encoding falls behind, the newest frame wins. Encoders open on first use and reopen, starting with a keyframe, on an error or a bitrate change over 20 %. `Encode` carries a bitrate per layer. When even the lowest layers don't fit, all of them are turned down together in 10 % steps, never below 30 %.
- **The CPU path.** Camera frames become NV12 on the CPU, and the encoders upload them. YUYV and MJPEG's full-range 4:2:2 or 4:2:0 planes convert directly in Rust. A layer exactly half or a quarter of a larger one is the 2×2 mean of that one's picture. Everything else goes through swscale (bilinear, BT.601). Uploading once and scaling on the GPU is V10's zero-copy work. See docs/performance.md for the measurements. Two notes on them:
  - At 30 fps the governor keeps clocks low, so every step costs 5–7× its back-to-back time.
  - VA-API's fixed per-frame overhead makes the small layers cost almost as much to encode as the large one.
- **Receiving and drawing.** Each visible tile gets a receiver thread, and hidden tiles drop their decoders. Pictures become RGBA fitted into the tile's size in physical pixels. Video reaches Flutter through irondash pixel-buffer textures (irondash_texture 0.5.0, irondash_engine_context 0.5.5). They are created and released on Flutter's main thread, and each holds only the newest picture. Textures count pictures presented and drawn, so tests can prove that Flutter drew a camera.
- **In the app.** The preview is mirrored, the sent video is not, and tiles crop to their shape. A camera that can't start turns the toggle back off and says why; "Camera limit reached in this channel" is the plan's own wording. A camera that stops by itself, or that the node stops, turns the toggle off. Leaving voice turns the camera off.
- **Tests and checks.** A virtual PipeWire camera (YUYV or MJPEG at 720p30) stands in for a webcam in tests, in the measurement examples (`camera_load`, `codec_costs`) and in the load check. Two scripts run the real app against a real server: `app/tool/check_v5.sh` (one voicebot camera) and `app/tool/check_v5_load.sh`, which is the acceptance check: nine cameras, run as a profile build. The real webcam and the V4L2 path aren't covered by tests.
- **What the load check found.** Three fixes came out of it:
  - The node forwarded each receiver its copy of a stream starting at the publisher's extended sequence number, rollovers included, for video and audio alike. SRTP takes a new stream's first packet to have no rollovers, so someone who joined after a publisher's sequence numbers had wrapped got nothing from that stream: no video, and from about 20 minutes into a call, no voice either. Copies now keep the low 16 bits and start their rollover count at 0.
  - A receiver waiting for a keyframe now asks for one (a PLI on its track, at most once a second). Before, it waited for the transport to notice a loss, which never happens when frames arrive before a tile's decoder exists.
  - The voicebot retries for up to 2 minutes when it can't connect or is rate-limited, because bots started together from one address hit the server's per-address limits.
- **The Camera portal asks unsandboxed apps too.** xdg-desktop-portal 1.22 shows its access dialog to the app, and turning the camera on waits for the answer. `OPENCORD_CAMERA_PORTAL=0` makes the app read cameras straight from the PipeWire session, which an unsandboxed app is allowed to do. Unattended checks use it, because nobody is there to answer.
- **Not done in V5.** Three pieces wait for the desktop UI brief's camera section (§17.5): the camera picker, a device list that follows hot-plugging, and a preview before turning the camera on. `chooseCamera` in the repository is the seam the picker will use.

## D30: Screen share (Phase 2 V6, 2026-10-09)

- **Server.** Streams live with voice states, one per user in their channel. `CreateStream` needs SCREENSHARE, a quality within the server cap and, rate-limited 5/min, a source. Going live again replaces the stream and keeps its viewers. `self_stream` is the server's: true while the user has a stream in that channel. Streams end when the streamer leaves, moves, loses SCREENSHARE or sight of the channel, or the cap drops below the stream. Watching needs a voice state in the stream's channel and room under `max_stream_viewers`; a full stream answers STREAM_VIEWER_LIMIT. Ready lists visible streams.
- **Node.** The SFU forwards a screen track only to the stream's viewers (tracked per channel and streamer). Nobody receives until they watch; viewers who leave the channel stop watching.
- **Capture.** Linux screen share comes through the ScreenCast portal (ashpd 0.13). The portal's session lives as long as the share. Restore tokens are kept for "Share the same screen again". The PipeWire consumer reads BGRx/BGRA/RGBx/RGBA, any size (2..8192), up to the share's frame rate, without modifiers (shared memory; DMA-BUF import is V10's zero-copy work). Empty buffers and errors end the capture.
- **Virtual screen.** A virtual PipeWire screen for tests and checks (`examples/virtual_camera -- screen WIDTH HEIGHT FPS`), in the same form as the camera but without the Camera role and with changing colour per frame. It can resize like a window.
- **Sending.** A screen's layers are "h" (the preset size, at the share's frame rate) and, when above 480p, "l" (at most 640×360 at 15 fps). Content mode is "motion" above 30 fps: frame rate first; otherwise "detail": resolution first. The planner cuts by fps, then by size. The sender keeps a last picture and sends it at 1 fps while the source is still.
- **Layer costs.** Both the node's receiver allocation and the sender's planner hold a layer on what it actually sent ×1.25 when under its maximum. This was wrong: declared maximum bitrates left layers under-sent, and the estimator settled at about 1.5× what was sent. Going up still costs the maxima. Probing starts from half the target, at the node and in the sender.
- **App.** The share dialog offers 480p·720p·1080p·1440p·Source and 15·30·60 fps as chips, with options above the server's cap disabled. The sound switch is disabled ("Sharing sound comes in a later update.") until V7. While live, the controls menu offers Change source, Change quality and Stop sharing. Stream tiles show "is live" and a Watch button. A watched tile opens in focus mode. The sidebar's LIVE pill joins the channel and watches. This device's own tile shows a 5 fps preview and an eye-count popover of viewers. Stream audio is V7.
- **Tests.** `app/tool/check_v6.sh` runs two checks: the app watches a voicebot's screen share, and the app shares a screen while a voicebot watches it. A virtual screen serves the app's share.
- **What was found.** Two bugs:
  - The video transport looked layers up by their `rid` index (1-based), but the frame carried the layer's position in the track (0-based for screens, which have [l, h]). Every screen main-layer frame was dropped, and every keyframe request for it was wrong. Fixed in `send_video` and `on_keyframe_request`.
  - The echo guard called any microphone tick 25 dB under what was just played "only echo". With headphones, someone else heard at a normal level, and this person's mic at −30 to −45 dBFS, 48–94 % of their speech was marked, so voice activity would not open and gain control skipped it. The guard now checks that the canceller also took at least 10 dB out of the microphone over the same 300 ms window: that many dB means the canceller had something to work on, and what is left is echo, not speech.

## D31: The soundboard (Phase 2 V8, 2026-10-13)

- **Sound files.** Sounds are Ogg Opus, 48 kHz, at most 256 KB and 5 seconds (plan §11.2). The Ogg reader and writer are our own, in `opencord-common` (about 300 lines, shared by the server and the client). They are strict: every page's CRC, one logical stream, consecutive page numbers. The server measures a file by its packets as well as its header, so a file whose header claims one second but whose packets play for six is refused. Uploads are mono, since every sound plays mono.
- **Crates.** Files to make sounds from are decoded by **Symphonia 0.6.1** (MPL-2.0, pure Rust, covering MP3, AAC/M4A, FLAC, Vorbis and WAV). Its features are limited to those formats. Symphonia has no Opus decoder, so Ogg Opus goes through our reader and libopus. Loudness normalization to −18 LUFS uses **ebur128 0.1.10** (MIT, Sebastian Dröge's port of libebur128): it applies a gain only, lowered so the true peak stays ≤ −1 dBTP. The app picks files with **file_selector 1.1.0** (Flutter's own plugin; macOS gets the user-selected read-only entitlement).
- **Default sounds.** The eight default sounds are synthesized by `examples/default_sounds.rs` from oscillators and seeded noise, so their licence is ours. They are built into the client and never uploaded.
- **Server.** Uploads are `POST /media/sounds?name=&emoji=&volume=` with the raw file as the body (no multipart). They answer with the gateway's protobuf `Response` envelope, so errors keep their codes. Every play is checked again on the server (plan §11.5): soundboard on, in that voice channel, not deafened, server-muted or suppressed, `USE_SOUNDBOARD`, the sound itself, and for external and default sounds their setting and permission. Then a per-user cooldown and at most three sounds at once per channel. `VoiceChannelEffect` goes only to the sessions connected to that channel, the player included, and carries the sound's length for the ring. External sounds are kept by hash, 50 MB at most, the least recently played going first.
- **Client.** The core speaks HTTPS to the media endpoints with a small HTTP/1.1 client over the server's pinned certificate, instead of adding reqwest or hyper for a few requests of at most 256 KB. Media tokens stay in Rust and are renewed once on a 401. One cache serves every server: `<app data>/sounds/<sha256>.opus`, 200 MB, least recently played first, checked against its hash when written and read. Every sound is fetched in the background on Ready and when it is added, two at a time per server, so a sound plays at once when someone plays it. Sounds play through the engine's playback mix like voices, so the echo canceller hears them and the master volume and limiter apply. They are not played when this device is deafened or has muted the player, the player's soundboard, or all soundboards. Previews play only here: in the call's engine, else the mic test's, else a speaker-only engine closed 500 ms after the last preview.
- **App.** The picker opens from the voice view's control bar, the voice panel and Ctrl/⌘+Shift+B. Its sections are search, Favorites, This server, each other connected server (their sounds play here as external) and Default. Sounds that can't be played here are dimmed, with the reason as a tooltip, and a bar shows the cooldown. Whoever plays a sound gets a dashed ring outside their avatar or tile for the sound's length. It sits outside the solid speaking ring, so both show together, and it stands still with Reduce motion. Server settings → Soundboard holds the settings (Manage server) and the sounds (Manage soundboard). The trim editor takes up to 5 seconds of a file, with preview, name, emoji, volume and normalization (on by default). User settings → Sounds has the soundboard volume and "Mute all soundboards".
- **Not done.** "Mute their soundboard" works in the core and the repositories, but has no menu item: the per-user voice menu (UI plan §17.3) does not exist yet. The join and leave sounds of UI plan §17.5 come with V9.
- **Tests.** `app/tool/check_v8.sh` runs the real app against a real server. The owner bot uploads a sound made from a WAV file, and another bot plays a default sound every few seconds. The app shows that bot's ring and plays the uploaded sound from the picker, and the bot receives it. The voicebot test `tests/soundboard.rs` has three listeners take a played sound through the cache-and-decode path; they were ready 16–19 ms after the play, within 3 ms of each other (target: 150 ms). A mix test checks that someone speaking during a sound is heard together with it.

## D32: The Linux bundle carries its FFmpeg (2026-10-13)

- **Why.** CI built the Linux bundle against Ubuntu's FFmpeg and shipped it without FFmpeg, so it started only where the system had that exact major (libavutil.so.58 for a 24.04 build). Ubuntu 25.04 and up, Arch and Fedora have other majors, and the app failed before showing a window.
- **What ships.** `app/tool/build_ffmpeg.sh` builds FFmpeg 9.0.2 (checksum-pinned, LGPL) as shared libraries with only what D29 uses: the H.264 and MJPEG decoders, VA-API H.264 decoding and encoding, NVENC, the MJPEG encoder and swscale. NVENC's headers come from nv-codec-headers 13.0.19.0; NVIDIA's libraries still load at run time. The libraries link only to libva, libva-drm, libm and libc. libva stays the system's: its drivers come with the system and must match it, and libva.so.2 is the same on every current distribution.
- **How it is found.** CI copies the libraries into `bundle/lib` and sets libopencord_core.so's RUNPATH to `$ORIGIN`, so the bundle's FFmpeg is used even when the system has another of the same major. A check starts nothing but `ldd` on the executable without the build's paths and fails the job if anything is missing or FFmpeg would come from the system. Both CI jobs build and test against this FFmpeg (cached by the script's hash), not the runner's.
- **glibc.** The Flutter job is pinned to ubuntu-24.04 rather than ubuntu-latest, so the bundle keeps running on glibc 2.39 and newer until the pin is moved on purpose.
- **Development builds** still use the system's FFmpeg. To try the shipped one, run the script and build with `PKG_CONFIG_PATH=<prefix>/lib/pkgconfig`.

## D33: A screen share keeps its main layer (2026-10-14)

- **Why.** In the M8 call across machines (docs/plans/macos.md §7), a screen share watched after it started showed nothing, and once it did show, viewers got the low layer: text could not be read. The empty tile was the core's: a share never answered the node's keyframe requests, because the core passed them only to the camera's sender. The Mac session fixed that (86666ae) and logged the rest for the node.
- **Still screens.** A sender sends an unchanged screen again only once a second (`video/sender.rs`, `KEEPALIVE`), and the node called a layer stopped after 1 s without a packet. A still screen's layers kept stopping and starting: each time, its viewers lost their layer, the node asked the sender for the low layer again, and the viewer climbed back from the bottom. In the node's test of a still screen sent every 1.1 s, a 1080 px viewer got **no picture at all in 10 s**. A screen's layers are now produced until **2.5 s** without a packet (a camera's stay at 1 s); the same viewer gets every picture on the main layer, and the sender is told nothing new.
- **Tile size.** A layer was the tallest no taller than its tile, so a 676 px tile of a 758 px share got the 378 px layer, scaled up. A screen is now never scaled up: it gets the smallest layer at least as tall as its tile, or the tallest when none is. Cameras keep the old rule.
- **Encode.** With the node's requests steady, the core passes them to the screen share's sender as it does to the camera's, so a share encodes only the layers someone watches, within its uplink.
- **Checked on Linux.** The proof is the node's test of a still screen (`a_still_screen_keeps_its_main_layer_and_its_sender_is_left_alone`), which fails without the change. A live run does not show the cycling here: with a release server, a voicebot sharing 720p30 (a moving scene) and one watching at 1080 px, the node's requests settled on the main layer within about 8 s and stayed there, before the change (1617 of 1696 pictures on the main layer) and after it (1506 of 1624). `check_v6.sh` passes. The M8 report of the camera's 720p layer never arriving does not happen here either (1614 of 1800 pictures on it), so it points to the senders' uplinks on the Mac and Windows, not the node.
- **A share starts sharp (same day, after a call between the Mac and Linux).** The share was still blurry at first and sharpened only as the screen moved. The node's log showed why: a viewer got a screen's main layer only once its downlink estimate had room for the layer's declared maximum plus 15 % (2.3 Mbit/s at 720p30, 4.6 at 1080p30), and an estimate grows only with what is sent, which for a still screen is almost nothing. A 1080p share's viewer stayed on the 360p layer for all of its 75 s. Four changes, each found by the node's tests or the voicebot run:
  - A screen's layer is **counted at what it is likely to send**: what it sends ×1.25, at least half its maximum, at most its maximum. Cameras keep their maximum. The downlink is still probed for the maximum, so the estimate grows with a screen that starts moving; probing only for the likely cost left a moving share's viewer switching layers every second or two.
  - A viewer that gets less than it asked for, with no congestion for 30 s, is **trusted to have room**: its estimate starts again at 1.5 times what it needs. str0m smooths its estimate over 3 s, so it grows about a quarter a second; a viewer of a still 1080p share otherwise waited over 6 s. If the room is not there, the loss and the falling estimate move it down. After congestion it probes from where it is, without the jump, which would itself take it up and down again every probe.
  - **A switch already chosen counts as the layer being sent**, at what that layer sends. It waits for the layer's next keyframe, a second away on a still screen, and the estimate settling toward what is sent meanwhile called it off.
  - **The sender trusts its uplink the same way while it shares a screen** (`transport/media.rs`), so a share starts at its full frame rate instead of a third of it.
- **Measured.** In the node's tests, a still 720p and a still 1080p share watched late reach the main layer in 1.1 s (before: 6.3 s and 9.3 s); a viewer capped at 1 Mbit/s tries a 1080p share's main layer once and stays on the low layer. With a release server, a voicebot sharing 720p30 and one watching at 1080 px: **1798 of 1798 pictures on the main layer**, and the node changed what it asked the sender for 4 times, all at the start.

## D34: macOS (M0 to M8, 2026-10-13 to 2026-10-14)

The Mac session's log is `docs/plans/macos.md` §7; this is what it decided. The test Mac is an Intel MacBook Pro (i7-9750H, 2019) on macOS 15.8, so Apple silicon is unmeasured.

- **Bindings.** The objc2 0.3 framework crates (cpal's), delegates through `define_class!`, `objc2::available!` for OS versions. VideoToolbox, Core Media and Core Video are declared by hand (`video/codec/macos/ffi.rs`, about 30 functions): their generated free functions are deprecated aliases that `-D warnings` refuses.
- **Minimum versions.** The app needs 13.0. The camera works on 13 and lists Continuity Camera from 14. Screen sharing needs 14.0 (the system's picker; 13 says it needs 14). A share's sound needs 14.2 (process taps). Naming a shared window's app, so the app doesn't ask "Sound from", needs 15.2 (`includedWindows`).
- **Keybinds.** A listen-only `kCGSessionEventTap` on its own thread, re-enabled when macOS disables it. Printable keys come from the current layout, read on the main queue (HIToolbox aborts off it in an app) and rebuilt on a layout change. With no keybinds the app neither taps nor asks for Input Monitoring, so the prompt comes with the first keybind. Keys a Mac doesn't have (Print, Scroll Lock, Pause, Menu, F21–F24) stay unbound. While any app has Secure Event Input on (a password field), no keybind works.
- **H.264.** VideoToolbox directly, no FFmpeg: hardware first, then its software encoder, each with low-latency rate control first (without it the hardware encoder overshot by 40–46 %). A process's first hardware encoder takes 0.67 s to open, so one is opened and dropped at launch. VideoToolbox conceals a damaged picture, so an access unit that gives no picture counts as an error and asks for a keyframe. Scaling and conversion use pic-scale and yuv (1.3 ms for 1080p NV12 to 640×360 RGBA, against 9.9 ms hand-written).
- **Textures.** irondash 0.5 makes BGRA IOSurfaces and copies bytes unchanged, with rows Metal refuses (it needs 256-byte rows; the app crashed at the first tile). The core stretches each picture to a width that is a multiple of 64 pixels and swaps red and blue (`video/flutter.rs`, macOS only).
- **Camera.** AVFoundation, the shared mode rule over the device's formats, NV12 scaled to fit 1280×720. With no camera chosen, macOS's `systemPreferredCamera` (14+). Turning it on takes 1.1 s, over §8's 1 s: the FaceTime HD camera gives its first frame 0.65 s after starting, and opening it earlier would light it before anyone asked.
- **Screen share.** ScreenCaptureKit through the system's `SCContentSharingPicker`, captured at the content's pixels within the share's budget (capturing the full Retina size cost 82 % of a core at 1080p30; at the budget, 24 %). After a pick Opencord brings itself back in front, 400 ms later, with the deprecated `activateIgnoringOtherApps` (the polite call does nothing). SDR only.
- **Share sound.** A Core Audio process tap in a private aggregate device, everything but Opencord's processes or one app's. An app's processes are found by bundle ID and through the process macOS holds responsible (a private symbol, looked up with `dlsym`): Safari's sound plays from WebKit's processes. Taps work inside the sandbox.
- **Audio devices.** On macOS a device opens at its own rate and the engine resamples: asking for 48 kHz sets the whole system's rate, and once AirPods' microphone opened (16 kHz) their speaker could not be reopened, so calls went silent. A device whose rate changes is silent for about 1.1 s.
- **Entitlements.** The release build has `network.server` (without it the sandbox stopped voice's UDP socket, silently) and differs from Debug only by `cs.allow-jit`. `CODE_SIGN_INJECT_BASE_ENTITLEMENTS = NO` keeps `get-task-allow` out of the release. The release is signed ad hoc and not notarized, so macOS asks for every permission again after each update.
- **Shared fixes found on the Mac.** A share now answers keyframe requests (a share watched late was empty everywhere); leaving voice clears the camera flag; a voice update is sent only when mute or deafen changed (three per join had run into the server's limit, the "slow down" lockout); a camera menu (right-click on Camera, or Voice & audio → Devices) switches at once.
- **The Mac's findings, as the Linux session took them** (2026-10-15):
  - A sign-in refused for coming too often now waits the server's `retry_after_ms` before trying again (it used its own backoff, which can be shorter). A voice rejoin after a new session that is told to slow down tries again after the wait, up to five times; before, it gave up, and the app thought it was in a channel the server had not put it in. The lockout the Mac saw was the three voice updates per join, fixed by the Mac.
  - A device that would not open is tried again, quietly, after 1 s, the wait doubling up to 10 s; before, it stayed silent until the call was rejoined.
  - When a headset's microphone and speaker fall back within 2 s of each other, one notice tells of both; the second used to hide the first.
  - Switching cameras retries for up to 1 s while the camera switched from closes, so choosing the camera already on by its name (after Default) no longer fails as busy.
  - Debug builds optimize `aec3` and `nnnoiseless` like DeepFilterNet's crates: unoptimized, they took 79 % of a throttled Mac's audio thread and the call cut out.
- **Checks.** `app/tool/macos/` holds the Mac's twins of the Linux checks (BlackHole, sox and `afplay` in place of PipeWire's sinks) and `serve_lan.sh` for calls across machines. Every acceptance passed, including AirPods during a call, sleep and wake, Continuity Camera, the "no double audio" test (Talker at −117.8 dBFS in the stream, the music at −12.0) and the call between the Mac, Linux and Windows. Not checked: two monitors with mixed DPI, HDR, macOS 13 and 14.0–14.1 (read only).

## D35: Windows (W0 to W8, 2026-10-13 to 2026-10-14)

The Windows session's log is `docs/plans/windows.md` §7. The test PC is an i7-13620H laptop with an RTX 4070 and Intel UHD, on Windows 10 22H2 (build 19045), so screen share sound was never captured.

- **Bindings.** The `windows` crate 0.62 (cpal's). COM is multithreaded, on Opencord's own threads, never on Flutter's UI thread.
- **Floors.** Windows 10 21H2 for voice, keybinds, camera, video and screen share; build 20348 for a share's sound, read with `RtlGetVersion` (`GetVersionExW` lies without a manifest). Below it the switch reads "Sharing sound needs Windows 11".
- **Keybinds.** One `WH_KEYBOARD_LL` hook on its own thread; `RegisterHotKey` is not used, so Opencord never takes a key away from another app. Keys the keyboard has twice match on the extended-key flag too, so keypad 4 with Num Lock off matches nothing, as on Linux. AltGr's phantom left Ctrl is dropped by its extended flag. Printable keys follow the foreground window's layout, rebuilt every 500 ms on the hook thread. Keys typed into a window run as administrator never reach the hook (UIPI).
- **Codec.** FFmpeg through the shared codec layer, encoders NVENC → AMF → Quick Sync → OpenH264, D3D11VA decoding on the default adapter, and the five LGPL DLLs bundled beside the exe (CI pins BtbN's asset by URL and SHA-256; the build fails without `FFMPEG_DIR`). Media Foundation is openable but left out of the list: it holds 16 pictures before the first comes out (about 533 ms). `profile=high` is set per encoder (`h264_mf` has no `profile` and refused to open). The decoder sets `err_recognition = EXPLODE | BITSTREAM` on Windows only: FFmpeg there concealed damage and never asked for a keyframe. Linux already reports it, so Linux is left as it is.
- **Camera.** Media Foundation's source reader with converters off; the id is the device's symbolic link. Windows' Frame Server shares cameras between apps, so "another app has the camera" doesn't happen.
- **Screen share.** Opencord's own picker (Screens and Windows tabs; GDI thumbnails, so the picker doesn't put a capture border on every window), Windows.Graphics.Capture polled on its own thread, the CPU path for conversion (1080p30 at 14.8 % of a core in a debug build). The `Closed` event never fires without a message pump, so the window's handle is checked every 250 ms. A window's sound is its process (for UWP apps, the hosted CoreWindow's).
- **Share sound.** WASAPI process loopback, excluding Opencord's process tree, or including one process's; the app list leaves out any `opencord` and `audiodg`. Written and unit-tested, never run: process loopback needs build 20348 and the PC is 19045.
- **Tests.** `app/tool/windows/` holds the PowerShell twins of the checks (VB-CABLE, ffmpeg/ffplay, `SendInput`); they exit 0 passed, 1 failed, 2 proved nothing. `cargo test` runs with `--test-threads=1` there: a GeForce card caps NVENC sessions, and a test suite opening dozens falls through to Quick Sync and misses the sender's timings.
- **Shared code findings, and what the Linux session did with them** (2026-10-15): `v5_test.dart` waits for both counters it asserts; CI gains Rust jobs on Windows and macOS. Still open: Print Screen can't be recorded as a keybind on Windows, though it works once set. Windows sends a focused window only its key-up, and Flutter's Windows embedder drops an up whose down it never saw (`keyboard_key_embedder_handler.cc`), so no Dart change can see it; the runner's message loop (`app/windows/runner/main.cpp`) would have to send the view a key-down before Print Screen's up, checked on Windows. Also open: the 120 MB of FFmpeg DLLs (a minimal build like D32's should take the download under 100 MB; it has to be built and checked on Windows); no software H.264 encoder on Windows (fetch Cisco's OpenH264, accept Media Foundation's half second, or require a GPU encoder: the user's decision); the echo canceller not demonstrated (the laptop's speakers reach its microphone 38 dB down); the screen share sound capture on Windows 11; code signing and an installer.
- **The top layer on Windows.** On this PC a camera never reached its top layer and screen shares barely arrived, in the app and in the shared `tests/video.rs`, while Linux reaches both. The screen share half was the missing keyframe answer and D33's layer cycling, both fixed since. The camera half is not explained: the node and the senders are the same code as on Linux, so the likely difference is pacing or timers on Windows (its default timer resolution is 15.6 ms), which the bandwidth estimate reads as queueing. It needs a measurement on Windows.

## D36: Basic Discord features (Phase 2 V9, 2026-10-15)

The desktop UI plan's §17.5 never arrived, so where Phase 2 §12 says only what, the voice view follows Discord. Opt-in stream watching (V6), channel limits and bitrates (V0), the priority speaker, the "You're muted" reminder and the mic test (V3) and server moderation (V0, V1) were already in place.

- **Chat in voice channels.** The server takes messages and typing in voice channels with View channel and Send messages, unless the channel's "Text chat in this channel" (§5.3) is off. The voice header's Chat button, with the unread count while closed, opens the chat as a 380 px column beside the tiles, in the member list's place. When fewer than 600 px would be left for the tiles, the chat shows on its own. The voice row shows unread messages and mentions like a text row. History from before someone joined the server counts as read, as for text channels.
- **Video-only tiles.** "Hide non-video participants" in the header leaves camera and screen tiles; with none, the view says "No one has video on". Both choices are remembered.
- **Voice sounds.** Ten short cues, synthesized by `examples/cue_sounds.rs` (CC0, peaking at −6 dBFS), for joining or moving, leaving, someone else coming or going, mute, deafen, and a stream starting or ending in the channel. They play on this device only, through the soundboard's preview path, so the leave sound plays after the call's engine has gone. Deafened, only one's own changes make a sound. User settings → Sounds has a switch for each of five groups, all on.
- **AFK.** Someone is moved to the AFK channel (and suppressed there) once they have neither spoken nor touched their keyboard or mouse for the server's timeout. Someone streaming is never moved. The node tells the main server when someone speaks, at most every 10 s per session (`SpeakingActivity`, which gained the channel). The app reports its device's idle time once it passes a minute, every minute while away, and when input returns (`UpdateVoiceActivity`); the server checks every 5 s. Idle time comes from ext-idle-notify-v1 on Wayland (KDE, Hyprland, Sway; `wayland-client` 0.31 and `wayland-protocols` 0.32 in the Linux section), GNOME's Mutter IdleMonitor, X11's screen saver extension, macOS's `CGEventSourceSecondsSinceLastEventType` and Windows' `GetLastInputInfo`. A system that cannot say is never reported idle, so its user is never moved. On this KDE Wayland session an untouched desktop read 65.3 s idle after 65 s.
- **Connection diagnostics.** Once a second the voice connection reports the round trip to the node (RTCP, else the ICE pair's), packet loss each way, bitrates, the voice node, and this device's camera and screen encoders. Jitter is the client's own RFC 3550 estimate over each voice it receives; str0m's ingress jitter read zero for these streams. The voice panel shows three, two or one bars (one at 300 ms or 8 % loss, two at 150 ms or 2 %), and the bars open the numbers. The node test puts 20 % loss, 50 ms each way and ±15 ms of jitter on a client: the reported loss is within 3 points of what arrived, the round trip 95–116 ms, the jitter 9–10 ms.
- **After sleep.** A 2 s timer is compared with the wall clock, which runs on while the computer sleeps: a jump of more than 5 s sends every server connection a heartbeat that must be answered within 5 s, or it reconnects and resumes, or rejoins its voice channel. A relay test that freezes the connection like a sleeping machine has the client back, still in its channel, in about a second; without the check, not within 10 s. Voice media then recovers on its own timeouts, as the Mac and Windows sleep checks saw.
- **Attenuation.** Voice & audio → Attenuation lowers other apps by 0–100 % (off by default) when others speak, when this device speaks, or both, and brings them back a second after the last voice and when the call ends. Linux scales other apps' PipeWire playback streams' channel volumes (not Opencord's own process tree), lowers streams that start meanwhile, and puts back exactly what was there: against this session's PipeWire a stream went 1.0 → 0.3 → 1.0, and back to 1.0 when the attenuator was dropped while lowered. Windows lowers other apps' audio sessions. macOS has no public way, so the setting is hidden there. Known risk: WirePlumber remembers a stream's volume, so an app whose stream ends while lowered starts lowered next time.
- **Not built or run here.** The macOS and Windows idle readers and the Windows attenuator are type-checked and clippy-clean for those targets in a scratch crate with the same dependencies, but have not run.
- **Tests.** `app/tool/check_v9.sh` runs the real app against a real server: a voicebot in General writes in its chat (`--say`, `--say-every`); the app sees the unread row, reads the message in the chat column, hides and shows the tiles, and gets real statistics (on localhost: 0 ms, 0.8 ms jitter, 72 kbit/s in).

## D37: Hardening, performance and packaging (Phase 2 V10, 2026-10-15)

- **Every §16.1 target is measured** (`docs/performance.md`, "Every target"). The new tools: `opencord-voice-load` fills a server with bots for the node's target; `app/tool/check_v10_load.sh` measures the app in a profile build (a call of ten with four cameras and a stream, or watching 1080p60); `check_screen_latency.sh` reads the time stamped into each frame of the virtual screen (`video/stamp.rs`) after a viewer decodes it; `check_voice_latency.sh` loops the app's speaker into its microphone through a PipeWire null sink while a bot times its beeps (`--ping`).
- **Memory.** The call of ten came down from 371–380 MB to a steady 344 on this hybrid laptop (298 with only the integrated GPU's EGL, which is what the reference laptop loads; NVIDIA's EGL libraries are 30 MB of it):
  - the core sets glibc's mmap threshold to a fixed 128 KB, so large allocations, decoded and converted pictures above all, go back to the system when freed instead of staying resident in heap arenas;
  - DeepFilterNet gives back what loading its networks leaves behind (`malloc_trim`, and `madvise` on the embedded model files, which are not read again): 37 MB to 23;
  - a layer nobody has wanted for 10 s closes its encoder; wanted again, a new one starts with the keyframe it needs anyway.
  Fewer malloc arenas and smaller VA-API surface pools changed nothing and were left out: VA surfaces live in GPU memory.
- **Zero-copy textures, Linux, opt-in** (`OPENCORD_ZERO_COPY=1`). A tile's texture becomes a GL texture. A hardware-decoded picture is exported from VA-API as DMA-BUF planes (`vaExportSurfaceHandle`; FFmpeg's own mapping needs libdrm, which the bundled FFmpeg leaves out), imported as two EGL images in a GL context of the core's own that shares textures with Flutter's (made on the raster thread the first time a tile is drawn, matching Flutter's API and config), and drawn through a BT.601 shader into one of the tile's three RGBA textures at the tile's size. Flutter's raster thread waits on the worker's fence; its own GL state is left alone but for one binding. Pictures in memory are uploaded the same way, so a GPU that cannot import the decoder's buffers (another vendor's EGL) falls back without a second texture kind. Decoders keep 4 extra surfaces for pictures still being drawn. Watching 1080p60 dropped from 87 % of a core to 38–52 %. It stays off by default until someone has looked at it on screen; macOS (IOSurface) and Windows (DXGI shared handles) are their machines' work.
- **Deployment.** `deploy/` has a two-stage Dockerfile (177 MB on `debian:trixie-slim`, the server and the voice node, data in `/data`), a compose file publishing 7710/tcp and 7711/udp with a data volume and `OPENCORD_PUBLIC_HOST` in `.env`, and a systemd unit with its state in `/var/lib/opencord`. Two voicebots on the host called through the container. The server logs without colours when not on a terminal.
- **`[gateway] identify_per_minute`** (`OPENCORD_IDENTIFY_PER_MINUTE`, default 5) makes the sign-ins allowed per address a setting: LAN parties share one address, and the checks' servers take 100.
- **Docs.** `docs/voice.md` (how voice fits together, ports, hosting behind a home router, troubleshooting) and `docs/packaging.md` (each desktop build: what it carries, needs, and what is still the owner's decision: signing, notarization, the hardened runtime, installers, a smaller Windows FFmpeg, a Windows software encoder; the Flatpak permissions a Flatpak will need).
- **Not done here.** `docker compose up` reached from another machine; a fresh macOS install's permission prompts; zero-copy looked at on screen, and on macOS and Windows.

## D38: Lavalink plays into Opencord through an audio bridge (Phase 3 L0, 2026-10-15)

- **The spike passed.** An unmodified Lavalink 4.2.2 (Koe 3.0.0-pre6 inside) played into a voice channel through the bridge on the main TLS port, and a voicebot there heard 60 s of it: 3002 packets, none lost, every one plain Opus, at most 26 ms between two, left and right channels apart by 75 dB. Halfway, the bridge dropped Lavalink's WebSocket as a missed heartbeat would; Lavalink resumed it (op 7, then 9) and not a frame was lost. The test is `crates/opencord-voicebot/tests/lavalink.rs`, ignored unless a Lavalink is given; `docs/music.md` shows how to run it.
- **What Lavalink does, read from Koe's source and seen in its log:**
  - It speaks voice gateway v8 to `wss://<endpoint>/?v=8`, port included, and checks the certificate and host name against Java's trust store. An imported self-signed certificate works; so would any trusted one. Let's Encrypt's test environment is no different from self-signed here: its roots are not in Java's store either.
  - It offers DAVE (`max_dave_protocol_version: 1`). The Session Description's `dave_protocol_version: 0` keeps its DAVE session in passthrough, and the Opus arrives plain inside the transport encryption.
  - It takes the first mode in Ready's list it knows: the bridge lists `aead_aes256_gcm_rtpsize` first and `aead_xchacha20_poly1305_rtpsize` second, and Lavalink picked AES-GCM. The nonce is a 4-byte counter after the tag, copied into the start of an otherwise zero nonce; the authenticated header is the 12-byte RTP header.
  - IP discovery wants exactly 74 bytes back, the address as a NUL-terminated string at offset 8 and the port big-endian at 72.
  - **Its audio does not come from the port it discovered from**: the native sender (udp-queue) has sockets of its own (discovery from 55886, audio from 53483). So the bridge checks only the address, as well as the SSRC and the key; plan §13.4's "only from the address that completed IP discovery" is the IP.
  - It heartbeats at the interval Hello gives (13.75 s) and sends `seq_ack`; on Resume it does not redo IP discovery or Select Protocol.
- **Rulings.**
  - DAVE's binary frames and opcodes 21–31 are ignored, not answered with a close: at version 0 Lavalink sends none, and closing would cost the music if one came.
  - Ready's `ip` is the address Lavalink can reach: loopback for a node on this machine, this machine's address on the route to a node on a private network, and the public address for the rest, so a node in the same house or Docker network never depends on hairpin NAT.
  - A channel's music is its own stream in the SFU, on an SSRC unique in the channel, announced on the voice gateway with `MusicStream` (sequenced, and in `Ready` for whoever joins mid-song) and muted per listener with `MediaSinkWants.music_muted`. Its numbering runs on across Lavalink connections, so a failover is a gap, not a new stream. Clients play it as a stereo stream, like a screen share's sound, under source key 0 (`MUSIC_SOURCE`), which no user id is.
  - Lavalink's Opus payload type (120) is rewritten to Opencord's (111).
  - The stream starts with a channel's first frame and stops when the main server says (`NodeCommand::StopMusic`, `CancelMusicBridge` for external nodes); a Lavalink connection ending does not end it, since a failover follows.
- **Not done here.** A public Lavalink node: it needs a domain with a trusted certificate and UDP 7712 reachable from it, which arrive with D2–D3 and are part of L3's acceptance.

## D39: Public address and ports (Phase 3 D1, 2026-10-15)

- **What the owner gets.** Server settings → Domains & network, the owner's alone: the public address (detected or set by hand), every port with its state and the router's mapping, UPnP, the main port, and the advanced settings (DNS resolvers for checks, Let's Encrypt's test environment, the local listener's port, trusted proxies). The page watches the network while it is open (`WatchNetwork`); the server sends `NetworkStatus` only to sessions watching, and the whole description each time, since it is small and seldom changes.
- **Seen on this network.** ipify answered 80.115.244.136, and the router answered UPnP (192.168.0.1) with the same public address. Its mapping table is full, so it refused the test mapping ("501: Port mapping table id full"): the page shows that reason against each port. UPnP's map-and-remove acceptance needs a router with room; the test is `network::upnp::tests::a_real_router_maps_and_unmaps`, ignored by default.
- **Rulings.**
  - Settings live in `server_meta` under `network.`; no new tables yet. The last detected address is kept, so a restart without the internet still knows it.
  - The address lookup is a setting in `opencord.toml` (`[network] ip_lookup_v4`, `ip_lookup_v6`), ipify by default: tests point it at a local stand-in, and an owner who never wants ipify contacted empties it before the first start.
  - The main port chosen in the app is taken at the next start; if it cannot be had, the server falls back to the configured port and logs why, so a bad choice never locks the owner out. Port 0 in the config (tests) keeps asking for a free port.
  - Port states come from trying to bind each port for a moment: Free, In use, or Not allowed (below 1024 without the capability).
  - Behind the local listener or a trusted proxy, the client's address is the last `X-Forwarded-For` entry that is not itself a trusted proxy, so an address a client wrote itself into the header is never believed. The voice gateway, the voice nodes' control channel and the music bridge all use it.
  - `NetworkStatus` and `NetworkInfo` are boxed in the protocol, as `Ready` is: they are far larger than the other events and responses.
  - Server notices (`ServerNotice`) show as the app's toast. The public address changing is the first one.
- **Not done here.** The page's Domains section and DNS providers arrive with D2–D4. Dynamic DNS for free subdomains and re-checking domains when the address changes hook into `Network::public_ips`, which already announces each change.

## D40: Domains (Phase 3 D2, 2026-10-15)

- **What the owner gets.** Domains on the Domains & network page: add one (own, or behind a reverse proxy; free subdomains and tunnels get their flows in D4), see the DNS records to create, check it, turn it on or off, make it the primary, read its last 10 checks, delete it. Invites go through the primary, or a chosen domain when more than one is active, with an `opencord://` link and a web page. Members' apps learn the domains and use them.
- **DNS checks are real DNS.** `domains/dns.rs` builds queries with hickory-proto and sends them over UDP (TCP after a truncated answer) to the domain's own name servers, recursion off, found through the owner's resolvers. Against the internet's DNS through 1.1.1.1 it resolved github.com, www.github.com (a CNAME into another zone), api.ipify.org and duckdns.org the same as the resolvers did (ignored test `real_names_resolve_at_their_name_servers`). A fake DNS server (`domains/dns/fake.rs`) covers matching, mismatching, CNAME chains and loops, truncation and TCP, missing names and silence.
- **Rulings.**
  - The zone of a name is found by asking a resolver for its SOA, walking up from the name: a CNAME owner is never a zone apex, and asking for its SOA makes resolvers follow the CNAME into the target's zone.
  - A check passes when the records include the server's address and nothing else: a second address (round robin to an old host) or a stale AAAA is "Partly correct", since some people would land elsewhere. No AAAA while the server has IPv6 passes, with a note.
  - Cloudflare's proxy is recognised by its published ranges (cloudflare.com/ips, kept in `check.rs`).
  - The first domain to pass becomes active and the primary by itself; the owner never has to find the switch for the common case.
  - The client keeps the server's key (the address it was added with) as its identity and saves the domains beside it, rather than re-keying servers: every part of the app keys by it. Connections try the domains first, then that address, then the IP address; only that address can raise the fingerprint prompt or a mismatch, others that fail are skipped, and an address answering with another `server_id` is skipped as "another server now".
  - **Host binding.** Behind a tunnel or proxy the client sees the proxy's certificate, which the server never holds, so the `Identify` signature over the certificate cannot match. A client that verified the server by an authority under a name also signs the challenge with `host_binding(name)` in place of the certificate; the server accepts it for names it answers to (its `public_host`, active domains, and domains turned off within 7 days). An authority vouches for the name, so the original goal holds: a server cannot replay a user's answer to another server.
  - Authorities on clients are the operating system's (rustls-platform-verifier), plus a PEM file in `OPENCORD_EXTRA_CA_CERTS` for private CAs; tests use a test CA (`opencord-server::testing::TestCa`).
  - Invite links through a domain without an authority's certificate still carry the fingerprint; the plan's fingerprint-free links apply once a domain has one (proxies now, Let's Encrypt in D3).
  - The local listener runs while a tunnel or reverse-proxy domain exists.
  - Turning a domain off keeps it bindable for 7 days; serving its certificate stops after that in D3.
- **Not done here.** Certificates (D3): until then own domains are checked by the pinned fingerprint, like the IP address. Free subdomains and tunnels (D4). The users-in-24-hours count lives in memory and starts over with the server.

## D41: Certificates (Phase 3 D3, 2026-10-15)

- **What the owner gets.** Let's Encrypt certificates for verified own domains, from the domain's menu, with the step shown on the row; uploading one's own certificate; DNS providers (Cloudflare, DuckDNS, deSEC) for the DNS challenge; renewal by itself, and notices before expiry. Apps then check those domains by the authority, with no fingerprint.
- **Libraries.** instant-acme 0.8 (ring, hyper-rustls) for ACME, with its ARI support for renewal timing; rcgen for the TLS-ALPN-01 certificate (`acmeIdentifier` extension); x509-parser for reading chains; chacha20poly1305 for sealing secrets. No ACME code of our own beyond the flow.
- **Tested against Pebble, Let's Encrypt's test CA.** `crates/opencord-server/tests/certificates.rs` gets a real certificate through each challenge (HTTP-01, TLS-ALPN-01, and DNS-01 through a fake Cloudflare API whose records land in Pebble's name server), checks that a TLS client trusting only Pebble's root accepts the served certificate for the name without a restart, and renews in place. Docker Desktop runs containers in a VM whose "host network" is not this machine's loopback, so the tests copy Pebble's and pebble-challtestsrv's binaries out of their images into `target/pebble` once and run them as local processes. They are `#[ignore]`d (they need Docker and the images) and run with `cargo test -p opencord-server --test certificates -- --ignored`; all pass.
- **Rulings.**
  - Challenge choice is automatic and in this order: DNS-01 when a connected provider holds the name (no ports needed), TLS-ALPN-01 when the main port is 443 (the TLS listener answers `acme-tls/1` with the challenge certificate, so nothing else has to listen), HTTP-01 on port 80 otherwise, with a listener that exists only while the CA checks and answers nothing but its token.
  - Serving is by SNI through one `ResolvesServerCert` (`certs/store.rs`) swapped in place; IP and unknown names get the self-signed certificate, so the fingerprint path never changes.
  - A domain's certificate is served while the domain is active or was turned off less than 7 days ago, matching host binding; test-environment certificates are kept for the owner to see but never served, and never count as trusted.
  - Secrets (private keys, the ACME account, provider tokens) are sealed with XChaCha20-Poly1305 under a 32-byte `secrets.key` (0600) in the data directory, with the purpose as associated data so a sealed value cannot be moved to another row. Lavalink passwords will use the same.
  - Renewal is looked at hourly rather than daily: the look is cheap, and an hour bounds how late an ARI window or a retry is noticed. Renewal runs at two thirds of the lifetime or when ARI's window opens.
  - Failure limits are ours (5 per hour per domain) on top of the CA's: a `rateLimited` problem waits an hour. Errors are rewritten in plain words per challenge, from the ACME problem type.
  - The ACME email is optional; the account is made once per directory, so turning the test environment on asks for agreement again.
- **Also fixed.** A D2 core test checked two domains at once and expected the first to become primary; under a loaded machine either could finish first. It now checks them one after the other.
- **Not done here.** Free subdomains, dynamic DNS and tunnels (D4).

## D42: Free subdomains, tunnels and reverse proxies (Phase 3 D4, 2026-10-15)

- **What the owner gets.** "Get a free domain" (DuckDNS or deSEC): paste the token and a name, and the server points it here, keeps it pointed here, checks it and certifies it by itself. "Use a tunnel": a quick tunnel (random trycloudflare.com address, limits shown), the owner's named tunnel by token and hostname, or cloudflared run elsewhere; the server runs and restarts cloudflared, shows its state, restarts and output, and can download it. Reverse-proxy domains show Caddy and Nginx configuration. Where a tunnel or proxy is involved, the page says that UDP (voice, video, music audio) still has to reach the server directly.
- **Verified for real.** `crates/opencord-core/tests/domains.rs` `a_quick_tunnel_is_a_domain_people_join_through` (ignored; needs cloudflared and the internet) runs a real quick tunnel from the server: the domain checks out through Cloudflare, an invite through it carries no fingerprint, and a second client joins through it, its certificate checked by the system's authorities. It passed in 13 to 27 seconds on each of four runs once checks retried long enough. Free subdomains are tested against a fake DuckDNS that writes into the fake name server and Pebble's, so the automatic certificate is issued by Pebble through DNS-01 (`tests/free_subdomains.rs`). Tunnel supervision is tested with a fake cloudflared script: quick address, named token kept out of arguments and output, a crash and its restart, Stop, and the tunnel starting again with the server. Downloading cloudflared is tested against a fake release with a right and a wrong checksum. DuckDNS, deSEC and named tunnels against the real services need accounts: owed as a manual check.
- **Rulings.**
  - A free subdomain's token is saved only after the provider accepted the update with it: DuckDNS has no way to check a token otherwise, and a wrong one should not stay connected.
  - New names are checked until they pass: every 5 seconds for a minute, then every 30 seconds, for 10 minutes at most (`domains::check_until_verified`). A quick tunnel's name took over a minute to reach this machine's resolver in one run; 60 seconds of checks was not enough.
  - The named tunnel's token goes to cloudflared in `TUNNEL_TOKEN`, not on its command line, so process lists do not show it; output lines have it replaced by `••••`. cloudflared runs with `HOME` in the data directory so the machine owner's `~/.cloudflared` configuration does not change the tunnel.
  - A quick tunnel's domain is the server's own: replaced when a new address comes, removed with the tunnel even when it is the primary (the primary moves to another active domain). A named tunnel's domain belongs to the owner and stays.
  - Download: the release's SHA-256 from Cloudflare's release notes must match the file, and GitHub's digest of it when given. Only on Linux, where the server runs.
  - The tunnel and the local listener: the listener runs while a tunnel is meant to run, as well as while a tunnel or proxy domain exists.
  - A tunnel whose local listener cannot start (its port taken) is `FAILED` with the reason, not left starting. Shutting the server down now closes the local listener and waits for its port to be free; before, it stayed open until the process ended, which a restart test found.
- **Not done here.** A TURN relay for servers without reachable UDP (after Phase 3). Detecting a cloudflared the owner runs by itself is the HTTP check of its domain.

## D43: Lavalink nodes (Phase 3 L1, 2026-10-15)

- **What admins get.** Server settings → Music (Manage server) lists the Lavalink nodes: up to 20, each with its address, TLS, password (sealed, never shown again), priority, player cap and strict address check. Each row shows its health in words (Connecting, Healthy, Degraded with the reason, Unavailable with the reason, Draining, Disabled), the Lavalink version, sources, channels, players, CPU and ping. Above them is the bridge checklist (where Lavalink connects and whether its certificate is trusted, where it sends audio, the router mapping), and below them the last 50 node events. Rows have Test, the Enabled switch, Edit, Drain or Undrain, and Delete. When every node is down there is a banner, and members who manage the server get one notice.
- **Verified for real.** `crates/opencord-server/tests/lavalink.rs` `a_real_lavalink_passes_the_connection_test` (ignored; needs a Lavalink, see `docs/music.md`) passed against Lavalink 4.2.2: the node is Healthy with its version and sources, all three test steps pass, and its temporary player signed in at the bridge and did IP discovery. A wrong password reads "Lavalink refused the password" (Lavalink answers 403). The same Lavalink run without the trust store failed the bridge step with "Lavalink couldn't verify Opencord's certificate…", seen from its TLS alert. Everything else runs against a fake Lavalink (`tests/common/fake_lavalink.rs`): REST, WebSocket sessions with resuming, stats, players, and Koe's sign-in and IP discovery at the bridge.
- **Rulings.**
  - Health is a pure function of what was observed (`lavalink/health.rs`), judged every second: a closed WebSocket counts after 3 s; two failed REST calls in a row are Unavailable, one is Degraded; CPU above 85 %, more than 2 % of frames missing or empty, REST over 1 s, or 3 track-loading failures in 5 minutes are Degraded. A node back from Unavailable gets no new channels for 30 s.
  - Best node: the penalty Lavalink clients use (players, CPU, deficit and nulled frames) plus 50 per recent loading failure and ping ÷ 10, within the best priority tier that has a candidate. Degraded nodes count only when no healthy one fits. Ties go to fewer Opencord channels.
  - The REST ping is the `GET /v4/stats` round trip every 15 s. The WebSocket is pinged every 5 s and dropped after 12 s without a pong. Reconnects back off from 1 s to 30 s, sooner when the address or password changes.
  - Each node's Lavalink session is resumable (60 s) and saved in `lavalink_nodes.resume_session`, so a dropped connection or an Opencord restart resumes it. The internal Lavalink user id is a snowflake in `server_meta`.
  - Failure texts come from the error chain: name not found, nothing listening, timeout, TLS against a plain port ("turn TLS off"), plain against TLS ("turn TLS on"), an untrusted node certificate, a non-Lavalink answer. They name the address, never the password; `Endpoint`'s Debug output leaves the password out.
  - The connection test's bridge step expects a single-use player at the bridge (with the node's addresses when strict), points a temporary Lavalink player at the bridge domain, and waits up to 10 s for the bridge's new `Discovered` event (or `Refused`: signed in from an address the strict check forbids), or for Lavalink's `WebSocketClosedEvent`. The main TLS listener records handshakes a peer aborted with a certificate alert (`handshakes.rs`), which is how an untrusted certificate is told apart from an unreachable address. The player is destroyed afterwards.
  - Node changes reach only sessions with the page open (`FetchLavalinkNodes { watch }`), like Domains & network; test steps go only to the session that asked.
- **Not done here.** Channels on nodes, failover and "Move players now" (L2), and the bridge address from `[music] bridge_domain` used for real players (L3; the test uses it already). Done in L2: see D44.

## D44: Music sessions and permissions (Phase 3 L2, 2026-10-15)

- **What members get.** Each voice channel's music runs on the server: a queue of up to 500 with history, repeat and shuffle, play next and play now, relative moves, previous, seek, pause, stop. Four permissions (Add music and Control playback on for `@everyone`, Manage the queue and Manage music off; migration 0006 turns the first two on for existing servers), the settings of plan §16.2, and blocking members from music from their member menu. Admins get "Move players now" on a node. The music panel itself is L5; the app shows the settings, blocked members and node actions now.
- **Verified for real.** `tests/music.rs` `two_real_lavalinks_fail_over` (ignored; needs two Lavalinks, see `docs/music.md`) passed against two Lavalink 4.2.2 nodes: a track played on the first, turning it off moved the channel to the second in about 0.1 s at 3.2 s into the track, and after an Opencord restart the music carried on there. The other seven tests run against the fake Lavalink, which now loads tracks, keeps players and sends their events.
- **Rulings.**
  - One actor per channel (`music/session.rs`) applies requests, Lavalink's events and timers in order. Requests reach it with the member's permissions in that channel; the handler checks music is set up, the member isn't blocked, is connected (or holds Manage music) and holds the action's permission, then the actor checks what depends on the queue (own track, others' items, limits).
  - Search results are kept per member for 15 minutes behind 22-character random ids (at most 20 000 at once); enqueueing takes only those ids, so a client can never send an encoded track. Tracks from Lavalink's `http` and `local` sources are dropped from results and refused unless `[music] allow_unsafe_sources`, which also lets a query that is a file path load through the `local` source (needed for the real-Lavalink tests).
  - Links: `https` only, no user name, to a known host of an enabled source; IP addresses, `localhost`, `.local`, `.internal`, `.lan` and single-label names are refused.
  - Lavalink's `playerUpdate` positions are ignored for 1.5 s after a track starts or seeks: a real Lavalink still reports the old position in the first updates after a seek (seen as 0 right after starting at 3.2 s). Subscribers hear of a position only when it drifts more than 1 s from what they can compute.
  - Three failures in a row (load failed, exception, stuck) pause the music with a notice instead of skipping through the whole queue. Lavalink voice-connection losses that aren't the bridge's own (4006, 4014) get a new voice state, at most 3 times a minute.
  - Failover: on Unavailable or Disabled, each channel on the node relocates from its estimated position to the best other node with the track's source, 5 at a time. "Move players now" and deleting a node use the same path. A node that is up again has every player no channel wants destroyed (`GET /v4/sessions/{id}/players`).
  - Restarts: sessions are saved at most every 2 s and on shutdown (before the bridge and database close), with the node they were on. On start, saved sessions are restored before the nodes connect; each waits up to 15 s for its old node, asks it whether its player still plays the same item, and if so only gives it a new voice state at the bridge. Members get 60 s to reconnect before the channel counts as empty.
  - The bridge domain moved into the music settings (seeded from `[music] bridge_domain`); only the owner, who sees the domains, can pick another.
  - Node events that arrive after the reply that already listed them are not added twice (`LavalinkNodes.withEvent`).
- **Not done here.** The music stream in the SFU, `MusicStream` and the audio watchdog (L3); artwork (L4); the music panel (L5); commands (L6); `MusicUpdate` deltas, vote skip voting and the server clock offset (L7). Vote skip's setting is kept for L7.

## D45: The bridge in production (Phase 3 L3, 2026-10-15)

- **What admins get.** Music works with external voice nodes: each node can run its own bridge (`[music]` in `opencord-voice-node.toml`), and a channel's music enters at the node serving it. The node list shows the bridge's counters (connections, packets, jitter, dropped, decrypt failures) and each node's bridge. A player that goes silent is restarted, and a node that is silent and doesn't answer loses its channels at once.
- **Already there from L0 and L2.** The music stream in the SFU, `MusicStream`, the forwarding rules, muting through `MediaSinkWants`, access control (single-use tokens, strict addresses, the discovery address, per-address rate limits) and the bridge domain from the music settings.
- **Verified.** `tests/music_bridge.rs`: a real external voice node with a bridge takes a fake Lavalink's sign-in for its channel and lets it go on Stop; the watchdog restarts a silent player once in 30 s, and moves a channel off a node whose `/v4/info` hangs. `opencord-voice` `tests/music.rs` `ten_listeners_hear_each_frame_together`: 10 listeners on one machine hear each of 80 frames within 150 ms of each other. Bridge unit tests cover `Audio`/`Silent` and jitter.
- **Rulings.**
  - The bridge reports `Audio` when packets start and `Silent` 2 s after they stop; jitter is RFC 3550's, from RTP timestamps (wrapping) against arrival, restarted after each silence so a pause is not jitter. The checklist shows the highest of the connections now.
  - The watchdog runs in the channel's session: audio is due 2 s after a track starts or resumes, and is checked once Lavalink has signed in at the bridge (a deadline that passes before the sign-in waits for it). A Lavalink that never reaches the bridge is a setup problem the connection test explains, not a reason to restart players. Verified against two real Lavalinks: after the failover and a restart, 302 packets arrived in 6 s and no player was restarted. The node gets 2 s for `/v4/info`; no answer marks it unavailable through its REST failures (`Lavalink::unresponsive`); the next successful stats poll judges it again.
  - A node's bridge is told about players over the control channel (`ExpectMusicBridge`), and reports what it sees (`MusicBridgeStatus`), which the main server takes only for channels that node serves. A channel whose voice node runs no bridge gets no voice state and waits, with a warning in the log.
  - When everyone leaves and the channel comes back on another voice node, the player gets a new voice state pointing at that node's bridge.
  - With voice only on external nodes, music is supported when `[music] enabled` is on, even though the main server runs no bridge itself.
- **Owed (manual).** A public Lavalink node through a domain with a Let's Encrypt certificate (plan L3 acceptance): it needs a public machine and a real domain. The 150 ms check on a real LAN with 10 devices.


## D46: Search, links and artwork (Phase 3 L4, 2026-10-15)

- **What admins get.** Search on every enabled source with its own prefix, links checked as in L2, and artwork through Opencord: `GET /media/music/artwork/{id}?size=128|512`, kept in `data/music-artwork` for 7 days and 200 MB. The app's core keeps what it fetched in its own 100 MB cache (`music_artwork` gives the file's path); `music_search` is in the core API.
- **Verified.** `tests/music_search.rs`: all 7 sources search with their prefixes; with only SoundCloud on, YouTube searches and links are refused and a typed `ytsearch:` stays words; 8 kinds of bad links never reach Lavalink; a track Lavalink would play through its `http` source isn't offered; an encoded track or made-up id can't be added, nor another member's results, and nothing the server sends has an encoded track; artwork is fetched once, resized to 512 and 128, matches the now-playing card, and needs a media token; artwork on a private address is refused (502, never fetched). Core `tests/music.rs`: search and artwork through the core, kept on the device. Against a real Lavalink 4.2.2 with youtube-source 1.18.2 (ignored test): YouTube Music and YouTube each found 15, a found link loaded again, and their artwork (from `yt3.googleusercontent.com` and `i.ytimg.com`) came through the proxy at 512×512 and 320×180.
- **Rulings.**
  - Artwork ids are a hash of the address (128 bits), so the same picture is kept once and an id says nothing about where it is. The server fetches only addresses it learned from Lavalink results or saved queues; it forgets the oldest of 100 000.
  - Each fetch looks the host up itself, refuses it unless every address is public, and connects to those addresses (no second lookup for the request to rebind). Redirects, at most 3, are checked the same way. `[music] allow_unsafe_sources` also allows private artwork addresses, as it does for tracks; tests use it.
  - Pictures are decoded with limits (8192 px a side, 256 MB), made no larger than 128 and 512 px, and encoded as JPEG at quality 85, which drops their metadata. Both sizes are made at once. A picture that fails isn't tried again for 10 minutes; 4 fetches at a time for the whole server.
  - The fake Lavalink moved into `opencord_server::testing`, so the core's tests use it too.
- **Findings.** Lavalink 4.2.2's built-in SoundCloud source fails every search ("Did not detect any supported formats"), and its Bandcamp source found nothing; both are inside Lavalink's player library. docs/music.md says so and points at the youtube-source plugin for YouTube.

## D47: Music UI (Phase 3 L5, 2026-10-16)

- **What members get.** The Music button in the voice panel and the call's control bar, People · Music in the right-side panel (340 px for music, one 120 ms resize; the overlay on narrow windows), the now-playing card at the top of People only, "In this voice channel" first in People, the music panel (cover, track, progress, controls, your volume, search with a source pill and skeleton rows, Up next virtualized and reorderable, row menus, Recently played with Add again), drop on the cover to play now, Expand into the voice view, read-only with Join, Set up music for who manages the server, and Music in User settings → Voice & audio.
- **Core.** `music_subscribe`, `music_unsubscribe`, `music_enqueue`, `music_playback`, `music_set_mode`, `music_queue_move`, `music_queue_remove`, `music_queue_clear`, `music_set_listening`; `MusicSession`, `MusicNowPlaying` and `MusicNotice` events. Anchors arrive on the device's clock: the server's offset comes from `Ready.server_time_ms` and `HeartbeatAck.server_time_ms` (the shortest round trip of the last 8 samples wins), plus 80 ms for the music's playout. Music volume, mute and ducking live in the media engine; muted, the voice node is asked not to send the music.
- **Server additions.** `MusicSettings.configured` (supported, on, a node added), broadcast when nodes are added or deleted, so the buttons appear and go live; `MusicEnqueue.history_item_ids`, so Add again needs no search and still never takes a track from a client; `MusicNowPlaying.source_display_name` for the card.
- **Verified.** Server `recently_played_tracks_can_be_added_again`; core `the_player_comes_with_its_changes_on_this_devices_clock`; core unit tests for the clock and ducking; 8 widget tests (button and panel width, search and add, card only in People and never a tile, shared controls, row menu moves sent as before-this-item, Add again, read-only with Join, Set up music, volume kept and sent) and 4 unit tests. `integration_test/music_ui_test.dart` in a profile build on this machine (load 4–6): the panel opens in one frame of 17–34 ms with 500 tracks queued; dragging through them, no frame over 16 ms; while paused, no frame at all for 3 s.
- **Rulings.**
  - Rows whose item, place and permissions did not change keep their widget, and the queue list reads the session when a menu or drop needs it, not when it was built.
  - Dragging between lists (a result into Up next at a position) is left out: results replace the queue while searching. Results and recently played tracks drop on the cover to play now, and their menus have Play next and Play now.
  - Music actions apply the server's answer at once; a move is applied here first and the answer follows. A stale action (`MUSIC_STALE`) says nothing: the new state is on its way.
- **Finding.** The frame where a dragged row is dropped rebuilds the visible rows (the reorderable list moves its keyed rows), 20–43 ms in 3 of 5 runs on this machine. Owed: the §0.1 targets on a mid-range laptop.

## D48: Chat commands and the Commands page (Phase 3 L6, 2026-10-16)

- **What members get.** Built-in `/` slash commands (Phase 3 §17) in the message composer. Autocomplete with command descriptions, option hints, choices, tab navigation between options, and escape to cancel. Music commands (`/play`, `/pause`, `/resume`, `/skip`, `/previous`, `/seek`, `/stop`, `/queue`, `/nowplaying` alias `/np`, `/remove`, `/move`, `/clear`, `/shuffle`, `/repeat`) targeting the member's voice channel (or any specified channel with `MUSIC_MANAGE`). Local commands (`/volume`, `/music`) stay on the client. Public results appear as compact messages by whoever ran the command (`kind = command_result`, with `interaction`); "only you" replies and errors are ephemeral with a Dismiss button.
- **Server side.** `commands.proto`, registry in `crates/opencord-server/src/commands/`, rate limits (5 autocompletes per second per user, shared music limits for executions), permissions enforcement, channel filtering. Server Settings → Commands page (`MANAGE_SERVER`) allows toggling commands, restricting to specific channels, and setting default public vs private results.
- **Verified.** Server integration tests in `crates/opencord-server/tests/commands.rs` (5 tests); Flutter widget tests in `app/test/widget/chat/commands_test.dart` (7 tests).

## D49: Sync, stale actions and vote skip (Phase 3 L7, 2026-10-16)

- **What members get.** Vote skip: when vote skip is enabled in Server settings → Music, listeners without `MUSIC_CONTROL` see the Next button become "Vote to skip (X/Y)" with a vote counter badge and voted indicator. Voting registers immediately; when enough listeners vote, the track skips and a notice is emitted. Skipping with `MUSIC_CONTROL` or on your own track skips immediately without counting votes. When listeners leave the voice channel, votes are pruned and the threshold recalculated; if remaining votes meet the new threshold, the track advances automatically.
- **Sync and stale-action handling.** `expected_item_id` in `MusicPlayback` and `MusicVoteSkip`: operations targeting a track that has already ended or advanced fail with `ErrorCode::MusicStale`, avoiding accidental skips or seeks on the subsequent song. Relative moves and server versioning prevent out-of-order state overwrites.
- **Accessibility and polish.** Focus order and semantic labels for screen readers across all music controls (Play/Pause, Next/Vote Skip, Shuffle, Repeat, Volume slider with `semanticFormatterCallback`), queue items, action menus, and command chips in the composer.
- **Verified.** Server integration tests in `crates/opencord-server/tests/music.rs` (`vote_skip_mechanism_advances_when_votes_reach_needed`) and `tests/commands.rs` (`skip_command_acts_as_vote_skip_when_missing_control_and_vote_skip_enabled`). Core integration and unit tests in `crates/opencord-core/tests/music.rs`. Flutter unit tests in `app/test/unit/music/music_session_test.dart` (4 tests) and widget tests in `app/test/widget/music/music_panel_test.dart` (8 tests).

## D50: Hardening and docs (Phase 3 L8, 2026-10-16)

- **What admins get.** Comprehensive documentation across all Phase 3 capabilities:
  - `docs/domains.md`: Full guide for domain kinds, DNS records, Let's Encrypt certificates (DNS-01, TLS-ALPN-01, HTTP-01), DuckDNS and deSEC free subdomains, Cloudflare quick and named tunnels, Caddy/Nginx reverse proxies, ports and low-port capabilities (`CAP_NET_BIND_SERVICE`), and a complete troubleshooting matrix for check results and certificate errors.
  - `docs/music.md`: Complete guide for running Lavalink next to Opencord with Docker Compose using the network alias trick, using public Lavalink nodes, IP-only self-signed certificate imports, sources and plugins (youtube-source, LavaSrc), vote skip rules, chat slash commands (`/play`, `/pause`, `/skip`, etc.), and a troubleshooting matrix covering node states, connection test failures, and voice gateway close codes (4001–4015).
  - Deploy updates: `deploy/docker-compose.yml` and `deploy/Dockerfile` updated with optional port 80/tcp mappings for Let's Encrypt HTTP-01 challenges alongside 7710/tcp, 7711/udp, and 7712/udp.
- **Load and resilience verification.** `twenty_channels_play_together_under_load` in `crates/opencord-server/tests/music.rs`: concurrently manages 20 active voice channels playing tracks under distributed user load; verifies all 20 players track state in Lavalink, `FetchLavalinkNodes` correctly reports 20 channels, empty channels pause without leaks (`paused_empty`), and listener arrival immediately resumes playback.
- **Phase 3 status.** Every milestone from D1–D4, L0–L8 is implemented and covered by automated tests, with the exceptions in D51 and the manual checks owed in D41–D47.

## D51: Review of L5–L8 and the voice fixes (Phase 3, 2026-10-18)

- **"RTC connecting", then "No route", after the app restarts.** Root cause: once an app has seen a server's domains, it connects to the primary first (plan §7.5). Through a Cloudflare tunnel or a reverse proxy, the embedded node's voice gateway still worked, since WebSockets pass, but its media candidate was empty ("the host you reached me with"), so the app sent UDP to the tunnel's host, which carries none (§4, §9.2–9.3). The first connection after adding a server used the address it was added with, which is why voice worked until a restart. Fix: a voice gateway reached through the local listener, from a trusted proxy with `X-Forwarded-For`, or with a tunnel or reverse-proxy domain as `Host` gets the server's public address as its candidate, unless the node has a public address of its own. Loopback alone does not count, so an app on the server's machine keeps `localhost`. Test: `voice_reached_through_a_tunnel_or_proxy_sends_media_to_the_public_address`. A server update fixes existing apps; nothing changes on the device.
- **Reverted from 0a7ada3.** A join with unchanged flags sent a new voice token, so clicking the channel you were in reconnected the call. That was the second reported glitch, not a fix for the first. And `RefreshVoiceServer` let any session of the user take the voice state over silently. Tests now say the opposite: `joining_the_channel_the_session_is_in_again_keeps_its_connection`, `another_session_of_the_same_user_gets_no_voice_token`, core `joining_the_channel_this_device_is_in_keeps_the_call`. The media task's retries after "No route" and a lost media path (also 0a7ada3) stay. The app no longer joins the channel it is already in, which also reset the camera and screen share (`VoiceSessionNotifier.join`).
- **Process death.** Core `after_the_app_dies_in_voice_the_next_launch_gets_voice_back` kills a client's whole runtime while in voice and joins again at once: it connects, also with the same local UDP port. The server's and node's grace periods were not the cause.
- **Right-side panel (89a95a9).** The now-playing card in a text channel opened nothing: Music needed the open channel to be a voice channel, though the card shows the call's music. With a voice channel's chat open, Members and Music did nothing; they now take the chat's place.
- **`MusicUpdate` (§14.3), owed since D44.** After the first snapshot, each change goes out as an update by item id: removals, then placements before another item (new items whole, others by id), from the longest run of items still in order, so moving one track sends one placement and no track. The small fields go in full each time. A client holding `base_version` applies it, one holding newer ignores it, and any other asks `FetchMusicSession`, which subscribes to nothing. Diff and apply live in `opencord_proto::music_sync`, shared by the server, the core and the tests; a 2 000-round random test checks that applying the diff of any two sessions gives the second. The core keeps each channel's session from snapshots, answers and updates, and still hands the app whole sessions, so the app is unchanged. Snapshots still go to everyone when someone joins the channel. Tests: `changes_reach_listeners_as_small_updates`, `the_session_can_be_fetched_without_subscribing`, core `changes_from_elsewhere_arrive_whole_from_the_servers_updates`, and `two_people_moving_tracks_at_once_end_with_the_same_order`.
- **User rulings.** The source logos (`font_awesome_flutter`, 89a95a9) stay, against §15.3: choosing a provider is easier with them. The music volume curve (8f41bcd: 100 % is −6 dB, quadratic) is intended, because Lavalink's music came too loud.
- **Also.** Unformatted Rust and Dart from L7 and 89a95a9 formatted; a clippy error (`manual_div_ceil` in vote skip) fixed; debug prints left in `commands_test.dart` removed.

## D52: Media uploads: protocol, rules, encryption format, schema (Phase 4 A0, 2026-10-18)

- **Why.** Phase 4 adds file attachments, S3 storage and voice messages (design `docs/superpowers/specs/2026-10-18-media-uploads-design.md`, plan `docs/superpowers/plans/2026-10-18-media-uploads.md`). A0 lays the shared ground the later milestones build on.
- **Protocol.**
  - `models.proto`: `Attachment`, `AttachmentKind`, `UploadSettings`, `UploadState`, `Upload`, `UploadStatus`, `StorageKind`, `S3Config`, `StorageBackend`, `StorageMove`, `Storage`, `StorageTestStep` and `StorageTest`; `Role.max_upload_bytes` (8), `Message.attachments` (9); error codes 41–47.
  - `requests.proto`: `SendMessage.attachment_ids`, `max_upload_bytes` on `CreateRole` (6) and `UpdateRole` (7), requests 160–170 (`CreateUpload` to `StopStorageMove`), responses 29–33.
  - `events.proto`: 110–113 (`UploadSettingsUpdate`, `UploadReady`, `UploadFailed`, `StorageUpdate`).
  - `gateway.proto`: `Ready.upload_settings` (20).
  - Until A1–A3 the server answers the new requests with "not implemented".
- **Permissions.** `ATTACH_FILES` (bit 10) now means something, and `SEND_VOICE_MESSAGES` is bit 14. Both are in `DEFAULT_EVERYONE`, and migration 0008 gives them to existing servers' @everyone along with a 50 MB limit.
- **Shared rules** (`opencord-common/src/upload.rs`):
  - the effective upload limit: the owner gets the ceiling; everyone else gets the highest of @everyone and their roles, capped at the ceiling;
  - filename sanitizing;
  - extension normalizing and blocking.

  One JSON fixture (`tests/upload_rules.json`) holds the cases, and the Dart mirror will be tested against it too.
- **Encryption** (`blob.rs`):
  - an `OCA1` header, then 64 KiB segments;
  - XChaCha20-Poly1305 with a random 24-byte nonce per segment;
  - AAD binding the kind, blob id, segment index and last flag;
  - `locate` and `open_span` serve byte ranges.
- **Sniffing and waveform** (`sniff.rs`, `waveform.rs`): 13 formats by magic bytes; 128 waveform values with −60…0 dBFS mapped to 0…255.
- **Schema** (migration 0008): `storage_backends` (the local storage is id 1 and active), `attachments`, `blob_deletions` filled by the `attachments_queue_blobs` trigger, and `roles.max_upload_bytes`.
- **Config and the secrets key.**
  - `[uploads] max_file_mb` (default 4096, 1–102,400) and `local_dir`; `[server] secrets_file` (`OPENCORD_SECRETS_FILE`).
  - When the key file is missing and the database already holds sealed data, the server refuses to start instead of making a new key.
- **Also.** The server's gateway frame reader boxes the envelope (clippy `large_enum_variant` after the proto grew).
- **Review.** A file named only `.exe` got past blocking; `is_blocked` now matches it, and the fixture has the case.

## D53: Media uploads: storage, uploads, processing and downloads (Phase 4 A1, 2026-10-18)

- **Why.** Milestone A1 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) implements the complete server lifecycle for file and voice attachments: local storage, chunked upload receiving, thumbnail generation and voice verification, range downloads, and background garbage collection.
- **Local storage backend** (`crates/opencord-server/src/attachments/storage/local.rs`):
  - Objects are stored under `{root}/{prefix}/{shard}/{id}`, where prefix is `a` for full attachments and `t` for thumbnails, and `shard` is `{:02x}` of snowflake ID bits `(id >> 22) & 0xff`.
  - Atomic rename (`tokio::fs::rename`) with fallback to buffered copy and source deletion across filesystem boundaries (handling `EXDEV` / `ErrorKind::CrossesDevices`).
  - Read ranges via `get_range` and deletion via `delete`.
- **Upload pipeline** (`crates/opencord-server/src/attachments/http.rs`, `crates/opencord-server/src/handlers/uploads.rs`):
  - `CreateUpload` validates permissions (`ATTACH_FILES` / `SEND_VOICE_MESSAGES`), enforces rate limit (`UPLOAD_CREATE_RATE`), checks 512 MiB temp disk headroom (`TEMP_HEADROOM_BYTES`), creates an encrypted temporary `.part` file, and inserts an `uploading` row.
  - `PUT /media/uploads/{id}` requires media token authentication and `Upload-Offset` matching current received bytes. Rejects out-of-order chunks with 409 `UploadStatus` without corrupting files. Receives 8 MiB chunks (`PIECE_BYTES`), seals them into encrypted `OCA1` segments, and streams into the `.part` file.
  - `GET /media/uploads/{id}` returns `UploadStatus`.
  - `CancelUpload` deletes the database row and unlinks the `.part` file.
- **Processing and background workers** (`crates/opencord-server/src/attachments/process.rs`, `images.rs`, `voice.rs`):
  - `POST /media/uploads/{id}/finish` sets the upload state to `processing` and spawns a background job governed by a 2-permit concurrency semaphore (`jobs`).
  - Interruptions and restarts: `attachments::start` resumes any pending `processing` rows on boot, and cleans up orphaned `.part` files lacking active DB rows.
  - Image processing: type sniffing via `sniff::sniff`. Images are decoded with EXIF orientation applied and, when a side is over 800 px, scaled to fit 800×800 keeping the aspect ratio (`DynamicImage::resize`, `FilterType::Triangle`). Images with alpha save as PNG; others as JPEG at quality 82. Animated GIF and WebP files are flagged `animated` and still get a thumbnail of their first frame. An image that fails to decode stays a file.
  - Voice processing: validates Ogg Opus structure, enforces maximum voice duration (`voice_message_max_seconds`), decodes PCM audio, measures duration, and calculates 128 normalized waveform bars (`waveform::waveform`).
  - Storage: finishes by encrypting the thumbnail (if generated), moving the attachment file into the active storage backend, marking state `ready`, and broadcasting `UploadReady` (or `UploadFailed` on error).
- **Downloads** (`crates/opencord-server/src/attachments/range.rs`, `crates/opencord-server/src/attachments/http.rs`):
  - `GET /media/attachments/{id}` checks media token, requires the attachment row to be `sent`, checks message deletion, and verifies channel permissions (`VIEW_CHANNEL | READ_HISTORY`) at request time.
  - Range requests: parses `Range: bytes=...` headers (`bytes=a-b`, `bytes=a-`, `bytes=-n`). Errors or invalid ranges return 416 Range Not Satisfiable.
  - Range streaming: streams decrypted plaintext chunks segment-by-segment using `futures_util::stream::unfold`, reading at most 16 segments per storage batch.
  - Security headers: sets `Cache-Control: private, no-store`, `X-Content-Type-Options: nosniff`, `Accept-Ranges: bytes`, and `Content-Disposition: attachment; filename*=UTF-8''...` with percent-encoding.
  - `GET /media/attachments/{id}/thumbnail` returns the decrypted thumbnail with MIME detection.
- **Garbage collection and blob deletion** (`crates/opencord-server/src/attachments/gc.rs`):
  - `sweep`: runs every 10 minutes, pruning abandoned uploads (`uploading` and `ready` > 24 hours, `failed` > 1 hour) and removing their temp files.
  - `drain_blobs`: runs every 30 seconds, deleting pending storage objects queued in `blob_deletions` by database triggers (`attachments_queue_blobs`), applying exponential retry backoff on failures.
  - Test hooks on `ServerHandle`: `hold_processing`, `db`, `sweep_attachments` and `drain_blob_deletions`.
- **Review.**
  - `PUT` and `finish` checked the media token only after buffering the body (up to 8 MiB); the token now comes first.
  - A poster sent with a non-video upload stayed in memory for good; every job now takes its poster.
  - An upload cancelled while processing could leave its stored objects behind: `set_ready` only updates a `processing` row, and a job whose row is gone queues what it stored with `queue_blob`. Objects stored before a failure are queued the same way.
  - A zero-byte upload finished without an empty `PUT` failed; `finish` now writes its empty last segment.
  - `attachments::start` deleted every temp file when its database query failed; startup now stops instead.
  - Voice decoding moved to `spawn_blocking`; animation detection runs before the full image decode.
  - `upload_locks` kept an entry for every upload ever made; unused entries are now dropped.
  - Blob retry delays overflowed after weeks of failures (`retry_delay_ms` caps them at 6 hours), and storage error details reached members; they now stay in the log.

## D54: Media uploads: messages, upload settings and role limits (Phase 4 A2, 2026-10-18)

- **Why.** Milestone A2 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) puts finished uploads on messages and lets the server's managers set the upload rules. Commits: 3b10a2c, 4f9f05a, d22e7ca.
- **Messages.**
  - `SendMessage.attachment_ids` takes up to 10 of the sender's own ready uploads from the same channel. The message keeps their order, and its text may then be empty (`validation::message_content_or_empty`).
  - Attach Files, or Send Voice Messages for a voice message, is checked again when the message is sent, not only when the upload starts.
  - A voice message stands alone: one attachment and no text, also after an edit.
  - `db::attachments::mark_sent` moves an upload only while it is still ready, the sender's and in that channel. Otherwise the whole send rolls back with "not found", so a cancel or a second send in between can't put a file on two messages.
  - `FetchMessages`, `EditMessage` and `MessageUpdate` carry the attachments.
  - Deleting a message deletes its attachment rows in the same transaction. The `attachments_queue_blobs` trigger then queues their objects, also when a channel's deletion cascades to them.
- **Upload settings.**
  - `UpdateUploadSettings` (Manage Server) normalizes the blocked list with `normalize_blocked_list` and takes a voice message limit of 30–1,800 seconds.
  - Everyone gets `UploadSettingsUpdate`, and `Ready.upload_settings` carries the settings at login.
  - A blocked upload's error names the list entry it matched ("tar.gz files can't be uploaded here").
- **Role limits.**
  - `CreateRole` and `UpdateRole` take `max_upload_bytes`. 0 clears a role's limit but is refused for @everyone.
  - Other values must be 1 MB up to the server's ceiling.
  - Only the owner may give a role a larger limit than their own.
- **Tests.** The A1 tests that put uploads on messages through `ServerHandle::db` now send real messages, and the hook is gone.
- **Review.** `DeleteMessage` read the message inside its transaction. In WAL mode a transaction that has read can't start writing once another connection has committed, and SQLite fails it at once with `SQLITE_BUSY_SNAPSHOT`, so a delete that raced any message send failed. The read and the permission check now come before `begin`, and the transaction holds only the two writes. The plan gained Working Rule 20 for this.
- **2026-10-21, CI on macOS and Windows.** `identify` hit the same failure: it reads the user, the invite and the claim token in its transaction, and the public address detection committed in between, so the owner's first sign-in failed with "internal server error" (code 517). Its reads have to stay inside, as the claim token is used up atomically even against `reset-claim-token` from another process, so transactions that read and then write start with `BEGIN IMMEDIATE` (`db::begin_read_write`): identify, the three CLI commands in `bootstrap.rs`, and the ban. They wait the busy timeout for the write lock instead of failing.

## D55: Media uploads: S3 storage, storage management and moving files (Phase 4 A3, 2026-10-19)

- **Why.** Milestone A3 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) lets the owner keep files in S3-compatible buckets. Commits: 5ad3003..f64bdf1.
- **The S3 backend** (`attachments/storage/s3.rs`).
  - Requests are presigned with `rusty-s3` 0.10.2 for 15 minutes and sent with the server's own `reqwest` client, with 300 seconds each. Network errors drop the signed URL (`without_url`), so no URL reaches a log or a reply.
  - Files up to 64 MiB go up in one request. Larger ones go up as multipart uploads in 16 MiB parts, aborted on failure.
  - Reads are ranged GETs. A missing object is `NotFound`, and a delete treats 404 as done.
  - S3 errors become "Code: Message" from the error body, at most 200 characters.
  - The bucket test writes 1 KiB, reads bytes 100–199 back, uploads 5 MiB + 1 KiB in two parts and deletes both. It stops at the first failed step.
- **Loading.** `Attachments::load` builds every S3 storage at startup and restores the active one. A storage whose secret can't be opened stops the server, rather than leaving its files unreadable.
- **Managing storage.**
  - Only the owner may send the storage requests; an administrator gets FORBIDDEN.
  - AddStorage, UpdateStorage with new bucket settings, and ActivateStorage of an S3 storage run the bucket test first. A failed step is STORAGE_UNAVAILABLE "the bucket test failed at “step”: detail".
  - The secret key is sealed as `storage-secret:<id>` and never sent back. An empty secret in UpdateStorage keeps the stored one. Answers show `has_secret` and the access key masked to its last 4 characters.
  - A storage that holds files can't change its endpoint, bucket or key prefix.
  - The local storage can't be removed. Another storage can be removed only when it's inactive, holds no files and has no deletions waiting.
  - Every change goes to the owner as StorageUpdate.
- **Moving files** (`attachments/move_job.rs`).
  - StartStorageMove copies every ready or sent file, with its thumbnail, from the other storages to the active one: 50 rows per page, two files at a time, through a temp file in 8 MiB reads. The ciphertext is copied as is.
  - The row switches with `set_storage_if` in the same transaction that queues the old copies for deletion. A file deleted meanwhile gets its new copies queued instead.
  - Old copies are deleted an hour after the switch, so downloads already under way can finish.
  - Each file gets three retries, 2, 4 and 8 seconds apart. After that it counts as failed and stays where it was.
  - Progress goes to the owner at most every 2 seconds, and once at the end. StopStorageMove stops before the next file, and activating another storage is refused while a move runs.
  - Startup deletes the temp files a move or a bucket test left behind.
- **Tests.** The S3 tests in `tests/s3.rs` are ignored by default and run against RustFS 1.0.3 in Docker, because MinIO no longer publishes images.
- **Review.**
  - Processing now picks the active storage under the write lock and writes it on the row before storing anything. Before, a file still being stored (minutes for a large one on S3) showed the storage it was created under. So changing the bucket of the storage it was going to, or removing that storage, didn't count it, and the file was lost.
  - UpdateStorage checks for files again under the write lock, next to the write. It also refuses to change a storage's endpoint, bucket or key prefix while a move runs, since the move may be writing there.
  - ActivateStorage only tests the bucket now and keeps the backend that startup, AddStorage or UpdateStorage loaded. Installing the one it tested could undo an UpdateStorage that finished meanwhile.
  - Startup creates the uploads temp folder and stops if it can't. The bucket test and moves write there, and on a fresh server it didn't exist yet. The implementation had created it with the error ignored and didn't report it.
  - The plan gained Working Rule 21 for the first two.

## D56: Media uploads: the client core (Phase 4 A4, 2026-10-19)

- **Why.** Milestone A4 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) gives the client core what the app needs to send, show and save files. Commits: 56089df..f00971f.
- **Types and requests.**
  - Messages carry their attachments, roles their upload limit, and Ready the upload settings. In RoleChanges, a limit of 0 removes a role's limit.
  - The core has the requests for upload settings and the owner's storage, and passes UploadSettingsUpdate and StorageUpdate to the app. CoreError gains Cancelled.
- **Transfers** (`attachments/transfer.rs`).
  - Uploads and downloads use one `reqwest` client per server, built with the gateway's own certificate check (`TlsTrust`), so a pinned certificate stays pinned. It is rebuilt when the server's route changes.
  - A changed certificate is FingerprintMismatch. A 401 renews the media token once, with the renewal now shared with `http.rs`.
  - Connecting may take 10 seconds, and 60 seconds without a byte ends a request. A piece may take 120 seconds, a status or finish 30, and a download has no overall limit.
- **Sending files** (`attachments/upload.rs`).
  - `send_message_with_files` takes 1 to 10 files, by path or by bytes. Two go up at a time, in the server's 8 MiB pieces, then SendMessage goes out with their ids in the files' order.
  - A network failure is tried again 1, 2, 4, 8 and 16 seconds later, six tries in all. A piece whose answer was lost goes again, and the server's 409 says where the upload really is.
  - Each file's verdict comes by its UploadReady or UploadFailed event, waited for from before the finish request. The status is also asked every 5 seconds, in case the event was lost.
  - UploadProgress follows each file: Preparing, Uploading (at most 10 a second), Processing, Sending, Done. On a failure the failing file is Failed and the others Cancelled.
  - `cancel_send` stops a send. After a cancel or a failure, every upload the send made is cancelled on the server.
- **Photo metadata** (`attachments/exif.rs`). A JPEG, PNG or WebP of at most 100 MiB loses its metadata before it leaves the device.
  - JPEG loses its Exif, XMP and IPTC segments. An orientation other than 1 comes back as a minimal Exif block, so the photo still shows the right way up.
  - PNG loses its eXIf, text and tIME chunks. WebP loses its EXIF and XMP chunks, their VP8X flags are cleared, and its RIFF size is rewritten.
  - A file that isn't well formed is sent as it is.
- **Downloads and the cache** (`attachments/{download,cache}.rs`).
  - `download_attachment` streams a file through `path.part` to `path`, with DownloadProgress; `cancel_download` stops it.
  - `attachment_bytes` reads a thumbnail, or an original of at most 64 MiB, for the image viewer and the voice and audio players. Thumbnails and originals up to 20 MiB are cached.
  - The cache is 1 GiB for every server, the least recently used going first. Each entry is sealed as a client-cache blob under a key the app keeps in the system keychain (`media_cache_key_store.dart`). The entry's name is a SHA-256 of server, attachment and variant, and its first 8 bytes are the blob id, so a file copied over another entry doesn't open. Without the keychain nothing is cached on disk.
- **Clipboard** (`attachments/clipboard.rs`).
  - `clipboard_image` pastes an image as PNG; with text on the clipboard it returns nothing. `clipboard_set_image` copies an image.
  - It uses `arboard` 3.7.0 on Linux, X11 and Wayland, with one clipboard kept for the app's life so a copied image stays.
  - macOS follows in P2. On Windows `arboard` needs BSL-1.0 crates, outside the allowed licences, so P1 settles Windows.
- **Tests.** `tests/attachments.rs` runs the core against a real server: sending, a lost piece answer, cancelling, a blocked file, an expired token, saving, the cache with the server stopped, and a photo's location. ServerHandle gains `uploads_in_state` for them.
- **Review.** The code matches the plan exactly, and every check passes, so nothing was changed. The working rules from the A1–A3 reviews moved from the plan to `GEMINI.md`, which now collects every review's lessons. From A5 on, the plan gives behavior and interfaces rather than code.

## D57: Media uploads: the app (Phase 4 A5, 2026-10-19)

- **Why.** Milestone A5 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) brings attachment support to the Flutter app: models, upload rules, composer staging, sending, chat attachment views, the media viewer, and server and user settings. Commits: 382bcab..98d96d3.
- **Models and upload rules** (`core/model/attachment.dart`, `core/model/storage.dart`, `core/upload_rules.dart`, `core/format.dart`).
  - `Attachment` holds an uploaded file's metadata (`id`, `filename`, `contentType`, `sizeBytes`, `kind`, dimensions, `durationMs`, `waveform`, thumbnail presence and size, and `animated`).
  - `UploadSettings` stores server limits: `blockedExtensions`, `voiceMessageMaxSeconds`, `maxFileBytes`, and `maxAttachments` (fallback 10). `UploadPhase` tracks `preparing`, `uploading`, `processing`, `sending`, `done`, `failed` and `cancelled`.
  - `StorageBackend` describes local or S3 storage (`id`, `kind`, `name`, `active`, `s3`, `hasSecret`, `fileCount`, `bytes`, and `lastError`). `StorageMove` reports file move progress (`toStorageId`, `running`, `filesDone`, `filesTotal`, `bytesDone`, `bytesTotal`, `failed`).
  - `effectiveUploadLimit` calculates member upload limit: the owner gets `ceiling` (`maxFileBytes`); other members take the maximum of `@everyone` or their assigned roles' limits, capped at `ceiling`.
  - `sanitizeFilename` strips directories, control characters, leading spaces and trailing dots/spaces; prefixes DOS device stems with `_`; caps length to 200 preserving extension; and defaults empty names to `file`. `normalizeExtension` trims, strips a leading dot, and validates 1..20 lowercase alphanumeric or dot characters.
  - `stageProblem` detects `StageProblem.tooLarge` and `StageProblem.blocked`. `blockProgramsPreset` copies the server's 31 program extensions. `formatFileSize` formats sizes as B, KB, MB, or GB.
- **Repository and state** (`core/repository/repository.dart`, `core/rust/rust_repository.dart`, `core/mock/mock_repository.dart`, `core/providers/providers.dart`, `core/providers/server_state.dart`).
  - `OpencordRepository` gains `sendMessageWithFiles`, `cancelSend`, `attachmentBytes`, `downloadAttachment`, `cancelDownload`, `clipboardImage`, `copyImage`, `updateUploadSettings`, `storage`, `addStorage`, `updateStorage`, `testStorage`, `activateStorage`, `removeStorage`, `startStorageMove`, `stopStorageMove`, `mediaCacheSize`, and `clearMediaCache`.
  - `ServerData` exposes `uploadSettings` and calculates `myUploadLimit`. `ChannelMessagesNotifier` handles `sendWithFiles` with a unique nonce, tracking pending uploads with `UploadProgressEvent` updates and supporting `cancelSend` and `discard`.
- **Composer, staging and sending** (`features/chat/composer.dart`, `features/chat/composer_state.dart`, `features/chat/attachment_drop.dart`, `features/chat/attachments/staged_tiles.dart`, `features/chat/attachments/upload_progress.dart`).
  - An `OcIconButton` with `OcIcons.attachFile` opens the system file picker (`openFiles`) when permitted.
  - `AttachmentDropZone` detects files dragged over chat and stages them in the composer. Ctrl+V (or Cmd+V) with no text on the clipboard checks `clipboardImage` and stages the image as `pasted-image-YYYYMMDD-HHMMSS.png`. Dropping and pasting need Attach Files, like the button.
  - `StagedTiles` shows previews for image files up to 20 MiB, filenames, sizes, and remove buttons. Files exceeding the limit or blocked show error indicators and disable sending. Beyond 10 files, tiles receive `StageProblem.tooMany`.
  - Outgoing uploads display `UploadProgressList` with sent and total sizes, a bar, the reason when a file fails, and a cancel button.
- **Attachments in messages and downloads** (`features/chat/attachments/{attachment_view,image_grid,file_card,download,text_preview_dialog}.dart`, `features/chat/bubble.dart`).
  - `AttachmentsView` displays attachments in message bubbles.
  - Images render in `ImageGrid`: up to 4 images in a grid layout, with a `+N` badge on the 4th item when 5 or more images are present.
  - Non-image files render in `FileCard` with icons by type, filename, size, and download button calling `saveAttachment`. Text files up to 256 KiB (`maxPreviewBytes`) offer a "Preview" button opening `TextPreviewDialog`, which shows the text in `OcText.mono` and never renders it.
  - `messageSummary` summarizes messages for notifications and pinned banners as text, `Photo`, `count photos`, `Video`, `Voice message (m:ss)`, or `filename`.
- **In-app media viewer** (`features/chat/attachments/media_viewer.dart`).
  - `showMediaViewer` opens a transparent fullscreen route with scrim background.
  - `MediaViewer` displays images with `InteractiveViewer` supporting pan and scroll-wheel zoom between 1.0x and 8.0x.
  - Top bar shows filename (`middleEllipsis` at 36 characters), index counter (`i / N`), download button, and close button. Left/right buttons and ArrowLeft/ArrowRight keyboard shortcuts navigate; Escape closes.
- **Settings** (`features/settings/server/{uploads_page,storage_page,storage_dialogs,roles_page}.dart`, `features/settings/user/app_pages.dart`).
  - `ServerUploadsPage` configures blocked file extensions (with a "Block programs" button) and voice message maximum duration.
  - `ServerStoragePage` (owner only) lists backends, active status, file counts, and byte sizes. A new bucket, or a change to a bucket's settings, needs its access key and secret and a passing `testStorage` of exactly those values before Save; a rename needs neither. Existing files can be migrated with `startStorageMove` and monitored with progress and `stopStorageMove`.
  - `_RoleEditor` in `roles_page.dart` provides "Max upload size" with MB/GB units and a "Not set" toggle for non-@everyone roles, validated against server limits and user upload capacity.
  - `_MediaCacheSection` in `BehaviorPage` shows cached bytes and provides a "Clear" button calling `clearMediaCache`.
- **Tests.**
  - Unit tests: `app/test/unit/model/upload_rules_test.dart` (limits, sanitization, normalization, blocking), `app/test/unit/format_test.dart` (`formatFileSize`, `middleEllipsis`), `app/test/unit/mock/mock_repository_test.dart` (`sendMessageWithFiles`), `app/test/unit/rust/core_mapping_test.dart` (error mapping), `app/test/unit/theme/oc_colors_test.dart` (tokens), `app/test/unit/message_summary_test.dart` (summaries).
  - Widget tests: `app/test/widget/chat/composer_attach_test.dart` (staging, problem blocking, pasting, sending progress, cancel), `app/test/widget/chat/attachment_drop_test.dart` (drag overlay and drop), `app/test/widget/chat/attachments_view_test.dart` (aspect ratio box, grid counter, text preview), `app/test/widget/chat/media_viewer_test.dart` (navigation and close), `app/test/widget/settings/media_settings_test.dart` (uploads page, storage owner/test requirement, role limits, cache clear).

- **Review.** Fixed after A5:
  - **Editing a bucket.** The dialog filled the access key with the masked value answers carry, and sent it back with every edit, so the server's own bucket test refused even a rename. It also tested the saved bucket instead of the values typed. Now the keys start empty, a rename sends only the name, and changed settings need both keys and a passing test of exactly what Save sends. The mock now masks keys like the server.
  - **The Dart filename rules** counted UTF-16 units and kept U+0080–U+009F, where Rust counts characters and drops them. The shared fixture gained both cases.
  - **Sending:**
    - a retry now starts each file's progress afresh, and never sends a message's text without its files;
    - a failed file shows why;
    - staging is one pass that uses the server's `maxAttachments`;
    - a blocked file names the extension that matched;
    - paste and drop need Attach Files and report clipboard errors.
  - **The viewer:**
    - it is dark in either theme, since it sits on the dark scrim;
    - it no longer zooms twice per wheel step, because `InteractiveViewer` zooms on the wheel itself (the plan wrongly asked for a `Listener`).
  - **Conventions:**
    - one file-type icon mapping in `file_icon.dart`, instead of three;
    - `ChoiceChips` and `OcSwitch` instead of Material widgets;
    - `on RepoException` instead of bare `catch`;
    - `mounted` checks after awaits;
    - cancels go through `_now`;
    - the download progress provider no longer hides a missing repository (the test that relied on that now provides one);
    - the media cache section reports its errors and says what it holds.
  - **Leftovers from before attachments:** the role editor still showed Attach files as "coming later", and Dart's default @everyone lacked it, unlike `Permissions::DEFAULT_EVERYONE`. Both are fixed.
  - **D57 itself** described the plan rather than the code in five places: the preset size, the paste handler, the upload rows, the text preview and a button label.

## D58: Media uploads: voice messages (Phase 4 A6, 2026-10-19)

- **Why.** Milestone A6 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`) brings voice messages: audio engine capture tap and local media mixing, the core voice recorder and player, holding and locking to record in the app, and voice message bubbles with waveform playback. Commits: bb91e83..7b2c8eb.
- **Audio engine tap and local media** (`crates/opencord-media/src/audio/{processor.rs,engine.rs}`).
  - `Processor::set_capture_tap(tap: Option<rtrb::Producer<f32>>, silence_call: bool)` pushes 480-sample ticks to the tap after input volume scaling; with `silence_call`, the tick is zeroed before `capture.push` so the call receives silence while the input level meter continues measuring the real microphone level.
  - `Processor::set_local_media(source: Option<rtrb::Consumer<f32>>)` pops interleaved stereo 48 kHz samples and adds them directly into `self.stereo` during `play()` (or discards them during `play_unheard()`), automatically including them in the acoustic echo cancellation reference signal.
  - `AudioEngine::set_capture_tap` and `AudioEngine::set_local_media` forward the commands (`Command::CaptureTap` and `Command::LocalMedia`) to the audio engine thread.
- **The voice recorder** (`crates/opencord-core/src/voice_message/recorder.rs`, `media.rs`, `client.rs`, `api/{types.rs,attachments.rs}`).
  - `voice_record_start(max_seconds)` initiates an `opencord-voice-recorder` worker thread using an active call's capture tap via `TapEngine` (silencing call audio) or opening a direct `Microphone` with `FromDevice` and `VoiceProcessing`.
  - Samples are encoded via `opus::Encoder` at 48 kHz Mono Voip with 32,000 bps bitrate, VBR enabled, DTX disabled, and complexity 10.
  - Mean square sample powers (`Σx²/480`) are collected per 10 ms tick; at stop, they are split into 128 slices the way `waveform()` splits samples, and each slice's `rms = sqrt(mean of slice)` is mapped to a byte with `level_byte(rms)`.
  - Packets are written into an Ogg Opus stream with encoder pre-skip lookahead via `ogg::write(1, pre_skip, &packets, samples)`.
  - Emits `MediaEvent::VoiceRecordLevel { level }` at most every 33 ms and `MediaEvent::VoiceRecordStopped { reason }` on `VoiceStopReason::MaxLength` or `VoiceStopReason::DeviceLost`.
  - `voice_record_stop()` returns `VoiceDraft { draft_id, duration_ms, waveform }` and caches `(ogg_bytes, draft)` in memory; `voice_record_cancel()` aborts without a draft; `voice_draft_discard(draft_id)` frees the cached draft.
  - `send_voice_message(server_key, channel_id, nonce, draft_id)` sends the draft via `attachments::upload::send` using `proto::AttachmentKind::VoiceMessage` as `voice-message.ogg` with empty text, discarding the draft upon completion.
- **Voice message playback** (`crates/opencord-core/src/voice_message/player.rs`, `now_playing.rs`, `api/{types.rs,attachments.rs}`).
  - `voice_play(item, position_ms, speed)` accepts `PlaybackItem::Attachment` or `PlaybackItem::Draft` at speeds 1.0, 1.5, or 2.0.
  - Employs `wsola::TimeStretch` for pitch-preserving time compression/expansion on 48 kHz mono PCM, seeking by calculating packet sample offsets with `opus::packet::get_nb_samples`.
  - Routes audio to the active call engine via `set_local_media` as interleaved stereo, or plays through `Speaker::open` with `ToDevice` and output volume.
  - `NowPlaying` manages a stoppable handle, stopping any existing player when a new item starts or when `voice_stop()` is called.
  - Exposes `voice_pause()`, `voice_resume()`, `voice_seek(position_ms)`, `voice_set_speed(speed)`, and `voice_stop()`.
  - Emits `MediaEvent::PlaybackUpdate { item, position_ms, duration_ms, playing, buffering }` (at source time rate), `MediaEvent::PlaybackEnded`, and `MediaEvent::PlaybackFailed`.
- **Permissions and repository** (`app/lib/core/model/{permissions.dart,voice.dart}`, `app/lib/core/repository/{repository.dart,events.dart}`, `app/lib/core/rust/rust_repository.dart`, `app/lib/core/mock/mock_repository.dart`, `app/lib/features/settings/server/permission_info.dart`).
  - Added `Permissions.sendVoiceMessages = Permissions(1 << 14)` to `Permissions`, `defaultEveryone` and `Permissions.all`, with UI entry "Send voice messages" ("Record and send voice messages") in server permission settings.
  - Added repository methods for voice recording and playback, mapping FRB calls to Dart types. `MockRepository` implements timer-driven level updates, draft storage, and simulated playback with pause/resume support.
- **Recording UI and composer bar** (`app/lib/features/chat/voice/{mic_button.dart,recording_bar.dart,waveform_painter.dart,voice_recorder_state.dart}`, `features/chat/composer.dart`, `features/channels/voice_panel.dart`).
  - `MicButton` uses raw pointer listeners with an inner `OcIconButton` (tooltip "Voice message"). Pressing calls `voiceRecordStart`; dragging left (`dx < -120`) enters cancelling; dragging up (`dy < -60`) locks; releasing under 500 ms displays a toast "Hold to record". Keyboard Space down/up and Escape are handled when focused.
  - `VoiceRecorderNotifier` manages phases `RecordingPhase.idle`, `holding`, `cancelling`, `locked`, and `preview`. It accumulates live 100 ms max levels and elapsed duration. `isRecordingVoiceProvider` updates `VoiceConnectedPanel` to show "Recording a voice message" during a call.
  - `RecordingBar` renders inside the composer:
    - Normal/holding: pulsing 10 px dot in `recording` (0.4 to 1.0 opacity every 600 ms), `m:ss` timer, live waveform scrolling left (3 px bars, 2 px gaps in `voice`), and hint "Release to send · Esc to cancel".
    - Cancelling: `recording` background at 15% opacity, static dot, and hint "Release to cancel".
    - Locked: actions Delete (`OcIcons.delete`), Stop (`OcIcons.stop`), and Send (`OcIcons.send`, inverted). Enter sends; Escape deletes.
    - Preview: play/pause (`OcIcons.playArrow` / `OcIcons.pause`) plays the draft through the voice player (`DraftPlayback`), with the static draft waveform filling as it plays, the duration, Delete and Send.
- **Voice message bubbles** (`app/lib/features/chat/voice/voice_bubble.dart`, `app/lib/core/providers/playback_state.dart`, `features/chat/attachments/attachment_view.dart`).
  - `VoiceBubble` width scales smoothly from 200 to 320 px via `lerpDouble(200.0, 320.0, clamp((durationSec - 10.0) / 50.0, 0.0, 1.0))`.
  - Max-pools 128 waveform bytes to `floor(width / 5)` bars via `resampleWaveform`; unplayed bars use `textMuted` (keeping resting dark/light screens monochrome) while played bars use `voice`.
  - Displays play/pause icon button, time label (`m:ss` idle or `m:ss / m:ss` playing), speed chip cycling `1×` → `1.5×` → `2×` (remembered per bubble), and seeking on tap/drag.
  - Secondary tap (right-click) shows attachment menu with "Download" via `showAttachmentMenu`.
  - `AttachmentsView` routes `AttachmentKind.voiceMessage` to `VoiceBubble`.
- **Tests.**
  - Rust unit tests: `crates/opencord-media/src/audio/processor.rs` (`the_tap_gets_processed_ticks_and_silence_reaches_the_call`, `local_media_is_played_and_heard_by_the_echo_canceller`); `crates/opencord-core/src/voice_message/recorder.rs` (`a_recording_becomes_valid_ogg_opus`, `the_draft_waveform_matches_the_server_formula`, `recording_stops_at_the_maximum`, `in_call_recording_silences_the_call`); `crates/opencord-core/src/voice_message/player.rs` (`tempo_two_halves_the_output`, `seek_lands_within_a_frame`, `starting_another_item_stops_the_first`, `a_draft_plays_without_network`).
  - Dart unit and widget tests: `app/test/unit/chat/waveform_test.dart` (`waveform_resampling_uses_max`); `app/test/widget/chat/voice_recording_test.dart` (`hold_shows_the_bar_and_release_sends`, `a_short_press_shows_hold_to_record`, `dragging_left_cancels`, `dragging_up_locks_and_enter_sends`, `max_length_while_locked_opens_the_preview`, `no_permission_hides_the_mic`); `app/test/widget/chat/voice_bubble_test.dart` (`voice_bubble_plays_seeks_and_changes_speed`); `app/test/golden/voice_bar_golden_test.dart` (`recording bar states (dark)`, `recording bar states (light)`).

- **Review.** Fixed after A6:
  - **Playback wasn't paced.**
    - The player decoded the whole file at once and pushed into the call's or the speaker's ring buffer with `let _ =`, dropping what didn't fit. On a real device a message would have played about its first second while its position raced to the end.
    - After the end it kept pulling `wsola`, which replays the input it still holds after `flush`, so every message ended with up to 1.4 s played again.
    - A speed change mid-play applied to nothing, since everything had been stretched already.
    - Now the player decodes a packet at a time, writes a tick only while the output is under 60 ms ahead, never pulls after the flush, and detaches from the call when it stops. `playing_into_a_call_drops_nothing_and_keeps_time` and `a_new_speed_applies_to_what_follows` feed a call-like sink at real time.
  - **Recorder errors.**
    - A microphone that wouldn't open, or an encoder error, became `None`, and `stop` said only "recording produced no draft". Now the error reaches `stop`, and the app hears `DeviceLost` at once.
    - A lost microphone kept nothing; now it keeps what was heard (`a_lost_microphone_keeps_what_was_heard`).
    - The encoder's lookahead error no longer becomes a pre-skip of 0.
    - The capture tap takes whole ticks or none.
  - **The draft preview's play button was a stub** that only flipped a flag. It now plays the draft, and deleting or sending stops it.
  - **The app:**
    - the generated `PlaybackItem` and `VoiceDraft` no longer leak past `core/rust`: the app has its own models in `core/model/voice_message.dart`;
    - the playback state moved from `features/` into `core/providers/playback_state.dart` and no longer applied each event twice;
    - the recorder's level was converted to decibels twice;
    - start and stop failures, and a lost microphone, now reach the member as a toast;
    - dragging on a bubble started a new player per pointer move and now seeks once on release;
    - a playback failure is shown;
    - the speed chip is monochrome (`voice` is for the waveform only) and uses `Hoverable`;
    - a message still being sent can't be played.
  - **The mock now refuses what the core refuses:** a second start, a stop with nothing recorded, a missing draft.
  - **D58 itself:** "max-pooled" (the slices are averaged), `allPermissions` (it is `Permissions.all`), and a working preview that wasn't.

## D59: Media uploads: video and audio playback (Phase 4 A7, 2026-10-20)

- **Why.** Milestone A7 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`, Tasks 32–35) plays video and audio attachments inside the app, and gives uploaded videos a poster and their size and length. Commits: cd70ef1..6c1c29b, and the review fixes after them.
- **FFmpeg** (`app/tool/build_ffmpeg.sh`, `.github/workflows/ci.yml`, `docs/packaging.md`).
  - dav1d 1.5.4 is built first, static, with meson and ninja (pinned SHA-256 `686616b7…a5c5`), and linked into libavcodec.
  - The decoders: `h264, mjpeg, hevc, vp8, vp9, libdav1d, mpeg4, aac, mp3float, opus, vorbis, flac, pcm_s16le, pcm_s24le, pcm_s32le, pcm_f32le, pcm_u8`. The demuxers: `mov, matroska, ogg, mp3, flac, wav, aac`, with their parsers and the `h264_mp4toannexb`, `hevc_mp4toannexb` and `vp9_superframe_split` bitstream filters.
  - swresample is now built: FFmpeg's Opus decoder depends on it and is silently left out without it. The core doesn't link it, so CI gives every bundled FFmpeg library `RUNPATH=$ORIGIN` for libavcodec to find it.
  - There are still no protocols, not even `file`: the player and the upload probe read through `StreamIo`.
  - CI installs meson and ninja.
- **The player engine** (`crates/opencord-media/src/player/{mod.rs,ffmpeg.rs,decode.rs,sync.rs,unsupported.rs}`).
  - `MediaPlayer::open(source, video, audio, events)` runs one `opencord-media-player` thread that demuxes and decodes the best video and audio streams. Video decodes on frame threads.
  - Pictures are scaled to RGBA, at most 1920 on the long side, and turned upright from the stream's display matrix (`rotate`, by 90, 180 or 270). Up to 8 wait to be shown.
  - Sound is converted in Rust from any of `u8`, `i16`, `i32`, `f32` or `f64`, planar or packed, to stereo f32: mono is doubled, and of more channels the first two are kept. rubato resamples it to 48 kHz. Half a second of it is decoded ahead, and it is written to `AudioOut` no more than 60 ms (2,880 frames) ahead of the listener, at the volume set at that moment.
  - The clock (`MediaClock`) is the sound heard: the frames written since the last seek, less those still queued. With no sound, or once the sound has ended, it is the wall clock. A picture is shown once its time is ≤ the clock + 10 ms, and dropped when more than 80 ms late.
  - A seek flushes the decoders and the resampler, and skips what decodes before the target, pictures and sound alike, the sound to the sample. A seek while paused shows the target's picture. Play after the end starts again from 0.
  - Events: `Opened { duration_ms, width, height, has_video, has_audio }`, `Position` every 50 ms while playing and after each seek, `Ended`, and `Failed(String)`.
  - Packets are read with `Packet::read`, so a failed read is `Failed`, not the end. A corrupt packet is skipped. A video stream that can't be decoded fails the open; sound that can't be decoded leaves a video silent.
  - `probe_and_poster(path, max_side)` returns `Probe { width, height, duration_ms, has_video }` (width and height upright) and the first picture as a JPEG (quality 82) at most `max_side` on its long side.
  - `supported()` is true on Linux and Windows. macOS gets `unsupported.rs` until P2.
- **The core** (`crates/opencord-core/src/{media_player.rs,output_sink.rs,now_playing.rs}`, `attachments/{range_reader.rs,upload.rs}`, `api/{attachments.rs,types.rs}`, `video.rs`; `opencord-media/src/audio/engine.rs`).
  - `RangeReader` reads an attachment with `GET /media/attachments/{id}` ranges: its size comes from a `bytes=0-0` request's `Content-Range`. It reads in 1 MiB blocks, keeps the 8 used last, and fetches the next 2 in the background, on cache hits too. A read waits for a block being fetched ahead instead of fetching it again. Only a `206` of the right length is accepted. `on_wait` tells the core while a read waits for the network.
  - `media_open(server_key, attachment_id)` → `MediaHandle { texture_id, duration_ms, width, height, has_video }`: the pictures go to a new Flutter texture (`Video::create_texture`), the sound through `OutputSink`. Then `media_play`, `media_pause`, `media_seek(position_ms)`, `media_set_volume(volume, muted)` and `media_close`, and the sync `media_supported()`. Events reuse `MediaEvent::PlaybackUpdate`, `PlaybackEnded` and `PlaybackFailed` with `PlaybackItem::Attachment`. Play, pause and buffering send the last position heard.
  - `OutputSink` moved to `output_sink.rs` and serves both players. In a call it attaches to the engine's local media slot on its first write and holds the engine weakly. `AudioEngine::attach_local_media` returns an id, and `detach_local_media(id)` and `has_local_media(id)` act only on that source. So a player attaches again when another took the slot, and one stopping no longer cuts off the other. When the call ends, the sound moves to the speaker. The `CallMix` trait is that slot; the tests stand in for it.
  - `NowPlaying` knows which player (`Player::Voice` or `Player::Media`) holds it. Starting one stops (voice) or pauses (media) the other, and `clear` empties the slot only for the player that holds it.
  - On upload, a file that sniffs as video gets `probe_and_poster(path, 800)` (on Linux and Windows). The hints go into `CreateUpload`, and a poster of up to 2 MiB is the body of `POST /media/uploads/{id}/finish` (`content-type: image/jpeg`). If the probe fails the video still uploads, without either, and the failure is logged.
- **The app** (`app/lib/features/chat/attachments/{video_tile.dart,video_player_view.dart,audio_card.dart,attachment_view.dart,media_viewer.dart}`, `app/lib/core/providers/playback_state.dart`, `app/lib/core/model/media_handle.dart`, the repository, `core/format.dart`).
  - `VideoTile`:
    - the stored size, scaled to fit 400×300;
    - the poster, or a `surface` box with `OcIcons.movie`;
    - a 48 px round play button and an `m:ss` badge, light on dark in both themes.
    - A tap opens the viewer on the message's images and videos, in order, at this video.
  - `VideoPlayerView` replaces the viewer's "Video playback coming soon":
    - it opens the media and plays it, showing a `Texture` at the video's aspect ratio (or `OcIcons.musicNote` without pictures) and a spinner while buffering;
    - the control bar has play/pause, a seek slider that seeks on release, `m:ss / m:ss`, mute (`OcIcons.volumeOff`/`volumeUp`) and an 80 px volume slider;
    - Space toggles play, and ←/→ still move between items, closing the media.
    - Without `mediaSupported()` it shows "This video can't play on this computer yet" and Download. A failure shows its message and Retry.
  - `AudioCard` (audio attachments):
    - a 320 px card with play/pause, the filename, the time (or the size until the length is known, or the failure), Download, and a seek slider once the length is known;
    - scrolled away while paused or finished, it closes the media.
  - `PlaybackNotifier` keeps the open media apart from what plays now. Pause, resume and seek go to the media player for it and to the voice player otherwise. `mediaPlay` takes playback back from a voice message, and `mediaClose` closes the media even while a voice message plays.
  - `formatDuration` and `formatDurationMs` (`m:ss`) in `core/format.dart` replace the recording bar's and the voice bubble's copies.
  - `MockRepository` keeps the open media's place while a voice message plays, and starts it again after its end, as the core does.
- **Tests.**
  - `crates/opencord-media/tests/player.rs`: `opens_and_reports_duration_and_size`, `plays_to_the_end_in_real_time` (and plays again from the start), `seeks_accurately` (the sound after a seek to 1 s is 1 s long), `rotation_is_applied`, `probe_and_poster_makes_a_jpeg`, `a_webm_with_opus_opens`, `a_read_that_fails_is_a_failure_not_the_end`, and `the_planned_decoders_are_there`. CI runs them on the FFmpeg it builds.
  - Unit tests: `player::decode` (`a_quarter_turn_moves_the_top_left_to_the_top_right`, `a_second_at_44_1_khz_comes_out_as_a_second_at_48_khz`), `player::sync` (`the_audio_clock_counts_what_was_heard_from_the_seek`, `when_the_sound_ends_time_goes_on_from_its_last_moment`), and `now_playing::tests::a_player_that_stops_leaves_the_other_playing`.
  - `crates/opencord-core/tests/attachments.rs`: `range_reader_reads_any_span` (with the wait signal and read-ahead) and `video_uploads_get_hints_and_a_poster`.
  - `app/test/widget/chat/media_playback_test.dart`: `video_tile_opens_the_viewer_and_plays`, `audio_card_plays_and_seeks`, `unsupported_video_offers_download`, `the_viewer_steps_from_a_video_to_the_messages_image`, and `a_video_a_voice_message_interrupted_plays_again`.
- **Review.** Fixed after A7:
  - **The tests ran on the system's FFmpeg**, which has everything, rather than the build the app ships, which had no `file` protocol and no Opus decoder. On the shipped FFmpeg:
    - no video upload would have had a poster or a size;
    - a WebM or Ogg file with Opus sound would have played silent.
  - **Seeking desynchronized sound and picture.** Only pictures before the target were skipped. The sound played from the keyframe before it, up to a whole GOP early, for the rest of playback. The test passed only because the player reported the target the moment it was asked to seek.
  - **A failed read ended playback as if the file were done**, because ffmpeg-next's packet iterator ends on any error.
  - **The player:**
    - slept 5 ms after every packet, so a seek far from a keyframe took seconds;
    - applied the volume half a second late, as it decoded;
    - flushed the resampler after every frame at the end of the file;
    - set frame threading after opening the decoder, where it does nothing;
    - couldn't play again after the end.
  - **Playing in a call:**
    - The voice player and the media player shared the engine's one local media slot, and each cleared it when it stopped, even after the other had taken it. A video could go silent and freeze: its clock follows the sound, and nothing drained the sound any more.
    - The sink held the call's engine strongly, so leaving the call kept its microphone open until the player closed.
  - **`NowPlaying::clear` stopped whatever played,** so stopping a voice message paused a video.
  - **The core's play and pause events said position 0.**
  - **The range reader** only fetched ahead after a cache miss, so every third block stalled. It fetched a block being read ahead a second time, and it never said it was buffering.
  - **The app:**
    - after a voice message, the video's play button did nothing;
    - closing the viewer after a voice message played left the media open;
    - videos opened the viewer alone, without their message's images;
    - the tile's play button and badge were dark on dark in the light theme;
    - errors other than `RepoException` were caught, and the duration formatter was copied again.
  - **The fixture README lost the voice test recordings' attribution,** which their CC BY-SA licence requires.
  - **D59 itself** named a checksum other than the script's, swresample for the resampling (it is rubato), `render_frame` (it is `present`), `seek(Duration)`, `PlayerEvent::Progress`, a `rotation` field `Probe` doesn't have, `POST /attachments/{key}/finish` with `x-attachment-*` headers, and JPEG quality 85.

## D60: Media uploads: hardening and docs (Phase 4 A8, 2026-10-20)

- **Why.** Milestone A8 of media uploads (`docs/superpowers/plans/2026-10-18-media-uploads.md`, Task 36) closes the Linux milestones: a security pass over `git diff pre..phase4-media`, the transfer measurements, and the docs for people who run a server. Claude did this milestone instead of Gemini, at the user's request.
- **Security pass.** Each check, and what it found:
  - *Secrets and signed URLs in logs:* none. S3 errors drop the request URL (`without_url`) and keep only the `Code` and `Message` of an error body; `S3Settings` has no `Debug`, and `Credentials` hides the secret. The core logs no tokens.
  - *Every media endpoint authorizes:* `media::authorize` verifies the signed media token against this server, its expiry and membership. Uploads answer only to their uploader; files and thumbnails need a sent message that still exists and `VIEW_CHANNEL | READ_HISTORY` in its channel.
  - *Body limits:* every body read is capped: a piece at 8 MiB + 1, a poster at 2 MiB, the older media uploads at their sound limit.
  - *`Content-Disposition`:* RFC 8187 percent-encoding of every byte outside `A-Za-z0-9-._~`. There was no test; `filenames_cannot_break_out_of_the_header` now feeds it a quote, CR LF, a space and `ü`.
  - *The 8 storage requests:* each starts with `require_owner`, as `storage_requests_are_owner_only` checks.
  - *Image limits:* 16,384 pixels a side, 50 megapixels and 512 MiB of allocation, checked from the header before decoding, two jobs at a time; posters at 8,192 a side. **Found:** the animation check opened its GIF and WebP decoders without those limits. It now applies them (`animation_is_found_within_the_limits`).
  - *`local.rs` keys:* only `a/<id>` or `t/<id>` with a non-negative integer id become a path, so nothing escapes the root (`keys_cannot_escape_the_root`).
- **Measured** (`app/tool/check_transfer.sh`, new; `crates/opencord-core/examples/transfer_load.rs`, new). A 1 GiB file to a release server on loopback and back, three runs on a busy laptop (load average about 10): upload 77.8, 94.9 and 58.3 MB/s, download 353.5, 91.2 and 77.0 MB/s, the server's memory growing 60, 72 and 51 MiB. The memory target (< 100 MB) holds; the 80 MB/s speed target was met in one upload run and two download runs, so it needs a run on an idle machine (`docs/performance.md`, "Media uploads (A8)"). Uploads go one 8 MiB piece at a time, each encrypted, flushed to disk and recorded before the answer.
- **Docs.**
  - `docs/attachments.md` (new): where files go, with a recipe per provider in the app's list (Amazon S3, Cloudflare R2, Backblaze B2, Wasabi, MinIO / Custom) and the minimal IAM policy; encryption and `secrets.key`, what it protects and what it doesn't, backing it up, moving it and the error when it's missing; limits and blocked extensions; what "only in the app" does and doesn't; moving between storages; testing, with the S3 tests and the checks by hand from A5, A6 and A7.
  - `docs/protocol.md`: the upload, settings and storage requests, `attachment_ids` and role upload limits, the four events, the seven error codes and their HTTP statuses, `SEND_VOICE_MESSAGES` (bit 14) and the new @everyone default, the limits, and an Attachments section with the upload flow and every file endpoint's answers.
  - `docs/domains.md`: pieces are 8 MiB because Cloudflare refuses requests over 100 MB, so a proxy must pass bodies of at least 9 MB. The plan asked for `client_max_body_size 16m;`; the guide and the app's Add domain dialog already give `50m`, which passes the pieces, so it stays.
  - `README.md`: "Files and storage".
  - `deploy/`: the compose file and the systemd unit show, commented, how to give the server `secrets.key` from outside the data directory (a compose secret at `/run/secrets/opencord-secrets.key`; `LoadCredential` with `OPENCORD_SECRETS_FILE=%d/opencord-secrets.key`). The compose form was checked with `docker compose config`.
- **Also.** `app/tool/e2e_server.sh` starts its servers with `OPENCORD_MUSIC_UDP_PORT=0`, so a check runs while another server holds UDP 7712.
- **Not done here:** the checks by hand from A5, A6 and A7 (listed in `docs/attachments.md`) and scrolling 200 image messages at 60 fps need a display; the transfer speed on an idle machine; the Windows and macOS work (P1, P2).


## D61: Private messages: protocol, shared rules, config and schema (Phase 5 E0, 2026-10-22)

- **Why.** Milestone E0 of private messages (`docs/superpowers/plans/2026-10-22-private-messages.md`, Tasks 1–3, commits `f424002..5911218`) adds the protocol messages, shared cryptographic signing rules and constants, server config section, schema migration, and Ready capability flag that the private messages and friend delivery architecture builds on.
- **Protocol messages.**
  - `proto/opencord/v1/private.proto` defines `DeviceKeys`, `Prekey`, `UserDevices`, `DeviceRef`, `DeviceBundle`, `DeviceCiphertext`, device registration and key claim requests/responses (`RegisterDevice`, `UploadKeys`, `UnregisterDevice`, `DeviceRegistration`, `FetchDevices`, `DeviceLists`, `ClaimKeys`, `ClaimedKeys`), pending offline delivery messages (`FetchPendingEvents`, `PendingEvents`, `AckPendingEvents`), messaging and privacy requests (`SendPrivateMessage`, `PrivateMessageSent`, `StartPrivateTyping`, `UpdatePrivacy`), friend requests and answers (`SendFriendRequest`, `FriendRequestCreated`, `AnswerFriendRequest`, `CancelFriendRequest`, `RemoveFriend`), pending event envelopes (`PendingEvent`, `FriendRequestReceived`, `FriendRequestSent`, `FriendRequestAnswered`, `FriendRequestCancelled`, `FriendRemoved`, `PrivateMessage`, `PrivateTypingStart`), and end-to-end payload bodies (`PrivatePayload`, `PrivateText`, `PrivateEdit`, `PrivateDelete`, `RelayMoved`, `RequestAccepted`, `SessionReset`).
  - Wire formats are integrated into `requests.proto` (Request kinds 180–193, Response results 34–39), `events.proto` (Event kinds 120–121), `gateway.proto` (`Ready.private_messages` = 21), and `models.proto` (ErrorCodes 48–52: `ERROR_CODE_FRIEND_REQUESTS_CLOSED`, `ERROR_CODE_MESSAGE_REQUESTS_CLOSED`, `ERROR_CODE_UNDELIVERABLE`, `ERROR_CODE_DEVICE_LIST_CHANGED`, `ERROR_CODE_PRIVATE_MESSAGES_UNAVAILABLE`).
  - Bridge error codes are mapped across Rust `opencord_core::api::types::ErrorCode` and Dart `RepoErrorKind` (`friendRequestsClosed`, `messageRequestsClosed`, `undeliverable`, `privateMessagesUnavailable`).
- **Signing payloads and shared limits.**
  - `opencord_common::private` defines fixed binary signing layouts using strict Ed25519 verification (`verify_strict`):
    - `device_payload` (`b"opencord-device-v1"` || 16-byte device ID || 32-byte Curve25519 key || 32-byte Ed25519 key = 98 bytes), signed by the owner's identity key via `sign_device` and verified via `verify_device`;
    - `prekey_payload` (`b"opencord-prekey-v1"` || 16-byte device ID || 32-byte key || 1-byte fallback flag = 67 bytes), signed by the device's Ed25519 key via `sign_prekey` and verified via `verify_prekey`;
    - `slip_payload` (`b"opencord-dm-allow-v1"` || 32-byte granter || 32-byte grantee = 84 bytes), signed by the granter via `sign_slip` and verified via `verify_slip`.
  - Slice length checks ensure non-matching lengths return `false` without panicking.
  - Rate limits in `opencord_common::limits`: `FRIEND_REQUEST_RATE` (burst 10, period 600 s), `PRIVATE_MESSAGE_RATE` (burst 5, period 5 s), and `KEY_CLAIM_RATE` (burst 30, period 60 s).
- **Server configuration, migration, and Ready flag.**
  - `[private_messages]` section in `Config` and `DEFAULT_CONFIG_TOML`: `enabled` (default `true`), `undelivered_days` (default `30`, range 1–365), `max_pending_per_device` (default `5000`, range 100–100,000), `device_expiry_days` (default `60`, range 7–365). Overrides via `OPENCORD_PRIVATE_ENABLED`, `OPENCORD_PRIVATE_UNDELIVERED_DAYS`, `OPENCORD_PRIVATE_MAX_PENDING_PER_DEVICE`, and `OPENCORD_PRIVATE_DEVICE_EXPIRY_DAYS`.
  - Migration `0010_private_messages.sql` creates tables `private_privacy`, `private_blocks`, `private_devices`, `private_one_time_keys`, `private_friend_requests`, `private_pending`, and `private_recent_sends`, with indexes for pair uniqueness, pending device queues, and request-based removal.
  - `Ready.private_messages` is populated from `state.config.private_messages.enabled`.
- **Tests.**
  - `crates/opencord-proto/tests/private.rs`: `field_numbers_are_the_planned_ones` pins encoded tags for Request 180, Event 120, Response 34, Ready 21, and error codes 48–52; `pending_events_and_payloads_round_trip` tests encoding/decoding of each `PendingEvent` kind and `PrivatePayload` body variant.
  - `crates/opencord-common/src/private.rs`: `payload_layouts_are_fixed`, `device_signatures_bind_every_field`, `prekey_signatures_bind_the_fallback_flag`, `slips_name_both_people_in_order`, `wrong_lengths_never_verify`.
  - `crates/opencord-server/src/config.rs`: `environment_overrides_the_file`, `private_message_limits_out_of_range_are_refused`, `default_file_matches_default_config`.
  - `crates/opencord-server/tests/private.rs`: `ready_says_whether_private_messages_are_on`.


## D62: Private messages: the server's devices, keys, waiting events, relay and friends (Phase 5 E1, 2026-10-22)

- **Why.** Milestone E1 of private messages (`docs/superpowers/plans/2026-10-22-private-messages.md`, Tasks 4–7, commits `65af9b1..de85f04`). The server registers each person's devices and their keys, keeps events for devices that are offline, relays encrypted messages it can't read, and carries friend requests. Tasks 4–6 were done by Claude through claude-bridge, Task 7 by Gemini.
- **Devices, keys and privacy (Task 4, `65af9b1`).** In `handlers/private.rs`, `private/mod.rs` and `db/private.rs`.
  - `RegisterDevice` takes a `DeviceKeys` (16-byte id, Curve25519 and Ed25519 keys, the identity's signature over them, checked with `verify_device` against the member's own key), one-time keys and a fallback key, each signed by the device (`verify_prekey`). The session then holds that device. Re-registering with new keys drops the old one-time keys in one conditional `DELETE`. An 11th device pushes out the one seen longest ago (ties by `rowid`), and any session holding it lets it go.
  - `UploadKeys` adds one-time keys and may replace the fallback key; the 100-key limit is checked inside the transaction after inserting, and the transaction rolls back if it's over. `UnregisterDevice` deletes the device; its keys and waiting events go with it (`ON DELETE CASCADE`).
  - `FetchDevices` (2 users at most) lists devices. `ClaimKeys` (20 devices at most, `KEY_CLAIM_RATE`) hands out one one-time key per device, oldest first, removed with `DELETE … RETURNING`. When none is left, it hands out the fallback key, marked `fallback`.
  - `UpdatePrivacy` replaces both switches and the block list (10,000 ids at most, repeats dropped) under `state.write_lock()`. `PrivateMessages` keeps them in memory, loaded at startup. Kick and ban call `forget_member` in their transaction, then `PrivateMessages::forget`.
  - `Ctx` gained `session: &Arc<Session>`, so handlers reach the session's device slot.
- **Waiting events (Task 5, `4c0ba47`).** In `private/mod.rs`.
  - `insert_pending` stores each event under a Snowflake id for one device, inside the caller's transaction, and drops a device's oldest events past `max_pending_per_device`. `push_pending`, after the commit, sends each one live to the session holding that device. `deliver` does both in its own transaction.
  - `FetchPendingEvents` pages by `after_id`, 100 events or 786,432 bytes at most, with `more` set when there are more. `AckPendingEvents` (500 ids at most) deletes only the caller's own device's events.
  - An hourly sweep (`sweep_private_messages` in `server.rs`) deletes events older than `undelivered_days`, devices unseen for `device_expiry_days` (sessions holding them let them go), and send ids older than 24 hours.
  - `forget_member` deletes the member's friend requests and the events for them, devices, privacy and blocks.
- **Messages and typing (Task 6, `9d6b4f1`).**
  - `SendPrivateMessage` checks, in order:
    - a device is required, and the rate is limited per sender and recipient (`PRIVATE_MESSAGE_RATE`);
    - the recipient is a member and not the sender;
    - the recipient hasn't blocked the sender (`UNDELIVERABLE`);
    - if the recipient turned message requests off, the sender has the recipient's slip (`MESSAGE_REQUESTS_CLOSED`), since the server keeps no friend list;
    - the copies have their shape: 1 to 20 copies, 32,768 bytes each, no device twice.

    Then, in one transaction:
    - the same `send_id` from the same device within 24 hours stores nothing and answers with the first send's time;
    - the recipient must have devices;
    - the copies must name exactly the recipient's devices and the sender's other devices (`DEVICE_LIST_CHANGED`).

    Each copy goes to its device as a `PrivateMessage` waiting event, and the answer gives the server's time.
  - `StartPrivateTyping` passes the same checks but answers success either way, so typing never reveals a block. It sends a live `PrivateTypingStart` event to the recipient's sessions, never stored.
- **Friend requests (Task 7, `de85f04`).** In `handlers/friends.rs`.
  - `SendFriendRequest` checks, in order:
    - a device is required, and the rate is limited (`FRIEND_REQUEST_RATE`);
    - the target is a member and not the caller;
    - the target hasn't blocked the caller (`UNDELIVERABLE`) and takes friend requests (`FRIEND_REQUESTS_CLOSED`);
    - the target has devices;
    - the slip (the caller's permission for the target) verifies;
    - the caller has fewer than 100 requests waiting, and no request exists between the two in either direction (`CONFLICT`, worded for who sent it).

    In one transaction, it then stores the request, sends `FriendRequestReceived` to the target's devices (tagged with the request id) and `FriendRequestSent` to the caller's other devices.
  - `AnswerFriendRequest`: only the target can answer, else `NOT_FOUND`. An accept needs the answerer's slip for the requester. The request is deleted with `WHERE id = ? AND to_id = ?`; if no row was deleted, the answer is `NOT_FOUND` and nothing is sent. `FriendRequestAnswered` (the slip is empty on a decline) goes to the requester's devices and the answerer's other devices.
  - `CancelFriendRequest`: deleted with `WHERE id = ? AND from_id = ?`, together with its undelivered `FriendRequestReceived` events. `FriendRequestCancelled` goes to the target's devices and the caller's other devices.
  - `RemoveFriend` stores nothing, since the server keeps no friend list. It sends `FriendRemoved` to the target's devices and the caller's other devices.
- **Tests.** `crates/opencord-server/tests/private.rs`, 44 tests, all passing:
  - 13 on devices, keys and privacy;
  - 9 on waiting events;
  - 9 on messages and typing;
  - 13 on friend requests.

  Workspace clippy, `check_platforms.sh`, `flutter analyze` and `flutter test` (957 passed, 1 skipped) also pass.
- **Review.** E1 was checked against the plan on 2026-10-22. The code matched; this entry was rewritten because the first version described a different design for Task 7 (an `accepted_at_ms` column, `FORBIDDEN` errors, a stored friend list) and wrong details for Tasks 4 and 6.


## D63: Private messages: client storage, keys and contacts (Phase 5 E2, 2026-10-23)

- **Why.** Milestone E2 of private messages (`docs/superpowers/plans/2026-10-22-private-messages.md`, Tasks 8–10, commits `aa7eb69..7fde6c6`). The client core stores encrypted data at rest in a SQLite database sealed with keys derived from a master key (kept in the system keychain from Task 16 on, not the identity key), manages device keys and Olm sessions via vodozemac, and tracks contacts, privacy settings and friend states. Tasks 8 and 9 were done by Claude through claude-bridge, Task 10 by Gemini.
- **The private database and sealing (Task 8, `aa7eb69`).** In `crates/opencord-core/src/private/mod.rs`, `seal.rs` and `store.rs`.
  - Migration `0001_private.sql` creates tables `device`, `peer_devices`, `sessions`, `contacts`, `messages`, `received_events`, and `settings`, with indexes `messages_by_conversation`, `messages_sending`, and `received_events_by_age`.
  - `derive_keys` derives `DerivedKeys` (`pickle` and `seal` 32-byte keys) from a 32-byte master key using HMAC-SHA256 with labels `b"opencord-private-pickle-v1"` and `b"opencord-private-seal-v1"`.
  - `Sealer` wraps XChaCha20-Poly1305. `seal` encrypts plaintext with a random 24-byte nonce and authenticates `"opencord-private:" || label || ":" || row`, so a sealed value can't be moved to another column or row. The nonce goes in front. `open` decrypts and authenticates, returning `PrivateError::Crypto` on mismatch or damage.
  - `PrivateStore` opens or creates a SQLite database under `PRIVATE_DIR` (`"private"`) named `<hex public key>.db`. It verifies a sealed setting `key_check` with label `"settings.key_check"` and row `b""`. An unreadable, corrupt, or wrong-key database is moved aside to `<name>.unreadable-<now_ms>`, logged without the file path, and started fresh (`started_fresh` in `Opened`).
  - `PrivateError` (`Storage`, `Invalid`, `Crypto`) implements `From` for `sqlx::Error` and `std::io::Error`, and converts into `CoreError` (`Invalid` to `CoreError::InvalidInput`, `Storage` and `Crypto` to `CoreError::Storage`).
- **Keys, the account and sessions (Task 9, `dccd83b`).** In `crates/opencord-core/src/private/device.rs`, `sessions.rs`, `identity.rs`, using `vodozemac` 0.11.1.
  - `Device::load_or_create` loads the single row from `device` or generates a 16-byte random `device_id` and a `vodozemac::olm::Account` encrypted with the `pickle` key. `keys` returns `proto::DeviceKeys` signed by the owner via `Identity::sign_device`. `fallback_prekey` returns the signed fallback prekey. `new_one_time_keys` generates signed one-time keys without marking them published, preserving the fallback key in vodozemac 0.11.1. `rotate_fallback_if_due` rotates the fallback prekey after 30 days (`FALLBACK_ROTATION_MS`). `save` persists the account pickle conditionally (`rows_affected() == 1`). `Identity::slip_for` creates 64-byte permission slips.
  - `PeerSessions` loads up to 5 sessions per peer Curve25519 key. `encrypt` picks the most recently active session (latest `last_received_at`, else `created_at`) and encrypts via `vodozemac::olm::Session::encrypt`. `decrypt` decrypts incoming messages, handling duplicates (`Decrypted::Duplicate`), failures (`Decrypted::Failed`), or plaintext (`Decrypted::Plaintext`); for prekey messages (`olm_type` 0), it tries existing sessions first and falls back to creating an inbound session with `Account::create_inbound_session`. `add_outbound` creates outbound sessions with `Account::create_outbound_session`. `save` prunes sessions to the 5 with the latest `MAX(created_at, COALESCE(last_received_at, created_at))`.
- **Contacts and their rules (Task 10, `7fde6c6`).** In `crates/opencord-core/src/private/contacts.rs`.
  - `Contact` stores peer state (`public_key`, sealed `name` with label `CONTACT_NAME_LABEL`, `friend_state` as `FriendState`, `request` as `RequestRef`, `friend_server_id`, `blocked`, `declines`, `conversation` as `Conversation`, `relay_server_id`, `their_slip`, `last_activity_at`, `last_read_at`, `muted`, `hidden`).
  - Pure rules evaluate transitions: `is_accepted` checks for `FriendState::Friends` or `Conversation::Accepted`; `incoming_message` returns `IncomingMessage` (`Drop`, `Request`, `Conversation`); `incoming_request` returns `IncomingRequest` (`Drop`, `AutoAccept`, `CrossedAccept`, `Spam`, `Pending`), treating requests after `SPAM_AFTER_DECLINES` declines as spam; `after_my_request` sets `FriendState::Outgoing` and accepts conversation; `after_my_answer` sets `FriendState::Friends` on accept or increments `declines` on decline; `after_my_cancel` and `after_block` reset friend state.
  - Storage functions: `load` and `load_all` read contacts and unseal names; `save` upserts contact rows sealing `name`; `delete` removes a contact; `privacy` and `set_privacy` read and write `PrivacySettings` (`allow_friend_requests`, `allow_message_requests`) via settings keys `PRIVACY_FRIEND_REQUESTS` and `PRIVACY_MESSAGE_REQUESTS`.
  - `apply_friend_event` applies incoming `FriendEvent` variants (`Received`, `Sent`, `Answered`, `Cancelled`, `Removed`), verifies slips, updates contact records and names, and returns `FriendOutcome` with optional `FollowUp::Answer` and `FollowUp::Cancel` commands.
- **Tests.** 32 tests across `crates/opencord-core`:
  - 8 on database storage, sealing, key derivation and corrupted file recovery in `private::store` and `private::seal`;
  - 13 on device keys, one-time key generation, Olm sessions, duplication, rotation, and signatures in `private::device`, `private::sessions`, and `identity`;
  - 11 on contact states, incoming message/request rules, decline thresholds, slip retention, crossed requests, sealed names, and privacy defaults in `private::contacts`.

  Workspace clippy, `app/tool/check_platforms.sh`, `dart format`, `flutter analyze` and `flutter test` (957 passed, 1 skipped) also pass.
- **Review.** E2 was checked against the plan on 2026-10-23. Task 10 matches the plan's rules and its table of friend events, with all 11 tests. This entry was corrected in two places: the keys come from a master key, not the identity key, and what sealing authenticates.
