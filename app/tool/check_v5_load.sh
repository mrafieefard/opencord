#!/usr/bin/env bash
# Phase 2 V5's acceptance with the real app (plan §16 V5): nine people in a
# call with every camera on. Eight voicebots (release builds) send cameras
# of real H.264 and watch everyone's, one in 720p tiles, one in 360p and
# the rest in 180p, so every camera sends all its layers. The app
# (integration_test/v5_load_test.dart, a profile build) joins, turns its
# camera on (a virtual PipeWire camera in MJPEG: this machine's real camera
# stays off) and reports how smoothly each tile's pictures came, its CPU
# and memory, and how long the camera took to turn on and off. Its own
# server, data folders and profile, so it never touches yours.
#
#   app/tool/check_v5_load.sh
set -euo pipefail
E2E_SERVER_PROFILE=release
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-media \
  --features testing --example virtual_camera
bot="$repo/target/release/opencord-voicebot"

claim_and_invite "$bot"

for number in 1 2 3 4 5 6 7 8; do
  case "$number" in
    1) watch=720 ;;
    2) watch=360 ;;
    *) watch=180 ;;
  esac
  "$bot" --server "$invite" --name "Camera $number" --tone 0 --camera \
    --watch "$watch" --seconds 900 >"$work/camera$number.log" 2>&1 &
  background+=("$!")
done

# The server lets one address identify 5 times a minute, then once every
# 12 s, and the bots retry until they are in. Then the app's turn.
joined=0
for _ in $(seq 1 300); do
  joined="$(cat "$work"/camera*.log | grep -c '^joined' || true)"
  [[ "$joined" -ge 8 ]] && break
  sleep 0.5
done
if [[ "$joined" -lt 8 ]]; then
  echo "Only $joined of 8 voicebots joined:" >&2
  tail -n 3 "$work"/camera*.log >&2
  exit 1
fi
sleep 13

"$repo/target/release/examples/virtual_camera" mjpeg >"$work/virtual_camera.log" 2>&1 &
background+=("$!")
camera=""
for _ in $(seq 1 50); do
  camera="$(grep -oE 'camera=.+' "$work/virtual_camera.log" | head -1 | cut -d= -f2- || true)"
  [[ -n "$camera" ]] && break
  sleep 0.2
done
if [[ -z "$camera" ]]; then
  echo "The virtual camera did not start:" >&2
  cat "$work/virtual_camera.log" >&2
  exit 1
fi

export OPENCORD_PROFILE=e2e_video_load
# The Camera portal would ask whoever sits here for access; the app reads
# the virtual camera from the PipeWire session instead.
export OPENCORD_CAMERA_PORTAL=0
rm -f "$app/build/integration_response_data.json"
cd "$app"
if ! flutter drive --profile -d linux \
  --driver=test_driver/integration_test.dart \
  --target=integration_test/v5_load_test.dart \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_CAMERA="$camera"; then
  for log in "$work"/camera*.log; do
    echo "== $log" >&2
    tail -5 "$log" >&2
  done
  exit 1
fi
# flutter drive can say all passed for a test that timed out; the test
# reports its numbers only once its checks held.
if ! grep -q camera_on_ms build/integration_response_data.json 2>/dev/null; then
  echo "The test reported no results." >&2
  exit 1
fi
cat build/integration_response_data.json
echo
echo "V5 load check passed."
