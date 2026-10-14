#!/usr/bin/env bash
# Others' video in the app on macOS (docs/plans/macos.md M4): a voicebot's
# camera and another voicebot's screen stream (real H.264 both), shown by
# the app (integration_test/macos/receive_video_test.dart): drawn, the
# stream opened large when watched, nothing received once the voice view is
# gone. Flutter's snapshots leave textures out, so colours are checked by
# eye (§7, M4). This Mac's camera and screen stay off. Its own
# server, data folders and profile, so it never touches yours.
#
#   app/tool/macos/check_receive_video.sh
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/profile.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"
claim_and_invite "$bot"

"$bot" --server "$invite" --name Camera --tone 0 --camera --seconds 600 \
  >"$work/camera.log" 2>&1 &
background+=("$!")
"$bot" --server "$invite" --name Sharer --tone 0 --screen 720p30 \
  --seconds 600 >"$work/sharer.log" 2>&1 &
background+=("$!")

export OPENCORD_PROFILE=e2e_receive_video
fresh_profile e2e_receive_video
cd "$app"
if ! flutter test integration_test/macos/receive_video_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" >"$work/receive.log" 2>&1; then
  cat "$work/receive.log" >&2
  exit 1
fi
grep -oE 'RECEIVE [A-Z]+: .*' "$work/receive.log"
echo "Receive video check passed."
