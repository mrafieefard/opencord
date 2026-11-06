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
    this.radius = 0,
  });

  /// Where dropped files go; takes none while null. Stays in the tree then,
  /// so what it wraps keeps its state as the channel changes kind.
  final ChannelRef? channel;
  final Widget child;

  /// The corners of the target's scrim.
  final double radius;

  @override
  ConsumerState<AttachmentDropZone> createState() => _AttachmentDropZoneState();
}

class _AttachmentDropZoneState extends ConsumerState<AttachmentDropZone> {
  var _dragging = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final channel = widget.channel;
    final data = channel == null
        ? null
        : ref.watch(serverProvider(channel.server).select((s) => s.data));
    final channelName = data?.channels[channel?.channel]?.name ?? '';
    final limit = data?.myUploadLimit ?? UploadSettings.fallback.maxFileBytes;
    final canAttach =
        channel != null &&
        (data?.permissionsIn(channel.channel).has(Permissions.attachFiles) ??
            false);

    return DropTarget(
      enable: channel != null,
      onDragEntered: (_) => setState(() => _dragging = true),
      onDragExited: (_) => setState(() => _dragging = false),
      onDragDone: (details) {
        setState(() => _dragging = false);
        if (!canAttach) return;
        final paths = details.files.map((f) => f.path).toList();
        unawaited(
          ref.read(composerProvider(channel).notifier).stagePaths(paths),
        );
      },
      child: Stack(
        fit: StackFit.passthrough,
        children: [
          widget.child,
          if (_dragging)
            Positioned.fill(
              child: IgnorePointer(
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    color: colors.scrim,
                    borderRadius: BorderRadius.circular(widget.radius),
                  ),
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
