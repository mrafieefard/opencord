import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/shell_layout.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/motion/spring_value.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/oc_float.dart';

/// What the shell's floats lay themselves out by.
class ShellMetrics extends InheritedWidget {
  const ShellMetrics({
    super.key,
    required this.mainInner,
    required this.notch,
    required this.layout,
    required super.child,
  });

  /// The narrowest the page's content may be laid out (§4.4).
  static const double _mainInnerMin = 240;

  /// The width inside the main float's border once what's moving has stopped.
  final double mainInner;

  /// The notch's side now. It follows the spring without a rebuild.
  final ValueListenable<double> notch;

  final ShellLayout layout;

  /// [mainInner] for a window of [window] wide, with a left column of [left]
  /// and a side slot of [side] beside main, 0 for each that isn't there.
  static double mainInnerFor({
    required double window,
    required double left,
    required double side,
  }) {
    double part(double width) => width > 0 ? width + OcFloats.gap : 0;
    return math.max(
      _mainInnerMin,
      window -
          2 * OcFloats.gap -
          part(OcFloats.rail) -
          part(left) -
          part(side) -
          2,
    );
  }

  static ShellMetrics? maybeOf(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<ShellMetrics>();

  @override
  bool updateShouldNotify(ShellMetrics oldWidget) =>
      mainInner != oldWidget.mainInner ||
      notch != oldWidget.notch ||
      layout != oldWidget.layout;
}

/// The right pane's place (§5.13): [width] wide and [child] in it while
/// [open], nothing while shut.
///
/// The slot's frame and the gap before it move; the float inside keeps its
/// width from the first frame and lies past the frame's right edge, so it
/// slides in from behind the window's edge instead of being squeezed.
class SideSlot extends StatelessWidget {
  const SideSlot({
    super.key,
    required this.open,
    required this.width,
    required this.child,
  });

  final bool open;
  final double width;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    // Shrinking between two open widths is still the opening's curve, as in
    // the prototype; only shutting is faster.
    final falling = open ? OcSpring.pane : OcSpring.paneOut;
    return SpringValue(
      target: open ? OcFloats.gap : 0,
      spring: OcSpring.pane,
      falling: falling,
      builder: (context, gap, slot) => Padding(
        padding: EdgeInsets.only(left: gap),
        child: slot,
      ),
      child: SpringValue(
        target: open ? width : 0,
        spring: OcSpring.pane,
        falling: falling,
        builder: (context, slot, float) => SizedBox(
          key: ShellKeys.side,
          width: slot,
          child: open || slot > 0 ? float : null,
        ),
        child: OverflowBox(
          alignment: Alignment.topLeft,
          minWidth: width,
          maxWidth: width,
          child: KeyedSubtree(
            key: ShellKeys.panel,
            child: OcFloat(color: context.oc.sidebar, child: child),
          ),
        ),
      ),
    );
  }
}

/// Lays its child out at the width main will end at (§4.4): the text inside
/// jumps once, on the first frame, and the frame then slides over it.
class MainInnerWidth extends StatelessWidget {
  const MainInnerWidth({super.key, required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) {
    final metrics = ShellMetrics.maybeOf(context);
    if (metrics == null) return child;
    return OverflowBox(
      alignment: Alignment.topLeft,
      minWidth: metrics.mainInner,
      maxWidth: metrics.mainInner,
      child: child,
    );
  }
}
