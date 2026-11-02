#!/usr/bin/env bash
# Builds the FFmpeg that the media player uses on macOS, and that
# Opencord.app carries in Contents/Frameworks (D89). It decodes only: the
# decoders, demuxers, parsers and bitstream filters are ../build_ffmpeg.sh's,
# with AV1 through a static dav1d, and VideoToolbox for H.264 and HEVC.
# Calls keep their own VideoToolbox codec (video/codec/macos/) and don't use
# it. No encoders, no protocols (the player reads through its own I/O), and
# LGPL only.
#
#   app/tool/macos/build_ffmpeg.sh <prefix>
#
# The dylibs are universal (x86_64 and arm64; one is a cross build), need
# macOS 13 (D34), and are named @rpath/libavcodec.N.dylib and so on. Then
# build the app with FFMPEG_DIR=<prefix> (the podspec links them and the
# Runner copies them into the app). Needs Xcode's command line tools, nasm,
# pkg-config, meson and ninja.
set -euo pipefail

dav1d_version=1.5.4
dav1d_sha256=686616b7c69eb88d44459391ab25cac13b6647a3b288835c5784e71c1514a5c5
ffmpeg_version=9.0.2
ffmpeg_sha256=8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e
min_macos=13.0

prefix=${1:?usage: build_ffmpeg.sh <prefix>}
mkdir -p "$prefix"
prefix=$(cd "$prefix" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
jobs=$(sysctl -n hw.ncpu)

fetch() { # url sha256 file
  curl -fsSL --retry 3 -o "$work/$3" "$1"
  echo "$2  $work/$3" | shasum -a 256 --check --quiet
}

fetch "https://downloads.videolan.org/pub/videolan/dav1d/$dav1d_version/dav1d-$dav1d_version.tar.xz" \
  "$dav1d_sha256" dav1d.tar.xz
fetch "https://ffmpeg.org/releases/ffmpeg-$ffmpeg_version.tar.xz" \
  "$ffmpeg_sha256" ffmpeg.tar.xz
tar -C "$work" -xf "$work/dav1d.tar.xz"
tar -C "$work" -xf "$work/ffmpeg.tar.xz"

# One architecture into $work/<arch>: dav1d as a static library, so
# libavcodec links it in, then FFmpeg's shared libraries.
build() { # arch: x86_64 or arm64
  local arch=$1 out="$work/$1"
  local flags="-arch $arch -mmacosx-version-min=$min_macos"

  local cpu=x86_64
  [ "$arch" = arm64 ] && cpu=aarch64
  cat > "$work/$arch.ini" <<EOF
[binaries]
c = ['clang', '-arch', '$arch']
ar = 'ar'
strip = 'strip'
nasm = 'nasm'

[built-in options]
c_args = ['-mmacosx-version-min=$min_macos']
c_link_args = ['-mmacosx-version-min=$min_macos']

[host_machine]
system = 'darwin'
cpu_family = '$cpu'
cpu = '$cpu'
endian = 'little'
EOF
  (
    cd "$work/dav1d-$dav1d_version"
    meson setup "build-$arch" --cross-file "$work/$arch.ini" --prefix="$out" --libdir=lib \
      --buildtype=release --default-library=static -Denable_tools=false -Denable_tests=false > /dev/null
    ninja -C "build-$arch" install > /dev/null
  )

  local cross=()
  [ "$arch" != "$(uname -m)" ] && cross=(--enable-cross-compile)
  rm -rf "$work/ffmpeg-$arch"
  cp -R "$work/ffmpeg-$ffmpeg_version" "$work/ffmpeg-$arch"
  (
    cd "$work/ffmpeg-$arch"
    # Threads are named because --disable-autodetect turns them off too.
    if ! PKG_CONFIG_PATH="$out/lib/pkgconfig" PKG_CONFIG_LIBDIR="$out/lib/pkgconfig" ./configure \
      "${cross[@]}" --arch="$arch" --target-os=darwin \
      --cc=clang --extra-cflags="$flags" --extra-ldflags="$flags" \
      --prefix="$out" --libdir="$out/lib" --install-name-dir=@rpath \
      --enable-shared --disable-static --enable-pic \
      --disable-programs --disable-doc --disable-network \
      --disable-autodetect --disable-everything \
      --disable-avdevice --disable-avfilter \
      --enable-pthreads \
      --enable-videotoolbox \
      --enable-libdav1d \
      --enable-decoder=h264,mjpeg,hevc,vp8,vp9,libdav1d,mpeg4,aac,mp3float,opus,vorbis,flac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_u8 \
      --enable-hwaccel=h264_videotoolbox,hevc_videotoolbox \
      --enable-demuxer=mov,matroska,ogg,mp3,flac,wav,aac \
      --enable-parser=h264,hevc,vp8,vp9,av1,mpeg4video,aac,mpegaudio,opus,vorbis,flac \
      --enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,vp9_superframe_split > /dev/null; then
      tail -n 80 ffbuild/config.log
      exit 1
    fi
    make -j"$jobs" > /dev/null
    make install > /dev/null
  )
}

build x86_64
build arm64

# The headers and pkg-config files are the same for both; the dylibs are
# merged, and their version links kept.
rm -rf "$prefix/include" "$prefix/lib"
mkdir -p "$prefix/lib/pkgconfig"
cp -R "$work/x86_64/include" "$prefix/include"
for lib in "$work"/x86_64/lib/lib{avcodec,avformat,avutil,swresample,swscale}*.dylib; do
  name=$(basename "$lib")
  if [ -L "$lib" ]; then
    cp -P "$lib" "$prefix/lib/$name"
  else
    lipo -create "$lib" "$work/arm64/lib/$name" -output "$prefix/lib/$name"
  fi
done
for pc in "$work"/x86_64/lib/pkgconfig/lib{avcodec,avformat,avutil,swresample,swscale}.pc; do
  sed "s|$work/x86_64|$prefix|g" "$pc" > "$prefix/lib/pkgconfig/$(basename "$pc")"
done
cp "$work/ffmpeg-$ffmpeg_version/COPYING.LGPLv2.1" "$prefix/LICENSE.txt"
