#!/usr/bin/env bash
# Phase 2 V0 acceptance against a real server: voice states without media.
# The app (integration_test/v0_test.dart) is a member who sees the owner
# join voice live, joins too, is moved to another channel and then
# disconnected by the owner, all without reconnecting. The owner is
# crates/opencord-core/examples/v0_peer.rs, on the same core. Its own
# server, data folders and profile, so it never touches yours.
#
#   app/tool/check_v0.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord_core \
  --example v0_peer
"$repo/target/debug/examples/v0_peer" "127.0.0.1:$port" "$claim" \
  "$work/invite" &
owner=$!
background+=("$owner")

# The owner claims the server, then leaves an invite for the app.
for _ in $(seq 1 150); do
  [[ -s "$work/invite" ]] && break
  if ! kill -0 "$owner" 2>/dev/null; then
    echo "The owner stopped before inviting anyone." >&2
    exit 1
  fi
  sleep 0.2
done
if [[ ! -s "$work/invite" ]]; then
  echo "No invite from the owner." >&2
  exit 1
fi

export OPENCORD_PROFILE=e2e_voice
cd "$app"
flutter test integration_test/v0_test.dart -d linux \
  --dart-define=OPENCORD_E2E_SERVER="127.0.0.1:$port" \
  --dart-define=OPENCORD_E2E_INVITE="$(cat "$work/invite")"
# The owner's own steps must have gone through as well.
wait "$owner"
echo "V0 check passed."
