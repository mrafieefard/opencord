# Opencord

Open-source, self-hostable voice, video and text chat. It pairs Discord-style UX (servers, channels, roles) with the TeamSpeak hosting model: anyone can run a server, the client connects to many servers at once, and your identity is a keypair on your device instead of a central account.

> **Status:** early development. Text chat works ([Phase 1](docs/plans/phase-1.md)); voice and video are in progress ([Phase 2](docs/plans/phase-2.md)).

## Repository layout

| Path | Contents |
|---|---|
| `crates/opencord-server` | Server binary, and the `opencord-voice-node` binary for voice on other machines |
| `crates/opencord-voice` | Voice node: voice gateway and the SFU that forwards media, embedded in the server or run on its own |
| `crates/opencord-core` | Client core in Rust (networking, protocol, identity), used by the app through flutter_rust_bridge |
| `crates/opencord-media` | Client media engine (so far the voice transport) |
| `crates/opencord-voicebot` | Headless voice client for tests and for checking a server's voice |
| `crates/opencord-proto` | Protobuf types generated from `proto/` |
| `crates/opencord-common` | Code shared by the server and the client core |
| `proto/` | Protocol definitions |
| `app/` | Flutter client for Linux, Windows, macOS, Android and iOS |
| `docs/` | Architecture, protocol, decision log and plans |

## Development setup (Arch Linux)

```bash
sudo pacman -S --needed rustup clang cmake ninja gtk3 pkgconf sqlite git openssl
rustup default stable
rustup component add clippy rustfmt
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
cargo install sqlx-cli --no-default-features --features sqlite,rustls
cargo install flutter_rust_bridge_codegen --version 2.13.0 --locked
git clone https://github.com/flutter/flutter.git -b stable ~/.local/share/flutter
echo 'export PATH="$HOME/.local/share/flutter/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
flutter config --enable-linux-desktop
flutter doctor
```

- The app build needs **rustup**, not only a Rust compiler: it compiles the Rust core through `rustup run`. If Arch's `rust` package is installed, pacman offers to replace it with `rustup`.
- The `flutter_rust_bridge_codegen` version must match the `flutter_rust_bridge` version in `Cargo.toml` and `app/pubspec.yaml`.
- On zsh or fish, add the two `PATH` lines to that shell's config instead of `~/.bashrc`.
- Android and iOS toolchains are not needed in Phase 1.

## Build and run

```bash
cargo build --workspace
cd app
flutter run -d linux
```

The first `flutter run` also compiles the Rust core, so it takes a while.

Two environment variables help while developing:

- `OPENCORD_MOCK=1` (or `--mock`) runs the app on built-in sample data
  instead of the Rust core, to work on the UI without a server.
- `OPENCORD_PROFILE=<name>` runs a separate copy with its own data,
  keychain entries and window, so two users can be tried on one machine:
  run a server, then `flutter run -d linux` in one terminal and
  `OPENCORD_PROFILE=friend flutter run -d linux` in another.

Only one Opencord runs at a time: launching it again brings the running
window forward and hands over any `opencord://` link it was given. To let
the browser open invite links in a local build on Linux, install
`app/linux/packaging/dev.opencord.opencord.desktop` into
`~/.local/share/applications/` with `Exec=` pointing at
`app/build/linux/x64/debug/bundle/opencord`, then run
`xdg-mime default dev.opencord.opencord.desktop x-scheme-handler/opencord`.

## Checks

CI runs these on pushes to `main` and `pre` and on pull requests.

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cd app
flutter analyze
flutter test
flutter test integration_test -d linux
```

The acceptance checks run the app against a real server. Each starts its
own server on a free port, with its own data folders and profile, so it
leaves your server and data alone (on Linux the keychain item is shared;
see D23 in `docs/decisions.md`). They open the app window, so they need a
desktop session:

- `check_m5.sh`: a fresh install joins as owner, restarts, and is still connected.
- `check_m6.sh`: two identities chat; a new role and restriction arrive live.
- `check_v0.sh`: voice states without media. The owner joins voice and it shows live; the member joins, is moved, then disconnected.
- `check_v2.sh`: voice media with this machine's audio devices. The app joins voice and its connection comes up through the server's voice node; nothing is played.

```bash
app/tool/check_m5.sh
app/tool/check_m6.sh
app/tool/check_v0.sh
app/tool/check_v2.sh
```

## Running a server

```bash
cargo run -p opencord-server
```

On first start the server writes `opencord.toml` with commented defaults, creates `data/` (database, a self-signed TLS certificate and `voice-signing.key`, which signs voice tokens; keep both keys private), and logs the certificate fingerprint and a one-time **owner claim token**. Add the server in the app with that token to become its owner. Every setting can also be set with an `OPENCORD_*` environment variable (see the comments in `opencord.toml`).

Other commands:

```bash
cargo run -p opencord-server -- fingerprint
cargo run -p opencord-server -- invite create --max-uses 10 --expires-in 86400
cargo run -p opencord-server -- reset-claim-token
```

`reset-claim-token` is for recovery: whoever uses the new token becomes the owner, even if the server already has one.

### Voice

Voice runs inside the server. Media goes over UDP port 7711, so forward that port on your router as well as the server's TCP port. Clients send media to the host they connected to; set `public_address` under `[voice]` in `opencord.toml` if that is not right for your network.

To check a server's voice, run two voicebots in the same channel. Each plays a tone and reports what it heard from the other, by user id:

```bash
cargo run -p opencord-voicebot -- --server "opencord://chat.example.com:7710/invite/CODE" --name Bot1 --tone 440 --seconds 10
cargo run -p opencord-voicebot -- --server "opencord://chat.example.com:7710/invite/CODE" --name Bot2 --tone 660 --seconds 10
```

### Voice on other machines

A voice node carries voice for the main server from another machine. The main server sends each voice channel to the node with the fewest people. When a node stops, its calls move to another node, and clients reconnect on their own.

On the node's machine, make a shared secret, and start the node once so it writes `opencord-voice-node.toml`:

```bash
cargo run -p opencord-server --bin opencord-voice-node -- generate-secret voice.secret
cargo run -p opencord-server --bin opencord-voice-node
```

In `opencord-voice-node.toml`, set `main_server` to the main server's `host:port`, and `main_fingerprint` to what `opencord-server fingerprint` prints there (leave it unset if the main server has a certificate from a public certificate authority). Set `endpoint` to the address clients reach the node at, for example `wss://voice1.example.com:7712`. Forward TCP port 7712 and UDP port 7711 to the node.

On the main server, copy `voice.secret` next to `opencord.toml` as `voice1.secret` (a name for each node) and list the node, with the same endpoint:

```toml
[voice]
mode = "external"

[[voice.external_nodes]]
endpoint = "wss://voice1.example.com:7712"
secret_file = "voice1.secret"
```

`mode = "external"` uses only external nodes; with the default `embedded`, the server's own node takes calls too. Restart both. The main server logs `a voice node registered` when the node has connected.

## Changing database queries

SQL queries are checked at compile time. Builds use the query data committed in `.sqlx/`, so no database is needed unless you change a query or a migration. Then create a development database and regenerate the data:

```bash
export DATABASE_URL="sqlite://$PWD/target/sqlx-dev.db"
sqlx database create
sqlx migrate run --source crates/opencord-server/migrations
cargo sqlx prepare --workspace -- --all-targets
```

## Changing the Rust API used by the app

Public functions in `crates/opencord-core/src/api/` are exposed to Dart. After changing them, regenerate the bindings and commit the result (`app/lib/src/rust/`, including the `*.freezed.dart` files, and `crates/opencord-core/src/frb_generated.rs`). The generator runs `build_runner` itself:

```bash
cd app
flutter_rust_bridge_codegen generate
```

## License

- The server and anything not listed below: [AGPL-3.0-or-later](LICENSE).
- The client app, client core, media engine, voicebot, shared crates and protocol definitions (`app/`, `crates/opencord-core`, `crates/opencord-media`, `crates/opencord-voicebot`, `crates/opencord-common`, `crates/opencord-proto`, `proto/`): MPL-2.0, see the `LICENSE` file in each of those directories.

The reasoning is in [docs/decisions.md](docs/decisions.md).
