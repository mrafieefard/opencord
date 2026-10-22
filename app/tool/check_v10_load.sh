#!/usr/bin/env bash
# Phase 2 V10's client numbers with the real app (plan §16.1), in a profile
# build (integration_test/v10_load_test.dart):
#
#   app/tool/check_v10_load.sh call    ten people, four cameras, a stream:
#                                      the app's memory and CPU
#   app/tool/check_v10_load.sh watch   the app watching a 1080p60 stream
#
# The bots are release builds sending real H.264. The app's camera is a
# virtual PipeWire camera, so this machine's real one stays off. Its own
# server, data folders and profile, so it never touches yours.
set -euo pipefail
mode="${1:?call or watch}"
E2E_SERVER_PROFILE=release
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-media \
  --features testing --example virtual_camera
bot="$repo/target/release/opencord-voicebot"

claim_and_invite "$bot" --server-screen-cap 1080p60

start_bot() {
  "$bot" --server "$invite" --seconds 900 "$@" >"$work/bot$((++bots)).log" 2>&1 &
  background+=("$!")
}
bots=0
case "$mode" in
  call)
    for number in 1 2 3; do
      start_bot --name "Camera $number" --tone 0 --camera --watch 360
    done
    start_bot --name Talker --tone 440 --watch 180
    for number in 1 2 3 4; do
      start_bot --name "Person $number" --tone 0 --watch 180
    done
    start_bot --name Sharer --tone 0 --screen 1080p30
    ;;
  watch)
    start_bot --name Sharer --tone 0 --screen 1080p60
    ;;
  *) echo "call or watch" >&2; exit 2 ;;
esac

# The bots retry while the server's sign-in limit holds them back.
joined=0
for _ in $(seq 1 400); do
  joined="$(cat "$work"/bot*.log | grep -c '^joined' || true)"
  [[ "$joined" -ge "$bots" ]] && break
  sleep 0.5
done
if [[ "$joined" -lt "$bots" ]]; then
  echo "Only $joined of $bots voicebots joined:" >&2
  tail -n 3 "$work"/bot*.log >&2
  exit 1
fi
sleep 13

camera=""
if [[ "$mode" == call ]]; then
  "$repo/target/release/examples/virtual_camera" camera mjpeg >"$work/virtual_camera.log" 2>&1 &
  background+=("$!")
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
fi

export OPENCORD_PROFILE=e2e_v10_load OPENCORD_CAMERA_PORTAL=0
# What the app reads besides, for an experiment: MALLOC_ARENA_MAX, say.
[[ -n "${V10_APP_ENV:-}" ]] && export ${V10_APP_ENV}
rm -f "$app/build/integration_response_data.json"
cd "$app"
if ! flutter drive --profile -d linux \
  --driver=test_driver/integration_test.dart \
  --target=integration_test/v10_load_test.dart \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_CAMERA="$camera" \
  --dart-define=OPENCORD_E2E_MODE="$mode" \
  --dart-define=OPENCORD_E2E_SECONDS="${V10_SECONDS:-20}"; then
  tail -n 5 "$work"/bot*.log >&2
  exit 1
fi
if ! grep -q rss_mb build/integration_response_data.json 2>/dev/null; then
  echo "The test reported no results." >&2
  exit 1
fi
cat build/integration_response_data.json
echo
echo "V10 $mode load check done."
