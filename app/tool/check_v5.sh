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

claim_and_invite "$bot"

"$bot" --server "$invite" --name Camera --tone 0 --camera --seconds 600 \
  >"$work/camera.log" 2>&1 &
background+=("$!")

export OPENCORD_PROFILE=e2e_video
cd "$app"
flutter test integration_test/v5_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite"
echo "V5 check passed."
