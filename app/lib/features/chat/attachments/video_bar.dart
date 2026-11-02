import 'dart:math';

import 'package:flutter/material.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_media_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_colors.dart';

/// `00:15`, or `1:02:03` past an hour, as Telegram's player shows times.
String playerClock(int ms) {
  final total = (ms ~/ 1000).clamp(0, 1 << 31);
  final hours = total ~/ 3600;
  final minutes = (total % 3600) ~/ 60;
  final seconds = total % 60;
  String two(int n) => n.toString().padLeft(2, '0');
  return hours > 0
      ? '$hours:${two(minutes)}:${two(seconds)}'
      : '${two(minutes)}:${two(seconds)}';
}

/// `1x`, `1.5x`.
String speedLabel(double speed) =>
    speed == speed.roundToDouble() ? '${speed.toInt()}x' : '${speed}x';

/// A video's controls, as Telegram's player (D87): a narrow, see-through
/// bar with the volume, play and the window's choices on its first row,
/// and the time played, the seek bar and the time left on its second.
class VideoBar extends StatelessWidget {
  const VideoBar({
    super.key,
    required this.playing,
    required this.positionMs,
    required this.durationMs,
    required this.volume,
    required this.muted,
    required this.onTogglePlay,
    required this.onToggleMute,
    required this.onVolume,
    required this.onSeekStart,
    required this.onSeek,
    required this.onSeekEnd,
    this.fullScreen = false,
    this.onFullScreen,
    this.onPip,
    this.speed = 1,
    this.onSpeed,
  });

  /// Telegram's measures.
  static const double width = 480;
  static const double height = 70;
  static const double bottom = 7;

  /// What a video keeps clear above and below itself for the bar.
  static const double clearance = height + 2 * bottom;

  final bool playing;
  final int positionMs;
  final int durationMs;
  final double volume;
  final bool muted;
  final VoidCallback onTogglePlay;
  final VoidCallback onToggleMute;
  final ValueChanged<double> onVolume;
  final ValueChanged<double> onSeekStart;
  final ValueChanged<double> onSeek;
  final ValueChanged<double> onSeekEnd;

  /// Whether the video fills the window; [onFullScreen] switches it, and
  /// without it there's no button.
  final bool fullScreen;
  final VoidCallback? onFullScreen;

  /// Picture-in-picture; without it there's no button.
  final VoidCallback? onPip;

  /// The speed playing, which [onSpeed] moves on to the next one.
  final double speed;
  final VoidCallback? onSpeed;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final time = OcText.small.copyWith(
      color: colors.text,
      fontFeatures: const [FontFeature.tabularFigures()],
    );
    return LayoutBuilder(
      builder: (context, constraints) => GestureDetector(
        // A click on the bar stays on it.
        behavior: HitTestBehavior.opaque,
        onTap: () {},
        child: Container(
          key: const ValueKey('video-bar'),
          width: min(width, constraints.maxWidth - 16),
          height: height,
          decoration: BoxDecoration(
            color: OcMediaColors.playerBar,
            borderRadius: BorderRadius.circular(6),
          ),
          child: Stack(
            children: [
              Positioned(
                left: 4,
                top: 6,
                height: 32,
                child: Row(
                  children: [
                    _button(
                      context,
                      muted || volume == 0
                          ? OcIcons.volumeOff
                          : OcIcons.volumeUp,
                      muted ? 'Unmute' : 'Mute',
                      onToggleMute,
                    ),
                    SizedBox(
                      width: 75,
                      child: _slider(
                        context,
                        value: muted ? 0 : volume,
                        max: 1,
                        onChanged: onVolume,
                      ),
                    ),
                  ],
                ),
              ),
              Positioned(
                left: 0,
                right: 0,
                top: 6,
                height: 32,
                child: Center(
                  child: _button(
                    context,
                    playing ? OcIcons.pause : OcIcons.playArrow,
                    playing ? 'Pause' : 'Play',
                    onTogglePlay,
                    size: 28,
                  ),
                ),
              ),
              Positioned(
                right: 6,
                top: 6,
                height: 32,
                child: Row(
                  children: [
                    if (onFullScreen case final onFullScreen?)
                      _button(
                        context,
                        fullScreen
                            ? OcIcons.fullscreenExit
                            : OcIcons.fullscreen,
                        fullScreen ? 'Exit full screen' : 'Full screen',
                        onFullScreen,
                      ),
                    if (onPip case final onPip?)
                      _button(
                        context,
                        OcIcons.pictureInPictureAlt,
                        'Picture in picture',
                        onPip,
                      ),
                    if (onSpeed case final onSpeed?)
                      SizedBox(
                        width: 36,
                        height: 32,
                        child: Tooltip(
                          message: 'Speed',
                          child: InkWell(
                            borderRadius: BorderRadius.circular(16),
                            onTap: onSpeed,
                            child: Center(
                              child: Text(
                                speedLabel(speed),
                                style: OcText.small.copyWith(
                                  fontWeight: FontWeight.w600,
                                  color: context.oc.text,
                                ),
                              ),
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
              Positioned(
                left: 14,
                right: 14,
                top: 46,
                height: 16,
                child: Row(
                  children: [
                    Text(playerClock(positionMs), style: time),
                    Expanded(
                      child: _slider(
                        context,
                        value: durationMs > 0
                            ? positionMs.clamp(0, durationMs).toDouble()
                            : 0,
                        max: durationMs > 0 ? durationMs.toDouble() : 1,
                        onChangeStart: onSeekStart,
                        onChanged: onSeek,
                        onChangeEnd: onSeekEnd,
                      ),
                    ),
                    Text(
                      '-${playerClock(max(0, durationMs - positionMs))}',
                      style: time,
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _button(
    BuildContext context,
    IconData icon,
    String tooltip,
    VoidCallback onPressed, {
    double size = 20,
  }) => SizedBox(
    width: 36,
    height: 32,
    child: IconButton(
      padding: EdgeInsets.zero,
      iconSize: size,
      color: context.oc.text,
      tooltip: tooltip,
      icon: Icon(icon),
      onPressed: onPressed,
    ),
  );

  Widget _slider(
    BuildContext context, {
    required double value,
    required double max,
    ValueChanged<double>? onChangeStart,
    required ValueChanged<double> onChanged,
    ValueChanged<double>? onChangeEnd,
  }) {
    final colors = context.oc;
    return SliderTheme(
      data: SliderTheme.of(context).copyWith(
        trackHeight: 2,
        thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 5),
        overlayShape: SliderComponentShape.noOverlay,
        activeTrackColor: colors.text,
        inactiveTrackColor: colors.textSecondary.withValues(alpha: 0.4),
        thumbColor: colors.text,
      ),
      child: Slider(
        padding: const EdgeInsets.symmetric(horizontal: 8),
        value: value.clamp(0, max),
        max: max,
        onChangeStart: onChangeStart,
        onChanged: onChanged,
        onChangeEnd: onChangeEnd,
      ),
    );
  }
}
