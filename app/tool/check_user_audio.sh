#!/usr/bin/env bash
# The per-user voice menu (desktop UI plan §17.3) against a real server,
# measured at the speaker: Talker (a voicebot) sends a 440 Hz tone, Player
# plays an 880 Hz soundboard sound every 3 s. The app
# (integration_test/user_audio_test.dart) plays into a mono null sink of
# its own, with a silent one as its microphone, so nothing reaches your
# speakers or comes from your microphone; it records the sink's monitor
# with parec and measures each tone while its menu sets Talker to 50 % and
# 200 %, mutes and unmutes them, and mutes Player's soundboard. Its own
# server, data folders and profile, so it never touches yours.
#
#   app/tool/check_user_audio.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

measure="opencord_measure_$$"
silence="opencord_silence_$$"
modules=()
for sink in "$measure" "$silence"; do
  modules+=("$(pactl load-module module-null-sink sink_name="$sink" \
    channels=1 rate=48000 sink_properties=device.description="$sink")")
done
unload_sinks() {
  for module in "${modules[@]}"; do
    pactl unload-module "$module" 2>/dev/null || true
  done
  stop_background
}
trap unload_sinks EXIT

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"

python3 - "$work/beep.wav" <<'PY'
import math, struct, sys, wave
with wave.open(sys.argv[1], "wb") as out:
    out.setnchannels(1)
    out.setsampwidth(2)
    out.setframerate(48000)
    frames = (int(0.3 * 32767 * math.sin(2 * math.pi * 880 * n / 48000)) for n in range(int(48000 * 1.5)))
    out.writeframes(b"".join(struct.pack("<h", f) for f in frames))
PY

claim_and_invite "$bot" --upload-sound "$work/beep.wav" --sound-name Beep
sound="$(grep -oE 'server:[0-9a-f]{64}' "$work/claim.log" | head -1)"
if [[ -z "$sound" ]]; then
  echo "The owner did not upload the sound:" >&2
  cat "$work/claim.log" >&2
  exit 1
fi

"$bot" --server "$invite" --name Talker --tone 440 --seconds 900 \
  >"$work/talker.log" 2>&1 &
background+=("$!")
"$bot" --server "$invite" --name Player --tone 0 --sound "$sound" \
  --sound-every 3 --seconds 900 >"$work/player.log" 2>&1 &
background+=("$!")

export OPENCORD_PROFILE=e2e_user_audio
cd "$app"
if ! flutter test integration_test/user_audio_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_SINK="$measure" \
  --dart-define=OPENCORD_E2E_SILENCE="$silence" \
  >"$work/user_audio.log" 2>&1; then
  cat "$work/user_audio.log" >&2
  exit 1
fi
grep -o 'USER AUDIO: .*' "$work/user_audio.log"
echo "Per-user audio check passed."
