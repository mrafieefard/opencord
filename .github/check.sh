#!/usr/bin/env bash
# The checks CI runs on Linux (.github/workflows/cicd.yml), to run before
# opening or updating a pull request (CLAUDE.md). It runs them all, even
# after a failure, and lists what failed at the end.
#
#   .github/check.sh            everything
#   .github/check.sh server     fmt, then the server's crates
#   .github/check.sh client     fmt, then the app's crates and Flutter
#   .github/check.sh mobile     fmt, then the phone facade and the phone app
#
# Windows and macOS, the Linux integration tests and the Docker image run
# only in CI: read the pull request's checks too.
cd "$(dirname "$0")/.."
[[ -f .superpowers/sdd/media/env.sh ]] && source .superpowers/sdd/media/env.sh
export SQLX_OFFLINE=true OPENCORD_SHARED_RUNNER=1

# As in the workflow.
SERVER_CRATES="-p opencord-server -p opencord-common -p opencord-proto"
CLIENT_CRATES="-p opencord_core -p opencord-media -p opencord-voicebot -p opencord-voice -p opencord-launcher -p opencord-mobile"
which=${1:-all}

failed=()
run() {
    echo "=== $1 ($(date +%T), $(df -h / | awk 'NR==2 {print $4}') free)"
    shift
    if ! "$@"; then
        failed+=("$*")
        echo "!!! failed: $*"
    fi
}

run "format" cargo fmt --all -- --check
if command -v actionlint > /dev/null; then
    run "workflow" actionlint .github/workflows/cicd.yml
fi
if [[ $which == all || $which == server ]]; then
    run "server clippy" cargo clippy --locked $SERVER_CRATES --all-targets -- -D warnings
    run "server tests" cargo test --locked --no-fail-fast $SERVER_CRATES
fi
if [[ $which == all || $which == client ]]; then
    run "client clippy" cargo clippy --locked $CLIENT_CRATES --all-targets -- -D warnings
    run "client tests" cargo test --locked --no-fail-fast $CLIENT_CRATES
    run "flutter analyze" bash -c 'cd app && flutter analyze'
    run "flutter test" bash -c 'cd app && flutter test'
fi
if [[ $which == all || $which == mobile ]]; then
    if [[ $which == mobile ]]; then
        run "mobile clippy" cargo clippy --locked -p opencord-mobile --all-targets -- -D warnings
        run "mobile facade tests" cargo test --locked --no-fail-fast -p opencord-mobile
    fi
    if [[ ! -d mobile/node_modules ]]; then
        run "mobile install" bash -c 'cd mobile && yarn install --frozen-lockfile'
    fi
    run "mobile bindings" mobile/scripts/check-bindings.sh
    run "mobile typecheck" bash -c 'cd mobile && yarn -s typecheck'
    run "mobile lint" bash -c 'cd mobile && yarn -s lint'
    run "mobile test" bash -c 'cd mobile && yarn -s test --ci'
    run "mobile licences" bash -c 'cd mobile && yarn -s licenses:check'
fi

echo "=== done ($(date +%T))"
if ((${#failed[@]})); then
    printf 'FAILED: %s\n' "${failed[@]}"
    exit 1
fi
echo "All checks passed."
