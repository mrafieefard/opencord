# Decision log

Decisions not covered by [the Phase 1 plan](plans/phase-1.md), and the reasons for them. Newest last.

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
