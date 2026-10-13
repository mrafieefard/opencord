# Sourced by the macOS check scripts after ../e2e_server.sh: the BlackHole
# 2ch virtual device in place of PipeWire null sinks (docs/plans/macos.md
# §3, decision F, and the M1 ruling on one device). Makes it the default
# output and input, so nothing reaches your speakers or comes from your
# microphone, and puts the defaults back at exit (with e2e_server.sh's
# cleanup). Sets $sox, $switch_audio, $blackhole and its cpal id
# $blackhole_id.

blackhole="BlackHole 2ch"

# MacPorts' and Homebrew's folders, even when they're not on PATH.
for bin in /opt/local/bin /opt/homebrew/bin /usr/local/bin; do
  [[ -d "$bin" ]] && PATH="$PATH:$bin"
done
sox="$(command -v sox || true)"
switch_audio="$(command -v SwitchAudioSource || true)"
if [[ -z "$sox" || -z "$switch_audio" ]]; then
  echo "Needs sox and SwitchAudioSource: sudo port install sox switchaudio-osx" >&2
  exit 1
fi
# cpal's id of a Core Audio device is "coreaudio:" and the device's UID.
blackhole_uid="$("$switch_audio" -a -t output -f json |
  python3 -c 'import json, sys
for line in sys.stdin:
    device = json.loads(line)
    if device["name"] == sys.argv[1]:
        print(device["uid"])
        break' "$blackhole")"
if [[ -z "$blackhole_uid" ]]; then
  echo "No $blackhole: sudo port install BlackHole" >&2
  exit 1
fi
blackhole_id="coreaudio:$blackhole_uid"

saved_output="$("$switch_audio" -c -t output)"
saved_input="$("$switch_audio" -c -t input)"
saved_system="$("$switch_audio" -c -t system)"
restore_devices() {
  "$switch_audio" -t output -s "$saved_output" >/dev/null || true
  "$switch_audio" -t input -s "$saved_input" >/dev/null || true
  "$switch_audio" -t system -s "$saved_system" >/dev/null || true
  stop_background
}
trap restore_devices EXIT
"$switch_audio" -t output -s "$blackhole" >/dev/null
"$switch_audio" -t system -s "$blackhole" >/dev/null
"$switch_audio" -t input -s "$blackhole" >/dev/null
