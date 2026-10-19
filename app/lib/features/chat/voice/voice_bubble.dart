import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/features/chat/voice/voice_player_state.dart';
import 'package:opencord/features/chat/voice/waveform_painter.dart';
import 'package:opencord/src/rust/api/types.dart' show PlaybackItem;
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

String _formatMs(int ms) {
  final totalSec = (ms / 1000).floor();
  final minutes = totalSec ~/ 60;
  final seconds = (totalSec % 60).toString().padLeft(2, '0');
  return '$minutes:$seconds';
}

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

  PlaybackItem get _item => PlaybackItem.attachment(
    serverKey: widget.serverKey,
    attachmentId: widget.attachment.id,
  );

  void _cycleSpeed(bool isPlayingThis) {
    setState(() {
      if (_speed == 1.0) {
        _speed = 1.5;
      } else if (_speed == 1.5) {
        _speed = 2.0;
      } else {
        _speed = 1.0;
      }
    });
    if (isPlayingThis) {
      ref.read(playbackProvider.notifier).setSpeed(_item, _speed);
    }
  }

  void _togglePlay(bool isPlayingThis, bool isPlaying) {
    final notifier = ref.read(playbackProvider.notifier);
    if (isPlayingThis) {
      if (isPlaying) {
        notifier.pause(_item);
      } else {
        notifier.resume(_item);
      }
    } else {
      notifier.play(_item, speed: _speed);
    }
  }

  void _seek(double localX, double width, int durationMs) {
    if (width <= 0) return;
    final ratio = (localX / width).clamp(0.0, 1.0);
    final targetMs = (ratio * durationMs).toInt();
    final notifier = ref.read(playbackProvider.notifier);
    final playback = ref.read(playbackProvider);
    if (playback.isItem(_item)) {
      notifier.seek(_item, targetMs);
    } else {
      notifier.play(_item, positionMs: targetMs, speed: _speed);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final playback = ref.watch(playbackProvider);
    final isThisItem = playback.isItem(_item);
    final isPlaying = isThisItem && playback.playing;

    final durationMs = widget.attachment.durationMs ?? 0;
    final durationSec = durationMs / 1000.0;
    final factor = ((durationSec - 10.0) / 50.0).clamp(0.0, 1.0);
    final width = ui.lerpDouble(200.0, 320.0, factor)!;

    final progress = isThisItem && durationMs > 0
        ? (playback.positionMs / durationMs).clamp(0.0, 1.0)
        : 0.0;

    final timeLabel =
        isThisItem && (playback.playing || playback.positionMs > 0)
        ? '${_formatMs(playback.positionMs)} / ${_formatMs(durationMs)}'
        : _formatMs(durationMs);

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
              onPressed: () => _togglePlay(isThisItem, isPlaying),
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
                        return GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onTapDown: (details) => _seek(
                            details.localPosition.dx,
                            constraints.maxWidth,
                            durationMs,
                          ),
                          onHorizontalDragUpdate: (details) => _seek(
                            details.localPosition.dx,
                            constraints.maxWidth,
                            durationMs,
                          ),
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
                      Text(
                        timeLabel,
                        style: OcText.meta.copyWith(color: colors.textMuted),
                      ),
                      InkWell(
                        onTap: () => _cycleSpeed(isThisItem),
                        borderRadius: BorderRadius.circular(4),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 4,
                            vertical: 1,
                          ),
                          child: Text(
                            _speedLabel(_speed),
                            style: OcText.meta.copyWith(
                              color: _speed != 1.0
                                  ? colors.voice
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
