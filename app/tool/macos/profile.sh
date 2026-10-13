# Sourced by the macOS check scripts: on Linux e2e_server.sh gives the app
# fresh data folders through XDG_DATA_HOME, but the sandboxed macOS app
# keeps its data in its container whatever the environment says. So each
# check empties its own e2e profile there before it starts; a run that
# failed half way would otherwise leave its servers behind.

# fresh_profile NAME: deletes profile NAME's data, which must be an e2e one.
fresh_profile() {
  if [[ "$1" != e2e* ]]; then
    echo "Not a test profile: $1" >&2
    exit 1
  fi
  rm -rf "$HOME/Library/Containers/dev.opencord.opencord/Data/Library/Application Support/dev.opencord.opencord/profiles/$1"
}
