#!/usr/bin/env bash
# Screen share sound's Core Audio taps (docs/plans/macos.md M7, step 5): the
# ignored tests in crates/opencord-media/tests/share_audio_macos.rs, with
# BlackHole 2ch as the default output so every afplay tone goes there and
# nothing reaches your speakers; your devices come back at exit. Needs
# audio capture ("System Audio Recording") allowed for the terminal app.
#
#   app/tool/macos/check_share_audio.sh
set -euo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
background=()
stop_background() {
  for pid in ${background[@]+"${background[@]}"}; do
    kill "$pid" 2>/dev/null || true
  done
}
source "$(dirname "$0")/blackhole.sh"

cargo test --manifest-path "$repo/Cargo.toml" -p opencord-media \
  --test share_audio_macos -- --ignored --test-threads=1 --nocapture 2>&1 |
  grep -vE '^\s*(Compiling|Finished|Running|warning)'
