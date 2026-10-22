#!/usr/bin/env bash
# The media uploads plan's transfer numbers (Milestone A8, Task 36): a 1 GiB
# file sent to a release server on loopback and saved back, with the
# server's memory sampled meanwhile. Targets: at least 80 MB/s each way, and
# the server's memory growing by less than 100 MB.
#
#   app/tool/check_transfer.sh [size in MiB, default 1024]
#
# Its own server and folders, under target/ so they're on disk (/tmp may be
# in memory). The file, its stored copy and the download take about three
# times the size, and are deleted at the end.
set -euo pipefail
size_mib="${1:-1024}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

free_mib=$(df --output=avail -m "$repo/target" | tail -1)
if (( free_mib < size_mib * 3 + 1024 )); then
  echo "Needs $((size_mib * 3 + 1024)) MiB free on $repo/target; $free_mib MiB are." >&2
  exit 1
fi

mkdir -p "$repo/target/tmp"
export TMPDIR="$repo/target/tmp"
E2E_SERVER_PROFILE=release
source "$(dirname "$0")/e2e_server.sh"
server_pid="${background[0]}"

cargo build --quiet --release --manifest-path "$repo/Cargo.toml" -p opencord_core \
  --example transfer_load

rss_mib() { awk '/^VmRSS:/ { printf "%d", $2 / 1024 }' "/proc/$server_pid/status"; }
sleep 2
baseline=$(rss_mib)
(
  while kill -0 "$server_pid" 2>/dev/null; do
    rss_mib >>"$work/rss"
    echo >>"$work/rss"
    sleep 0.2
  done
) &
background+=("$!")

"$repo/target/release/examples/transfer_load" "127.0.0.1:$port" "$claim" "$size_mib" "$work"

peak=$(sort -n "$work/rss" | tail -1)
echo "check_transfer: server memory $baseline MiB before, $peak MiB at most: grew $((peak - baseline)) MiB"
