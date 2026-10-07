import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// The small bar beside a hovered bubble (§4.5, Discord): React · Reply ·
/// More. Each callback gets the button's global rectangle to anchor to.
class HoverActionBar extends StatelessWidget {
  const HoverActionBar({
    super.key,
    required this.onMore,
    this.onReact,
    this.onReply,
  });

  final ValueChanged<Rect>? onReact;
  final VoidCallback? onReply;
  final ValueChanged<Rect> onMore;

  /// How wide the bar is with [buttons] buttons.
  static double widthFor(int buttons) =>
      buttons * OcSize.hitCompact + 2 * OcSpace.s2 + 2;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    Widget button(IconData icon, String tooltip, ValueChanged<Rect> onTap) =>
        Builder(
          builder: (context) => OcIconButton(
            icon: icon,
            tooltip: tooltip,
            size: OcIconButtonSize.compact,
            onPressed: () => onTap(globalRectOf(context)),
          ),
        );
    return Container(
      padding: const EdgeInsets.all(OcSpace.s2),
      decoration: BoxDecoration(
        color: colors.elevated,
        borderRadius: BorderRadius.circular(OcRadius.menu),
        border: Border.all(color: colors.border),
        boxShadow: OcShadows.menu(colors),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (onReact case final onReact?)
            button(OcIcons.addReaction, 'Add reaction', onReact),
          if (onReply case final onReply?)
            button(OcIcons.reply, 'Reply', (_) => onReply()),
          button(OcIcons.moreHoriz, 'More', onMore),
        ],
      ),
    );
  }
}
