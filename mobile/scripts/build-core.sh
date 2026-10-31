#!/usr/bin/env bash
# Builds crates/opencord-mobile for a phone and regenerates the bindings.
#
#   build-core.sh android            the emulator's x86_64 only, debug
#   build-core.sh android --release  arm64-v8a and x86_64, release
#   build-core.sh ios [--release]    device and simulator
#   build-core.sh bindings           the bindings again from the newest
#                                    Android library already built
#
# The bindings (src/generated, cpp/generated) depend only on the facade, so
# any build regenerates them. The turbo-module glue (android/build.gradle
# and the rest) also names the ABIs, so it always comes from the targets
# in ubrn.config.yaml, never from a one-ABI debug build.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh
cd packages/core-native

ubrn="$mobile_root/node_modules/.bin/ubrn"
package="$mobile_root/packages/core-native"

generate() {
    local library=$1
    # Library mode reads the crate's metadata: it runs inside the Rust workspace.
    (cd "$repo_root" && "$ubrn" generate jsi bindings --library \
        --ts-dir "$package/src/generated" --cpp-dir "$package/cpp/generated" "$library")
    "$ubrn" generate jsi turbo-module --config ubrn.config.yaml opencord_mobile
}

newest_android_library() {
    local found=()
    for candidate in "$repo_root"/target/{x86_64,aarch64}-linux-android/{debug,release}/libopencord_mobile.a; do
        [[ -f $candidate ]] && found+=("$candidate")
    done
    if ((${#found[@]})); then
        ls -t "${found[@]}" | head -1
    fi
}

platform=${1:-}
release=()
if [[ ${2:-} == --release ]]; then
    release=(--release)
fi

case $platform in
android)
    if ((${#release[@]})); then
        npx ubrn build android --config ubrn.config.yaml --release
        generate "$repo_root/target/x86_64-linux-android/release/libopencord_mobile.a"
    else
        npx ubrn build android --config ubrn.config.yaml --targets x86_64-linux-android
        generate "$repo_root/target/x86_64-linux-android/debug/libopencord_mobile.a"
    fi
    ;;
ios)
    # The app's minimum (Expo's template). Without it the cc crates build
    # for the SDK's iOS and rustc links for iOS 10: ___chkstk_darwin from
    # opus is then undefined.
    export IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_DEPLOYMENT_TARGET:-16.4}
    npx ubrn build ios --config ubrn.config.yaml --and-generate "${release[@]}"
    ;;
bindings)
    library=$(newest_android_library)
    if [[ -z $library ]]; then
        echo "No Android build yet: run yarn core:android first." >&2
        exit 1
    fi
    generate "$library"
    ;;
*)
    echo "usage: build-core.sh android|ios [--release] | bindings" >&2
    exit 2
    ;;
esac
