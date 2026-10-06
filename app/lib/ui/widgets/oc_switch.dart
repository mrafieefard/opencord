import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// A small toggle: inverted track when on (§1, inversion = emphasis).
class OcSwitch extends StatelessWidget {
  const OcSwitch({
    super.key,
    required this.value,
    required this.onChanged,
    this.semanticLabel,
  });

  final bool value;
  final ValueChanged<bool>? onChanged;
  final String? semanticLabel;

  static const double _width = 36;
  static const double _height = 20;
  static const double _thumb = 14;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final motion = OcMotion.of(context);
    final enabled = onChanged != null;
    return Semantics(
      toggled: value,
      enabled: enabled,
      child: Opacity(
        opacity: enabled ? 1 : 0.4,
        child: Hoverable(
          button: false,
          semanticLabel: semanticLabel,
          onTap: enabled ? () => onChanged!(!value) : null,
          focusRadius: BorderRadius.circular(_height / 2),
          builder: (context, state) => AnimatedContainer(
            duration: motion.hover,
            curve: OcMotion.curve,
            width: _width,
            height: _height,
            padding: const EdgeInsets.all((_height - _thumb) / 2),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(_height / 2),
              color: value
                  ? colors.accent
                  : state.active
                  ? colors.border
                  : colors.selected,
            ),
            child: AnimatedAlign(
              duration: motion.hover,
              curve: OcMotion.curve,
              alignment: value ? Alignment.centerRight : Alignment.centerLeft,
              child: Container(
                width: _thumb,
                height: _thumb,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: value ? colors.onAccent : colors.textSecondary,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
