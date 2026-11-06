import 'dart:math' as math;

import 'package:flutter/animation.dart';
import 'package:flutter/physics.dart';

/// How far from its target a spring counts as settled (§4.2).
const double _settledWithin = 0.004;

/// A spring by duration and bounce (§4.2): mass 1, stiffness `(2π/d)²`, damping
/// `(1−b)·4π/d`, as `SpringDescription.withDurationAndBounce` has it. Only what
/// arrives bounces; layout never overshoots.
class OcSpring {
  const OcSpring(this.seconds, this.bounce)
    : assert(seconds > 0),
      assert(bounce >= 0 && bounce < 0.9);

  /// A side pane opening, the voice card appearing, the left column's width,
  /// drawers opening, the notch re-opening.
  static const pane = OcSpring(0.28, 0);

  /// A side pane closing, the voice card leaving, the notch closing, drawers
  /// closing on the right, the rail popover sliding between icons.
  static const paneOut = OcSpring(0.2, 0);

  /// Menus, popovers, the rail popover's scale, dialogs, the pane button coming
  /// back, the voice controls appearing, the join prompt, a camera's silhouette.
  static const pop = OcSpring(0.2, 0.1);

  /// A tile arriving.
  static const tile = OcSpring(0.3, 0.07);

  /// A tile moving or resizing.
  static const move = OcSpring(0.3, 0);

  static final _settles = Expando<Duration>();
  static final _curves = Expando<Curve>();

  final double seconds;
  final double bounce;

  /// The value at [time], going from 0 at rest to the target 1.
  double valueAt(Duration time) {
    final t = time.inMicroseconds / Duration.microsecondsPerSecond;
    final omega = 2 * math.pi / seconds;
    if (bounce == 0) return 1 - (1 + omega * t) * math.exp(-omega * t);
    final decay = (1 - bounce) * omega;
    final ringing = math.sqrt(omega * omega - decay * decay);
    return 1 -
        math.exp(-decay * t) *
            (math.cos(ringing * t) + decay / ringing * math.sin(ringing * t));
  }

  /// The last moment it is more than 0.4 % from 1, to the millisecond.
  Duration get settle => _settles[this] ??= _findSettle();

  Duration _findSettle() {
    // With a bounce under 0.9, nothing moves that far out after 10 durations.
    final latest = (seconds * 10 * Duration.millisecondsPerSecond).ceil();
    for (var ms = latest; ms > 0; ms--) {
      final time = Duration(milliseconds: ms);
      if ((1 - valueAt(time)).abs() > _settledWithin) return time;
    }
    return Duration.zero;
  }

  /// [valueAt] over [settle], for a route, ending exactly at 1. The same object
  /// each time, so a widget given it again doesn't restart anything.
  Curve get curve => _curves[this] ??= _SpringCurve(this);

  /// This spring from [from] to [to] at [velocity]. It's done where [settle]
  /// says, measured on the distance it has left, and ends exactly on [to].
  SpringSimulation simulation(double from, double to, double velocity) {
    return SpringSimulation(
      SpringDescription.withDurationAndBounce(
        duration: Duration(
          milliseconds: (seconds * Duration.millisecondsPerSecond).round(),
        ),
        bounce: bounce,
      ),
      from,
      to,
      velocity,
      snapToEnd: true,
      tolerance: Tolerance(
        distance: _settledWithin * (to - from).abs(),
        velocity: double.infinity,
      ),
    );
  }
}

class _SpringCurve extends Curve {
  const _SpringCurve(this.spring);

  final OcSpring spring;

  // Curve.transform gives exactly 0 and 1 at its ends.
  @override
  double transformInternal(double t) => spring.valueAt(spring.settle * t);
}
