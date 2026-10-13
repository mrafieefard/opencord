"""The level of one or more tones in raw audio, in dBFS, for the Windows checks.

The Linux checks get this from the voicebot (`tone_dbfs` in
crates/opencord-voicebot/src/lib.rs) or from Dart (`_levels` in
app/integration_test/user_audio_test.dart). The Windows ones also need it in a
script, because what they measure comes out of ffmpeg rather than out of a
voicebot (windows.md §3, decision F). Same Goertzel, same definition: a
full-scale sine reads 0 dBFS.

Input is raw 32-bit float mono at 48 kHz on stdin, which is what

    ffmpeg -f dshow -i audio="CABLE Output (VB-Audio Virtual Cable)" \
        -f f32le -ar 48000 -ac 1 -

writes. Run it with `py -3` (windows.md §3).

    ... | py -3 app/tool/windows/measure.py 440 880

prints one line per frequency:

    440 Hz -12.3 dBFS
    880 Hz -inf dBFS

With --blocks it instead prints, per frequency, the level of each 100 ms block
and then the median and the loudest, which is how the per-user audio check
tells a steady tone from a sound that comes and goes:

    440 Hz blocks=20 median=-12.3 dBFS loudest=-12.1 dBFS
"""

import argparse
import array
import math
import sys

RATE = 48000

# 100 ms, a whole number of cycles of every frequency these checks use.
BLOCK = 4800

# Below this, nothing is heard; the same floor the Dart test uses.
FLOOR = -200.0


def tone_dbfs(samples, frequency):
    """The level of `frequency` in mono `samples`, in dBFS."""
    if len(samples) == 0:
        return FLOOR
    coefficient = 2.0 * math.cos(2.0 * math.pi * frequency / RATE)
    s1 = s2 = 0.0
    for sample in samples:
        s1, s2 = sample + coefficient * s1 - s2, s1
    power = max(s1 * s1 + s2 * s2 - coefficient * s1 * s2, 0.0)
    amplitude = 2.0 * math.sqrt(power) / len(samples)
    if amplitude <= 0.0:
        return FLOOR
    return 20.0 * math.log10(amplitude)


def blocks(samples, frequency):
    """Each whole 100 ms block's level of `frequency`."""
    return [
        tone_dbfs(samples[start : start + BLOCK], frequency)
        for start in range(0, len(samples) - BLOCK + 1, BLOCK)
    ]


def show(level):
    return "-inf" if level <= FLOOR else f"{level:.1f}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "frequencies",
        nargs="+",
        type=float,
        help="the frequencies to measure, in Hz",
    )
    parser.add_argument(
        "--blocks",
        action="store_true",
        help="report each 100 ms block, with the median and the loudest",
    )
    parser.add_argument(
        "--input",
        help="read from this file instead of stdin",
    )
    arguments = parser.parse_args()

    if arguments.input:
        with open(arguments.input, "rb") as source:
            raw = source.read()
    else:
        raw = sys.stdin.buffer.read()

    samples = array.array("f")
    samples.frombytes(raw[: len(raw) - len(raw) % samples.itemsize])
    if len(samples) == 0:
        print("no samples", file=sys.stderr)
        return 1

    print(f"{len(samples)} samples, {len(samples) / RATE:.2f} s", file=sys.stderr)
    for frequency in arguments.frequencies:
        if arguments.blocks:
            levels = blocks(samples, frequency)
            if not levels:
                print(f"{frequency:g} Hz blocks=0", flush=True)
                continue
            median = sorted(levels)[len(levels) // 2]
            print(
                f"{frequency:g} Hz blocks={len(levels)} "
                f"median={show(median)} dBFS loudest={show(max(levels))} dBFS",
                flush=True,
            )
        else:
            print(f"{frequency:g} Hz {show(tone_dbfs(samples, frequency))} dBFS", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
