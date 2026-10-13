#!/usr/bin/env bash
# The macOS port of app/tool/check_user_audio.sh: the per-user voice menu
# (desktop UI plan §17.3) against a real server, measured at the speaker.
# Talker (a voicebot) sends a 440 Hz tone, Player plays an 880 Hz soundboard
# sound every 3 s. The app (integration_test/macos/user_audio_test.dart)
# plays into BlackHole 2ch, with the silent BlackHole 16ch as its
# microphone, so nothing reaches your speakers or comes from your
# microphone; it records BlackHole 2ch with sox and measures each tone while
# its menu sets Talker to 50 % and 200 %, mutes and unmutes them, and mutes
# Player's soundboard. The system's default devices are set to the two
# BlackHoles while it runs and put back at exit. Its own server, data
# folders and profile, so it never touches yours.
#
# Needs: brew install sox switchaudio-osx
#        brew install --cask blackhole-2ch blackhole-16ch
#
#   app/tool/macos/check_user_audio.sh
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/blackhole.sh"

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
if ! flutter test integration_test/macos/user_audio_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_OUTPUT="$blackhole_2ch_id" \
  --dart-define=OPENCORD_E2E_INPUT="$blackhole_16ch_id" \
  --dart-define=OPENCORD_E2E_RECORD="$blackhole_2ch" \
  --dart-define=OPENCORD_E2E_SOX="$sox" \
  >"$work/user_audio.log" 2>&1; then
  cat "$work/user_audio.log" >&2
  exit 1
fi
grep -o 'USER AUDIO: .*' "$work/user_audio.log"
echo "Per-user audio check passed."
