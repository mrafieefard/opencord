import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// A row of choices where one is picked, shown inverted (§5.3 "active
/// toggle"): expiry times, use limits, settings with a few values.
class ChoiceChips<T> extends StatelessWidget {
  const ChoiceChips({
    super.key,
    required this.options,
    required this.value,
    required this.onChanged,
    this.iconOf,
  });

  final List<(T, String)> options;
  final T value;
  final ValueChanged<T> onChanged;

  /// An icon before an option's label, when it has one.
  final IconData? Function(T option)? iconOf;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Wrap(
      spacing: OcSpace.s6,
      runSpacing: OcSpace.s6,
      children: [
        for (final (option, label) in options)
          Hoverable(
            onTap: () => onChanged(option),
            semanticLabel: label,
            selected: option == value,
            focusRadius: BorderRadius.circular(OcRadius.reaction + 4),
            builder: (context, state) {
              final selected = option == value;
              return AnimatedContainer(
                duration: OcMotion.of(context).hover,
                padding: const EdgeInsets.symmetric(
                  horizontal: OcSpace.s12,
                  vertical: OcSpace.s6,
                ),
                decoration: BoxDecoration(
                  color: selected
                      ? colors.accent
                      : state.active
                      ? colors.hover
                      : null,
                  borderRadius: BorderRadius.circular(OcRadius.reaction + 4),
                  border: Border.all(
                    color: selected ? colors.accent : colors.border,
                  ),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (iconOf?.call(option) case final icon?) ...[
                      Icon(
                        icon,
                        size: 14,
                        color: selected
                            ? colors.onAccent
                            : colors.textSecondary,
                      ),
                      const SizedBox(width: OcSpace.s4),
                    ],
                    Flexible(
                      child: Text(
                        label,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: OcText.small.copyWith(
                          fontWeight: FontWeight.w600,
                          color: selected ? colors.onAccent : colors.text,
                        ),
                      ),
                    ),
                  ],
                ),
              );
            },
          ),
      ],
    );
  }
}
