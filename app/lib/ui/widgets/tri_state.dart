import 'package:flutter/material.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// What a channel overwrite says about one permission.
enum TriState {
  deny('Deny', OcIcons.close),
  inherit('Inherit', OcIcons.remove),
  allow('Allow', OcIcons.check);

  const TriState(this.label, this.icon);

  final String label;
  final IconData icon;
}

/// Deny · Inherit · Allow as ✕ / – / ✓, the chosen segment inverted
/// (§8.2). [enabled] says which segments can be chosen.
class TriStateControl extends StatelessWidget {
  const TriStateControl({
    super.key,
    required this.value,
    required this.onChanged,
    this.enabled = const {TriState.deny, TriState.inherit, TriState.allow},
    this.label,
  });

  final TriState value;
  final ValueChanged<TriState>? onChanged;
  final Set<TriState> enabled;

  /// What the control is about, for screen readers.
  final String? label;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(9),
        border: Border.all(color: colors.border),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final state in TriState.values)
            Opacity(
              opacity: onChanged == null || enabled.contains(state) ? 1 : 0.35,
              child: Hoverable(
                onTap: onChanged == null || !enabled.contains(state)
                    ? null
                    : () => onChanged!(state),
                semanticLabel: label == null
                    ? state.label
                    : '${state.label} $label',
                selected: state == value,
                focusRadius: BorderRadius.circular(7),
                builder: (context, hover) {
                  final chosen = state == value;
                  return AnimatedContainer(
                    duration: OcMotion.of(context).hover,
                    width: 30,
                    height: 24,
                    decoration: BoxDecoration(
                      color: chosen
                          ? colors.accent
                          : hover.active
                          ? colors.hover
                          : null,
                      borderRadius: BorderRadius.circular(7),
                    ),
                    child: Icon(
                      state.icon,
                      size: 16,
                      color: chosen ? colors.onAccent : colors.textSecondary,
                    ),
                  );
                },
              ),
            ),
        ],
      ),
    );
  }
}
