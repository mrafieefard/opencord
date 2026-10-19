import 'package:flutter/material.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/file_card.dart';
import 'package:opencord/features/chat/attachments/image_grid.dart';
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
          FileCard(serverKey: serverKey, attachment: item, message: message),
          const SizedBox(height: OcSpace.s4),
        ],
      ],
    );
  }
}
