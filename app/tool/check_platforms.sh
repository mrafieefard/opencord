#!/usr/bin/env bash
# Checks from Linux that the client crates still build where Linux's code
# doesn't run (docs/plans/platforms.md §2): clippy with every target and
# -D warnings for x86_64-unknown-freebsd. That target takes every path a
# system without a backend takes (each area's `unsupported.rs`, which the
# macOS and Windows stubs mirror) and none of Linux's, so it catches shared
# code that only builds on Linux. It cannot see inside the macOS or Windows
# files; their own machines build those.
#
#   app/tool/check_platforms.sh
#
# Needs the target (`rustup target add x86_64-unknown-freebsd`) and gcc. The
# C dependencies build with the host's compiler and headers, which is
# enough for checking.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_INCREMENTAL=0 SQLX_OFFLINE=true
export CC_x86_64_unknown_freebsd=gcc
# aws-lc wants the GNU extensions glibc hides by default.
export CFLAGS_x86_64_unknown_freebsd=-D_GNU_SOURCE
# libspa-sys (cpal's PipeWire) runs bindgen over the host's headers.
export BINDGEN_EXTRA_CLANG_ARGS=--target=x86_64-unknown-linux-gnu
export PKG_CONFIG_ALLOW_CROSS=1

rustup target list --installed | grep -qx x86_64-unknown-freebsd ||
  rustup target add x86_64-unknown-freebsd

cargo clippy --manifest-path "$repo/Cargo.toml" --target x86_64-unknown-freebsd \
  -p opencord_core -p opencord-media -p opencord-voicebot --all-targets \
  -- -D warnings
echo "The client crates build off Linux."
