#!/usr/bin/env bash
# Phase 1 M5 acceptance against a real server: a fresh install creates an
# identity, joins a local server as its owner (TOFU), restarts and is still
# connected. Runs the app twice through integration_test/m5_test.dart, with
# its own server, data folders and profile, so it never touches yours.
#
#   app/tool/check_m5.sh
set -euo pipefail

app="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(dirname "$app")"
work="$(mktemp -d)"
server_pid=""

cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-server
mkdir -p "$work/server"
(
  cd "$work/server"
  OPENCORD_BIND="127.0.0.1:$port" OPENCORD_PUBLIC_HOST=127.0.0.1 \
    exec "$repo/target/debug/opencord-server"
) >"$work/server.log" 2>&1 &
server_pid=$!

# The token is in the first start's log; drop colour codes before reading it.
claim=""
for _ in $(seq 1 150); do
  claim="$(sed 's/\x1b\[[0-9;]*m//g' "$work/server.log" |
    grep -oE 'claim_token=[A-Za-z0-9_-]+' | head -1 | cut -d= -f2 || true)"
  [[ -n "$claim" ]] && break
  sleep 0.2
done
if [[ -z "$claim" ]]; then
  echo "The server did not start:" >&2
  cat "$work/server.log" >&2
  exit 1
fi

export XDG_DATA_HOME="$work/data" XDG_CONFIG_HOME="$work/config"
export OPENCORD_PROFILE=e2e
cd "$app"
for phase in join restart; do
  flutter test integration_test/m5_test.dart -d linux \
    --dart-define=OPENCORD_E2E_SERVER="127.0.0.1:$port" \
    --dart-define=OPENCORD_E2E_CLAIM="$claim" \
    --dart-define=OPENCORD_E2E_PHASE="$phase"
done
echo "M5 check passed."
