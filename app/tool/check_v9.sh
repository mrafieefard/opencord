#!/usr/bin/env bash
# Phase 2 V9 against a real server with the real app: a voicebot in General
# writes in the channel's chat. The app (integration_test/v9_test.dart)
# joins General, sees the voice row's unread messages, opens the chat beside
# the tiles and reads it, hides the bot's tile (it has no video), and shows
# its connection's bars and numbers from the real voice node. Its own
# server, data folders and profile, so it never touches yours.
#
#   app/tool/check_v9.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"

claim_and_invite "$bot"

"$bot" --server "$invite" --name Talker --tone 440 --seconds 600 \
  --say "can you read me?" --say-every 5 >"$work/talker.log" 2>&1 &
background+=("$!")

export OPENCORD_PROFILE=e2e_v9
cd "$app"
if ! flutter test integration_test/v9_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  >"$work/v9.log" 2>&1; then
  cat "$work/v9.log" >&2
  cat "$work/talker.log" >&2
  exit 1
fi
grep -E "^STATS" "$work/v9.log" || true
echo "V9 check passed."
