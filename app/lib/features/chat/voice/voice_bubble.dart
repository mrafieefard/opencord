import 'dart:async';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/voice_message.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/core/providers/playback_state.dart';
import 'package:opencord/features/chat/voice/waveform_painter.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

String _speedLabel(double speed) => speed == 1.5 ? '1.5×' : '${speed.toInt()}×';

class VoiceBubble extends ConsumerStatefulWidget {
  const VoiceBubble({
    super.key,
    required this.serverKey,
    required this.attachment,
    this.message,
  });

  final String serverKey;
  final Attachment attachment;
  final Message? message;

  @override
  ConsumerState<VoiceBubble> createState() => _VoiceBubbleState();
}

class _VoiceBubbleState extends ConsumerState<VoiceBubble> {
  double _speed = 1.0;

  /// Where a drag on the bars would seek to; null when not dragging.
  int? _dragMs;

  PlaybackItem get _item =>
      AttachmentPlayback(widget.serverKey, widget.attachment.id);

  /// A message still being sent has no file to play yet.
  bool get _sent => widget.attachment.id > 0;

  /// Its message, for the audio player bar; null in a draft's preview.
  AudioEntry? get _origin => switch (widget.message) {
    final message? => AudioEntry(
      serverKey: widget.serverKey,
      channelId: message.channelId,
      messageId: message.id,
      authorId: message.authorId,
      attachment: widget.attachment,
    ),
    null => null,
  };

  /// The speed shown: the player's while this one plays (the audio player
  /// bar can change it), else the one picked here.
  double _shownSpeed(PlaybackState playback) =>
      playback.item == _item ? playback.speed : _speed;

  void _cycleSpeed(PlaybackState playback) {
    setState(
      () => _speed = switch (_shownSpeed(playback)) {
        1.0 => 1.5,
        1.5 => 2.0,
        _ => 1.0,
      },
    );
    ref.read(playbackProvider.notifier).setSpeed(_item, _speed);
  }

  void _togglePlay(PlaybackState playback) {
    final notifier = ref.read(playbackProvider.notifier);
    if (playback.item != _item || playback.error != null) {
      unawaited(notifier.play(_item, speed: _speed, origin: _origin));
    } else if (playback.playing) {
      notifier.pause(_item);
    } else {
      notifier.resume(_item);
    }
  }

  int _msAt(double localX, double width, int durationMs) =>
      width <= 0 ? 0 : ((localX / width).clamp(0.0, 1.0) * durationMs).toInt();

  /// One seek per gesture: a tap, or the end of a drag.
  void _seekTo(int positionMs) {
    final notifier = ref.read(playbackProvider.notifier);
    if (ref.read(playbackProvider).item == _item) {
      notifier.seek(_item, positionMs);
    } else {
      unawaited(
        notifier.play(
          _item,
          positionMs: positionMs,
          speed: _speed,
          origin: _origin,
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final playback = ref.watch(playbackProvider);
    final isThisItem = playback.item == _item;
    final speed = _shownSpeed(playback);
    final isPlaying = isThisItem && playback.playing;
    final failure = isThisItem ? playback.error : null;
    final durationMs = widget.attachment.durationMs ?? 0;
    final durationSec = durationMs / 1000.0;
    final factor = ((durationSec - 10.0) / 50.0).clamp(0.0, 1.0);
    final width = ui.lerpDouble(200.0, 320.0, factor)!;
    final positionMs = _dragMs ?? (isThisItem ? playback.positionMs : 0);
    final progress = (isThisItem || _dragMs != null) && durationMs > 0
        ? (positionMs / durationMs).clamp(0.0, 1.0)
        : 0.0;
    final timeLabel =
        failure ??
        (isThisItem && (playback.playing || playback.positionMs > 0)
            ? '${formatDurationMs(positionMs)} / ${formatDurationMs(durationMs)}'
            : formatDurationMs(durationMs));

    return GestureDetector(
      onSecondaryTapUp: (details) => showAttachmentMenu(
        context: context,
        ref: ref,
        position: details.globalPosition,
        serverKey: widget.serverKey,
        attachment: widget.attachment,
        message: widget.message,
      ),
      child: Container(
        width: width,
        height: 56,
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s8,
        ),
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: BorderRadius.circular(OcRadius.bubble),
          border: Border.all(color: colors.border),
        ),
        child: Row(
          children: [
            OcIconButton(
              icon: isPlaying ? OcIcons.pause : OcIcons.playArrow,
              tooltip: isPlaying ? 'Pause' : 'Play',
              onPressed: _sent ? () => _togglePlay(playback) : null,
            ),
            const SizedBox(width: OcSpace.s8),
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: LayoutBuilder(
                      builder: (context, constraints) {
                        int at(Offset position) => _msAt(
                          position.dx,
                          constraints.maxWidth,
                          durationMs,
                        );
                        return GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onTapUp: _sent
                              ? (details) => _seekTo(at(details.localPosition))
                              : null,
                          onHorizontalDragUpdate: _sent
                              ? (details) => setState(
                                  () => _dragMs = at(details.localPosition),
                                )
                              : null,
                          onHorizontalDragEnd: _sent
                              ? (_) {
                                  if (_dragMs case final target?) {
                                    setState(() => _dragMs = null);
                                    _seekTo(target);
                                  }
                                }
                              : null,
                          child: SizedBox(
                            width: constraints.maxWidth,
                            height: double.infinity,
                            child: CustomPaint(
                              painter: WaveformPainter.static(
                                values: widget.attachment.waveform,
                                progress: progress,
                                playedColor: colors.voice,
                                unplayedColor: colors.textMuted,
                              ),
                            ),
                          ),
                        );
                      },
                    ),
                  ),
                  const SizedBox(height: OcSpace.s4),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.spaceBetween,
                    children: [
                      Expanded(
                        child: Text(
                          timeLabel,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: OcText.meta.copyWith(
                            color: failure != null
                                ? colors.textSecondary
                                : colors.textMuted,
                          ),
                        ),
                      ),
                      // The speed is text, not a hue: `voice` is for the
                      // waveform only.
                      Hoverable(
                        onTap: () => _cycleSpeed(playback),
                        semanticLabel: 'Playback speed ${_speedLabel(speed)}',
                        focusRadius: BorderRadius.circular(4),
                        builder: (context, hover) => Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 4,
                            vertical: 1,
                          ),
                          child: Text(
                            _speedLabel(speed),
                            style: OcText.meta.copyWith(
                              color: speed != 1.0 || hover.hovered
                                  ? colors.text
                                  : colors.textMuted,
                              fontWeight: FontWeight.w600,
                            ),
                          ),
                        ),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
