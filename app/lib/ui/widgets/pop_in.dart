import 'package:flutter/material.dart';

import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_motion.dart';

class _ClampedCurve extends Curve {
  const _ClampedCurve(this.curve);

  final Curve curve;

  @override
  double transformInternal(double t) => curve.transform(t).clamp(0.0, 1.0);
}

/// The menu opening of the floating UI (§4.3): scale 0.97 to 1 from a
/// corner, a two-pixel slide, and opacity on the same curve, on `pop`.
class PopTransition extends StatelessWidget {
  const PopTransition({
    super.key,
    required this.animation,
    required this.alignment,
    required this.rise,
    required this.child,
  });

  final Animation<double> animation;

  /// Read every frame: a route's layout finds its corner after the first
  /// frame is built, while the popup is still transparent.
  final ValueGetter<Alignment> alignment;

  /// Where it starts from vertically: −2 for what opens downward, 2 for
  /// what opens upward.
  final double rise;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final curved = CurvedAnimation(
      parent: animation,
      curve: OcSpring.pop.curve,
    );
    final opacity = CurvedAnimation(
      parent: animation,
      curve: _ClampedCurve(OcSpring.pop.curve),
    );
    final scale = Tween<double>(begin: 0.97, end: 1.0).animate(curved);
    return AnimatedBuilder(
      animation: curved,
      builder: (context, child) => Transform.translate(
        offset: Offset(0, rise * (1.0 - curved.value)),
        child: ScaleTransition(
          alignment: alignment(),
          scale: scale,
          child: child,
        ),
      ),
      child: FadeTransition(opacity: opacity, child: child),
    );
  }
}

/// Opens its child with [PopTransition] once, when it is inserted. It
/// closes at once, by being removed. Nothing animates under reduced motion.
class PopIn extends StatefulWidget {
  const PopIn({
    super.key,
    required this.child,
    this.alignment = Alignment.bottomLeft,
    this.rise = 2,
  });

  final Widget child;
  final Alignment alignment;
  final double rise;

  @override
  State<PopIn> createState() => _PopInState();
}

class _PopInState extends State<PopIn> with SingleTickerProviderStateMixin {
  late final _controller = AnimationController(
    vsync: this,
    duration: OcSpring.pop.settle,
  );
  var _started = false;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_started) return;
    _started = true;
    if (OcMotion.of(context).reduced) {
      _controller.value = 1;
    } else {
      _controller.forward();
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (OcMotion.of(context).reduced) return widget.child;
    return PopTransition(
      animation: _controller,
      alignment: () => widget.alignment,
      rise: widget.rise,
      child: widget.child,
    );
  }
}
