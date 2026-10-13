#!/usr/bin/env bash
# Builds the FFmpeg that ships inside the Linux bundle. Distributions ship
# different FFmpeg majors (libavutil.so.58 on Ubuntu 24.04, .59 on 25.04,
# .60 and up elsewhere), so a bundle linked against the build machine's
# FFmpeg runs only there. This one has just what the media engine uses
# (decisions.md, "Codecs"): FFmpeg's H.264 and MJPEG decoders, VA-API
# decoding and encoding, NVENC and the MJPEG encoder, plus swscale. It is
# LGPL, links only to libva and libva-drm, and loads NVIDIA's libraries at
# run time.
#
#   app/tool/build_ffmpeg.sh <prefix>
#
# Then build the app with PKG_CONFIG_PATH=<prefix>/lib/pkgconfig and copy
# <prefix>/lib/*.so.* next to libopencord_core.so (CI does both). Needs a C
# compiler, make, nasm, pkg-config and libva's headers.
set -euo pipefail

ffmpeg_version=9.0.2
ffmpeg_sha256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
nvcodec_version=13.0.19.0
nvcodec_sha256=86d15d1a7c0ac73a0eafdfc57bebfeba7da8264595bf531cf4d8db1c22940116

prefix=${1:?usage: build_ffmpeg.sh <prefix>}
mkdir -p "$prefix"
prefix=$(cd "$prefix" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

fetch() { # url sha256 file
  curl -fsSL --retry 3 -o "$work/$3" "$1"
  echo "$2  $work/$3" | sha256sum --check --quiet
}

fetch "https://github.com/FFmpeg/nv-codec-headers/archive/refs/tags/n$nvcodec_version.tar.gz" \
  "$nvcodec_sha256" nv-codec-headers.tar.gz
fetch "https://ffmpeg.org/releases/ffmpeg-$ffmpeg_version.tar.xz" \
  "$ffmpeg_sha256" ffmpeg.tar.xz

# NVENC's headers only declare what FFmpeg loads at run time.
tar -C "$work" -xf "$work/nv-codec-headers.tar.gz"
make -C "$work/nv-codec-headers-n$nvcodec_version" install PREFIX="$prefix" > /dev/null

tar -C "$work" -xf "$work/ffmpeg.tar.xz"
cd "$work/ffmpeg-$ffmpeg_version"
PKG_CONFIG_PATH="$prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}" ./configure \
  --prefix="$prefix" --libdir="$prefix/lib" \
  --enable-shared --disable-static --enable-pic \
  --disable-programs --disable-doc --disable-network \
  --disable-autodetect --disable-everything \
  --disable-avdevice --disable-avfilter --disable-swresample \
  --enable-vaapi --enable-ffnvcodec --enable-nvenc \
  --enable-decoder=h264,mjpeg \
  --enable-encoder=h264_vaapi,h264_nvenc,mjpeg \
  --enable-hwaccel=h264_vaapi > /dev/null
make -j"$(nproc)" > /dev/null
make install > /dev/null
