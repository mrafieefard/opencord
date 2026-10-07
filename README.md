# Opencord

Open-source, self-hostable voice, video and text chat. It pairs Discord-style UX (servers, channels, roles) with the TeamSpeak hosting model: anyone can run a server, the client connects to many servers at once, and your identity is a keypair on your device instead of a central account.

> **Status:** early development (Phase 1, milestone M0). Nothing is usable yet. See [the Phase 1 plan](docs/plans/phase-1.md).

## Repository layout

| Path | Contents |
|---|---|
| `crates/opencord-server` | Server binary |
| `crates/opencord-core` | Client core in Rust (networking, protocol, identity), used by the app through flutter_rust_bridge |
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

The Phase 1 acceptance checks run the app against a real server. Each
starts its own server on a free port, with its own data folders and
profile, so it leaves your server and data alone (on Linux the keychain
item is shared; see D23 in `docs/decisions.md`). They open the app window,
so they need a desktop session:

```bash
app/tool/check_m5.sh  # a fresh install joins as owner, restarts, is still connected
app/tool/check_m6.sh  # two identities chat; a new role and restriction arrive live
```

## Running a server

```bash
cargo run -p opencord-server
```

On first start the server writes `opencord.toml` with commented defaults, creates `data/` (database and a self-signed TLS certificate), and logs the certificate fingerprint and a one-time **owner claim token**. Add the server in the app with that token to become its owner. Every setting can also be set with an `OPENCORD_*` environment variable (see the comments in `opencord.toml`).

Other commands:

```bash
cargo run -p opencord-server -- fingerprint
cargo run -p opencord-server -- invite create --max-uses 10 --expires-in 86400
cargo run -p opencord-server -- reset-claim-token
```

`reset-claim-token` is for recovery: whoever uses the new token becomes the owner, even if the server already has one.

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
- The client app, client core, shared crates and protocol definitions (`app/`, `crates/opencord-core`, `crates/opencord-common`, `crates/opencord-proto`, `proto/`): MPL-2.0, see the `LICENSE` file in each of those directories.

The reasoning is in [docs/decisions.md](docs/decisions.md).
