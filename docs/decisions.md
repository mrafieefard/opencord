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
