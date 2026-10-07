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

