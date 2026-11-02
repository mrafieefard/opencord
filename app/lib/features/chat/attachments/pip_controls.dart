import 'dart:math';

import 'package:flutter/material.dart';
import 'package:opencord/features/chat/attachments/video_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_media_colors.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// What picture-in-picture's own buttons do.
@immutable
class PipActions {
  const PipActions({required this.onBack, required this.onClose});

  /// Back to the viewer, playing on.
  final VoidCallback onBack;

  /// Stops the video and closes the window.
  final VoidCallback onClose;
}

/// Picture-in-picture's controls over the video, as Telegram's (D87): the
/// volume at the top left, back and close at the top right, play in the
/// middle, and the times and the seek bar along the bottom.
class PipControls extends StatelessWidget {
  const PipControls({
    super.key,
    required this.actions,
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
  });

  final PipActions actions;
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

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final time = OcText.small.copyWith(
      color: colors.text,
      fontFeatures: const [FontFeature.tabularFigures()],
    );
    return Stack(
      children: [
        // Clicks between the buttons reach the video, to drag the window.
        const Positioned.fill(
          child: IgnorePointer(
            child: ColoredBox(color: OcMediaColors.pipShade),
          ),
        ),
        Positioned(
          left: 4,
          top: 4,
          child: Row(
            children: [
              _button(
                context,
                muted || volume == 0 ? OcIcons.volumeOff : OcIcons.volumeUp,
                muted ? 'Unmute' : 'Mute',
                onToggleMute,
              ),
              SizedBox(
                width: 64,
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
          right: 4,
          top: 4,
          child: Row(
            children: [
              _button(
                context,
                OcIcons.openInFull,
                'Back to the viewer',
                actions.onBack,
              ),
              _button(context, OcIcons.close, 'Close', actions.onClose),
            ],
          ),
        ),
        Center(
          child: _button(
            context,
            playing ? OcIcons.pause : OcIcons.playArrow,
            playing ? 'Pause' : 'Play',
            onTogglePlay,
            size: 36,
          ),
        ),
        Positioned(
          left: 10,
          right: 10,
          bottom: 18,
          child: Row(
            children: [
              Text(playerClock(positionMs), style: time),
              const Spacer(),
              Text(
                '-${playerClock(max(0, durationMs - positionMs))}',
                style: time,
              ),
            ],
          ),
        ),
        Positioned(
          left: 0,
          right: 0,
          bottom: 2,
          height: 14,
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
      ],
    );
  }

  Widget _button(
    BuildContext context,
    IconData icon,
    String tooltip,
    VoidCallback onPressed, {
    double size = 20,
  }) => SizedBox(
    width: size + 12,
    height: size + 12,
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
        thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 4),
        overlayShape: SliderComponentShape.noOverlay,
        activeTrackColor: colors.text,
        inactiveTrackColor: colors.textSecondary.withValues(alpha: 0.4),
        thumbColor: colors.text,
      ),
      child: Slider(
        padding: const EdgeInsets.symmetric(horizontal: 6),
        value: value.clamp(0, max),
        max: max,
        onChangeStart: onChangeStart,
        onChanged: onChanged,
        onChangeEnd: onChangeEnd,
      ),
    );
  }
}
