#!/usr/bin/env bash
# Headsets coming and going during a call (docs/plans/macos.md M8, step 1):
# the ignored tests in crates/opencord-media/tests/devices_macos.rs, which
# plug a "headset" (an aggregate device over BlackHole 2ch) and pull it out
# or change its sample rate while the audio engine runs. BlackHole 2ch is
# the default output and input meanwhile, so nothing reaches your speakers
# or headphones; your devices come back at exit (about 20 s).
#
#   app/tool/macos/check_devices.sh
set -euo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
background=()
stop_background() {
  for pid in ${background[@]+"${background[@]}"}; do
    kill "$pid" 2>/dev/null || true
  done
}
# Built before the defaults move, so they move only for the tests.
cargo test --quiet --manifest-path "$repo/Cargo.toml" -p opencord-media \
  --test devices_macos --no-run
source "$(dirname "$0")/blackhole.sh"

cargo test --manifest-path "$repo/Cargo.toml" -p opencord-media \
  --test devices_macos -- --ignored --test-threads=1 --nocapture 2>&1 |
  grep -vE '^\s*(Compiling|Finished|Running|warning)'
