# Platform work: three machines, one codebase

> For the three Claude Code sessions working at the same time and the person
> running them:
> - the **Linux** session, on the dev machine;
> - the **Mac** session, following [macos.md](macos.md);
> - the **Windows** session, following [windows.md](windows.md).
>
> Read this before the platform plan. Where the platform plan and this file
> disagree about who may change a file, this file wins.

## 1. Branches

| Branch | Who commits | Made from |
|---|---|---|
| `phase2-remaining` | Linux session | — (the integration branch) |
| `phase2-macos` | Mac session | `phase2-remaining` |
| `phase2-windows` | Windows session | `phase2-remaining` |

**Platform sessions:**
- Start each milestone with `git fetch && git merge origin/phase2-remaining`.
  That is how they get the seams and the other platform's merged work.
- Merge, never rebase: the branches are pushed.
- Push at the end of each milestone.
- Never merge each other's branch.
- Never commit to `main` or `pre`.

**The Linux session**, when the user says a platform pushed:
1. Merges that branch into `phase2-remaining`.
2. Resolves conflicts (§4).
3. Runs these checks:
   - Rust tests, clippy and fmt;
   - `flutter analyze` and `flutter test`;
   - the Linux live checks the change could affect;
   - the off-Linux type-check (L1 adds it).
4. Pushes.

It reports a Linux regression to the user. It fixes one only in Linux-owned
files, or in a shared file with a `fix(shared):` commit.

## 2. The seams (L1): Linux session, before M2 and W2

Today almost every platform-specific piece is behind
`cfg(target_os = "linux")`, inside files that both platforms would have to
change: the hotkey dispatch, the camera/screen/share-audio modules, the codec
module list, about 25 sites in the core, and the generated Dart bindings. If
the Mac and Windows sessions each opened those up, they would collide. So the
Linux session opens them once: every platform gets its own stub files, and the
shared code calls them. Afterwards each platform only fills in its own files.

**Doing L1 on Linux** means its refactor of the Linux code can be checked live
there (`check_v5`–`check_v8`, `check_user_audio`). Neither platform machine
can run those.

**Status:** L1 has landed when
`git log origin/phase2-remaining --oneline --grep 'platform seams (L1)'`
prints a commit. Until then:

| Platform | Can do before L1 | Must wait for L1 |
|---|---|---|
| Mac | M0, M1 | M2 onwards |
| Windows | W0, W1 | W2 onwards |

What L1 delivers:
1. **Everything compiles on every desktop OS.**
   - Shared code no longer has `cfg(target_os = "linux")` gates, except where
     it calls Linux-owned files.
   - A platform feature that isn't there yet reports itself unsupported at
     run time.
   - The off-Linux dead-code warnings that exist today go away, because the
     stubs use those items.
   - `app/tool/check_platforms.sh` type-checks macOS and Windows from Linux.
     It uses cargo-xwin and cargo-zigbuild if they work; otherwise it falls
     back to a FreeBSD target, which only exercises the non-Linux code paths.
2. **Platform files, each a stub with doc comments that state the contract:**

   | Area | Linux (existing) | macOS stub | Windows stub |
   |---|---|---|---|
   | Global hotkeys | `hotkeys/{x11,portal}.rs` | `hotkeys/macos.rs` | `hotkeys/windows.rs` |
   | Camera | `video/camera/{pipewire,v4l2}.rs` | `video/camera/avfoundation.rs` | `video/camera/media_foundation.rs` |
   | Screen pick + capture | `video/screen/{linux,capture,portal}.rs` | `video/screen/macos.rs` | `video/screen/windows.rs` |
   | Screen share sound | `share_audio/pipewire.rs` | `share_audio/coreaudio.rs` | `share_audio/wasapi.rs` |
   | H.264 + scaling | `video/codec/{encoder,decoder,raw,scale,ffmpeg,openh264,vaapi}.rs` | `video/codec/macos/` | `video/codec/windows_pending.rs` (W3 deletes it; Windows then uses the FFmpeg files) |

   All paths are under `crates/opencord-media/src/`. Each area's `mod.rs`
   keeps one block per OS, separated by a comment line. A session edits only
   its own block.
3. **Contracts:**
   - **Hotkeys:**
     - `bind(bindings, events) -> (Option<Keys>, HotkeySupport)`.
     - Uses the shared `Held` and `action_of` from `hotkeys/mod.rs`.
   - **Camera:**
     - `cameras()`.
     - `Capture::start(id, frames)`, plus the capture's `mode`.
     - Each OS has its own variant in `camera::Capture`.
   - **Screen:**
     - `support() -> ScreenSupport { supported, reason, own_picker }`.
     - `sources()`: own-picker platforms only.
     - `start(StartRequest { fps, restore_token, source_id }, frames)
       -> (Capture, Picked { kind, restore_token, audio })`.
     - `audio` is the window's app when the platform knows it.
     - The core's `video/screen.rs` stops using the portal and PipeWire.
   - **Share audio:**
     - `Source`, `AppInfo` and `Captured` move into `share_audio/mod.rs`.
     - Each OS provides `Capture`, `available`, `apps_with_audio` and
       `support` (with the reason why not).
     - `sender.rs` is shared.
   - **Codec:**
     - Each OS module exposes `encoder::{Backend, Encoder, EncoderConfig,
       Encoded}`, `decoder::Decoder`, `scale::Scaler` and
       `raw::RawDecoder`, with the same signatures as the Linux files.
     - Plus `SUPPORTED: bool`.
   - **Core:**
     - The core's video, camera, screen and texture code is no longer gated.
     - irondash becomes a dependency on all desktop OSes.
4. **Rust↔Dart API, all changes made here,** so neither platform regenerates
   the bindings:
   - `video_support() -> VideoSupport { receive, camera, screen, own_picker }`
     replaces `video_supported()`. `RepoCapabilities` follows it.
     - `receive` comes from the OS codec's `SUPPORTED`.
     - `camera`, `screen` and `own_picker` come from the OS modules'
       `support()`.
   - `screen_sources() -> Vec<ScreenSource { id, kind, title, app,
     thumbnail: { width, height, rgba } }>`. Empty unless the OS uses its own
     picker.
   - `screen_share_start(request, source_id: Option<String>)`, and the
     Repository's `startScreenShare(…, sourceId)`.
   - `HotkeySupport::FocusedOnly { reason, settings_url: Option<String> }`.
     The Keybinds page shows an "Open settings" button when a URL is given.
   - Mock repository: fake sources and support values, so the Windows
     session can write widget tests.
5. **Dart:** the screen share dialog's `_portal` check becomes `!ownPicker`.
   No shared Dart file then needs a platform check for screen sharing.
6. **Tests that need a platform feature skip at run time** when it isn't
   supported, instead of using `cfg`:
   - the voicebot's camera, screen and screen-sound tests;
   - core video tests.

   They light up on each OS as its backends arrive, with no file edits.
7. **Empty, separate sections ready for each OS's dependencies:**
   - `[target.'cfg(target_os = "macos")'.dependencies]` and
     `[target.'cfg(windows)'.dependencies]` in
     `crates/opencord-media/Cargo.toml`.
   - The same in `crates/opencord-core/Cargo.toml`.
8. **Folders:**
   - `app/tool/macos/` and `app/tool/windows/`, each with a short README.
   - `app/integration_test/macos/` and `app/integration_test/windows/`.
9. **Linux regression:**
   - All tests pass.
   - `check_v5`, `check_v6`, `check_v7`, `check_v8` and `check_user_audio`
     pass live.
   - Commit title: `refactor: platform seams (L1)`.

## 3. Who owns what

All Rust paths are under `crates/opencord-media/src/` unless they say
otherwise. "Owner" means the only session that changes the file.

| Path | Owner |
|---|---|
| `hotkeys/macos.rs`, `video/camera/avfoundation.rs`, `video/screen/macos.rs` (and files it adds beside it), `share_audio/coreaudio.rs`, `video/codec/macos/**` | Mac |
| `hotkeys/windows.rs`, `video/camera/media_foundation.rs`, `video/screen/windows.rs` (and files it adds beside it), `share_audio/wasapi.rs`, `video/codec/windows_pending.rs` | Windows |
| The FFmpeg codec files: `video/codec/{encoder,decoder,raw,scale,ffmpeg,openh264}.rs` and the FFmpeg block of `video/codec/mod.rs` | Windows from W3, keeping Linux's behaviour; the Linux session leaves them alone until W3 is merged |
| `vaapi.rs`, the PipeWire/V4L2/X11/portal files, `share_audio/pipewire.rs` | Linux |
| `app/macos/**`, `app/rust_builder/macos/**`, `app/tool/macos/**`, `app/integration_test/macos/**`, `crates/*/tests/*_macos.rs` | Mac |
| `app/windows/**`, `app/rust_builder/windows/**`, `app/tool/windows/**`, `app/integration_test/windows/**`, `crates/*/tests/*_windows.rs` | Windows |
| `app/lib/features/voice/screenshare_picker.dart`, a new `screen_source_picker.dart` beside it, and the source-passing part of the share flow in `voice_controls.dart` | Windows |
| The OS's own section in each `Cargo.toml` (§2.7) | that OS |
| `.github/workflows/ci.yml`: steps with `if: matrix.target == 'macos'` / `'windows'` | that OS |
| `docs/plans/macos.md` §7 / `docs/plans/windows.md` §7 (the logs) | Mac / Windows |
| Everything else, including the docs, the shared Dart, the core, transport, audio and the generated bindings | Linux session |

## 4. Rules for everything shared

- **A platform session needs a change in a file it doesn't own:**
  - Keep the change minimal, in a commit of its own titled `shared: …`.
  - Say why in the log.
  - Never reformat, reorder or rename things in a shared file.
  - The Linux session reviews it when merging.
  - Before writing it, ask: should this have been a seam? If yes, record it in
    the log; the Linux session may move it into a seam.
- **Generated bindings** (`crates/opencord-core/src/frb_generated.rs`,
  `app/lib/src/rust/**`):
  - Platform sessions don't change the Rust↔Dart API; L1 makes every change
    either platform's plan needs.
  - If one is still unavoidable, make it as a `shared:` commit and say so.
  - On a merge conflict, never edit by hand: take one side, then run
    `flutter_rust_bridge_codegen generate` from `app/`.
- **`Cargo.lock`:** on a conflict, take either side and run `cargo check
  --workspace`; cargo fixes it. Never edit it by hand.
- **Dependencies:**
  - A crate only one OS needs goes in that OS's target section of the crate's
    own `Cargo.toml` (§2.7), with a version. Never in
    `[workspace.dependencies]`.
  - Use the versions already in `Cargo.lock` so only one copy builds:
    - `windows` 0.62 (cpal's);
    - the objc2 0.3 crates and `objc2` 0.6 (cpal's).
- **Docs:**
  - Platform sessions don't edit `docs/decisions.md`, `docs/performance.md`,
    `docs/plans/phase-2.md` or the ledger.
  - Rulings and measurements go in the platform plan's §7 log.
  - The Linux session moves them into `decisions.md` (numbering them D33
    onward, in merge order; D32 is the Linux bundle's FFmpeg) and `performance.md` when merging.
- **CI:**
  - A platform session changes only its own steps in the `desktop` job.
  - It proposes new jobs (a `cargo test` job for its OS, say) in its log; the
    Linux session adds them, so the two never append at the same place.
- **Check scripts and integration tests:**
  - New ones go in the OS's own folder (§2.8). Scripts may `source
    ../e2e_server.sh` but don't change it.
  - Existing shared tests aren't edited by platform sessions. If one fails on
    an OS for a platform reason, note it in the log, and the Linux session
    adjusts the shared test.
- **Warnings:**
  - Before L1, the off-Linux dead-code warnings in shared files are expected.
    Don't fix them; L1 removes them.
  - Run clippy with `-D warnings` on your own files. Report any other
    warning in a shared file.
- **Bugs in shared code** (transport, audio engine, core logic), found while
  testing on one OS:
  - Fix it if it's small, as a `fix(shared): …` commit with a test.
  - Otherwise describe it in the log and keep going.
  - Either way, the Linux session checks the fix on Linux.

## 5. Order and hand-offs

```
Linux:    L1 seams ──────────────► merges each platform milestone as pushed ──► L2: final merge, docs
Mac:      M0 ─ M1 ─ (L1) ─ M2 ─ M3 ─ M4 ─ M5 ─ M6 ─ M7 ─ M8
Windows:  W0 ─ W1 ─ (L1) ─ W2 ─ W3 ─ W4 ─ W5 ─ W6 ─ W7 ─ W8
```

- The Mac and Windows sessions never wait for each other, only for L1.
- **The tests that need two machines** run at M8/W8, with the user:
  - a call between the Mac, the Windows PC and the Linux machine through one
    server on the LAN (V2's "two machines talk" acceptance);
  - each machine watching another's screen share.
- **L2:** when both M8 and W8 are merged, the Linux session updates
  `phase-2.md`'s status and the docs, and asks the user about merging
  `phase2-remaining` onward.
