#!/usr/bin/env bash
# Phase 1 M5 acceptance against a real server: a fresh install creates an
# identity, joins a local server as its owner (TOFU), restarts and is still
# connected. Runs the app twice through integration_test/m5_test.dart, with
# its own server, data folders and profile, so it never touches yours.
#
#   app/tool/check_m5.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

export OPENCORD_PROFILE=e2e
cd "$app"
for phase in join restart; do
  flutter test integration_test/m5_test.dart -d linux \
    --dart-define=OPENCORD_E2E_SERVER="127.0.0.1:$port" \
    --dart-define=OPENCORD_E2E_CLAIM="$claim" \
    --dart-define=OPENCORD_E2E_PHASE="$phase"
done
echo "M5 check passed."
