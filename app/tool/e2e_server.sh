# Sourced by the check_m*.sh acceptance scripts: an opencord-server of their
# own on a free port, in a temporary folder that goes when the script exits,
# and data folders of their own for the app, so a check never touches your
# server, your data or your identity. Sets $app, $repo, $work, $port and
# $claim; anything started later can be added to $background to be stopped
# at exit.

app="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo="$(dirname "$app")"
work="$(mktemp -d)"
background=()

stop_background() {
  for pid in "${background[@]}"; do
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done
  rm -rf "$work"
}
trap stop_background EXIT

port="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')"

# A check that measures sets E2E_SERVER_PROFILE=release before sourcing
# this: a debug build's processing delays distort the node's bandwidth
# estimates (docs/decisions.md D28).
server_profile="${E2E_SERVER_PROFILE:-debug}"
server="$repo/target/$server_profile/opencord-server"
if [[ "$server_profile" == release ]]; then
  cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord-server
else
  cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-server
fi
mkdir -p "$work/server"
(
  cd "$work/server"
  OPENCORD_BIND="127.0.0.1:$port" OPENCORD_PUBLIC_HOST=127.0.0.1 \
    OPENCORD_VOICE_UDP_PORT=0 \
    exec "$server"
) >"$work/server.log" 2>&1 &
background+=("$!")

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

# Claims the server as Owner with the voicebot at $1, then starts it again
# (a server with an owner logs its invite link at startup) and sets
# $invite to that link.
claim_and_invite() {
  "$1" --server "127.0.0.1:$port" --claim "$claim" --name Owner --tone 0 \
    --seconds 1 >"$work/claim.log"
  kill "${background[0]}"
  wait "${background[0]}" 2>/dev/null || true
  (
    cd "$work/server"
    OPENCORD_BIND="127.0.0.1:$port" OPENCORD_PUBLIC_HOST=127.0.0.1 \
      OPENCORD_VOICE_UDP_PORT=0 \
      exec "$server"
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
}
