import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/ui/widgets/oc_segmented.dart';

export 'package:opencord/ui/widgets/oc_segmented.dart'
    show OcSegmented, OcSegmentedButton;

/// Retained for test compatibility with existing finders.
typedef PaneSwitchButton = OcSegmentedButton;

/// The switch in the side slot's header: People · Music (§5.13).
class PaneSwitch extends ConsumerWidget {
  const PaneSwitch({
    super.key = const GlobalObjectKey('pane-switch'),
    this.mode,
    this.onChanged,
  });

  final RightPanelMode? mode;
  final ValueChanged<RightPanelMode>? onChanged;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final current = mode ?? ref.watch(rightPanelModeProvider);

    void select(RightPanelMode next) {
      if (onChanged != null) {
        onChanged!(next);
      } else {
        ref.read(rightPanelModeProvider.notifier).set(next);
      }
    }

    return OcSegmented<RightPanelMode>(
      keyPrefix: 'pane-switch',
      value: current,
      options: const [
        (RightPanelMode.people, 'People'),
        (RightPanelMode.music, 'Music'),
      ],
      onChanged: select,
    );
  }
}

/// The fade of a pane that has just been swapped in (§5.13), for the part
/// under its header. The header stays: the switch in it has its own motion.
class PaneSwapFade extends InheritedWidget {
  const PaneSwapFade({super.key, required this.opacity, required super.child});

  final Animation<double> opacity;

  @override
  bool updateShouldNotify(PaneSwapFade oldWidget) =>
      opacity != oldWidget.opacity;
}

/// What a pane has under its header, fading with [PaneSwapFade].
class PaneSwapBody extends StatelessWidget {
  const PaneSwapBody({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final fade = context.dependOnInheritedWidgetOfExactType<PaneSwapFade>();
    if (fade == null) return child;
    return FadeTransition(opacity: fade.opacity, child: child);
  }
}
