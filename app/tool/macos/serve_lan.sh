#!/usr/bin/env bash
# A server for M8's checks by hand (docs/plans/macos.md M8, step 1):
# reachable on the LAN (bound to 0.0.0.0, advertising this Mac's address),
# so the Mac app, the Linux app and later the Windows PC join one call.
# Prints the invite to paste into each app. Its own temporary folder; stops
# with Ctrl-C or after --hours.
#
#   app/tool/macos/serve_lan.sh [--talker] [--camera] [--port N] [--hours N]
#     --talker  a voicebot "Talker" in General saying 440 Hz, to hear the
#               call come back (after sleep, when AirPods go)
#     --camera  a voicebot "Test camera" in General sending its camera
set -euo pipefail
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
port=47700
hours=3
talker=false
camera=false
while (($#)); do
  case "$1" in
    --talker) talker=true ;;
    --camera) camera=true ;;
    --port) port="$2"; shift ;;
    --hours) hours="$2"; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
host="$(ipconfig getifaddr en0 2>/dev/null || ipconfig getifaddr en1)"
work="$(mktemp -d)"
pids=()
stop() {
  kill ${pids[@]+"${pids[@]}"} 2>/dev/null || true
  rm -rf "$work"
}
trap stop EXIT

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-server -p opencord-voicebot
server="$repo/target/debug/opencord-server"
bot="$repo/target/debug/opencord-voicebot"
mkdir -p "$work/server"
start() {
  (
    cd "$work/server"
    OPENCORD_BIND="0.0.0.0:$port" OPENCORD_PUBLIC_HOST="$host" OPENCORD_VOICE_UDP_PORT=0 \
      exec "$server"
  ) >"$work/$1" 2>&1 &
  pids+=("$!")
}
# The first value of $2 in log $1, without colour codes.
grab() {
  for _ in $(seq 1 150); do
    value="$(sed 's/\x1b\[[0-9;]*m//g' "$work/$1" | grep -oE "$2=[^ ]+" | head -1 | cut -d= -f2- || true)"
    [[ -n "$value" ]] && { echo "$value"; return; }
    sleep 0.2
  done
  cat "$work/$1" >&2
  exit 1
}

# Claimed by a voicebot "Owner", then started again: a server with an owner
# logs its invite.
start server.log
claim="$(grab server.log claim_token)"
"$bot" --server "127.0.0.1:$port" --claim "$claim" --name Owner --tone 0 --seconds 1 >"$work/claim.log"
kill "${pids[0]}"
wait "${pids[0]}" 2>/dev/null || true
start server2.log
invite="$(grab server2.log invite)"

seconds=$((hours * 3600))
if [[ "$talker" == true ]]; then
  "$bot" --server "$invite" --name Talker --tone 440 --seconds "$seconds" >"$work/talker.log" 2>&1 &
  pids+=("$!")
fi
if [[ "$camera" == true ]]; then
  "$bot" --server "$invite" --name "Test camera" --tone 0 --camera --seconds "$seconds" >"$work/camera.log" 2>&1 &
  pids+=("$!")
fi
echo "Server on $host:$port for $hours h. In each app: Add a server → paste:"
echo "$invite"
# In the background, so Ctrl-C runs the trap at once.
sleep "$seconds" &
pids+=("$!")
wait "$!"
