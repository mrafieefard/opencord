import 'package:flutter/widgets.dart';

import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

/// The width of a float's border line.
const double _line = 1;

/// A float (the floating UI's design, §2.3): a surface with a 1 px `border`
/// inside its box, rounded corners and the float shadow. Its [child] sits
/// inside the border, clipped to the border's inner edge.
class OcFloat extends StatelessWidget {
  const OcFloat({
    super.key,
    required this.child,
    this.color,
    this.radius = OcFloats.radius,
    this.duration,
  });

  final Widget child;

  /// The surface; `sidebar` when null.
  final Color? color;

  /// The outer corner radius; the content is clipped one less.
  final double radius;

  /// Optional duration for animating color changes.
  final Duration? duration;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final decoration = BoxDecoration(
      color: color ?? colors.sidebar,
      border: Border.all(color: colors.border, width: _line),
      borderRadius: BorderRadius.circular(radius),
      boxShadow: OcShadows.float(colors),
    );
    final inner = Padding(
      padding: const EdgeInsets.all(_line),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(radius - _line),
        child: child,
      ),
    );
    if (duration != null) {
      return AnimatedContainer(
        duration: duration!,
        curve: Curves.easeOut,
        decoration: decoration,
        child: inner,
      );
    }
    return DecoratedBox(decoration: decoration, child: inner);
  }
}
