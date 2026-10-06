import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';

/// Primary buttons are inverted; destructive actions are primary buttons
/// with explicit wording, never a color (§4.11).
enum OcButtonVariant { primary, secondary, ghost }

class OcButton extends StatelessWidget {
  const OcButton({
    super.key,
    required this.label,
    this.onPressed,
    this.variant = OcButtonVariant.secondary,
    this.dense = false,
    this.icon,
    this.busy = false,
    this.expand = false,
    this.autofocus = false,
    this.focusNode,
  });

  const OcButton.primary({
    super.key,
    required this.label,
    this.onPressed,
    this.dense = false,
    this.icon,
    this.busy = false,
    this.expand = false,
    this.autofocus = false,
    this.focusNode,
  }) : variant = OcButtonVariant.primary;

  const OcButton.ghost({
    super.key,
    required this.label,
    this.onPressed,
    this.dense = false,
    this.icon,
    this.busy = false,
    this.expand = false,
    this.autofocus = false,
    this.focusNode,
  }) : variant = OcButtonVariant.ghost;

  final String label;
  final VoidCallback? onPressed;
  final OcButtonVariant variant;

  /// 28 px tall instead of 36.
  final bool dense;
  final IconData? icon;

  /// Shows a spinner in place of the label and ignores presses.
  final bool busy;

  /// Fills the available width.
  final bool expand;
  final bool autofocus;
  final FocusNode? focusNode;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final enabled = onPressed != null;
    final radius = BorderRadius.circular(dense ? 8 : 10);
    final style = (dense ? OcText.small : OcText.body).copyWith(
      fontWeight: FontWeight.w600,
    );
    return Opacity(
      opacity: enabled ? 1 : 0.4,
      child: Hoverable(
        onTap: enabled && !busy ? onPressed : null,
        focusNode: focusNode,
        autofocus: autofocus,
        focusRadius: radius,
        semanticLabel: label,
        builder: (context, state) {
          final (Color background, Color foreground) = switch (variant) {
            OcButtonVariant.primary => (
              state.pressed
                  ? Color.lerp(colors.accent, colors.onAccent, 0.2)!
                  : state.hovered
                  ? Color.lerp(colors.accent, colors.onAccent, 0.12)!
                  : colors.accent,
              colors.onAccent,
            ),
            OcButtonVariant.secondary => (
              state.active ? colors.selected : colors.hover,
              colors.text,
            ),
            OcButtonVariant.ghost => (
              state.pressed
                  ? colors.selected
                  : state.hovered
                  ? colors.hover
                  : Colors.transparent,
              colors.text,
            ),
          };
          final content = Row(
            mainAxisSize: MainAxisSize.min,
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              if (icon != null) ...[
                Icon(icon, size: dense ? 16 : 18, color: foreground),
                const SizedBox(width: 6),
              ],
              Flexible(
                child: Text(
                  label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: style.copyWith(color: foreground),
                ),
              ),
            ],
          );
          return AnimatedContainer(
            duration: motion.hover,
            curve: OcMotion.curve,
            height: dense ? 28 : 36,
            width: expand ? double.infinity : null,
            padding: EdgeInsets.symmetric(horizontal: dense ? 12 : 16),
            decoration: BoxDecoration(color: background, borderRadius: radius),
            alignment: Alignment.center,
            child: busy
                ? Stack(
                    alignment: Alignment.center,
                    children: [
                      Opacity(opacity: 0, child: content),
                      OcSpinner(size: dense ? 14 : 16, color: foreground),
                    ],
                  )
                : content,
          );
        },
      ),
    );
  }
}
