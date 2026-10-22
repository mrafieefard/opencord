import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

/// Above the input while replying or editing (§4.6): the icon, a 2 px line,
/// "Reply to Kai" or "Edit message" over a one-line preview, and a close
/// button. A click on it shows the message.
class ComposerContextBar extends StatelessWidget {
  const ComposerContextBar({
    super.key,
    required this.editing,
    required this.title,
    required this.preview,
    required this.onClose,
    this.onTap,
  });

  final bool editing;
  final String title;
  final String preview;
  final VoidCallback onClose;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.only(bottom: OcSpace.s6),
      child: Row(
        children: [
          SizedBox(
            width: OcSize.hitDefault,
            child: Icon(
              editing ? OcIcons.edit : OcIcons.reply,
              size: OcSize.iconRow,
              color: colors.textSecondary,
            ),
          ),
          Expanded(
            child: Hoverable(
              onTap: onTap,
              semanticLabel: '$title: $preview',
              builder: (context, state) => AnimatedContainer(
                duration: OcMotion.of(context).hover,
                padding: const EdgeInsets.symmetric(
                  horizontal: OcSpace.s8,
                  vertical: OcSpace.s2,
                ),
                decoration: BoxDecoration(
                  color: state.active ? colors.hover : null,
                  borderRadius: BorderRadius.circular(OcRadius.quote),
                ),
                child: IntrinsicHeight(
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Container(width: 2, color: colors.text),
                      const SizedBox(width: OcSpace.s8),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Text(
                              title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.small.copyWith(
                                fontWeight: FontWeight.w600,
                                color: colors.text,
                              ),
                            ),
                            Text(
                              preview,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: OcText.small.copyWith(
                                color: colors.textSecondary,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          OcIconButton(
            icon: OcIcons.close,
            tooltip: editing ? 'Cancel edit' : 'Cancel reply',
            size: OcIconButtonSize.compact,
            onPressed: onClose,
          ),
        ],
      ),
    );
  }
}
