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

cargo build --quiet --manifest-path "$repo/Cargo.toml" -p opencord-server
mkdir -p "$work/server"
(
  cd "$work/server"
  OPENCORD_BIND="127.0.0.1:$port" OPENCORD_PUBLIC_HOST=127.0.0.1 \
    OPENCORD_VOICE_UDP_PORT=0 \
    exec "$repo/target/debug/opencord-server"
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
