# Packaging

What each desktop build carries, what it needs on the machine, and what is still to decide before a public release. CI builds all three (`.github/workflows/ci.yml`); a version tag attaches them to a GitHub release. The server's packaging is `deploy/` (docs/voice.md, "Docker").

## Linux

The release is `bundle/` from `flutter build linux --release`, as a tarball.

- **Carried:** FFmpeg 9.0.2 (LGPL, H.264, MJPEG, HEVC, VP8, VP9, MPEG-4 and static dav1d 1.5.4 (BSD-2) AV1 video decoders, AAC, MP3, Opus, Vorbis, FLAC and PCM audio decoders, demuxers, parsers, bitstream filters, VA-API and NVENC encoding, and swscale), next to `libopencord_core.so`, found through its `RUNPATH` (D32). Opus, RNNoise, DeepFilterNet and the echo canceller are built into the core.
- **Needed on the machine:** glibc 2.39 or newer (the build runs on Ubuntu 24.04), GTK 3, libva 2 (and a VA-API driver for hardware video; without one, video falls back to software), PipeWire with its PulseAudio or ALSA layer for sound, and the xdg-desktop-portal for screen sharing, the camera on Wayland, and global keybinds on Wayland. A Secret Service (GNOME Keyring or KWallet) keeps the identity key.
- **Written on first run:** with no packaged desktop file, the app writes `~/.local/share/applications/dev.opencord.opencord.desktop` for wherever it runs from, and rewrites it when the app moves. The GlobalShortcuts portal refuses keybinds for an app it can't find there ("App info not found"). A package's own desktop file is left alone.
- **Optional:** a compositor with ext-idle-notify-v1 (KDE, Hyprland, Sway), GNOME's Mutter, or X11, for AFK moves; without one, nobody is moved for being away.

### Flatpak

There is no Flatpak yet. When one is made, the app needs:

| Permission | For |
|---|---|
| `--socket=wayland`, `--socket=fallback-x11`, `--share=ipc` | The window |
| `--share=network` | Servers and calls |
| `--device=dri` | VA-API video encoding and decoding |
| `--filesystem=xdg-run/pipewire-0` | Direct PipeWire access: screen share sound links other apps' streams, and attenuation lowers them. The camera and screen share themselves come through portals and need nothing more |
| `--socket=pulseaudio` | Microphone and speakers through cpal |
| `--talk-name=org.freedesktop.secrets` | The identity key in the keyring |
| `--talk-name=org.gnome.Mutter.IdleMonitor` | Idle time on GNOME, for AFK moves |
| `--talk-name=org.kde.StatusNotifierWatcher` | The tray icon |

V4L2 cameras without the camera portal would need `--device=all`; the camera portal is the way inside a Flatpak.

## macOS

The release is a universal (x86_64 and arm64) `Opencord.app`, zipped, needing macOS 13 (D34).

- **Usage strings** (`app/macos/Runner/Info.plist`), which macOS shows when it asks:
  - microphone: "Opencord uses your microphone in voice channels.";
  - camera: "Opencord uses your camera when you turn it on in a voice channel.";
  - other apps' sound: "Opencord records other apps' sound when you share your screen with sound."
  - Screen recording and Input Monitoring have no usage string; macOS asks with its own words when a share starts and when the first keybind is set.
- **Entitlements** (`Release.entitlements`): the app sandbox, `device.audio-input`, `device.camera`, `network.client` and `network.server` (the sandbox stops voice's UDP socket without it), and read-only access to files the user picks (sound uploads).
- **Not done:**
  - **Signing and notarization.** The release is signed ad hoc and not notarized: the first launch needs System Settings → Privacy & Security → Open Anyway, and macOS asks for every permission again after each update. A Developer ID certificate, notarization in CI and the hardened runtime fix both; that is the owner's decision (it costs an Apple Developer membership).
  - **The hardened runtime** (`ENABLE_HARDENED_RUNTIME`) is off. It goes with the Developer ID signature: under an ad hoc signature, its library validation may refuse the app's own embedded frameworks, which has to be checked on a Mac before turning it on. With it on, the same `device.audio-input` and `device.camera` entitlements serve it.

## Windows

The release is a zip of the `Release` folder (W8, D35).

- **Carried:** the five FFmpeg 9.0.2 LGPL DLLs (avcodec, avformat, avutil, swresample, swscale; 120 MB) with FFmpeg's license, and Microsoft's C++ runtime (`msvcp140.dll`, `vcruntime140.dll`, `vcruntime140_1.dll`), so no redistributable is needed.
- **Needs:** Windows 10 21H2 or newer; build 20348 (Windows 11, or Server 2022) for sharing a screen's sound.
- **Not done:**
  - **Code signing:** unsigned, so SmartScreen warns on every download (an OV or EV certificate, or Azure Trusted Signing, plus a signing step in CI).
  - **An installer:** MSIX, or WiX or Inno Setup, which would also bring updates.
  - **A smaller FFmpeg:** a build with only what the app uses, as Linux has, should take the download under 100 MB.
  - **A software H.264 encoder:** a PC with no GPU encoder cannot send video (D35).
