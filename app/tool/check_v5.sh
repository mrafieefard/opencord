#!/usr/bin/env bash
# Phase 2 V5 against a real server with the real app: a voicebot in General
# sends a camera of real H.264; the app (integration_test/v5_test.dart)
# joins, shows it in the bot's tile, and Flutter draws its texture. This
# machine's camera stays off. Its own server, data folders and profile, so
# it never touches yours.
#
#   app/tool/check_v5.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"

# Claim the server, then start it again: a server with an owner logs its
# invite link at startup.
"$bot" --server "127.0.0.1:$port" --claim "$claim" --name Owner --tone 0 \
  --seconds 1 >"$work/claim.log"
kill "${background[0]}"
wait "${background[0]}" 2>/dev/null || true
(
  cd "$work/server"
  OPENCORD_BIND="127.0.0.1:$port" OPENCORD_PUBLIC_HOST=127.0.0.1 \
    OPENCORD_VOICE_UDP_PORT=0 \
    exec "$repo/target/debug/opencord-server"
) >"$work/server2.log" 2>&1 &
background+=("$!")
invite=""
for _ in $(seq 1 150); do
  invite="$(sed 's/\x1b\[[0-9;]*m//g' "$work/server2.log" |
    grep -oE 'invite=[^ ]+' | head -1 | cut -d= -f2- || true)"
  [[ -n "$invite" ]] && break
  sleep 0.2
done
if [[ -z "$invite" ]]; then
  echo "The server logged no invite:" >&2
  cat "$work/server2.log" >&2
  exit 1
fi

"$bot" --server "$invite" --name Camera --tone 0 --camera --seconds 600 \
  >"$work/camera.log" 2>&1 &
background+=("$!")

export OPENCORD_PROFILE=e2e_video
cd "$app"
flutter test integration_test/v5_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite"
echo "V5 check passed."
