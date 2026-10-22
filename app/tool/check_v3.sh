#!/usr/bin/env bash
# Phase 2 V3 against a real server, with this machine's audio devices and
# CPU: the app (integration_test/v3_test.dart) picks noise suppression by
# benchmark, joins voice with the whole processing chain on the microphone,
# shows its level and asks about hotkeys. Nothing is played. Its own server,
# data folders and profile, so it never touches yours.
#
#   app/tool/check_v3.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

export OPENCORD_PROFILE=e2e_processing
cd "$app"
flutter test integration_test/v3_test.dart -d linux \
  --dart-define=OPENCORD_E2E_SERVER="127.0.0.1:$port" \
  --dart-define=OPENCORD_E2E_CLAIM="$claim"
echo "V3 check passed."
