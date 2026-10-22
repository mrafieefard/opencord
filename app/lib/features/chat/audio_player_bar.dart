import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/chat_controller.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// How an entry is named: a voice message by who sent it, an audio file by
/// its name.
String audioEntryTitle(WidgetRef ref, AudioEntry entry) {
  if (!entry.voice) {
    return middleEllipsis(entry.attachment.filename, maxLength: 48);
  }
  final author = ref
      .watch(serverProvider(entry.serverKey))
      .data
      ?.members[entry.authorId]
      ?.displayName;
  return '${author ?? 'Someone'} · Voice message';
}

String _speedLabel(double speed) => speed == 1.5 ? '1.5×' : '${speed.toInt()}×';

/// Telegram Desktop's audio player: while a voice message or an audio file
/// from the chat plays, this bar under the header controls it from any
/// channel. Previous, play or pause, next; its name, which opens its
/// message; the time; a voice message's speed; × to stop it. Hovering
/// shows the queue: the voice messages and audio files of its channel.
class AudioPlayerBar extends ConsumerStatefulWidget {
  const AudioPlayerBar({super.key});

  static const double height = 40;

  @override
  ConsumerState<AudioPlayerBar> createState() => _AudioPlayerBarState();
}

class _AudioPlayerBarState extends ConsumerState<AudioPlayerBar> {
  final _queue = OverlayPortalController();
  final _anchor = LayerLink();
  Timer? _hover;

  /// The queue opens after the pointer rests on the bar a moment, and
  /// closes a moment after it leaves both.
  void _pointerIn() {
    _hover?.cancel();
    if (_queue.isShowing) return;
    _hover = Timer(const Duration(milliseconds: 350), _queue.show);
  }

  void _pointerOut() {
    _hover?.cancel();
    _hover = Timer(const Duration(milliseconds: 250), () {
      if (_queue.isShowing) _queue.hide();
    });
  }

  @override
  void dispose() {
    _hover?.cancel();
    super.dispose();
  }

  void _openMessage(AudioEntry entry) {
    final chat = ref.read(chatControllerProvider);
    final open =
        ref.read(currentServerProvider) == entry.serverKey &&
        ref.read(currentChannelProvider) == entry.channelId;
    if (open) {
      unawaited(chat.jumpToMessage(entry.messageId));
      return;
    }
    chat.jumpWhenOpened((
      server: entry.serverKey,
      channel: entry.channelId,
    ), entry.messageId);
    ref
        .read(navigationProvider.notifier)
        .openChannel(entry.serverKey, entry.channelId);
  }

  @override
  Widget build(BuildContext context) {
    final playback = ref.watch(playbackProvider);
    final entry = playback.origin;
    final item = playback.item;
    if (entry == null || item == null) return const SizedBox.shrink();
    final colors = context.oc;
    final notifier = ref.read(playbackProvider.notifier);
    final previous = notifier.previousEntry;
    final next = notifier.nextEntry;
    final durationMs = playback.durationMs > 0
        ? playback.durationMs
        : entry.attachment.durationMs ?? 0;
    final positionMs = playback.positionMs.clamp(0, durationMs);

    void togglePlay() {
      if (playback.playing) {
        notifier.pause(item);
      } else if (playback.error != null) {
        unawaited(notifier.playEntry(entry));
      } else {
        notifier.resume(item);
      }
    }

    return CompositedTransformTarget(
      link: _anchor,
      child: OverlayPortal(
        controller: _queue,
        overlayChildBuilder: (context) => Positioned(
          left: 0,
          top: 0,
          child: CompositedTransformFollower(
            link: _anchor,
            showWhenUnlinked: false,
            targetAnchor: Alignment.bottomLeft,
            offset: const Offset(OcSpace.s12, 4),
            child: MouseRegion(
              onEnter: (_) => _hover?.cancel(),
              onExit: (_) => _pointerOut(),
              child: _QueueList(current: entry),
            ),
          ),
        ),
        child: MouseRegion(
          onEnter: (_) => _pointerIn(),
          onExit: (_) => _pointerOut(),
          child: Container(
            height: AudioPlayerBar.height,
            decoration: BoxDecoration(
              color: colors.sidebar,
              border: Border(bottom: BorderSide(color: colors.border)),
            ),
            child: Stack(
              children: [
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
                  child: Row(
                    children: [
                      OcIconButton(
                        icon: OcIcons.skipPrevious,
                        size: OcIconButtonSize.compact,
                        tooltip: 'Previous',
                        onPressed: previous == null
                            ? null
                            : () => unawaited(notifier.playEntry(previous)),
                      ),
                      OcIconButton(
                        icon: playback.playing
                            ? OcIcons.pause
                            : OcIcons.playArrow,
                        size: OcIconButtonSize.compact,
                        tooltip: playback.playing ? 'Pause' : 'Play',
                        onPressed: togglePlay,
                      ),
                      OcIconButton(
                        icon: OcIcons.skipNext,
                        size: OcIconButtonSize.compact,
                        tooltip: 'Next',
                        onPressed: next == null
                            ? null
                            : () => unawaited(notifier.playEntry(next)),
                      ),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Align(
                          alignment: Alignment.centerLeft,
                          child: Hoverable(
                            onTap: () => _openMessage(entry),
                            semanticLabel: 'Show the message',
                            focusRadius: BorderRadius.circular(4),
                            builder: (context, state) => Text(
                              playback.error ?? audioEntryTitle(ref, entry),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.bodyStrong.copyWith(
                                color: colors.text,
                                decoration: state.hovered
                                    ? TextDecoration.underline
                                    : null,
                              ),
                            ),
                          ),
                        ),
                      ),
                      const SizedBox(width: OcSpace.s12),
                      Text(
                        '${formatDurationMs(positionMs)} / '
                        '${formatDurationMs(durationMs)}',
                        style: OcText.small.copyWith(
                          color: colors.textSecondary,
                        ),
                      ),
                      if (entry.voice) ...[
                        const SizedBox(width: OcSpace.s8),
                        Hoverable(
                          onTap: () =>
                              notifier.setSpeed(item, switch (playback.speed) {
                                1.0 => 1.5,
                                1.5 => 2.0,
                                _ => 1.0,
                              }),
                          semanticLabel:
                              'Playback speed ${_speedLabel(playback.speed)}',
                          focusRadius: BorderRadius.circular(4),
                          builder: (context, state) => Padding(
                            padding: const EdgeInsets.symmetric(
                              horizontal: 4,
                              vertical: 1,
                            ),
                            child: Text(
                              _speedLabel(playback.speed),
                              style: OcText.meta.copyWith(
                                color: playback.speed != 1.0 || state.hovered
                                    ? colors.text
                                    : colors.textMuted,
                                fontWeight: FontWeight.w600,
                              ),
                            ),
                          ),
                        ),
                      ],
                      const SizedBox(width: OcSpace.s4),
                      OcIconButton(
                        icon: OcIcons.close,
                        size: OcIconButtonSize.compact,
                        tooltip: 'Stop',
                        onPressed: notifier.closePlayer,
                      ),
                    ],
                  ),
                ),
                // Where it is; a click seeks there.
                Positioned(
                  left: 0,
                  right: 0,
                  bottom: 0,
                  height: 6,
                  child: LayoutBuilder(
                    builder: (context, constraints) => GestureDetector(
                      behavior: HitTestBehavior.opaque,
                      onTapDown: durationMs <= 0
                          ? null
                          : (details) => notifier.seek(
                              item,
                              (details.localPosition.dx /
                                      constraints.maxWidth *
                                      durationMs)
                                  .round()
                                  .clamp(0, durationMs),
                            ),
                      child: Align(
                        alignment: Alignment.bottomLeft,
                        child: FractionallySizedBox(
                          widthFactor: durationMs <= 0
                              ? 0
                              : positionMs / durationMs,
                          child: Container(height: 2, color: colors.text),
                        ),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The queue under the bar: the voice messages and audio files of the
/// playing one's channel, oldest first; a click plays one.
class _QueueList extends ConsumerWidget {
  const _QueueList({required this.current});

  final AudioEntry current;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final queue = ref.watch(audioQueueProvider(current));
    final notifier = ref.read(playbackProvider.notifier);
    return Material(
      color: colors.elevated,
      borderRadius: BorderRadius.circular(OcRadius.menu),
      child: Container(
        width: 360,
        constraints: const BoxConstraints(maxHeight: 320),
        padding: const EdgeInsets.all(OcSpace.s4),
        decoration: BoxDecoration(
          border: Border.all(color: colors.border),
          borderRadius: BorderRadius.circular(OcRadius.menu),
          boxShadow: OcShadows.menu(colors),
        ),
        child: ListView(
          shrinkWrap: true,
          children: [
            for (final entry in queue)
              Hoverable(
                onTap: () => unawaited(notifier.playEntry(entry)),
                semanticLabel: audioEntryTitle(ref, entry),
                selected: entry == current,
                builder: (context, state) => Container(
                  height: 36,
                  padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
                  decoration: BoxDecoration(
                    color: entry == current
                        ? colors.selected
                        : state.active
                        ? colors.hover
                        : null,
                    borderRadius: BorderRadius.circular(OcRadius.menu - 2),
                  ),
                  child: Row(
                    children: [
                      Icon(
                        entry.voice ? OcIcons.mic : OcIcons.musicNote,
                        size: OcSize.iconRow,
                        color: colors.textSecondary,
                      ),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Text(
                          audioEntryTitle(ref, entry),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: OcText.body.copyWith(color: colors.text),
                        ),
                      ),
                      if (entry.attachment.durationMs case final ms?)
                        Text(
                          formatDurationMs(ms),
                          style: OcText.small.copyWith(color: colors.textMuted),
                        ),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
