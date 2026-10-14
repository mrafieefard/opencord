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

## 2. The seams (L1): landed on 2026-10-13

Before L1, almost every platform-specific piece was behind
`cfg(target_os = "linux")`, inside files that both platforms would have had to
change. L1 opened them once, on Linux, where the refactor of the Linux code
could be checked live. Every platform now has its own files, and the shared
code calls them. **From M2 and W2 on, each session fills in only its own
files.** Check that you have L1 with
`git log --oneline --grep 'platform seams (L1)'` after merging
`origin/phase2-remaining`.

1. **Everything compiles on every desktop system.**
   - Shared code has no `cfg(target_os = "linux")` gates left, except to pick
     Linux's own files.
   - A feature a system doesn't have yet reports itself unsupported at run
     time, and the app hides or disables it.
   - The off-Linux warnings both machines reported are gone.
   - The five Linux-only examples (`camera_load`, `camera_probe`,
     `codec_costs`, `screen_load`, `virtual_camera`) are folders: `main.rs`
     says they need Linux; `linux.rs` holds the code.
   - `app/tool/check_platforms.sh` checks from Linux that the client crates
     pass clippy with every target and `-D warnings` off Linux. It uses a
     FreeBSD target, which takes the paths of a system without backends
     (`unsupported.rs`). The macOS and Windows stub files mirror those, but
     only their own machines compile them.
2. **Each system's files.** Every stub's doc comment states its contract.

   | Area | Linux | macOS | Windows | Other systems |
   |---|---|---|---|---|
   | Global hotkeys | `hotkeys/linux.rs` (+ `portal.rs`, `x11.rs`) | `hotkeys/macos.rs` | `hotkeys/windows.rs` | `hotkeys/unsupported.rs` |
   | Camera | `video/camera/linux.rs` (+ `pipewire.rs`, `v4l2.rs`) | `video/camera/avfoundation.rs` | `video/camera/media_foundation.rs` | `video/camera/unsupported.rs` |
   | Screen pick + capture | `video/screen/linux.rs` (+ `capture.rs`, `portal.rs`) | `video/screen/macos.rs` | `video/screen/windows.rs` | `video/screen/unsupported.rs` |
   | Screen share sound | `share_audio/pipewire.rs` | `share_audio/coreaudio.rs` | `share_audio/wasapi.rs` | `share_audio/unsupported.rs` |
   | H.264 + scaling | the FFmpeg files in `video/codec/` | `video/codec/macos/` | `video/codec/windows_pending.rs` until W3, then the FFmpeg files | `video/codec/unsupported.rs` |

   All paths are under `crates/opencord-media/src/`. Each area's `mod.rs`
   picks one file per system, one block each, separated by blank lines and
   comments.
3. **The contracts,** in each area's `mod.rs` and the stubs:
   - **Hotkeys:** `bind(app_id, bindings, events) -> (Option<Keys>,
     HotkeySupport)`. The keys stay bound while `Keys` is kept. Presses and
     releases go through the shared `Held`.
     `HotkeySupport::FocusedOnly { reason, settings_url }`.
   - **Camera:** `SUPPORTED`, `cameras()`, and `Capture` with
     `start(id, frames)` and `mode()`. The shared `camera::Capture` wraps
     each system's.
   - **Screen:**
     - `support() -> ScreenSupport { supported, reason, own_picker }`.
     - `sources() -> Vec<ScreenSource { id, kind, title, app, thumbnail }>`:
       empty where the system picks.
     - `start(StartRequest { fps, restore_token, source_id }, frames) ->
       (Capture, Picked { kind, restore_token, sound })`. `sound` is
       `Some(WindowSound { source, app })` when the system knows a window's
       app, and the app then doesn't ask "Sound from".
     - `Capture` has `size()` and `finished()`. The Linux portal's session
       lives inside Linux's `Capture`.
   - **Screen share sound:** `Source`, `AppInfo`, `Captured`, `CaptureError`
     and `ShareAudioSupport` live in `share_audio/mod.rs`. Each system has
     `support()`, `apps_with_audio()`, and `Capture` with `start(source,
     frames)` and `share(source)`. `sender.rs` is shared.
   - **Codec:**
     - Each system exposes `supported()`, `encoder::{Backend, Encoder}`,
       `decoder::Decoder`, `scale::Scaler` and `raw::RawDecoder`, with the
       FFmpeg files' signatures.
     - `EncoderConfig` and `Encoded` are shared, in `codec/mod.rs`.
     - Unit tests that need a codec start with `needs_codec!()`, which ends
       them early where there is none.
   - **Core:** the core's video, camera, screen and texture code isn't gated
     any more. irondash is a dependency on every desktop system.
4. **Rust↔Dart API.** Neither platform plan needs another API change.
   - `video_support() -> VideoSupport { receive, camera, screen,
     screen_reason, own_screen_picker }` replaced `video_supported()`.
     - `receive` is the codec's `supported()`.
     - `camera` and `screen` also need that system's backend.
     - `RepoCapabilities.ownScreenPicker` carries `own_screen_picker`.
   - `screen_sources()`, and the repository's `screenSources()`.
   - `screen_share_start(request, source_id)`, and the repository's
     `startScreenShare(…, sourceId)`.
   - `ScreenShareStarted.audio_app`, which becomes `OwnScreenShare.audioFrom`.
   - `HotkeySupport::FocusedOnly.settings_url`. The Keybinds page then shows
     "Open settings", and the app binds the hotkeys again whenever its window
     is focused (`features/desktop/hotkey_binding.dart`).
   - Mock repository: `debugScreenSources`, `lastScreenSourceId`,
     `debugShareSound` and `hotkeyBinds`, for widget tests.
5. **Dart:** the share dialog's "your desktop asks" line shows where
   `ownScreenPicker` is false. No shared Dart file checks the platform for
   screen sharing.
6. **Tests that need a platform feature skip at run time:**
   - the video sender and receiver unit tests;
   - the voicebot's encoded camera test and its screen-sound test.

   They run on each system as soon as its codec's `supported()` is true.
7. **Each system's dependencies:** each `Cargo.toml` of opencord-media and
   opencord-core has an empty section per system. A crate only one system
   needs goes in that system's section, with a version.
8. **Shared fixes from the M0/W0 logs:**
   - The D-Bus tests (`linux_notifications_test.dart`, `linux_tray_test.dart`)
     are `@TestOn('linux')`.
   - `hardcoded_colors_test.dart` works with Windows' backslashes.
   - The server's config tests use absolute paths that are absolute on
     Windows too.
   - `app/test/golden/failures/` is gitignored.
   - The Mac's `shared:` gate on the voicebot test is now a run-time skip.

## 3. Who owns what

All Rust paths are under `crates/opencord-media/src/` unless they say
otherwise. "Owner" means the only session that changes the file.

| Path | Owner |
|---|---|
| `hotkeys/macos.rs`, `video/camera/avfoundation.rs`, `video/screen/macos.rs` (and files it adds beside it), `share_audio/coreaudio.rs`, `video/codec/macos/**` | Mac |
| `hotkeys/windows.rs`, `video/camera/media_foundation.rs`, `video/screen/windows.rs` (and files it adds beside it), `share_audio/wasapi.rs`, `video/codec/windows_pending.rs` | Windows |
| The FFmpeg codec files: `video/codec/{encoder,decoder,raw,scale,ffmpeg,openh264}.rs` and the FFmpeg block of `video/codec/mod.rs` | Windows from W3, keeping Linux's behaviour; the Linux session leaves them alone until W3 is merged |
| Each area's `linux.rs`, `vaapi.rs`, the PipeWire/V4L2/X11/portal files, `share_audio/pipewire.rs`, and every `unsupported.rs` | Linux |
| `app/macos/**`, `app/rust_builder/macos/**`, `app/tool/macos/**`, `app/integration_test/macos/**`, `crates/*/tests/*_macos.rs` | Mac |
| `app/windows/**`, `app/rust_builder/windows/**`, `app/tool/windows/**`, `app/integration_test/windows/**`, `crates/*/tests/*_windows.rs` | Windows |
| `app/lib/features/voice/screenshare_picker.dart`, a new `screen_source_picker.dart` beside it, and the source-passing part of the share flow in `voice_controls.dart` | Windows |
| The OS's own section in each `Cargo.toml` (§2, item 7) | that OS |
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
    own `Cargo.toml` (§2, item 7), with a version. Never in
    `[workspace.dependencies]`.
  - Use the versions already in `Cargo.lock` so only one copy builds:
    - `windows` 0.62 (cpal's);
    - the objc2 0.3 crates and `objc2` 0.6 (cpal's).
- **Docs:**
  - Platform sessions don't edit `docs/decisions.md`, `docs/performance.md`,
    `docs/plans/phase-2.md` or the ledger.
  - Rulings and measurements go in the platform plan's §7 log.
  - The Linux session moves them into `decisions.md` (numbering them from
    the next free number, in merge order; D33 is the screen share layer fix)
    and `performance.md` when merging.
- **CI:**
  - A platform session changes only its own steps in the `desktop` job.
  - It proposes new jobs (a `cargo test` job for its OS, say) in its log; the
    Linux session adds them, so the two never append at the same place.
- **Check scripts and integration tests:**
  - New ones go in the OS's own folder (`app/tool/<os>/`, `app/integration_test/<os>/`). Scripts may `source
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

M0, M1, W0, W1 and L1 are done and merged (2026-10-13). The user's order from
here: **the Mac first, M2 to M8, then Windows, W2 to W8.**

```
Linux:    merges each pushed milestone ─────────────────────────────► L2: final merge, docs
Mac:      M2 ─ M3 ─ M4 ─ M5 ─ M6 ─ M7 ─ M8
Windows:                                    W2 ─ W3 ─ W4 ─ W5 ─ W6 ─ W7 ─ W8
```

- Windows starts by merging `origin/phase2-remaining`, which by then holds
  everything the Mac did. The rules in §3 and §4 still apply: the ownership
  table keeps the two apart even when they run one after the other.
- **The tests that need two machines** run at M8/W8, with the user:
  - a call between the Mac, the Windows PC and the Linux machine through one
    server on the LAN (V2's "two machines talk" acceptance);
  - each machine watching another's screen share.
- **L2:** when both M8 and W8 are merged, the Linux session updates
  `phase-2.md`'s status and the docs, and asks the user about merging
  `phase2-remaining` onward.
