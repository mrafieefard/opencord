import 'package:flutter/material.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/audio_card.dart';
import 'package:opencord/features/chat/attachments/file_card.dart';
import 'package:opencord/features/chat/attachments/image_grid.dart';
import 'package:opencord/features/chat/attachments/media_viewer.dart';
import 'package:opencord/features/chat/attachments/video_tile.dart';
import 'package:opencord/features/chat/voice/voice_bubble.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

class AttachmentsView extends StatelessWidget {
  const AttachmentsView({
    super.key,
    required this.serverKey,
    required this.message,
  });

  final String serverKey;
  final Message message;

  @override
  Widget build(BuildContext context) {
    if (message.attachments.isEmpty) return const SizedBox.shrink();

    final images = message.attachments
        .where((a) => a.kind == AttachmentKind.image)
        .toList();
    final nonImages = message.attachments
        .where((a) => a.kind != AttachmentKind.image)
        .toList();
    // The viewer steps through the message's images and videos in order.
    final viewable = message.attachments
        .where(
          (a) =>
              a.kind == AttachmentKind.image || a.kind == AttachmentKind.video,
        )
        .toList();
    void view(Attachment attachment) => showMediaViewer(
      context,
      serverKey: serverKey,
      media: viewable,
      index: viewable.indexOf(attachment),
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (images.isNotEmpty) ...[
          ImageGrid(
            serverKey: serverKey,
            message: message,
            images: images,
            onTap: (index) => view(images[index]),
          ),
          if (nonImages.isNotEmpty) const SizedBox(height: OcSpace.s8),
        ],
        for (final item in nonImages) ...[
          if (item.kind == AttachmentKind.voiceMessage)
            VoiceBubble(
              serverKey: serverKey,
              attachment: item,
              message: message,
            )
          else if (item.kind == AttachmentKind.video)
            VideoTile(
              serverKey: serverKey,
              attachment: item,
              message: message,
              onTap: () => view(item),
            )
          else if (item.kind == AttachmentKind.audio)
            AudioCard(serverKey: serverKey, attachment: item, message: message)
          else
            FileCard(serverKey: serverKey, attachment: item, message: message),
          const SizedBox(height: OcSpace.s4),
        ],
      ],
    );
  }
}
