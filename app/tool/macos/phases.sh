# Sourced by the macOS check scripts after ../e2e_server.sh: measures what
# the app sends, phase by phase. The app's integration test prints
# "PHASE: <name>" and holds that phase for 10 s; for each one a Listener
# voicebot joins General, listens for 5 s and reports how loud a frequency
# was in what the others sent. "PHASE: done" ends it.

# listen_in_phases BOT INVITE LOG HZ [IGNORED_USER_ID...]
# Prints "<phase>: <the Listener's line about the app>", or "<phase>:
# nothing" when nothing came from the app, for each phase until the test
# prints "PHASE: done" or stops.
listen_in_phases() {
  local bot="$1" invite="$2" log="$3" hz="$4"
  local ignored=("${@:5}")
  local seen=0 phase report line user skip
  while :; do
    phase="$(grep -o 'PHASE: .*' "$log" 2>/dev/null | sed -n "$((seen + 1))p" |
      cut -d' ' -f2-)"
    if [[ -z "$phase" ]]; then
      # The test may have stopped before its last phase.
      kill -0 "$test_pid" 2>/dev/null || return 0
      sleep 0.2
      continue
    fi
    seen=$((seen + 1))
    [[ "$phase" == done ]] && return 0
    report="$("$bot" --server "$invite" --name Listener --tone 0 \
      --listen-for "$hz" --seconds 5 2>&1 || true)"
    line=""
    while IFS= read -r candidate; do
      user="$(sed -nE 's/^user ([0-9]+): .* packets.*/\1/p' <<<"$candidate")"
      [[ -z "$user" ]] && continue
      skip=""
      # macOS bash 3.2 calls an empty array unbound under set -u.
      for id in ${ignored[@]+"${ignored[@]}"}; do
        [[ "$user" == "$id" ]] && skip=1
      done
      [[ -z "$skip" ]] && line="$candidate"
    done <<<"$report"
    echo "$phase: ${line:-nothing}"
  done
}

# The user id a voicebot's log says it joined as.
voicebot_user() {
  local id=""
  for _ in $(seq 1 150); do
    id="$(sed -nE 's/^joined .* as user ([0-9]+)$/\1/p' "$1" | head -1)"
    [[ -n "$id" ]] && break
    sleep 0.2
  done
  echo "$id"
}

# The level the Listener's line gives for its frequency, or -200 for none.
tone_level() {
  local level
  level="$(grep -oE '[0-9.]+ Hz -?[0-9.]+ dBFS' <<<"$1" | tail -1 |
    awk '{print $3}')"
  echo "${level:--200}"
}
