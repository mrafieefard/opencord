# Sourced by the macOS check scripts after ../e2e_server.sh: the BlackHole
# virtual devices in place of PipeWire null sinks (docs/plans/macos.md §3,
# decision F). Makes BlackHole 2ch the default output and BlackHole 16ch the
# default input, so nothing reaches your speakers or comes from your
# microphone, and puts the defaults back at exit (with e2e_server.sh's
# cleanup). Sets $sox, $switch_audio, $blackhole_2ch, $blackhole_16ch and
# the cpal ids $blackhole_2ch_id and $blackhole_16ch_id.

blackhole_2ch="BlackHole 2ch"
blackhole_16ch="BlackHole 16ch"
# cpal's id of a Core Audio device is "coreaudio:" and its UID.
blackhole_2ch_id="coreaudio:BlackHole2ch_UID"
blackhole_16ch_id="coreaudio:BlackHole16ch_UID"

# Homebrew's folder, Apple silicon or Intel, even when it's not on PATH.
for brew_bin in /opt/homebrew/bin /usr/local/bin; do
  [[ -d "$brew_bin" ]] && PATH="$PATH:$brew_bin"
done
sox="$(command -v sox || true)"
switch_audio="$(command -v SwitchAudioSource || true)"
if [[ -z "$sox" || -z "$switch_audio" ]]; then
  echo "Needs sox and SwitchAudioSource: brew install sox switchaudio-osx" >&2
  exit 1
fi
for device in "$blackhole_2ch" "$blackhole_16ch"; do
  if ! "$switch_audio" -a -t output | grep -qx "$device"; then
    echo "No $device: brew install --cask blackhole-2ch blackhole-16ch" >&2
    exit 1
  fi
done

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
"$switch_audio" -t output -s "$blackhole_2ch" >/dev/null
"$switch_audio" -t system -s "$blackhole_2ch" >/dev/null
"$switch_audio" -t input -s "$blackhole_16ch" >/dev/null
