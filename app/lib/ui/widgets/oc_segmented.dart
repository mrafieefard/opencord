import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// A segmented control of two or more choices: 28 px high segments, 12 px
/// padding, radius 8, 13/500 text, in a hover pill (§5.13, §8.2).
class OcSegmented<T> extends StatelessWidget {
  const OcSegmented({
    super.key,
    required this.options,
    required this.value,
    required this.onChanged,
    this.keyPrefix,
  });

  final List<(T, String)> options;
  final T? value;
  final ValueChanged<T> onChanged;
  final String? keyPrefix;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(
        color: colors.hover,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (var i = 0; i < options.length; i++) ...[
            if (i > 0) const SizedBox(width: 2),
            OcSegmentedButton(
              key: keyPrefix != null
                  ? ValueKey('$keyPrefix-${options[i].$2.toLowerCase()}')
                  : null,
              label: options[i].$2,
              selected: value == options[i].$1,
              onTap: () => onChanged(options[i].$1),
            ),
          ],
        ],
      ),
    );
  }
}

/// One segment of [OcSegmented]: 28 px high, 12 px horizontal padding,
/// radius 8, 13/500 text (§5.13).
class OcSegmentedButton extends StatelessWidget {
  const OcSegmentedButton({
    super.key,
    required this.label,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final bool selected;
  final VoidCallback onTap;

  bool get active => selected;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: label,
      focusRadius: BorderRadius.circular(8),
      builder: (context, state) {
        final duration = OcMotion.of(context).fade;
        return AnimatedContainer(
          duration: duration,
          curve: Curves.ease,
          height: 28,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(
            color: selected ? colors.accent : null,
            borderRadius: BorderRadius.circular(8),
          ),
          alignment: Alignment.center,
          child: AnimatedDefaultTextStyle(
            duration: duration,
            curve: Curves.ease,
            style: OcText.body.copyWith(
              fontSize: 13,
              fontWeight: FontWeight.w500,
              color: selected ? colors.onAccent : colors.textSecondary,
            ),
            child: Text(label),
          ),
        );
      },
    );
  }
}
