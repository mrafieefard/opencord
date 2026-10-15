#!/usr/bin/env bash
# Voice latency through real devices (plan §16.1: within 150 ms on a LAN,
# devices included). The app (integration_test/voice_latency_test.dart)
# plays into a PipeWire null sink of its own and uses that sink's monitor
# as its microphone, so what it hears goes back out; a voicebot beeps
# every second and times each beep coming back. That round trip holds one
# playback device, one capture device, the app's audio processing and two
# trips through the node and a jitter buffer: an upper bound on one-way,
# mouth-to-ear latency. Nothing reaches your speakers. Release server and
# bot; the app is a debug build.
#
#   app/tool/check_voice_latency.sh
set -euo pipefail
E2E_SERVER_PROFILE=release
source "$(dirname "$0")/e2e_server.sh"

loop="opencord_loop_$$"
module="$(pactl load-module module-null-sink sink_name="$loop" \
  channels=1 rate=48000 sink_properties=device.description="$loop")"
unload_sink() {
  pactl unload-module "$module" 2>/dev/null || true
  stop_background
}
trap unload_sink EXIT

cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/release/opencord-voicebot"

claim_and_invite "$bot"
"$bot" --server "$invite" --name Pinger --ping 1000 --seconds 60 \
  >"$work/pinger.log" 2>&1 &
pinger=$!
background+=("$pinger")

export OPENCORD_PROFILE=e2e_voice_latency
cd "$app"
if ! flutter test integration_test/voice_latency_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_LOOP_SINK="$loop" \
  >"$work/loop.log" 2>&1; then
  cat "$work/loop.log" >&2
  exit 1
fi
wait "$pinger" || true
if ! grep -q "^round trip" "$work/pinger.log"; then
  echo "No beep came back:" >&2
  cat "$work/pinger.log" >&2
  exit 1
fi
grep "^round trip" "$work/pinger.log"
