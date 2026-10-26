# Sourced before every yarn, npx, ubrn, adb, emulator and Gradle command
# (docs/mobile.md): the Android SDK, React Native's NDK, and Rust.
mobile_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(cd "$mobile_root/.." && pwd)"
if [[ -f "$repo_root/.superpowers/sdd/media/env.sh" ]]; then
    source "$repo_root/.superpowers/sdd/media/env.sh"
fi
export PATH="$HOME/.cargo/bin:$PATH"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/sdk/android}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
# React Native 0.88's NDK (node_modules/react-native/gradle/libs.versions.toml):
# the Rust library and the turbo-module are built with the same one.
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.1.12297006"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/cmdline-tools/latest/bin:$PATH"
