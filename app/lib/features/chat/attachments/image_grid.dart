import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';

class ImageGrid extends StatelessWidget {
  const ImageGrid({
    super.key,
    required this.serverKey,
    required this.message,
    required this.images,
    this.onTap,
  });

  final String serverKey;
  final Message message;
  final List<Attachment> images;
  final void Function(int index)? onTap;

  @override
  Widget build(BuildContext context) {
    if (images.isEmpty) return const SizedBox.shrink();

    if (images.length == 1) {
      final img = images.first;
      final w = (img.width ?? img.thumbnailWidth ?? 4).toDouble();
      final h = (img.height ?? img.thumbnailHeight ?? 3).toDouble();
      final ratio = (w <= 0 || h <= 0) ? (4.0 / 3.0) : (w / h);
      final double boxWidth;
      final double boxHeight;
      if (ratio >= 400.0 / 300.0) {
        boxWidth = 400.0;
        boxHeight = 400.0 / ratio;
      } else {
        boxHeight = 300.0;
        boxWidth = 300.0 * ratio;
      }

      return SizedBox(
        width: boxWidth,
        height: boxHeight,
        child: _Cell(
          serverKey: serverKey,
          message: message,
          attachment: img,
          index: 0,
          onTap: onTap,
        ),
      );
    }

    if (images.length == 2) {
      return SizedBox(
        width: 400,
        height: 200,
        child: Row(
          children: [
            Expanded(
              child: _Cell(
                serverKey: serverKey,
                message: message,
                attachment: images[0],
                index: 0,
                onTap: onTap,
              ),
            ),
            const SizedBox(width: OcSpace.s4),
            Expanded(
              child: _Cell(
                serverKey: serverKey,
                message: message,
                attachment: images[1],
                index: 1,
                onTap: onTap,
              ),
            ),
          ],
        ),
      );
    }

    if (images.length == 3) {
      return SizedBox(
        width: 400,
        height: 266,
        child: Row(
          children: [
            Expanded(
              flex: 2,
              child: _Cell(
                serverKey: serverKey,
                message: message,
                attachment: images[0],
                index: 0,
                onTap: onTap,
              ),
            ),
            const SizedBox(width: OcSpace.s4),
            Expanded(
              flex: 1,
              child: Column(
                children: [
                  Expanded(
                    child: _Cell(
                      serverKey: serverKey,
                      message: message,
                      attachment: images[1],
                      index: 1,
                      onTap: onTap,
                    ),
                  ),
                  const SizedBox(height: OcSpace.s4),
                  Expanded(
                    child: _Cell(
                      serverKey: serverKey,
                      message: message,
                      attachment: images[2],
                      index: 2,
                      onTap: onTap,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      );
    }

    // 4 or more images: 2x2 grid in 400x300 box
    final remaining = images.length > 4 ? images.length - 4 : 0;
    return SizedBox(
      width: 400,
      height: 300,
      child: Column(
        children: [
          Expanded(
            child: Row(
              children: [
                Expanded(
                  child: _Cell(
                    serverKey: serverKey,
                    message: message,
                    attachment: images[0],
                    index: 0,
                    onTap: onTap,
                  ),
                ),
                const SizedBox(width: OcSpace.s4),
                Expanded(
                  child: _Cell(
                    serverKey: serverKey,
                    message: message,
                    attachment: images[1],
                    index: 1,
                    onTap: onTap,
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: OcSpace.s4),
          Expanded(
            child: Row(
              children: [
                Expanded(
                  child: _Cell(
                    serverKey: serverKey,
                    message: message,
                    attachment: images[2],
                    index: 2,
                    onTap: onTap,
                  ),
                ),
                const SizedBox(width: OcSpace.s4),
                Expanded(
                  child: _Cell(
                    serverKey: serverKey,
                    message: message,
                    attachment: images[3],
                    index: 3,
                    remaining: remaining,
                    onTap: onTap,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Cell extends ConsumerWidget {
  const _Cell({
    required this.serverKey,
    required this.message,
    required this.attachment,
    required this.index,
    this.remaining = 0,
    this.onTap,
  });

  final String serverKey;
  final Message message;
  final Attachment attachment;
  final int index;
  final int remaining;
  final void Function(int index)? onTap;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final isAnimatedSmall =
        attachment.animated && attachment.sizeBytes <= 15 * 1024 * 1024;
    final useThumbnail = !isAnimatedSmall;
    final key = (server: serverKey, id: attachment.id, thumbnail: useThumbnail);
    final bytesAsync = ref.watch(attachmentBytesProvider(key));

    Widget content = bytesAsync.when(
      loading: () => Container(color: colors.surface),
      error: (error, _) => Container(
        color: colors.surface,
        alignment: Alignment.center,
        padding: const EdgeInsets.all(OcSpace.s8),
        child: InkWell(
          onTap: () => ref.invalidate(attachmentBytesProvider(key)),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                OcIcons.error,
                size: OcSize.iconButton,
                color: colors.textSecondary,
              ),
              const SizedBox(height: OcSpace.s4),
              Text(
                "Couldn't load · Retry",
                style: OcText.small.copyWith(color: colors.textSecondary),
                textAlign: TextAlign.center,
              ),
            ],
          ),
        ),
      ),
      data: (bytes) {
        if (bytes.isEmpty) {
          return Container(color: colors.surface);
        }
        return Image.memory(
          bytes,
          fit: BoxFit.cover,
          width: double.infinity,
          height: double.infinity,
          errorBuilder: (context, error, stackTrace) => Container(
            color: colors.surface,
            alignment: Alignment.center,
            child: Icon(
              OcIcons.image,
              size: OcSize.iconButton,
              color: colors.textSecondary,
            ),
          ),
        );
      },
    );

    if (remaining > 0) {
      content = Stack(
        fit: StackFit.expand,
        children: [
          content,
          Container(
            color: colors.scrim,
            alignment: Alignment.center,
            child: Text(
              '+$remaining',
              style: OcText.title.copyWith(
                color: OcColors.dark.text,
                fontWeight: FontWeight.bold,
              ),
            ),
          ),
        ],
      );
    }

    return ClipRRect(
      borderRadius: BorderRadius.circular(OcRadius.bubble),
      child: GestureDetector(
        onTap: () => onTap?.call(index),
        onSecondaryTapUp: (details) => showAttachmentMenu(
          context: context,
          ref: ref,
          position: details.globalPosition,
          serverKey: serverKey,
          attachment: attachment,
          message: message,
        ),
        child: content,
      ),
    );
  }
}
