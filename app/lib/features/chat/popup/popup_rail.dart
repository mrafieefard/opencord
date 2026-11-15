import 'package:flutter/material.dart';

import 'package:opencord/features/chat/popup/popup_rows.dart';
import 'package:opencord/features/chat/popup/source_icon.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';

/// The popup's column of sources, one icon each: clicking one goes to its
/// commands, and the highlight follows the list.
class PopupRail extends StatefulWidget {
  const PopupRail({
    super.key,
    required this.serverKey,
    required this.groups,
    required this.selected,
    required this.onPick,
  });

  static const double width = 52;
  static const double _button = 44;
  static const double _icon = 32;
  static const double _highlight = 36;
  static const double _padding = 6;

  final String serverKey;
  final List<PopupGroup> groups;

  /// The group whose highlight shows.
  final int selected;
  final ValueChanged<int> onPick;

  @override
  State<PopupRail> createState() => _PopupRailState();
}

class _PopupRailState extends State<PopupRail> {
  final _scroll = ScrollController();

  @override
  void didUpdateWidget(PopupRail oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.selected != widget.selected) _keepInView();
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  void _keepInView() {
    if (!_scroll.hasClients) return;
    final top = widget.selected * PopupRail._button;
    final bottom = top + PopupRail._button + 2 * PopupRail._padding;
    final position = _scroll.position;
    if (top < position.pixels) {
      _scroll.jumpTo(top);
    } else if (bottom > position.pixels + position.viewportDimension) {
      _scroll.jumpTo(
        (bottom - position.viewportDimension).clamp(
          position.minScrollExtent,
          position.maxScrollExtent,
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final inset = (PopupRail.width - PopupRail._highlight) / 2;
    final buttonInset = (PopupRail._button - PopupRail._highlight) / 2;
    return Container(
      width: PopupRail.width,
      decoration: BoxDecoration(
        border: Border(right: BorderSide(color: colors.border)),
      ),
      child: SingleChildScrollView(
        controller: _scroll,
        padding: const EdgeInsets.symmetric(vertical: PopupRail._padding),
        child: Stack(
          children: [
            AnimatedPositioned(
              duration: OcMotion.of(context).hover,
              curve: OcMotion.curve,
              left: inset,
              top: widget.selected * PopupRail._button + buttonInset,
              width: PopupRail._highlight,
              height: PopupRail._highlight,
              child: DecoratedBox(
                decoration: BoxDecoration(
                  color: colors.selected,
                  borderRadius: BorderRadius.circular(11),
                ),
              ),
            ),
            Column(
              children: [
                for (final (index, group) in widget.groups.indexed)
                  _RailButton(
                    serverKey: widget.serverKey,
                    source: group.source,
                    onTap: () => widget.onPick(index),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _RailButton extends StatelessWidget {
  const _RailButton({
    required this.serverKey,
    required this.source,
    required this.onTap,
  });

  final String serverKey;
  final PopupSource source;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Tooltip(
    message: source.name,
    waitDuration: const Duration(milliseconds: 300),
    child: MouseRegion(
      cursor: SystemMouseCursors.click,
      // No focus: the arrow keys stay in the composer.
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: onTap,
        child: Semantics(
          button: true,
          label: source.name,
          child: SizedBox(
            width: PopupRail.width,
            height: PopupRail._button,
            child: Center(
              child: CommandSourceIcon(
                serverKey: serverKey,
                source: source,
                size: PopupRail._icon,
                radius: 10,
              ),
            ),
          ),
        ),
      ),
    ),
  );
}
