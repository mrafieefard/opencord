import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';

class VideoTile extends ConsumerWidget {
  const VideoTile({
    super.key,
    required this.serverKey,
    required this.attachment,
    required this.message,
    this.onTap,
  });

  final String serverKey;
  final Attachment attachment;
  final Message message;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;

    final width = (attachment.width ?? attachment.thumbnailWidth ?? 4)
        .toDouble();
    final height = (attachment.height ?? attachment.thumbnailHeight ?? 3)
        .toDouble();
    final (w, h) = width > 0 && height > 0 ? (width, height) : (4.0, 3.0);
    final scale = [1.0, 400 / w, 300 / h].reduce(min);
    final tileWidth = w * scale;
    final tileHeight = h * scale;

    final key = (server: serverKey, id: attachment.id, thumbnail: true);
    final thumbnailAsync = attachment.hasThumbnail
        ? ref.watch(attachmentBytesProvider(key))
        : null;

    final durationText = attachment.durationMs != null
        ? formatDurationMs(attachment.durationMs!)
        : null;

    return GestureDetector(
      onTap: onTap,
      onSecondaryTapUp: (details) => showAttachmentMenu(
        context: context,
        ref: ref,
        position: details.globalPosition,
        serverKey: serverKey,
        attachment: attachment,
        message: message,
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(OcRadius.bubble),
        child: Container(
          width: tileWidth,
          height: tileHeight,
          color: colors.surface,
          child: Stack(
            fit: StackFit.expand,
            children: [
              // Poster or surface with movie icon
              if (thumbnailAsync != null)
                thumbnailAsync.when(
                  data: (bytes) => bytes.isNotEmpty
                      ? Image.memory(
                          bytes,
                          fit: BoxFit.cover,
                          width: tileWidth,
                          height: tileHeight,
                        )
                      : _buildFallback(colors),
                  loading: () => _buildFallback(colors),
                  error: (_, _) => _buildFallback(colors),
                )
              else
                _buildFallback(colors),

              // The play button and badge sit on the poster: light on dark in
              // either theme, like the image grid's overlay.
              Center(
                child: Container(
                  width: 48,
                  height: 48,
                  decoration: BoxDecoration(
                    color: OcColors.dark.scrim,
                    shape: BoxShape.circle,
                  ),
                  child: Icon(
                    OcIcons.playArrow,
                    size: 28,
                    color: OcColors.dark.text,
                  ),
                ),
              ),

              if (durationText != null)
                Positioned(
                  bottom: OcSpace.s8,
                  right: OcSpace.s8,
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: OcSpace.s6,
                      vertical: OcSpace.s2,
                    ),
                    decoration: BoxDecoration(
                      color: OcColors.dark.scrim,
                      borderRadius: BorderRadius.circular(4),
                    ),
                    child: Text(
                      durationText,
                      style: OcText.small.copyWith(
                        color: OcColors.dark.text,
                        fontWeight: FontWeight.w600,
                      ),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildFallback(OcColors colors) {
    return Center(
      child: Icon(OcIcons.movie, size: 48, color: colors.textSecondary),
    );
  }
}
