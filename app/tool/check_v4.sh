#!/usr/bin/env bash
# Phase 2 V4 live check with the real binaries: an opencord-server of its
# own, one voicebot sending the synthetic three-layer camera, another
# watching it in a 360-pixel tile. Every picture must arrive intact, none
# taller than the tile. Its own server and data folders, so it never
# touches yours.
#
#   app/tool/check_v4.sh
set -euo pipefail
source "$(dirname "$0")/e2e_server.sh"

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-voicebot
bot="$repo/target/debug/opencord-voicebot"

# Claim the server, then start it again: a server with an owner logs its
# invite link at startup.
"$bot" --server "127.0.0.1:$port" --claim "$claim" --name Owner --tone 0 \
  --seconds 1 >"$work/claim.log"
kill "${background[0]}"
wait "${background[0]}" 2>/dev/null || true
(
  cd "$work/server"
  OPENCORD_BIND="127.0.0.1:$port" OPENCORD_PUBLIC_HOST=127.0.0.1 \
    OPENCORD_VOICE_UDP_PORT=0 \
    exec "$repo/target/debug/opencord-server"
) >"$work/server2.log" 2>&1 &
background+=("$!")
invite=""
for _ in $(seq 1 150); do
  invite="$(sed 's/\x1b\[[0-9;]*m//g' "$work/server2.log" |
    grep -oE 'invite=[^ ]+' | head -1 | cut -d= -f2- || true)"
  [[ -n "$invite" ]] && break
  sleep 0.2
done
if [[ -z "$invite" ]]; then
  echo "The server logged no invite:" >&2
  cat "$work/server2.log" >&2
  exit 1
fi

"$bot" --server "$invite" --name Camera --camera --seconds 14 \
  >"$work/camera.log" 2>&1 &
camera=$!
background+=("$camera")
sleep 2
"$bot" --server "$invite" --name Watcher --tone 0 --watch 360 --seconds 10 \
  >"$work/watcher.log" 2>&1
wait "$camera"

cat "$work/watcher.log"
seen="$(grep -E 'pictures' "$work/watcher.log" || true)"
if [[ -z "$seen" ]]; then
  echo "The watcher saw no video." >&2
  exit 1
fi
read -r pictures intact high < <(
  sed -E 's/.*: ([0-9]+) pictures, ([0-9]+) intact.*by layer [0-9]+\/[0-9]+\/([0-9]+)$/\1 \2 \3/' \
    <<<"$seen"
)
if ((pictures < 100 || intact != pictures || high != 0)); then
  echo "Expected over 100 pictures, all intact, none of the top layer." >&2
  exit 1
fi
echo "V4 check passed."
