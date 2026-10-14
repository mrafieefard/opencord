#!/usr/bin/env bash
# V6 on the Mac (docs/plans/macos.md M6, step 6): the app
# (integration_test/macos/screen_share_test.dart) shares the main display
# through ScreenCaptureKit, without the system picker
# (OPENCORD_SCREEN_DISPLAY=main), with a voicebot's moving camera on
# screen; a Watcher voicebot watches the stream and must decode every
# picture at about the frame rate, and stopping must end the stream for it.
# The app's CPU is sampled while it shares. Needs Screen Recording for the
# terminal app. Its own server, data folders and profile, so it never
# touches yours.
#
#   app/tool/macos/check_v6.sh [720p|1080p] [30|60]
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/profile.sh"
resolution="${1:-720p}"
fps="${2:-30}"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"
claim_and_invite "$bot"
"$bot" --server "$invite" --name Camera --tone 0 --camera --seconds 600 \
  >"$work/camera.log" 2>&1 &
background+=("$!")

export OPENCORD_PROFILE=e2e_screen_share OPENCORD_SCREEN_DISPLAY=main
fresh_profile e2e_screen_share
cd "$app"
flutter test integration_test/macos/screen_share_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_RESOLUTION="$resolution" \
  --dart-define=OPENCORD_E2E_FPS="$fps" >"$work/share.log" 2>&1 &
test_pid=$!
background+=("$test_pid")

until grep -q 'PHASE: live' "$work/share.log" 2>/dev/null; do
  if ! kill -0 "$test_pid" 2>/dev/null; then
    cat "$work/share.log" >&2
    exit 1
  fi
  sleep 0.2
done
# Watches every stream at full size until it ends (or 60 s).
"$bot" --server "$invite" --name Watcher --tone 0 --watch 2160 --watch-streams \
  --until-stream-ends --seconds 60 >"$work/watcher.log" 2>&1 &
watcher=$!
background+=("$watcher")
watch_started=$(date +%s)
cpu=()
for _ in $(seq 1 10); do
  sleep 1
  cpu+=("$(ps -A -o %cpu=,comm= | awk '/Opencord.app\/Contents\/MacOS\/Opencord$/ && !found {print $1; found = 1}')")
done
if ! wait "$test_pid"; then
  cat "$work/share.log" >&2
  exit 1
fi
wait "$watcher" || true
watched=$(($(date +%s) - watch_started))

grep -oE 'SHARE [A-Z]+: .*' "$work/share.log"
echo "Opencord CPU while sharing (%, one core = 100): ${cpu[*]}"
grep -vE '^joined|^live' "$work/watcher.log" || true
seen="$(grep -E 'pictures' "$work/watcher.log" | tail -1)"
if [[ -z "$seen" ]]; then
  echo "FAIL: the watcher saw no screen" >&2
  exit 1
fi
read -r pictures decoded < <(
  sed -E 's/.*: ([0-9]+) pictures, [0-9]+ intact, ([0-9]+) decoded.*/\1 \2/' <<<"$seen"
)
echo "watched about $watched s: $pictures pictures, $decoded decoded"
status=0
if ((decoded != pictures || pictures < 10)); then
  echo "FAIL: $pictures pictures, $decoded decoded" >&2
  status=1
fi
if ! grep -qiE 'ended|stopped' "$work/watcher.log"; then
  echo "FAIL: the watcher did not see the stream end" >&2
  status=1
fi
[[ $status == 0 ]] && echo "V6 screen share check passed."
exit "$status"
