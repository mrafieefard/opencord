import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// The switch in the side slot's header: People · Music (§5.13).
class PaneSwitch extends ConsumerWidget {
  const PaneSwitch({
    super.key,
    this.mode,
    this.onChanged,
  });

  final RightPanelMode? mode;
  final ValueChanged<RightPanelMode>? onChanged;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final current = mode ?? ref.watch(rightPanelModeProvider);

    void select(RightPanelMode next) {
      if (onChanged != null) {
        onChanged!(next);
      } else {
        ref.read(rightPanelModeProvider.notifier).set(next);
      }
    }

    return Container(
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(
        color: colors.hover,
        borderRadius: BorderRadius.circular(10),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          PaneSwitchButton(
            label: 'People',
            selected: current == RightPanelMode.people,
            onTap: () => select(RightPanelMode.people),
          ),
          const SizedBox(width: 2),
          PaneSwitchButton(
            label: 'Music',
            selected: current == RightPanelMode.music,
            onTap: () => select(RightPanelMode.music),
          ),
        ],
      ),
    );
  }
}

/// One segment of the pane switch: 28 px high, 12 px horizontal padding,
/// radius 8, 13/500 text (§5.13).
class PaneSwitchButton extends StatelessWidget {
  const PaneSwitchButton({
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
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).fade,
        height: 28,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(
          color: selected ? colors.accent : null,
          borderRadius: BorderRadius.circular(8),
        ),
        alignment: Alignment.center,
        child: Text(
          label,
          style: OcText.body.copyWith(
            fontSize: 13,
            fontWeight: FontWeight.w500,
            color: selected ? colors.onAccent : colors.textSecondary,
          ),
        ),
      ),
    );
  }
}
