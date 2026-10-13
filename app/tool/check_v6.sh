#!/usr/bin/env bash
# Phase 2 V6 against a real server with the real app: a voicebot in General
# shares a screen of real H.264; the app (integration_test/v6_test.dart)
# joins, watches it, and Flutter draws the texture. A second test has the
# app go live with a virtual screen and a bot watching it, checking the
# viewer count. This machine's own screen stays off. Its own server, data
# folders and profile, so it never touches yours.
#
#   app/tool/check_v6.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"

# A virtual screen for the app to share.
cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-media
virtual_screen="$repo/target/release/examples/virtual_camera"
screen_log="$work/screen.log"

export OPENCORD_PROFILE=e2e_screen
cd "$app"

# Test 1: app watches a bot's screen share
claim_and_invite "$bot"

"$bot" --server "$invite" --name Sharer --tone 0 --screen 720p30 --seconds 600 \
  >"$work/sharer.log" 2>&1 &
background+=("$!")

flutter test integration_test/v6_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_TEST=watches \
  >"$work/v6_watches.log" 2>&1 || { cat "$work/v6_watches.log" >&2; exit 1; }
echo "V6 watches check passed."

# Test 2: app shares a screen, bot watches it
# Restart the server fresh.
stop_background
rm -rf "$work"
work="$(mktemp -d)"
background=()
port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"
server="$repo/target/debug/opencord-server"
mkdir -p "$work/server"
(
  cd "$work/server"
  OPENCORD_BIND="127.0.0.1:$port" OPENCORD_PUBLIC_HOST=127.0.0.1 \
    OPENCORD_VOICE_UDP_PORT=0 \
    exec "$server"
) >"$work/server.log" 2>&1 &
background+=("$!")
claim=""
for _ in $(seq 1 150); do
  claim="$(sed 's/\x1b\[[0-9;]*m//g' "$work/server.log" |
    grep -oE 'claim_token=[A-Za-z0-9_-]+' | head -1 | cut -d= -f2 || true)"
  [[ -n "$claim" ]] && break
  sleep 0.2
done
"$bot" --server "127.0.0.1:$port" --claim "$claim" --name Bot --tone 0 \
  --watch-streams --seconds 600 >"$work/bot.log" 2>&1 &
background+=("$!")

# Start the virtual screen for the app to share.
export OPENCORD_SCREEN_NODE=opencord-virtual-screen
"$virtual_screen" screen >"$screen_log" 2>&1 &
background+=("$!")

sleep 1

# Get the invite from the server.
(
  cd "$work/server"
  OPENCORD_BIND="127.0.0.1:$port" OPENCORD_PUBLIC_HOST=127.0.0.1 \
    OPENCORD_VOICE_UDP_PORT=0 \
    exec "$server"
) >"$work/server2.log" 2>&1 &
background+=("$!")
for _ in $(seq 1 150); do
  invite="$(sed 's/\x1b\[[0-9;]*m//g' "$work/server2.log" |
    grep -oE 'invite=[^ ]+' | head -1 | cut -d= -f2- || true)"
  [[ -n "$invite" ]] && break
  sleep 0.2
done

export XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/config"

flutter test integration_test/v6_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_TEST=shares \
  --dart-define=OPENCORD_SCREEN_NODE="$OPENCORD_SCREEN_NODE" \
  >"$work/v6_shares.log" 2>&1 || { cat "$work/v6_shares.log" >&2; exit 1; }
echo "V6 shares check passed."
