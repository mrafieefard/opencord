#!/usr/bin/env bash
# Builds the plugin areas' Flutter module (mobile/flutter_module) for the
# phone apps (plugins M4):
#
#   build-flutter.sh android [--release]   AARs into flutter_module/build/host/outputs/repo
#   build-flutter.sh ios [--release]       frameworks into modules/plugin-area/ios/Frameworks
#
# Without --release only the debug build is made (Android: x86_64, for the
# emulator); with it, only the release one (Android: arm64 and x86_64, the
# processor types the core is built for).
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh

platform=${1:-}
release=false
[[ ${2:-} == --release ]] && release=true

# PLUGIN_GATES=1 compiles in Stage A's measurements (flutter_module/lib/src/gates.dart).
defines=()
[[ ${PLUGIN_GATES:-} == 1 ]] && defines=(--dart-define=PLUGIN_GATES=true)

cd flutter_module
flutter pub get > /dev/null
case $platform in
android)
    if $release; then
        flutter build aar --no-debug --no-profile --target-platform android-arm64,android-x64 "${defines[@]}"
    else
        flutter build aar --no-profile --no-release --target-platform android-x64 "${defines[@]}"
    fi
    ;;
ios)
    out=../modules/plugin-area/ios/Frameworks
    rm -rf "$out"
    if $release; then
        flutter build ios-framework --no-debug --no-profile --output="$out"
        mv "$out/Release"/* "$out/" && rmdir "$out/Release"
    else
        flutter build ios-framework --no-profile --no-release --output="$out"
        mv "$out/Debug"/* "$out/" && rmdir "$out/Debug"
    fi
    ;;
*)
    echo "usage: build-flutter.sh android|ios [--release]" >&2
    exit 2
    ;;
esac
