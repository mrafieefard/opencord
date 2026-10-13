#!/usr/bin/env bash
# The macOS port of app/tool/check_user_audio.sh: the per-user voice menu
# (desktop UI plan §17.3) against a real server, measured at the speaker.
# Talker (a voicebot) sends a 440 Hz tone, Player plays an 880 Hz soundboard
# sound every 3 s. The app (integration_test/macos/user_audio_test.dart)
# plays into BlackHole 2ch and uses it as its microphone too, so nothing
# reaches your speakers or comes from your microphone. This script records
# BlackHole with sox and measures each tone while the app's menu sets
# Talker to 50 % and 200 %, mutes and unmutes them, and mutes Player's
# soundboard. What the app sends isn't measured here. The system's default devices are set to
# BlackHole while it runs and put back at exit. Its own server, data
# folders and profile, so it never touches yours.
#
# Needs: sudo port install sox switchaudio-osx BlackHole
#
#   app/tool/macos/check_user_audio.sh
set -euo pipefail
source "$(dirname "$0")/../e2e_server.sh"
source "$(dirname "$0")/blackhole.sh"
source "$(dirname "$0")/profile.sh"

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

# The app sandbox may not start sox, so it records here, for the whole
# test, from the moment written to recording.start (ms since the epoch).
python3 - "$work/recording.start" "$sox" "$blackhole" "$work/recording.raw" <<'PY' &
import os, sys, time
start, sox, device, raw = sys.argv[1:]
with open(start, "w") as out:
    out.write(str(int(time.time() * 1000)))
os.execv(sox, [sox, "-q", "-t", "coreaudio", device, "-t", "raw", "-e",
               "floating-point", "-b", "32", "-L", "-r", "48000", "-c", "1", raw])
PY
recorder=$!
background+=("$recorder")

export OPENCORD_PROFILE=e2e_user_audio
fresh_profile e2e_user_audio
cd "$app"
if ! flutter test integration_test/macos/user_audio_test.dart -d macos \
  --dart-define=OPENCORD_E2E_INVITE="$invite" \
  --dart-define=OPENCORD_E2E_OUTPUT="$blackhole_id" \
  --dart-define=OPENCORD_E2E_INPUT="$blackhole_id" \
  >"$work/user_audio.log" 2>&1; then
  cat "$work/user_audio.log" >&2
  exit 1
fi
kill "$recorder"
wait "$recorder" 2>/dev/null || true
python3 -c 'import time; print(int(time.time() * 1000))' >"$work/recording.end"

# The shared test's measurements and numbers, over the app's windows.
grep -o 'WINDOW: .*' "$work/user_audio.log" >"$work/windows"
python3 - "$work/recording.start" "$work/recording.end" "$work/recording.raw" \
  "$work/windows" <<'PY'
import array, math, sys
start_file, end_file, raw_file, windows_file = sys.argv[1:]
RATE, BLOCK = 48000, 4800  # 100 ms: a whole number of cycles of both tones
start = int(open(start_file).read())
end = int(open(end_file).read())
samples = array.array("f")
with open(raw_file, "rb") as raw:
    data = raw.read()
samples.frombytes(data[: len(data) // 4 * 4])
if sys.byteorder != "little":
    samples.byteswap()
# The windows are found by time, so the recording must have kept time.
lost_ms = (end - start) - len(samples) * 1000 // RATE
if not 0 <= lost_ms < 1000:
    sys.exit(f"the recording is {lost_ms} ms shorter than the time it ran")

# sox starts recording a little after recording.start says (100-200 ms
# here), so each window leaves this much out at both ends.
MARGIN_MS = 300

def levels(begin_ms, end_ms, hz):
    """Each 100 ms block's level of hz in dBFS (a full-scale sine is 0)."""
    first = max(0, (begin_ms + MARGIN_MS - start) * RATE // 1000)
    last = min(len(samples), (end_ms - MARGIN_MS - start) * RATE // 1000)
    coefficient = 2 * math.cos(2 * math.pi * hz / RATE)
    out = []
    for block in range(first, last - BLOCK + 1, BLOCK):
        s1 = s2 = 0.0
        for n in range(block, block + BLOCK):
            s1, s2 = samples[n] + coefficient * s1 - s2, s1
        power = s1 * s1 + s2 * s2 - coefficient * s1 * s2
        amplitude = 2 * math.sqrt(max(power, 0)) / BLOCK
        out.append(-200.0 if amplitude <= 0 else 20 * math.log10(amplitude))
    if not out:
        sys.exit(f"nothing recorded from {begin_ms} to {end_ms}")
    return out

windows = {}
for line in open(windows_file):
    _, name, begin, end = line.split()
    windows[name] = (int(begin), int(end))
def median(values):
    return sorted(values)[len(values) // 2]

voice = {name: median(levels(*windows[name], 440))
         for name in ("at100", "at50", "at200", "muted", "unmuted", "voiceMeanwhile")}
sound = {name: max(levels(*windows[name], 880))
         for name in ("soundBefore", "soundMuted", "soundAfter")}
db = lambda level: f"{level:.1f} dBFS"
change = lambda d: f"{'+' if d >= 0 else ''}{d:.1f} dB"
v, s = voice, sound
print("USER AUDIO: " + "; ".join([
    f"Talker 440 Hz at 100 %: {db(v['at100'])}",
    f"at 50 %: {db(v['at50'])} ({change(v['at50'] - v['at100'])})",
    f"at 200 %: {db(v['at200'])} ({change(v['at200'] - v['at100'])})",
    f"muted: {db(v['muted'])}",
    f"unmuted: {db(v['unmuted'])}",
    f"Player's 880 Hz sound: {db(s['soundBefore'])}",
    f"soundboard muted: {db(s['soundMuted'])}",
    f"Talker with the soundboard muted: {db(v['voiceMeanwhile'])}",
    f"soundboard unmuted: {db(s['soundAfter'])}",
]))
SILENT = -70.0  # below this nothing is heard
failures = [reason for ok, reason in [
    (v["at100"] > -40, "Talker's tone never came"),
    (-7 <= v["at50"] - v["at100"] <= -5, "50 % is -6 dB"),
    (5 <= v["at200"] - v["at100"] <= 7, "200 % is +6 dB"),
    (v["muted"] < SILENT, "muted is silent"),
    (abs(v["unmuted"] - v["at100"]) < 1, "unmuted is as before"),
    (s["soundBefore"] > -30, "the sound plays"),
    (s["soundMuted"] < SILENT, "the muted soundboard is silent"),
    (abs(v["voiceMeanwhile"] - v["at100"]) < 1, "the voice is unchanged"),
    (s["soundAfter"] > -30, "the sound plays again"),
] if not ok]
for reason in failures:
    print(f"FAIL: {reason}", file=sys.stderr)
if failures:
    # Each window's 100 ms blocks, to see what went wrong.
    for name, (begin, end) in windows.items():
        hz = 880 if name.startswith("sound") else 440
        blocks = " ".join(f"{level:.0f}" for level in levels(begin, end, hz))
        print(f"{name} ({hz} Hz): {blocks}", file=sys.stderr)
sys.exit(1 if failures else 0)
PY
echo "Per-user audio check passed."
