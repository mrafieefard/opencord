import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/popover.dart';

/// The small bar beside a hovered bubble (§4.5, Discord): React · Reply ·
/// More. Each callback gets the button's global rectangle to anchor to.
///
/// The bar names the hovered button itself instead of using tooltips: a
/// tooltip is an overlay of its own, and nested inside the bar's overlay it
/// breaks when the message disappears under the pointer.
class HoverActionBar extends StatefulWidget {
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

  static const double height = OcSize.hitCompact + 2 * OcSpace.s2 + 2;

  /// How far the bar overlaps what it sits on when placed on an edge.
  static const double overlap = 14;

  @override
  State<HoverActionBar> createState() => _HoverActionBarState();
}

class _HoverActionBarState extends State<HoverActionBar> {
  int? _hovered;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final buttons = <(IconData, String, ValueChanged<Rect>)>[
      if (widget.onReact case final onReact?)
        (OcIcons.addReaction, 'Add reaction', onReact),
      if (widget.onReply case final onReply?)
        (OcIcons.reply, 'Reply', (_) => onReply()),
      (OcIcons.moreHoriz, 'More', widget.onMore),
    ];
    final hovered = _hovered;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        Container(
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
              for (final (index, (icon, label, onTap)) in buttons.indexed)
                MouseRegion(
                  onEnter: (_) => setState(() => _hovered = index),
                  onExit: (_) => setState(() {
                    if (_hovered == index) _hovered = null;
                  }),
                  child: Builder(
                    builder: (context) => OcIconButton(
                      icon: icon,
                      tooltip: label,
                      showTooltip: false,
                      size: OcIconButtonSize.compact,
                      onPressed: () => onTap(globalRectOf(context)),
                    ),
                  ),
                ),
            ],
          ),
        ),
        if (hovered != null && hovered < buttons.length)
          Positioned(
            left:
                OcSpace.s2 +
                1 +
                hovered * OcSize.hitCompact +
                OcSize.hitCompact / 2,
            bottom: OcSize.hitCompact + 2 * OcSpace.s2 + 2 + OcSpace.s4,
            child: FractionalTranslation(
              translation: const Offset(-0.5, 0),
              child: IgnorePointer(
                child: ExcludeSemantics(
                  child: Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: OcSpace.s8,
                      vertical: OcSpace.s4,
                    ),
                    decoration: BoxDecoration(
                      color: colors.elevated,
                      borderRadius: BorderRadius.circular(8),
                      border: Border.all(color: colors.border),
                    ),
                    child: Text(
                      buttons[hovered].$2,
                      maxLines: 1,
                      style: OcText.small.copyWith(color: colors.text),
                    ),
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}
