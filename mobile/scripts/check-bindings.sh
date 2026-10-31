#!/usr/bin/env bash
# Fails when the committed bindings or turbo-module glue differ from what
# the facade generates now: run `yarn core:bindings` and commit the result.
# Generates from the newest Android library built (yarn core:android).
set -euo pipefail
cd "$(dirname "$0")/.."
scripts/build-core.sh bindings > /dev/null
changed=$(git status --porcelain -- packages/core-native)
if [[ -n $changed ]]; then
    echo "The bindings are out of date: run yarn core:bindings and commit them." >&2
    echo "$changed" >&2
    exit 1
fi
echo "The bindings are up to date."
