import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/popup_route.dart';

sealed class OcMenuEntry {
  const OcMenuEntry();
}

class OcMenuItem extends OcMenuEntry {
  const OcMenuItem({
    required this.label,
    this.icon,
    this.shortcut,
    this.onSelected,
    this.checked,
    this.submenu,
  });

  final String label;
  final IconData? icon;

  /// Shown on the right, in the platform's notation.
  final SingleActivator? shortcut;

  /// Disabled when null (and there is no [submenu]).
  final VoidCallback? onSelected;

  /// A check mark on the right when true; reserves its place when false.
  final bool? checked;
  final List<OcMenuEntry>? submenu;

  bool get enabled => onSelected != null || submenu != null;
}

class OcMenuDivider extends OcMenuEntry {
  const OcMenuDivider();
}

/// Opens a context menu at [position] (global). The chosen item runs after
/// the menu has closed, so it can open dialogs of its own.
Future<void> showOcMenu({
  required BuildContext context,
  required Offset position,
  required List<OcMenuEntry> entries,
}) async {
  final action = await _openMenu(context, entries, _MenuLayout.at(position));
  action?.call();
}

Future<VoidCallback?> _openMenu(
  BuildContext context,
  List<OcMenuEntry> entries,
  _MenuLayout layout,
) {
  return pushPopup<VoidCallback>(
    context,
    duration: OcMotion.of(context).menu,
    layout: layout,
    builder: (context) => OcMenuPanel(entries: entries),
  );
}

class _MenuLayout extends SingleChildLayoutDelegate {
  const _MenuLayout.at(Offset this.point) : beside = null;

  const _MenuLayout.beside(Rect this.beside) : point = null;

  final Offset? point;
  final Rect? beside;

  @override
  BoxConstraints getConstraintsForChild(BoxConstraints constraints) =>
      BoxConstraints.loose(
        constraints.biggest,
      ).deflate(const EdgeInsets.all(popupMargin));

  @override
  Offset getPositionForChild(Size size, Size child) {
    double x;
    double y;
    if (point case final point?) {
      x = point.dx + child.width > size.width - popupMargin
          ? point.dx - child.width
          : point.dx;
      y = point.dy + child.height > size.height - popupMargin
          ? size.height - popupMargin - child.height
          : point.dy;
    } else {
      final rect = beside!;
      const edge = _MenuItemTile.panelPadding + 1;
      x = rect.right + edge + child.width <= size.width - popupMargin
          ? rect.right + edge
          : rect.left - edge - child.width;
      y = rect.top - _MenuItemTile.panelPadding;
      if (y + child.height > size.height - popupMargin) {
        y = size.height - popupMargin - child.height;
      }
    }
    return Offset(
      x.clamp(popupMargin, math.max(popupMargin, size.width - child.width)),
      math.max(popupMargin, y),
    );
  }

  @override
  bool shouldRelayout(_MenuLayout old) =>
      old.point != point || old.beside != beside;
}

/// The menu surface. Arrow keys move between items; Enter chooses; Escape
/// or Left (in a submenu) closes.
class OcMenuPanel extends StatelessWidget {
  const OcMenuPanel({super.key, required this.entries});

  final List<OcMenuEntry> entries;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Shortcuts(
      shortcuts: const {
        SingleActivator(LogicalKeyboardKey.arrowDown): NextFocusIntent(),
        SingleActivator(LogicalKeyboardKey.arrowUp): PreviousFocusIntent(),
        SingleActivator(LogicalKeyboardKey.arrowLeft): DismissIntent(),
      },
      child: FocusScope(
        autofocus: true,
        child: Container(
          constraints: const BoxConstraints(minWidth: 190, maxWidth: 300),
          padding: const EdgeInsets.all(_MenuItemTile.panelPadding),
          decoration: popupDecoration(colors),
          child: IntrinsicWidth(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                for (final entry in entries)
                  switch (entry) {
                    OcMenuItem() => _MenuItemTile(item: entry),
                    OcMenuDivider() => Padding(
                      padding: const EdgeInsets.symmetric(vertical: 4),
                      child: Divider(height: 1, color: colors.border),
                    ),
                  },
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _MenuItemTile extends StatelessWidget {
  const _MenuItemTile({required this.item});

  static const double panelPadding = 4;

  final OcMenuItem item;

  Future<void> _activate(BuildContext context) async {
    final submenu = item.submenu;
    if (submenu == null) {
      Navigator.pop(context, item.onSelected);
      return;
    }
    final chosen = await _openMenu(
      context,
      submenu,
      _MenuLayout.beside(globalRectOf(context)),
    );
    if (chosen != null && context.mounted) Navigator.pop(context, chosen);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final foreground = item.enabled ? colors.text : colors.textMuted;
    final tile = Hoverable(
      onTap: item.enabled ? () => _activate(context) : null,
      showFocusRing: false,
      focusRadius: BorderRadius.circular(6),
      semanticLabel: item.label,
      builder: (context, state) => Container(
        height: 32,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        decoration: BoxDecoration(
          color: item.enabled && (state.active || state.focused)
              ? colors.selected
              : null,
          borderRadius: BorderRadius.circular(6),
        ),
        child: Row(
          children: [
            if (item.icon != null) ...[
              Icon(
                item.icon,
                size: 18,
                color: item.enabled ? colors.textSecondary : colors.textMuted,
              ),
              const SizedBox(width: 10),
            ],
            Expanded(
              child: Text(
                item.label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.body.copyWith(color: foreground),
              ),
            ),
            if (item.shortcut != null) ...[
              const SizedBox(width: 16),
              Text(
                shortcutLabel(item.shortcut!, Theme.of(context).platform),
                style: OcText.meta.copyWith(color: colors.textMuted),
              ),
            ],
            if (item.checked != null) ...[
              const SizedBox(width: 12),
              SizedBox(
                width: 18,
                child: item.checked!
                    ? Icon(OcIcons.check, size: 18, color: colors.text)
                    : null,
              ),
            ],
            if (item.submenu != null) ...[
              const SizedBox(width: 12),
              Icon(OcIcons.chevronRight, size: 18, color: colors.textMuted),
            ],
          ],
        ),
      ),
    );
    if (item.submenu == null) return tile;
    return Shortcuts(
      shortcuts: const {
        SingleActivator(LogicalKeyboardKey.arrowRight): ActivateIntent(),
      },
      child: tile,
    );
  }
}
