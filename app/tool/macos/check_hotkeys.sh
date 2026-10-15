#!/usr/bin/env bash
# Global push-to-talk on macOS (docs/plans/macos.md M2, step 8): afplay
# plays a 1 kHz tone into BlackHole 2ch, the app's microphone
# (integration_test/macos/hotkeys_test.dart), whose push-to-talk key is
# F13. The script focuses TextEdit and posts F13 down and up as real key
# events; a Listener voicebot measures the tone in what the app sends with
# the key not held, held, and released. Accept: the tone only while held.
# Its own server, data folders and profile, so it never touches yours.
#
# Needs, for the terminal app, under System Settings → Privacy & Security:
# Input Monitoring (the app's event tap) and Accessibility (posting keys).
#
#   app/tool/macos/check_hotkeys.sh
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/blackhole.sh"
source "$(dirname "$0")/profile.sh"
source "$(dirname "$0")/phases.sh"

# Posts F13 (kVK_F13) down or up, as the keyboard would.
post_f13() {
  python3 - "$1" <<'PY'
import ctypes, sys
cg = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
cg.CGPreflightPostEventAccess.restype = ctypes.c_bool
if not cg.CGPreflightPostEventAccess():
    sys.exit("Allow the terminal app under Accessibility to post keys")
cg.CGEventCreateKeyboardEvent.restype = ctypes.c_void_p
cg.CGEventCreateKeyboardEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint16, ctypes.c_bool]
cg.CGEventPost.argtypes = [ctypes.c_uint32, ctypes.c_void_p]
event = cg.CGEventCreateKeyboardEvent(None, 0x69, sys.argv[1] == "down")
cg.CGEventPost(0, event)  # kCGHIDEventTap
PY
}
post_f13 up

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

# TextEdit, focused while the keys are posted: quit at the end only if
# this script started it, so nobody's documents are touched.
textedit_was_running=""
pgrep -xq TextEdit && textedit_was_running=1
echo "Opencord's push-to-talk check: nothing to type here." >"$work/notes.txt"
front_app() {
  lsappinfo info -only name "$(lsappinfo front)" | sed -E 's/.*="(.*)"/\1/'
}
before_phase() {
  case "$1" in
    "not held")
      open -a TextEdit "$work/notes.txt"
      sleep 2
      echo "focused: $(front_app)" >>"$work/focus"
      ;;
    held)
      echo "focused: $(front_app)" >>"$work/focus"
      post_f13 down
      ;;
    released)
      post_f13 up
      echo "focused: $(front_app)" >>"$work/focus"
      # Push-to-talk's release delay ends before the listener joins.
      sleep 1.5
      ;;
  esac
}
quit_textedit() {
  post_f13 up || true
  [[ -z "$textedit_was_running" ]] && pkill -x TextEdit 2>/dev/null || true
}

export OPENCORD_PROFILE=e2e_hotkeys
fresh_profile e2e_hotkeys
cd "$app"
flutter test integration_test/macos/hotkeys_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_OUTPUT="$blackhole_id" \
  --dart-define=OPENCORD_E2E_INPUT="$blackhole_id" \
  >"$work/hotkeys.log" 2>&1 &
test_pid=$!
background+=("$test_pid")
results="$(listen_in_phases "$bot" "$invite" "$work/hotkeys.log" 1000)"
quit_textedit
if ! wait "$test_pid"; then
  cat "$work/hotkeys.log" >&2
  echo "$results" >&2
  exit 1
fi
grep -oE 'HOTKEYS: .*' "$work/hotkeys.log"
cat "$work/focus"
echo "$results"

status=0
if grep -v 'focused: TextEdit$' "$work/focus" >/dev/null; then
  echo "FAIL: TextEdit was not focused throughout" >&2
  status=1
fi
for phase in "not held" released; do
  if [[ "$(grep "^$phase: " <<<"$results")" != "$phase: nothing" ]]; then
    echo "FAIL: $phase, the app still sent" >&2
    status=1
  fi
done
held="$(tone_level "$(grep '^held: ' <<<"$results" || true)")"
if ! python3 -c "import sys; sys.exit(0 if $held > -30 else 1)"; then
  echo "FAIL: held, 1 kHz at $held dBFS, wanted > -30" >&2
  status=1
fi
[[ $status == 0 ]] && echo "Global push-to-talk check passed."
exit "$status"
