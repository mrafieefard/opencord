#!/usr/bin/env bash
# V3's echo acceptance on a Mac (docs/plans/macos.md M1, step 7). Needs you
# at the Mac with the speakers on at a normal volume and the room quiet:
# the app (integration_test/macos/echo_test.dart) uses the Mac's own
# speakers and microphone, Talker (a voicebot) sends 440 Hz at about
# -12 dBFS, and a Listener voicebot measures 440 Hz in what the app sends,
# first with the app's processing, then with echo cancellation and noise
# suppression off. Accept: under -50 dBFS with the processing; the second
# number shows the echo was there. Its own server, data folders and
# profile, so it never touches yours.
#
#   app/tool/macos/check_echo.sh ["MacBook Pro Speakers" "MacBook Pro Microphone"]
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/blackhole.sh"
source "$(dirname "$0")/profile.sh"
source "$(dirname "$0")/phases.sh"

speakers="${1:-MacBook Pro Speakers}"
microphone="${2:-MacBook Pro Microphone}"
"$switch_audio" -t output -s "$speakers" >/dev/null
"$switch_audio" -t input -s "$microphone" >/dev/null

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"
claim_and_invite "$bot"
"$bot" --server "$invite" --name Talker --tone 440 --seconds 900 \
  >"$work/talker.log" 2>&1 &
background+=("$!")
talker="$(voicebot_user "$work/talker.log")"

export OPENCORD_PROFILE=e2e_echo
fresh_profile e2e_echo
cd "$app"
flutter test integration_test/macos/echo_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  >"$work/echo.log" 2>&1 &
test_pid=$!
background+=("$test_pid")
results="$(listen_in_phases "$bot" "$invite" "$work/echo.log" 440 "$talker")"
if ! wait "$test_pid"; then
  cat "$work/echo.log" >&2
  echo "$results" >&2
  exit 1
fi
grep -oE 'ECHO SETTINGS: .*' "$work/echo.log"
echo "$results"

chosen="$(tone_level "$(grep '^as chosen: ' <<<"$results" || true)")"
off="$(tone_level "$(grep '^echo cancellation off: ' <<<"$results" || true)")"
echo "440 Hz in what the app sent: $chosen dBFS with its processing," \
  "$off dBFS without"
if ! python3 -c "import sys; sys.exit(0 if $chosen < -50 else 1)"; then
  echo "FAIL: the echo is not cancelled" >&2
  exit 1
fi
if ! python3 -c "import sys; sys.exit(0 if $off > -50 else 1)"; then
  echo "Not shown: without echo cancellation the echo was quiet too;" \
    "turn the speakers up and run again." >&2
  exit 1
fi
echo "Echo check passed."
