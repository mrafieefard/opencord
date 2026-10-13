#!/usr/bin/env bash
# Phase 2 V7 against a real server with the real app: plan §10.7's "no
# double audio" test. The app (integration_test/v7_test.dart) shares a
# virtual screen with its sound while "music" (a 1 kHz tone, another app)
# plays; Talker (a voicebot) says 440 Hz; Watcher (a voicebot) watches the
# stream and measures each source. Watcher must hear Talker once, in his
# voice; the music once, in the stream; and no Talker in the stream (the
# app plays Talker into a null sink: Opencord's own sound is never
# shared). Everything plays into null sinks: nothing reaches your
# speakers. Its own server, data folders and profile.
#
#   app/tool/check_v7.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

speaker="opencord_v7_speaker_$$"
silence="opencord_v7_silence_$$"
music_sink="opencord_v7_music_$$"
modules=()
for sink in "$speaker" "$silence" "$music_sink"; do
  modules+=("$(pactl load-module module-null-sink sink_name="$sink" \
    channels=2 rate=48000 sink_properties=device.description="$sink")")
done
music_pid=""
cleanup() {
  [[ -n "$music_pid" ]] && kill "$music_pid" 2>/dev/null || true
  for module in "${modules[@]}"; do
    pactl unload-module "$module" 2>/dev/null || true
  done
  stop_background
}
trap cleanup EXIT

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"
cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-media --features testing --example virtual_camera
virtual_screen="$repo/target/release/examples/virtual_camera"

python3 - "$work/music.wav" <<'PY'
import math, struct, sys, wave
with wave.open(sys.argv[1], "wb") as out:
    out.setnchannels(2)
    out.setsampwidth(2)
    out.setframerate(48000)
    frames = (int(0.25 * 32767 * math.sin(2 * math.pi * 1000 * n / 48000)) for n in range(48000 * 300))
    out.writeframes(b"".join(struct.pack("<hh", f, f) for f in frames))
PY

claim_and_invite "$bot"

"$bot" --server "$invite" --name Talker --tone 440 --seconds 900 \
  >"$work/talker.log" 2>&1 &
background+=("$!")
"$bot" --server "$invite" --name Watcher --tone 0 --watch-streams \
  --until-stream-ends --listen-for 440,1000 --seconds 900 \
  >"$work/watcher.log" 2>&1 &
watcher="$!"
background+=("$watcher")

# Another app's music, not a child of anything Opencord runs.
setsid --fork pw-play --target "$music_sink" \
  -P '{ application.name = "OcMusic" }' "$work/music.wav"
sleep 0.5
music_pid="$(pgrep -f "$work/music.wav" | head -1)"

"$virtual_screen" screen 1280 720 30 >"$work/screen.log" 2>&1 &
background+=("$!")
screen_node=""
for _ in $(seq 1 100); do
  screen_node="$(grep -oE '^screen=.+' "$work/screen.log" | cut -d= -f2- || true)"
  [[ -n "$screen_node" ]] && break
  sleep 0.1
done
if [[ -z "$screen_node" ]]; then
  echo "The virtual screen did not start:" >&2
  cat "$work/screen.log" >&2
  exit 1
fi
export OPENCORD_SCREEN_NODE="$screen_node"

export OPENCORD_PROFILE=e2e_share_sound
cd "$app"
if ! flutter test integration_test/v7_test.dart -d linux \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_SINK="$speaker" \
  --dart-define=OPENCORD_E2E_SILENCE="$silence" \
  >"$work/v7.log" 2>&1; then
  cat "$work/v7.log" >&2
  echo "--- virtual screen:" >&2
  cat "$work/screen.log" >&2
  exit 1
fi
for _ in $(seq 1 100); do
  kill -0 "$watcher" 2>/dev/null || break
  sleep 0.2
done
cat "$work/watcher.log"
talker="$(grep -oE 'joined General as user [0-9]+' "$work/talker.log" | grep -oE '[0-9]+$')"
python3 - "$work/watcher.log" "$talker" <<'PY'
import re, sys
log = open(sys.argv[1]).read()
talker = sys.argv[2]
def level(line, name):
    found = re.search(re.escape(name) + r" Hz (-?[0-9.]+) dBFS", line)
    return float(found.group(1)) if found else None
voice = next(l for l in log.splitlines() if l.startswith(f"user {talker}:"))
stream = next((l for l in log.splitlines() if l.startswith("stream sound from user")), None)
assert stream, "Watcher heard no stream sound"
checks = {
    "Talker's voice has Talker (440 Hz)": level(voice, "440") > -30,
    "Talker's voice has no music (1 kHz)": level(voice, "1000") < -50,
    "the stream has the music, left": level(stream, "left 1000") > -30,
    "the stream has the music, right": level(stream, "right 1000") > -30,
    "the stream has no Talker (440 Hz), left": level(stream, "left 440") < -50,
    "the stream has no Talker (440 Hz), right": level(stream, "right 440") < -50,
}
for name, ok in checks.items():
    print(("ok   " if ok else "FAIL ") + name)
sys.exit(0 if all(checks.values()) else 1)
PY
echo "V7 no-double-audio check passed."
