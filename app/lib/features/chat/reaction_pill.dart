import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/chat/emoji_picker.dart';
import 'package:opencord/ui/emoji/emoji.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/popover.dart';

sealed class _Choice {
  const _Choice();
}

final class _Picked extends _Choice {
  const _Picked(this.emoji);

  final String emoji;
}

final class _More extends _Choice {
  const _More();
}

/// Opens the quick reaction pill by [anchor] (§4.5); "more" swaps it for
/// the full picker. Returns the chosen emoji.
Future<String?> pickReaction(
  BuildContext context, {
  required Rect anchor,
}) async {
  final choice = await showPopover<_Choice>(
    context: context,
    anchor: anchor,
    side: PopoverSide.above,
    padding: const EdgeInsets.all(OcSpace.s4),
    builder: (context) => QuickReactions(
      onPicked: (emoji) => Navigator.pop(context, _Picked(emoji)),
      onMore: () => Navigator.pop(context, const _More()),
    ),
  );
  switch (choice) {
    case _Picked(:final emoji):
      return emoji;
    case _More():
      if (!context.mounted) return null;
      return showEmojiPicker(context, anchor: anchor);
    case null:
      return null;
  }
}

/// The eight most used emoji in a row, then a button for the rest.
class QuickReactions extends ConsumerWidget {
  const QuickReactions({
    super.key,
    required this.onPicked,
    required this.onMore,
  });

  final ValueChanged<String> onPicked;
  final VoidCallback onMore;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final emoji = frequentEmoji(ref.watch(emojiUsageProvider));
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        for (final (index, char) in emoji.indexed)
          Hoverable(
            autofocus: index == 0,
            onTap: () {
              ref.read(emojiUsageProvider.notifier).use(char);
              onPicked(char);
            },
            semanticLabel: 'React with ${emojiForChar(char)?.name ?? char}',
            focusRadius: BorderRadius.circular(OcRadius.menu),
            builder: (context, state) => AnimatedScale(
              scale: state.active ? 1.18 : 1,
              duration: OcMotion.of(context).hover,
              child: Container(
                width: 36,
                height: 36,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: state.active ? colors.hover : null,
                  borderRadius: BorderRadius.circular(OcRadius.menu),
                ),
                child: Text(char, style: const TextStyle(fontSize: 22)),
              ),
            ),
          ),
        const SizedBox(width: OcSpace.s2),
        OcIconButton(
          icon: OcIcons.addReaction,
          tooltip: 'More reactions',
          size: OcIconButtonSize.compact,
          onPressed: onMore,
        ),
      ],
    );
  }
}
