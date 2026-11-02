# The media viewer's player, rotate and picture-in-picture: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Telegram's player in the media viewer: rotate, its bar, speed with the pitch kept, full screen and picture-in-picture (D87).

**Architecture:**
- The viewer (`media_viewer.dart`) and the video view (`video_player_view.dart`) change in Dart.
- Speed goes through the FFmpeg player in `opencord-media` and the bridge in `opencord-core`.
- Picture-in-picture is a third mode of D86's window on each runner.

**Tech Stack:** Flutter 3.44, Riverpod 3, Rust (`ffmpeg-next`, `wsola` 0.2), GTK 3, Win32, AppKit.

**Spec:** `docs/superpowers/specs/2026-11-02-media-viewer-player-design.md`

## Global Constraints

- **Branch and pull request:** `feat/media-viewer-player`, from `feat/media-viewer-window` (#25). The pull request goes into `beta`, to be merged after #25.
- **Colours:** monochrome only (desktop UI plan §1), held in `OcMediaColors`.
- **Commands:**
  - `cargo`, `flutter` and `dart` commands start with `source .superpowers/sdd/media/env.sh &&`;
  - `CARGO_INCREMENTAL=0`;
  - `.github/check.sh` passes before each push.
- **Platforms:**
  - FFmpeg playback runs on Linux and Windows only; `unsupported.rs` gains the same API as no-ops.
  - Desktop only; nothing under `mobile/`.

## Review Focus

- **Rotated picture and clicks:** with a rotated portrait photo, a click on the rotated picture's area must not close the viewer, and the zoom's edges must follow the rotated shape.
- **Speed and position:** a speed change mid-play must not jump the position shown, and seeking at 2x lands where asked.
- **Picture-in-picture playback:** switching to it and back must not restart or stop playback, because the `VideoPlayerView` is reparented, not rebuilt.
- **Esc in full screen:** it ends full screen and leaves the viewer open; a second Esc closes it.
- **The normal window's memory:** full screen from a normal window must leave the remembered mode as normal.

---

### Task 1: Rotate

**Files:** `app/lib/features/chat/attachments/media_viewer.dart`, `app/test/widget/chat/media_viewer_test.dart`

- [ ] **Tests:**
  - `rotate_turns_the_picture_a_quarter_left`: the video's `Texture` rect is taller than wide after one click on the `Rotate` tooltip.
  - `rotate_resets_with_the_next_item`.
  - `a_click_on_the_turned_picture_does_not_close`.
- [ ] **Build it:**
  - `_quarterTurns` in the viewer state; a `RotatedBox` around the picture inside `_zoom`.
  - The button, with the `Rotate` tooltip, to the right of Download.
  - It resets the zoom; `_previous`/`_next` reset the turns.
- [ ] Commit `feat(desktop): the media viewer rotates`.

### Task 2: The bar

**Files:** `video_player_view.dart`, `media_viewer.dart`, `oc_media_colors.dart`, tests in `media_viewer_test.dart` and `media_playback_test.dart`.

- [ ] **Tests:**
  - `the_video_bar_is_telegrams_size`: 480 × 70, centred, 7 above the bottom.
  - `the_video_bar_fades_with_the_controls`.
  - `the_video_keeps_clear_of_the_bar`: the texture's bottom is at or above the bar's top.
  - `the_time_left_is_shown_negative`.
- [ ] **Build it:**
  - `VideoPlayerView` gets `frameControls` (`Widget Function(Widget bar)`) and `showPip` (bool), replacing `controlsInset`.
  - The viewer wraps the bar in its fade and `_control`, and keeps the video 84 clear above and below.
  - The bottom-left text is clipped to the left of the bar.
- [ ] Commit `feat(desktop): the video's controls as Telegram's`.

### Task 3: Full screen

- [ ] **Tests:**
  - `full_screen_fills_the_window_with_the_video`.
  - `esc_ends_full_screen_before_closing`.
  - `full_screen_from_a_normal_window_comes_back_normal`: `setMode:fullscreen` then `setMode:normal` in the window's calls.
- [ ] **Build it:**
  - The viewer's `_videoFullScreen`, set by `VideoPlayerView`'s `onFullScreen`.
  - While it's set: no header, nav, Download or Rotate; zero padding; the bar over the video.
- [ ] Commit `feat(desktop): the video's full screen`.

### Task 4: Speed

**Files:**
- `crates/opencord-media/Cargo.toml` (`wsola.workspace = true`);
- `crates/opencord-media/src/player/{ffmpeg.rs,sync.rs,unsupported.rs}`;
- `crates/opencord-core/src/{media_player.rs,client.rs,api/attachments.rs}`;
- the bridge, regenerated;
- `app/lib/core/{repository/repository.dart,rust/core_api.dart,rust/rust_repository.dart,mock/mock_repository.dart,providers/playback_state.dart}`;
- `video_player_view.dart`.

- [ ] **Rust tests:**
  - `sync.rs`:
    - `at_twice_the_speed_heard_sound_counts_double`;
    - `the_wall_clock_runs_at_the_speed`;
    - `a_speed_change_keeps_the_position`.
  - `ffmpeg.rs`: `at_2x_a_tone_keeps_its_pitch_and_halves_its_time`, a tone through `Tempo` measured by zero crossings.
- [ ] **Rust:**
  - `MediaClock::set_speed(f32, queued_frames)`.
  - The player's `Command::SetSpeed`.
  - The audio track's pending samples go through a `Tempo` (1x straight, else `TimeStretch` at 48 kHz, two channels), reset on seek.
  - `MediaPlayer::set_speed`, then `CoreMediaPlayer::set_speed`, `Client::media_set_speed` and `api::attachments::media_set_speed(speed: f32)`.
- [ ] **Bridge:** regenerate it with `flutter_rust_bridge_codegen generate`.
- [ ] **Dart:**
  - `mediaSetSpeed(double)` on the repository, the core API, the Rust repository and the mock (records it).
  - `PlaybackNotifier.setSpeed` sends it for media.
- [ ] **Widget test:** `speed_cycles_one_to_three_and_back`, with the mock's recorded speeds `[1.5, 2, 3, 1]`.
- [ ] Commit `feat(media): the video player's speed, pitch kept`.

### Task 5: Picture-in-picture

**Files:**
- `media_window_service.dart` (`MediaWindowMode.pip`, `setMode(mode, {double? aspect})`);
- `media_viewer_session.dart` (`enterPip`, `leavePip`, the pip placement);
- `media_viewer.dart` (the pip layout);
- `app_views.dart`;
- the three runners;
- `integration_test/linux/media_window_test.dart`.

- [ ] **Tests:**
  - Unit, `the_pip_placement_is_remembered`.
  - Widget:
    - `pip_keeps_the_same_player`: the `VideoPlayerView` state is identical before and after;
    - `pip_shows_its_own_controls`;
    - `back_returns_to_the_mode_it_had`;
    - `opening_other_media_leaves_pip`.
- [ ] **Dart:**
  - `MediaWindowMode.pip`; the placement gains `pip` (a `WindowGeometry?`).
  - The notifier's `enterPip(aspect)` and `leavePip()`.
  - The viewer, in pip mode, shows only the video and its pip controls, keeping the same `VideoPlayerView` under a `GlobalKey`.
- [ ] **Runners:**
  - `setMode` with `"pip"` and `aspect`: 320 wide, at the bottom right, 24 in, or where it was remembered.
  - Kept above: GTK keep-above, `HWND_TOPMOST`, the floating level. Its frame is reported in `geometry`.
- [ ] **Linux integration test:** the window goes to 320 wide, then back.
- [ ] Commit `feat(desktop): picture-in-picture`.

### Task 6: Docs

- [ ] **D87** in `docs/decisions.md`.
- [ ] **The check docs:** `docs/plans/media-check-windows.md` and `media-check-macos.md` gain rotate, the bar, speed, full screen and picture-in-picture checks.
- [ ] Commit `docs: D87, the viewer's player`.
