import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// Pictures a profile picture or server icon can be made from.
const pictureTypes = XTypeGroup(
  label: 'Pictures',
  extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'],
);

/// A profile picture or server icon in settings: the picture, Change and
/// Remove. What is chosen waits for the save bar; the owner keeps it.
class PictureChooser extends StatelessWidget {
  const PictureChooser({
    super.key,
    required this.id,
    required this.name,
    required this.image,
    required this.onPicked,
    required this.onRemoved,
    this.square = false,
  });

  final Object id;
  final String name;

  /// The picture shown: the one chosen, or the one saved; null for none.
  final ImageProvider? image;

  /// A picture file was chosen: its path.
  final ValueChanged<String> onPicked;

  /// Null when there is nothing to remove.
  final VoidCallback? onRemoved;

  /// A server's rounded square rather than a person's circle.
  final bool square;

  Future<void> _pick() async {
    final file = await openFile(acceptedTypeGroups: const [pictureTypes]);
    if (file != null) onPicked(file.path);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Row(
      children: [
        OcAvatar(
          id: id,
          name: name.isEmpty ? '?' : name,
          size: 64,
          borderRadius: square ? 18 : null,
          image: image,
        ),
        const SizedBox(width: OcSpace.s16),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  OcButton(
                    label: image == null ? 'Add a picture' : 'Change',
                    dense: true,
                    onPressed: _pick,
                  ),
                  if (onRemoved case final remove?) ...[
                    const SizedBox(width: OcSpace.s8),
                    OcButton.ghost(
                      label: 'Remove',
                      dense: true,
                      onPressed: remove,
                    ),
                  ],
                ],
              ),
              const SizedBox(height: OcSpace.s6),
              Text(
                'PNG, JPEG, GIF or WebP, up to 8 MB. The middle square is '
                'kept, at 256 pixels.',
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
