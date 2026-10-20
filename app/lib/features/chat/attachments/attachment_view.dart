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
    this.onTapImage,
  });

  final String serverKey;
  final Message message;
  final void Function(int index)? onTapImage;

  @override
  Widget build(BuildContext context) {
    if (message.attachments.isEmpty) return const SizedBox.shrink();

    final images = message.attachments
        .where((a) => a.kind == AttachmentKind.image)
        .toList();
    final nonImages = message.attachments
        .where((a) => a.kind != AttachmentKind.image)
        .toList();

    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (images.isNotEmpty) ...[
          ImageGrid(
            serverKey: serverKey,
            message: message,
            images: images,
            onTap: onTapImage,
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
              onTap: () {
                if (onTapImage != null) {
                  onTapImage!(message.attachments.indexOf(item));
                } else {
                  showMediaViewer(
                    context,
                    serverKey: serverKey,
                    media: [item],
                    index: 0,
                  );
                }
              },
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
