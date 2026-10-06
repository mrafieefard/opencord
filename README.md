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

## Changing the Rust API used by the app

Public functions in `crates/opencord-core/src/api/` are exposed to Dart. After changing them, regenerate the bindings and commit the result (`app/lib/src/rust/` and `crates/opencord-core/src/frb_generated.rs`):

```bash
cd app
flutter_rust_bridge_codegen generate
```

## License

- The server and anything not listed below: [AGPL-3.0-or-later](LICENSE).
- The client app, client core, shared crates and protocol definitions (`app/`, `crates/opencord-core`, `crates/opencord-common`, `crates/opencord-proto`, `proto/`): MPL-2.0, see the `LICENSE` file in each of those directories.

The reasoning is in [docs/decisions.md](docs/decisions.md).
