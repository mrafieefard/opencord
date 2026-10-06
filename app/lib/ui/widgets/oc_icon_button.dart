import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/key_hint.dart';

/// Hit area and icon size (§2.3).
enum OcIconButtonSize {
  normal(OcSize.hitDefault, OcSize.iconButton),
  compact(OcSize.hitCompact, OcSize.iconRow),
  voice(OcSize.hitVoice, 22);

  const OcIconButtonSize(this.extent, this.iconSize);

  final double extent;
  final double iconSize;
}

/// How an active (toggled on) button looks: muted and deafened buttons in
/// the user panel use [selected]; voice controls use [inverted] (§4.2, §4.10).
enum OcActiveStyle { selected, inverted }

/// A round icon-only button with a tooltip, which also labels it for
/// screen readers (§9).
class OcIconButton extends StatelessWidget {
  const OcIconButton({
    super.key,
    required this.icon,
    required this.tooltip,
    this.onPressed,
    this.onSecondaryTap,
    this.activeIcon,
    this.size = OcIconButtonSize.normal,
    this.active = false,
    this.activeStyle = OcActiveStyle.selected,
    this.shortcut,
    this.color,
    this.focusNode,
  });

  final IconData icon;

  /// Icon while [active], like `mic_off` for a muted microphone.
  final IconData? activeIcon;
  final String tooltip;
  final VoidCallback? onPressed;
  final ValueChanged<Offset>? onSecondaryTap;
  final OcIconButtonSize size;
  final bool active;
  final OcActiveStyle activeStyle;

  /// Shown in the tooltip, like "Search (Ctrl+K)".
  final SingleActivator? shortcut;

  /// Icon color at rest; defaults to `textSecondary`.
  final Color? color;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final enabled = onPressed != null;
    final message = shortcut == null
        ? tooltip
        : '$tooltip (${shortcutLabel(shortcut!, Theme.of(context).platform)})';
    return Tooltip(
      message: message,
      excludeFromSemantics: true,
      child: Hoverable(
        onTap: onPressed,
        onSecondaryTap: onSecondaryTap,
        focusNode: focusNode,
        semanticLabel: tooltip,
        selected: active ? true : null,
        focusRadius: BorderRadius.circular(size.extent / 2),
        builder: (context, state) {
          final (Color background, Color foreground) = switch ((
            enabled,
            active,
            activeStyle,
          )) {
            (false, _, _) => (Colors.transparent, colors.textMuted),
            (true, true, OcActiveStyle.inverted) => (
              state.active
                  ? Color.lerp(colors.accent, colors.onAccent, 0.12)!
                  : colors.accent,
              colors.onAccent,
            ),
            (true, true, OcActiveStyle.selected) => (
              colors.selected,
              colors.text,
            ),
            (true, false, _) => (
              state.active ? colors.selected : Colors.transparent,
              state.active ? colors.text : (color ?? colors.textSecondary),
            ),
          };
          return AnimatedContainer(
            duration: motion.hover,
            curve: OcMotion.curve,
            width: size.extent,
            height: size.extent,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: background,
            ),
            child: Icon(
              active ? (activeIcon ?? icon) : icon,
              size: size.iconSize,
              color: foreground,
            ),
          );
        },
      ),
    );
  }
}
