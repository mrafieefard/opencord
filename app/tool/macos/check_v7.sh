#!/usr/bin/env bash
# V7 on the Mac (docs/plans/macos.md M7, step 6): plan §10.7's "no double
# audio" test. The app (integration_test/macos/share_sound_test.dart)
# shares the main display with its sound (OPENCORD_SCREEN_DISPLAY=main)
# while afplay plays "music" (1 kHz, another app); Talker (a voicebot) says
# 440 Hz, which the app plays; Watcher (a voicebot) measures the stream.
# Watcher must hear the music in the stream on both channels and no
# Talker in it: Opencord's own sound is never shared. Everything plays into
# BlackHole 2ch: nothing reaches your speakers.
#
#   app/tool/macos/check_v7.sh             the check
#   app/tool/macos/check_v7.sh --mutation  Opencord's sound kept in: Talker
#                                          must then be in the stream
#   app/tool/macos/check_v7.sh --window    a window you pick (Safari, playing
#                                          600 Hz) in place of the display;
#                                          afplay's 1 kHz must stay out
#
# Needs audio capture ("System Audio Recording") and Screen Recording for
# the terminal app. Its own server, data folders and profile.
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/blackhole.sh"
source "$(dirname "$0")/profile.sh"
mode="${1:-check}"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"

# A 5-minute stereo tone at a quarter of full scale (-12 dBFS).
tone() {
  "$sox" -q -n -r 48000 -c 2 -b 16 "$1" synth 300 sine "$2" vol 0.25
}
tone "$work/music.wav" 1000

claim_and_invite "$bot"
"$bot" --server "$invite" --name Talker --tone 440 --seconds 900 \
  >"$work/talker.log" 2>&1 &
background+=("$!")
"$bot" --server "$invite" --name Watcher --tone 0 --watch-streams \
  --until-stream-ends --listen-for 440,600,1000 --seconds 900 \
  >"$work/watcher.log" 2>&1 &
watcher=$!
background+=("$watcher")

# Another app's music, not a child of anything Opencord runs.
music_pid="$(sh -c "afplay '$work/music.wav' >/dev/null 2>&1 & echo \$!")"
background+=("$music_pid")

window=false
case "$mode" in
  --mutation) export OPENCORD_SHARE_AUDIO_INCLUDE_SELF=1 ;;
  --window)
    window=true
    tone "$work/safari.wav" 600
    cat >"$work/safari.html" <<'HTML'
<!doctype html><title>Opencord sound check</title>
<h1>Opencord sound check</h1><p>Press play, then pick this window.</p>
<audio src="safari.wav" controls loop></audio>
HTML
    open -a Safari "$work/safari.html"
    ;;
esac
[[ "$window" == false ]] && export OPENCORD_SCREEN_DISPLAY=main

export OPENCORD_PROFILE=e2e_share_sound
fresh_profile e2e_share_sound
cd "$app"
if ! flutter test integration_test/macos/share_sound_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_DEVICE="$blackhole_id" \
  --dart-define=OPENCORD_E2E_WINDOW="$window" >"$work/v7.log" 2>&1; then
  cat "$work/v7.log" >&2
  exit 1
fi
for _ in $(seq 1 100); do
  kill -0 "$watcher" 2>/dev/null || break
  sleep 0.2
done
grep -oE 'SHARE SOUND: .*' "$work/v7.log"
cat "$work/watcher.log"
talker="$(grep -oE 'joined General as user [0-9]+' "$work/talker.log" | grep -oE '[0-9]+$')"
python3 - "$work/watcher.log" "$talker" "$mode" <<'PY'
import re, sys
log, talker, mode = open(sys.argv[1]).read(), sys.argv[2], sys.argv[3]
def level(line, name):
    found = re.search(re.escape(name) + r" Hz (-?[0-9.]+) dBFS", line or "")
    return float(found.group(1)) if found else -200.0
voice = next((l for l in log.splitlines() if l.startswith(f"user {talker}:")), "")
stream = next((l for l in log.splitlines() if l.startswith("stream sound from user")), None)
if not stream:
    sys.exit("FAIL: Watcher heard no stream sound")
if mode == "--mutation":
    kept = max(level(stream, "left 440"), level(stream, "right 440"))
    print(f"Talker in the stream with Opencord kept in: {kept:.1f} dBFS")
    if kept < -50:
        sys.exit("FAIL: the mutation went unnoticed: Talker is not in the stream")
    print("Mutation check passed: without leaving Opencord out, the check fails.")
    sys.exit(0)
wanted = "600" if mode == "--window" else "1000"
checks = {
    "Talker's voice has Talker (440 Hz)": level(voice, "440") > -30,
    f"the stream has the shared sound ({wanted} Hz), left": level(stream, f"left {wanted}") > -30,
    f"the stream has the shared sound ({wanted} Hz), right": level(stream, f"right {wanted}") > -30,
    "the stream has no Talker (440 Hz), left": level(stream, "left 440") < -55,
    "the stream has no Talker (440 Hz), right": level(stream, "right 440") < -55,
}
if mode == "--window":
    checks["the stream has no afplay (1 kHz), left"] = level(stream, "left 1000") < -50
    checks["the stream has no afplay (1 kHz), right"] = level(stream, "right 1000") < -50
for name, ok in checks.items():
    print(("ok   " if ok else "FAIL ") + name)
sys.exit(0 if all(checks.values()) else 1)
PY
echo "V7 check ($mode) passed."
