#!/usr/bin/env bash
# Phase 2 V8 against a real server with the real app: the owner (a voicebot)
# adds a sound made from a WAV file while claiming the server; another bot
# in General plays a default sound every few seconds. The app
# (integration_test/v8_test.dart) joins General, gets the uploaded sound
# with the server, shows the bot's soundboard ring, and plays the uploaded
# sound from the picker; the bot must receive it. Its own server, data
# folders and profile, so it never touches yours.
#
#   app/tool/check_v8.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"

python3 - "$work/beep.wav" <<'PY'
import math, struct, sys, wave
with wave.open(sys.argv[1], "wb") as out:
    out.setnchannels(1)
    out.setsampwidth(2)
    out.setframerate(44100)
    frames = (int(0.3 * 32767 * math.sin(2 * math.pi * 880 * n / 44100)) for n in range(int(44100 * 1.5)))
    out.writeframes(b"".join(struct.pack("<h", f) for f in frames))
PY

claim_and_invite "$bot" --upload-sound "$work/beep.wav" --sound-name Beep
sound="$(grep -oE 'server:[0-9a-f]{64}' "$work/claim.log" | head -1)"
if [[ -z "$sound" ]]; then
  echo "The owner did not upload the sound:" >&2
  cat "$work/claim.log" >&2
  exit 1
fi

"$bot" --server "$invite" --name Player --tone 0 --sound default:boing \
  --sound-every 4 --seconds 600 >"$work/player.log" 2>&1 &
background+=("$!")

export OPENCORD_PROFILE=e2e_soundboard
cd "$app"
if ! flutter test integration_test/v8_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  >"$work/v8.log" 2>&1; then
  cat "$work/v8.log" >&2
  exit 1
fi
if ! grep -q "sound $sound (Beep)" "$work/player.log"; then
  echo "The bot never got the sound the app played:" >&2
  cat "$work/player.log" >&2
  exit 1
fi
echo "V8 soundboard check passed."
