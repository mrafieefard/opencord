#!/usr/bin/env bash
# The app icon: "oc" in Inter Bold, #EDEDEF on #0E0E10, until there is a
# logo (design §11). Drawn by ImageMagick straight from the app's own font.
set -euo pipefail
cd "$(dirname "$0")/../assets"
font=fonts/Inter-Bold.ttf
# iOS and the store: the whole square.
magick -size 1024x1024 xc:'#0E0E10' -font "$font" -pointsize 500 -fill '#EDEDEF' \
    -gravity center -annotate +0-30 'oc' icon.png
# Android's adaptive icon: the letters alone, inside its 66 % safe zone, on
# the background color app.config.ts sets.
magick -size 1024x1024 xc:none -font "$font" -pointsize 330 -fill '#EDEDEF' \
    -gravity center -annotate +0-20 'oc' adaptive-icon.png
