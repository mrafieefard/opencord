#!/usr/bin/env bash
# The microphone path on macOS (docs/plans/macos.md M1, step 6): afplay
# plays a 1 kHz tone into BlackHole 2ch, which the app
# (integration_test/macos/microphone_test.dart) uses as its microphone. It
# plays into BlackHole too, where its own near-silence is the echo
# canceller's reference; nothing reaches your speakers. The app
# goes through voice activity, push-to-talk held and push-to-talk released;
# in each, a Listener voicebot measures the tone in what the app sends.
# Its own server, data folders and profile, so it never touches yours.
#
#   app/tool/macos/check_microphone.sh
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/blackhole.sh"
source "$(dirname "$0")/profile.sh"
source "$(dirname "$0")/phases.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"
claim_and_invite "$bot"

# 10 s of 1 kHz at -12 dBFS, played into BlackHole again and again.
python3 - "$work/tone.wav" <<'PY'
import math, struct, sys, wave
with wave.open(sys.argv[1], "wb") as out:
    out.setnchannels(1)
    out.setsampwidth(2)
    out.setframerate(48000)
    frames = (int(0.25 * 32767 * math.sin(2 * math.pi * 1000 * n / 48000)) for n in range(48000 * 10))
    out.writeframes(b"".join(struct.pack("<h", f) for f in frames))
PY
# Stopping the loop stops the afplay playing at that moment too.
(
  trap 'kill "$player" 2>/dev/null; exit' TERM
  while :; do
    afplay "$work/tone.wav" &
    player=$!
    wait "$player"
  done
) &
background+=("$!")

export OPENCORD_PROFILE=e2e_microphone
fresh_profile e2e_microphone
cd "$app"
flutter test integration_test/macos/microphone_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_OUTPUT="$blackhole_id" \
  --dart-define=OPENCORD_E2E_INPUT="$blackhole_id" \
  >"$work/microphone.log" 2>&1 &
test_pid=$!
background+=("$test_pid")
results="$(listen_in_phases "$bot" "$invite" "$work/microphone.log" 1000)"
if ! wait "$test_pid"; then
  cat "$work/microphone.log" >&2
  echo "$results" >&2
  exit 1
fi
grep -oE 'MIC (DEVICES|LEVELS): .*' "$work/microphone.log"
echo "$results"

status=0
check() {
  local phase="$1" want="$2" line level
  line="$(grep "^$phase: " <<<"$results" || true)"
  level="$(tone_level "$line")"
  if ! python3 -c "import sys; sys.exit(0 if $level $want else 1)"; then
    echo "FAIL: $phase: 1 kHz at $level dBFS, wanted $want" >&2
    status=1
  fi
}
check "voice activity" "> -30"
check "push-to-talk held" "> -30"
# Released, nothing at all may come: not even silence.
if [[ "$(grep '^push-to-talk released: ' <<<"$results")" != "push-to-talk released: nothing" ]]; then
  echo "FAIL: push-to-talk released, the app still sent" >&2
  status=1
fi
[[ $status == 0 ]] && echo "Microphone check passed."
exit "$status"
