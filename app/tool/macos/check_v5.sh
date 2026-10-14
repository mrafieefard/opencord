#!/usr/bin/env bash
# V5's camera on the Mac (docs/plans/macos.md M5, step 3): the app
# (integration_test/macos/camera_test.dart) turns this Mac's camera on in a
# call; three voicebots watch it in 180-, 360- and 720-pixel tiles, so each
# of its three layers is received, and each must decode every picture it
# gets. Camera on and off are timed by the app (under 1 s each); the app's
# CPU is sampled while the camera is on. Needs you at the Mac: a camera,
# and camera access for the terminal app. Its own server, data folders and
# profile, so it never touches yours.
#
#   app/tool/macos/check_v5.sh
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/profile.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"
claim_and_invite "$bot"

export OPENCORD_PROFILE=e2e_camera
fresh_profile e2e_camera
cd "$app"
flutter test integration_test/macos/camera_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" >"$work/camera.log" 2>&1 &
test_pid=$!
background+=("$test_pid")

until grep -q 'PHASE: camera' "$work/camera.log" 2>/dev/null; do
  if ! kill -0 "$test_pid" 2>/dev/null; then
    cat "$work/camera.log" >&2
    exit 1
  fi
  sleep 0.2
done
watchers=()
for height in 180 360 720; do
  "$bot" --server "$invite" --name "Watcher$height" --tone 0 \
    --watch "$height" --seconds 10 >"$work/watch$height.log" 2>&1 &
  watchers+=("$!")
done
# The app's CPU while the camera is on: a sample a second.
cpu=()
for _ in $(seq 1 8); do
  sleep 1
  cpu+=("$(ps -A -o %cpu=,comm= | awk '/Opencord.app\/Contents\/MacOS\/Opencord$/ && !found {print $1; found = 1}')")
done
for watcher in "${watchers[@]}"; do
  wait "$watcher"
done
if ! wait "$test_pid"; then
  cat "$work/camera.log" >&2
  exit 1
fi
grep -oE 'CAMERA [A-Z]+: .*' "$work/camera.log"
echo "Opencord CPU while the camera is on (%, one core = 100): ${cpu[*]}"

status=0
for height in 180 360 720; do
  seen="$(grep -E 'pictures' "$work/watch$height.log" || true)"
  echo "watching at $height: ${seen:-nothing}"
  if [[ -z "$seen" ]]; then
    status=1
    continue
  fi
  read -r pictures decoded < <(
    sed -E 's/.*: ([0-9]+) pictures, [0-9]+ intact, ([0-9]+) decoded.*/\1 \2/' <<<"$seen"
  )
  if ((pictures < 50 || decoded != pictures)); then
    echo "FAIL: at $height, $pictures pictures, $decoded decoded" >&2
    status=1
  fi
done
[[ $status == 0 ]] && echo "V5 camera check passed."
exit "$status"
