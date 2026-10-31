#!/usr/bin/env bash
# FFmpeg's half of build_ffmpeg.ps1, which runs it in MSYS2 with a Visual
# Studio shell's PATH: FFmpeg's configure is a shell script. It is not meant
# to be run by hand.
#
#   build_ffmpeg.sh <prefix> <ffmpeg source> <nv-codec-headers source>
#
# The paths are Windows paths with forward slashes (C:/ffmpeg). The
# pkg-config is pkgconf's Windows build, which build_ffmpeg.ps1 puts on PATH:
# MSYS2's own would split C:/ffmpeg at the colon, and cl can't read the
# /c/ffmpeg paths it gives back.
set -euo pipefail

prefix=${1:?usage: build_ffmpeg.sh <prefix> <ffmpeg source> <nv-codec-headers source>}
ffmpeg=${2:?}
nvcodec=${3:?}

# NVENC's headers only declare what FFmpeg loads at run time.
make -C "$nvcodec" install PREFIX="$prefix" > /dev/null

cd "$ffmpeg"
# The decoders, demuxers, parsers and bitstream filters are ../build_ffmpeg.sh's.
# The encoders and the hardware are Windows': D3D11VA decoding, then NVENC,
# AMF, Quick Sync (through libvpl) and Media Foundation.
# Threads are named because --disable-autodetect turns them off too.
# The C runtime is the DLL one (-MD), as dav1d's and libvpl's.
# libvpl's .pc file names no system libraries for MSVC, so they're named here,
# as .lib files: configure's first link test passes --extra-libs to link.exe
# before it turns -l flags into MSVC's.
if ! PKG_CONFIG_PATH="$prefix/lib/pkgconfig" ./configure \
  --toolchain=msvc --arch=x86_64 --pkg-config=pkgconf \
  --prefix="$prefix" \
  --enable-shared --disable-static \
  --extra-cflags="-MD -I$prefix/include" \
  --extra-libs="ole32.lib uuid.lib advapi32.lib gdi32.lib" \
  --disable-programs --disable-doc --disable-network \
  --disable-autodetect --disable-everything \
  --disable-avdevice --disable-avfilter \
  --enable-w32threads \
  --enable-d3d11va \
  --enable-ffnvcodec --enable-nvenc --enable-amf --enable-libvpl --enable-mediafoundation \
  --enable-libdav1d \
  --enable-decoder=h264,mjpeg,hevc,vp8,vp9,libdav1d,mpeg4,aac,mp3float,opus,vorbis,flac,pcm_s16le,pcm_s24le,pcm_s32le,pcm_f32le,pcm_u8 \
  --enable-encoder=h264_nvenc,h264_amf,h264_qsv,h264_mf,mjpeg \
  --enable-hwaccel=h264_d3d11va,h264_d3d11va2 \
  --enable-demuxer=mov,matroska,ogg,mp3,flac,wav,aac \
  --enable-parser=h264,hevc,vp8,vp9,av1,mpeg4video,aac,mpegaudio,opus,vorbis,flac \
  --enable-bsf=h264_mp4toannexb,hevc_mp4toannexb,vp9_superframe_split; then
  tail -n 80 ffbuild/config.log
  exit 1
fi
make -j"$(nproc)" > /dev/null
make install > /dev/null
