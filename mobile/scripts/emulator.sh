#!/usr/bin/env bash
# Boots the phone (Pixel_7) or tablet (Pixel_Tablet) emulator unless one is
# running, and waits until Android has started.
#
#   emulator.sh [phone|tablet]
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh

case ${1:-phone} in
phone) avd=Pixel_7 device=pixel_7 ;;
tablet) avd=Pixel_Tablet device=pixel_tablet ;;
*)
    echo "usage: emulator.sh [phone|tablet]" >&2
    exit 2
    ;;
esac

if ! emulator -list-avds | grep -qx "$avd"; then
    # From the system image the phone uses; nothing new to download.
    echo no | avdmanager create avd -n "$avd" \
        -k "system-images;android-33;google_apis;x86_64" -d "$device"
fi

# One emulator at a time: stop the other one first (adb emu kill).
if [[ $(adb devices | grep -c 'emulator-.*device$') -eq 0 ]]; then
    nohup emulator -avd "$avd" -no-snapshot-save -allow-host-audio > "/tmp/emulator-$avd.log" 2>&1 &
fi
adb wait-for-device
until [[ $(adb shell getprop sys.boot_completed 2> /dev/null | tr -d '\r') == 1 ]]; do
    sleep 2
done
echo "$avd is up"
