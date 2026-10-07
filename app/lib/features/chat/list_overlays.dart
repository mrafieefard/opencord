import 'package:flutter/material.dart';

import 'package:opencord/features/chat/chat_rows.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// The round button that brings back the newest message (§4.5), with the
/// number of messages that arrived while scrolled up.
class JumpToBottomButton extends StatelessWidget {
  const JumpToBottomButton({
    super.key,
    required this.visible,
    required this.count,
    required this.onPressed,
  });

  final bool visible;
  final int count;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    return IgnorePointer(
      ignoring: !visible,
      child: AnimatedOpacity(
        opacity: visible ? 1 : 0,
        duration: motion.morph,
        child: AnimatedScale(
          scale: visible ? 1 : 0.9,
          duration: motion.morph,
          child: Stack(
            clipBehavior: Clip.none,
            children: [
              Tooltip(
                message: 'Jump to the newest message',
                child: Hoverable(
                  onTap: onPressed,
                  semanticLabel: count > 0
                      ? 'Jump to the newest message, $count new'
                      : 'Jump to the newest message',
                  focusRadius: BorderRadius.circular(OcSize.jumpButton / 2),
                  builder: (context, state) => AnimatedContainer(
                    duration: motion.hover,
                    width: OcSize.jumpButton,
                    height: OcSize.jumpButton,
                    decoration: BoxDecoration(
                      color: state.active ? colors.hover : colors.elevated,
                      shape: BoxShape.circle,
                      border: Border.all(color: colors.border),
                      boxShadow: OcShadows.menu(colors),
                    ),
                    child: Icon(
                      OcIcons.arrowDownward,
                      size: OcSize.iconButton,
                      color: colors.text,
                    ),
                  ),
                ),
              ),
              if (count > 0)
                Positioned(
                  top: -6,
                  right: -4,
                  child: ExcludeSemantics(child: UnreadBadge(count: count)),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The date of what is at the top of the view, shown while scrolling
/// (§4.5, Telegram).
class FloatingDayPill extends StatelessWidget {
  const FloatingDayPill({super.key, required this.label});

  /// Null hides the pill.
  final String? label;

  @override
  Widget build(BuildContext context) {
    final motion = OcMotion.of(context);
    final label = this.label;
    return IgnorePointer(
      child: AnimatedOpacity(
        opacity: label == null ? 0 : 1,
        duration: motion.morph,
        child: label == null
            ? const SizedBox(height: 24)
            : ExcludeSemantics(child: CenterPill(text: label)),
      ),
    );
  }
}
