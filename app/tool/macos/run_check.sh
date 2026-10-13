#!/usr/bin/env bash
# Runs one of the shared acceptance scripts that need nothing from Linux but
# the app's device (check_m5, check_m6, check_v0, check_v2, check_v3,
# check_v8) against the macOS app: the same steps, with `-d macos` in place
# of `-d linux`. The shared script stays as it is (docs/plans/platforms.md
# §4); the ones that use PipeWire have macOS ports beside this file.
#
#   app/tool/macos/run_check.sh m6
set -euo pipefail
tool="$(cd "$(dirname "$0")/.." && pwd)"
script="$tool/check_$1.sh"
if [[ ! -f "$script" ]]; then
  echo "No such check: $script" >&2
  exit 1
fi
if grep -qE 'pactl|parec|pw-play|pw-cat' "$script"; then
  echo "check_$1.sh needs PipeWire; use its port in $(dirname "$0")." >&2
  exit 1
fi

copy="$(mktemp)"
trap 'rm -f "$copy"' EXIT
sed -e "s#\$(dirname \"\$0\")#$tool#g" -e 's/ -d linux / -d macos /g' \
  "$script" >"$copy"
bash "$copy" "${@:2}"
