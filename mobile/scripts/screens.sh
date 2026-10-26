#!/usr/bin/env bash
# Screenshots of the running app, each route in dark and light, for UI
# review (plan: "Screenshots are part of done").
#
#   screens.sh <task> <name>=<route> … [--tablet]
#
# Routes are the app's paths ("/servers", "/server/fake-night-owls"). The
# PNGs go to .superpowers/sdd/mobile/screens/<task>/<name>-<theme>-<size>.png.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/env.sh

task=${1:?usage: screens.sh <task> <name>=<route> … [--tablet]}
shift
size=phone
pairs=()
for argument in "$@"; do
    if [[ $argument == --tablet ]]; then
        size=tablet
    else
        pairs+=("$argument")
    fi
done

out="$repo_root/.superpowers/sdd/mobile/screens/$task"
mkdir -p "$out"
for theme in dark light; do
    if [[ $theme == dark ]]; then
        adb shell cmd uimode night yes > /dev/null
    else
        adb shell cmd uimode night no > /dev/null
    fi
    for pair in "${pairs[@]}"; do
        name=${pair%%=*}
        route=${pair#*=}
        adb shell am start -W -a android.intent.action.VIEW \
            -d "opencord-app://${route}" dev.opencord.mobile > /dev/null
        sleep 2
        adb exec-out screencap -p > "$out/$name-$theme-$size.png"
        echo "$out/$name-$theme-$size.png"
    done
done
