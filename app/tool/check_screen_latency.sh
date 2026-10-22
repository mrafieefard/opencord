#!/usr/bin/env bash
# How long the app's screen share takes from capture to a viewer's decoded
# picture (plan §16.1: capture to display within 250 ms on a LAN). The app
# (integration_test/v6_test.dart, "shares") goes live with a virtual
# screen whose every frame carries the time it was made; a voicebot watches
# it in a 1080 px tile, decodes it, and reads the time back. Release
# server and bot. Its own server, data folders and profile.
#
#   app/tool/check_screen_latency.sh
set -euo pipefail
E2E_SERVER_PROFILE=release
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-media \
  --features testing --example virtual_camera
bot="$repo/target/release/opencord-voicebot"

claim_and_invite "$bot" --server-screen-cap 1080p60
"$bot" --server "$invite" --name Watcher --tone 0 --watch-streams --watch 1080 \
  --seconds 75 >"$work/watcher.log" 2>&1 &
watcher=$!
background+=("$watcher")

"$repo/target/release/examples/virtual_camera" screen 1920 1080 30 >"$work/screen.log" 2>&1 &
background+=("$!")
screen_node=""
for _ in $(seq 1 100); do
  screen_node="$(grep -oE '^screen=.+' "$work/screen.log" | cut -d= -f2- || true)"
  [[ -n "$screen_node" ]] && break
  sleep 0.1
done
if [[ -z "$screen_node" ]]; then
  echo "The virtual screen did not start:" >&2
  cat "$work/screen.log" >&2
  exit 1
fi

export OPENCORD_PROFILE=e2e_screen_latency OPENCORD_SCREEN_NODE="$screen_node"
cd "$app"
if ! flutter test integration_test/v6_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_TEST=shares \
  --dart-define=OPENCORD_E2E_HOLD_SECONDS=30 \
  --dart-define=OPENCORD_SCREEN_NODE="$screen_node" \
  >"$work/share.log" 2>&1; then
  cat "$work/share.log" >&2
  exit 1
fi
wait "$watcher" || true
if ! grep -q "made to decoded" "$work/watcher.log"; then
  echo "The watcher read no stamped pictures:" >&2
  cat "$work/watcher.log" >&2
  exit 1
fi
grep -E "made to decoded|pictures" "$work/watcher.log"
