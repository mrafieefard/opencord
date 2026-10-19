import 'dart:async';

import 'package:desktop_drop/desktop_drop.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/composer_state.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';

/// Files dragged onto the chat show a drop target and are staged in the composer.
class AttachmentDropZone extends ConsumerStatefulWidget {
  const AttachmentDropZone({
    super.key,
    required this.channel,
    required this.child,
  });

  final ChannelRef channel;
  final Widget child;

  @override
  ConsumerState<AttachmentDropZone> createState() => _AttachmentDropZoneState();
}

class _AttachmentDropZoneState extends ConsumerState<AttachmentDropZone> {
  var _dragging = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final data = ref.watch(
      serverProvider(widget.channel.server).select((s) => s.data),
    );
    final channelName = data?.channels[widget.channel.channel]?.name ?? '';
    final limit = data?.myUploadLimit ?? UploadSettings.fallback.maxFileBytes;
    final canAttach =
        data
            ?.permissionsIn(widget.channel.channel)
            .has(Permissions.attachFiles) ??
        false;

    return DropTarget(
      onDragEntered: (_) => setState(() => _dragging = true),
      onDragExited: (_) => setState(() => _dragging = false),
      onDragDone: (details) {
        setState(() => _dragging = false);
        if (!canAttach) return;
        final paths = details.files.map((f) => f.path).toList();
        unawaited(
          ref.read(composerProvider(widget.channel).notifier).stagePaths(paths),
        );
      },
      child: Stack(
        fit: StackFit.passthrough,
        children: [
          widget.child,
          if (_dragging)
            Positioned.fill(
              child: IgnorePointer(
                child: ColoredBox(
                  color: colors.scrim,
                  child: Center(
                    child: Container(
                      width: 320,
                      padding: const EdgeInsets.all(OcSpace.s24),
                      decoration: BoxDecoration(
                        color: colors.elevated,
                        borderRadius: BorderRadius.circular(OcRadius.dialog),
                        border: Border.all(color: colors.text, width: 1.5),
                      ),
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(OcIcons.upload, size: 32, color: colors.text),
                          const SizedBox(height: OcSpace.s12),
                          Text(
                            canAttach
                                ? 'Drop to send to #$channelName'
                                : "You can't attach files here",
                            textAlign: TextAlign.center,
                            style: OcText.title.copyWith(color: colors.text),
                          ),
                          const SizedBox(height: OcSpace.s6),
                          if (canAttach)
                            Text(
                              'Up to ${formatFileSize(limit)} each',
                              textAlign: TextAlign.center,
                              style: OcText.small.copyWith(
                                color: colors.textSecondary,
                              ),
                            ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
