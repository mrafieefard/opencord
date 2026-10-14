#!/usr/bin/env bash
# V4 on the Mac (docs/plans/macos.md M3, step 8), in two parts:
# 1. The shared check_v4.sh as it is: a voicebot's synthetic three-layer
#    camera (checksummed, not H.264) watched in a 360-pixel tile; every
#    picture intact, none of the top layer. It checks the transport.
# 2. The same with real H.264 through the macOS codec (VideoToolbox): a
#    voicebot's camera (--camera, the moving test scene at 720p30 through
#    the app's encoders), and a watcher that decodes every picture
#    (VideoToolbox's decoder).
# Its own servers and data folders, so it never touches yours.
#
#   app/tool/macos/check_v4.sh
set -euo pipefail
"$(dirname "$0")/run_check.sh" v4

source "$(dirname "$0")/../e2e_server.sh"
cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"
claim_and_invite "$bot"

"$bot" --server "$invite" --name Camera --camera --seconds 14 \
  >"$work/camera.log" 2>&1 &
camera=$!
background+=("$camera")
sleep 2
"$bot" --server "$invite" --name Watcher --tone 0 --watch 360 --seconds 10 \
  >"$work/watcher.log" 2>&1
wait "$camera"

seen="$(grep -E 'pictures' "$work/watcher.log" || true)"
echo "real H.264: $seen"
if [[ -z "$seen" ]]; then
  cat "$work/camera.log" "$work/watcher.log" >&2
  echo "The watcher saw no video." >&2
  exit 1
fi
read -r pictures decoded high < <(
  sed -E 's/.*: ([0-9]+) pictures, [0-9]+ intact, ([0-9]+) decoded.*by layer [0-9]+\/[0-9]+\/([0-9]+)$/\1 \2 \3/' \
    <<<"$seen"
)
if ((pictures < 100 || decoded != pictures || high != 0)); then
  echo "Expected over 100 pictures, every one decoded, none of the top layer." >&2
  exit 1
fi
echo "V4 check (real H.264) passed."
