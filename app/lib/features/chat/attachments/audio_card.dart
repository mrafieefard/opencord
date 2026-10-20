import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/playback_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

class AudioCard extends ConsumerStatefulWidget {
  const AudioCard({
    super.key,
    required this.serverKey,
    required this.attachment,
    this.message,
  });

  final String serverKey;
  final Attachment attachment;
  final Message? message;

  @override
  ConsumerState<AudioCard> createState() => _AudioCardState();
}

class _AudioCardState extends ConsumerState<AudioCard> {
  bool _draggingSeek = false;
  double _dragPositionMs = 0;
  PlaybackNotifier? _notifier;
  PlaybackState? _lastPlayback;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _notifier = ref.read(playbackProvider.notifier);
  }

  AttachmentPlayback get _item =>
      AttachmentPlayback(widget.serverKey, widget.attachment.id);

  /// Its message, for the audio player bar.
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

  Future<void> _togglePlay(PlaybackState playback) async {
    final notifier = ref.read(playbackProvider.notifier);
    if (playback.item == _item && playback.playing) {
      notifier.pause(_item);
    } else if (notifier.openMedia == _item && playback.error == null) {
      notifier.mediaPlay(_item);
    } else {
      try {
        await notifier.mediaOpen(_item, origin: _origin);
        notifier.mediaPlay(_item);
      } on RepoException {
        // mediaOpen put the message in the playback state, shown below.
      }
    }
  }

  @override
  void dispose() {
    // Scrolled away while paused or finished: let the player go. Still
    // playing, it plays on until something else opens.
    final notifier = _notifier;
    if (notifier != null &&
        notifier.openMedia == _item &&
        !(_lastPlayback?.playing ?? false)) {
      notifier.mediaCloseOnDispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final playback = ref.watch(playbackProvider);
    final isCurrent = playback.item == _item;
    _lastPlayback = isCurrent ? playback : null;
    final isPlaying = isCurrent && playback.playing;
    final failure = isCurrent ? playback.error : null;

    final durationMs =
        widget.attachment.durationMs ??
        (isCurrent && playback.durationMs > 0 ? playback.durationMs : 0);
    final currentPosMs = isCurrent
        ? (_draggingSeek
              ? _dragPositionMs.toInt()
              : playback.positionMs.clamp(0, durationMs))
        : 0;

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
        width: 320,
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s8,
        ),
        decoration: BoxDecoration(
          color: colors.surface,
          borderRadius: BorderRadius.circular(OcRadius.bubble),
          border: Border.all(color: colors.border),
        ),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                // Play / Pause button
                OcIconButton(
                  icon: isPlaying ? OcIcons.pause : OcIcons.playArrow,
                  tooltip: isPlaying ? 'Pause' : 'Play',
                  onPressed: () => _togglePlay(playback),
                ),
                const SizedBox(width: OcSpace.s8),

                // Filename and duration
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        middleEllipsis(
                          widget.attachment.filename,
                          maxLength: 22,
                        ),
                        style: OcText.bodyStrong.copyWith(color: colors.text),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                      const SizedBox(height: 2),
                      Text(
                        failure ??
                            (durationMs > 0
                                ? '${formatDurationMs(currentPosMs)} / ${formatDurationMs(durationMs)}'
                                : formatFileSize(widget.attachment.sizeBytes)),
                        style: OcText.small.copyWith(
                          color: failure != null
                              ? colors.text
                              : colors.textSecondary,
                        ),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                      ),
                    ],
                  ),
                ),

                // Download button
                OcIconButton(
                  icon: OcIcons.download,
                  size: OcIconButtonSize.compact,
                  tooltip: 'Download',
                  onPressed: () => saveAttachment(
                    context,
                    ref,
                    widget.serverKey,
                    widget.attachment,
                  ),
                ),
              ],
            ),

            // Seek slider
            if (durationMs > 0)
              SliderTheme(
                data: SliderTheme.of(context).copyWith(
                  trackHeight: 2,
                  thumbShape: const RoundSliderThumbShape(
                    enabledThumbRadius: 4,
                  ),
                  activeTrackColor: colors.text,
                  inactiveTrackColor: colors.border,
                  thumbColor: colors.text,
                  overlayShape: const RoundSliderOverlayShape(overlayRadius: 8),
                ),
                child: Slider(
                  value: currentPosMs.toDouble().clamp(
                    0.0,
                    durationMs.toDouble(),
                  ),
                  max: durationMs.toDouble(),
                  onChangeStart: (v) {
                    setState(() {
                      _draggingSeek = true;
                      _dragPositionMs = v;
                    });
                  },
                  onChanged: (v) {
                    setState(() {
                      _dragPositionMs = v;
                    });
                  },
                  onChangeEnd: (v) {
                    setState(() {
                      _draggingSeek = false;
                    });
                    ref.read(playbackProvider.notifier).seek(_item, v.toInt());
                  },
                ),
              ),
          ],
        ),
      ),
    );
  }
}
