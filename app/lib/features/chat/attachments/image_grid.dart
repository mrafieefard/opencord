import 'dart:math';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/attachment_bytes.dart';
import 'package:opencord/features/chat/attachments/download.dart';
import 'package:opencord/features/chat/attachments/media_viewer.dart';
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
    final handleTap =
        onTap ??
        (int index) => showMediaViewer(
          context,
          serverKey: serverKey,
          media: images,
          index: index,
        );
    Widget cell(int index, {int remaining = 0}) => _Cell(
      serverKey: serverKey,
      message: message,
      attachment: images[index],
      index: index,
      remaining: remaining,
      onTap: handleTap,
    );
    const gap = SizedBox(width: OcSpace.s4, height: OcSpace.s4);

    switch (images.length) {
      case 1:
        // The box is sized before loading, so the list doesn't jump; a
        // small picture keeps its own size.
        final image = images.first;
        final width = (image.width ?? image.thumbnailWidth ?? 4).toDouble();
        final height = (image.height ?? image.thumbnailHeight ?? 3).toDouble();
        final (w, h) = width > 0 && height > 0 ? (width, height) : (4.0, 3.0);
        final scale = [1.0, 400 / w, 300 / h].reduce(min);
        return SizedBox(width: w * scale, height: h * scale, child: cell(0));
      case 2:
        return SizedBox(
          width: 400,
          height: 200,
          child: Row(
            children: [
              Expanded(child: cell(0)),
              gap,
              Expanded(child: cell(1)),
            ],
          ),
        );
      case 3:
        return SizedBox(
          width: 400,
          height: 266,
          child: Row(
            children: [
              Expanded(flex: 2, child: cell(0)),
              gap,
              Expanded(
                child: Column(
                  children: [
                    Expanded(child: cell(1)),
                    gap,
                    Expanded(child: cell(2)),
                  ],
                ),
              ),
            ],
          ),
        );
      default:
        return SizedBox(
          width: 400,
          height: 300,
          child: Column(
            children: [
              Expanded(
                child: Row(
                  children: [
                    Expanded(child: cell(0)),
                    gap,
                    Expanded(child: cell(1)),
                  ],
                ),
              ),
              gap,
              Expanded(
                child: Row(
                  children: [
                    Expanded(child: cell(2)),
                    gap,
                    Expanded(child: cell(3, remaining: images.length - 4)),
                  ],
                ),
              ),
            ],
          ),
        );
    }
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
    // A small animated picture plays from its original.
    final isAnimatedSmall =
        attachment.animated && attachment.sizeBytes <= 15 * 1024 * 1024;
    final useThumbnail = attachment.hasThumbnail && !isAnimatedSmall;
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
