#!/usr/bin/env bash
# Phase 2 V2 against a real server, with this machine's audio devices: the
# app (integration_test/v2_test.dart) claims the server, joins voice, and
# its media connection comes up through the server's voice node. Nothing is
# played. Its own server, data folders and profile, so it never touches
# yours.
#
#   app/tool/check_v2.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

export OPENCORD_PROFILE=e2e_media
cd "$app"
flutter test integration_test/v2_test.dart -d linux \
  --dart-define=OPENCORD_E2E_SERVER="127.0.0.1:$port" \
  --dart-define=OPENCORD_E2E_CLAIM="$claim"
echo "V2 check passed."
