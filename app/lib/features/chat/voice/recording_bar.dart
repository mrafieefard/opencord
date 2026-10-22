import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/voice_message.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/voice/voice_recorder_state.dart';
import 'package:opencord/features/chat/voice/waveform_painter.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

class RecordingBar extends ConsumerStatefulWidget {
  const RecordingBar({required this.channel, super.key});

  final ChannelRef channel;

  @override
  ConsumerState<RecordingBar> createState() => _RecordingBarState();
}

class _RecordingBarState extends ConsumerState<RecordingBar>
    with SingleTickerProviderStateMixin {
  late final AnimationController _pulse;
  late final FocusNode _focusNode;

  @override
  void initState() {
    super.initState();
    _pulse = AnimationController(
      vsync: this,
      duration: const Duration(milliseconds: 600),
      lowerBound: 0.4,
      upperBound: 1.0,
    )..repeat(reverse: true);

    _focusNode = FocusNode();
    _focusNode.requestFocus();
  }

  @override
  void dispose() {
    _pulse.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  KeyEventResult _handleKeyEvent(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) return KeyEventResult.ignored;

    final state = ref.read(voiceRecorderProvider(widget.channel));
    final notifier = ref.read(voiceRecorderProvider(widget.channel).notifier);

    if (event.logicalKey == LogicalKeyboardKey.escape) {
      if (state.phase == RecordingPhase.holding ||
          state.phase == RecordingPhase.cancelling) {
        notifier.cancel();
        return KeyEventResult.handled;
      } else if (state.phase == RecordingPhase.locked ||
          state.phase == RecordingPhase.preview) {
        notifier.discard();
        return KeyEventResult.handled;
      }
    } else if (event.logicalKey == LogicalKeyboardKey.enter) {
      if (state.phase == RecordingPhase.locked ||
          state.phase == RecordingPhase.preview) {
        notifier.send();
        return KeyEventResult.handled;
      }
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final state = ref.watch(voiceRecorderProvider(widget.channel));
    final notifier = ref.read(voiceRecorderProvider(widget.channel).notifier);

    final isCancelling = state.phase == RecordingPhase.cancelling;
    final isLocked = state.phase == RecordingPhase.locked;
    final isPreview = state.phase == RecordingPhase.preview;
    final draft = state.draft;
    final previewItem = draft == null ? null : DraftPlayback(draft.id);
    final playback = ref.watch(playbackProvider);
    final previewing = previewItem != null && playback.item == previewItem;
    final previewPlaying = previewing && playback.playing;

    final bgColor = isCancelling
        ? colors.recording.withValues(alpha: 0.15)
        : Colors.transparent;

    return Focus(
      focusNode: _focusNode,
      onKeyEvent: _handleKeyEvent,
      child: Container(
        height: 40,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        decoration: BoxDecoration(
          color: bgColor,
          borderRadius: BorderRadius.circular(8),
        ),
        child: Row(
          children: [
            if (!isPreview) ...[
              // Pulsing or static 10 px dot
              AnimatedBuilder(
                animation: _pulse,
                builder: (context, _) => Opacity(
                  opacity: isCancelling ? 1.0 : _pulse.value,
                  child: Container(
                    width: 10,
                    height: 10,
                    decoration: BoxDecoration(
                      color: colors.recording,
                      shape: BoxShape.circle,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: 8),
              // Timer m:ss
              Text(
                formatDuration(state.elapsed),
                style: OcText.body.copyWith(
                  color: isCancelling ? colors.recording : colors.text,
                ),
              ),
              const SizedBox(width: 12),
              // Live waveform
              Expanded(
                child: SizedBox(
                  height: 24,
                  child: CustomPaint(
                    painter: WaveformPainter.live(
                      bars: state.bars,
                      color: isCancelling ? colors.recording : colors.voice,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: 12),
            ],
            if (isPreview && draft != null && previewItem != null) ...[
              OcIconButton(
                icon: previewPlaying ? OcIcons.pause : OcIcons.playArrow,
                tooltip: previewPlaying ? 'Pause' : 'Play',
                color: colors.text,
                onPressed: () {
                  final player = ref.read(playbackProvider.notifier);
                  if (!previewing || playback.error != null) {
                    unawaited(player.play(previewItem));
                  } else if (previewPlaying) {
                    player.pause(previewItem);
                  } else {
                    player.resume(previewItem);
                  }
                },
              ),
              const SizedBox(width: 8),
              Expanded(
                child: SizedBox(
                  height: 24,
                  child: CustomPaint(
                    painter: WaveformPainter.static(
                      values: draft.waveform,
                      progress: previewing && draft.durationMs > 0
                          ? playback.positionMs / draft.durationMs
                          : 0,
                      playedColor: colors.voice,
                      unplayedColor: colors.textMuted,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: 8),
              Text(
                previewing && playback.error != null
                    ? playback.error!
                    : formatDuration(Duration(milliseconds: draft.durationMs)),
                style: OcText.body.copyWith(color: colors.textMuted),
              ),
              const SizedBox(width: 12),
            ],
            // Action hints or buttons
            if (state.phase == RecordingPhase.holding)
              Text(
                'Release to send · Esc to cancel',
                style: OcText.small.copyWith(color: colors.textMuted),
              )
            else if (isCancelling)
              Text(
                'Release to cancel',
                style: OcText.small.copyWith(color: colors.recording),
              )
            else if (isLocked) ...[
              OcIconButton(
                icon: OcIcons.delete,
                tooltip: 'Delete',
                color: colors.textMuted,
                onPressed: notifier.discard,
              ),
              const SizedBox(width: 4),
              OcIconButton(
                icon: OcIcons.stop,
                tooltip: 'Stop',
                color: colors.textMuted,
                onPressed: notifier.stopForPreview,
              ),
              const SizedBox(width: 4),
              OcIconButton(
                icon: OcIcons.send,
                tooltip: 'Send',
                active: true,
                activeStyle: OcActiveStyle.inverted,
                onPressed: () => notifier.send(),
              ),
            ] else if (isPreview) ...[
              OcIconButton(
                icon: OcIcons.delete,
                tooltip: 'Delete',
                color: colors.textMuted,
                onPressed: notifier.discard,
              ),
              const SizedBox(width: 4),
              OcIconButton(
                icon: OcIcons.send,
                tooltip: 'Send',
                active: true,
                activeStyle: OcActiveStyle.inverted,
                onPressed: () => notifier.send(),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
