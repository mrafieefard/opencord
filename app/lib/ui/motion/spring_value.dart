import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:opencord/ui/motion/oc_spring.dart';

/// Springs to [target] and builds with the value of each frame.
///
/// It starts on its first target without moving. A new target takes over from
/// the current value and speed, so changing course never jerks (§4.2). Under
/// reduced motion the value goes straight to the target.
class SpringValue extends StatefulWidget {
  const SpringValue({
    super.key,
    required this.target,
    required this.spring,
    this.falling,
    this.delay = Duration.zero,
    this.fallingDelay = Duration.zero,
    this.immediate = false,
    required ValueWidgetBuilder<double> this.builder,
    this.child,
  }) : listenableBuilder = null;

  /// Builds once with the moving value itself, for what repaints from it, or
  /// hands it on, without a rebuild each frame.
  const SpringValue.listenable({
    super.key,
    required this.target,
    required this.spring,
    this.falling,
    this.delay = Duration.zero,
    this.fallingDelay = Duration.zero,
    this.immediate = false,
    required ValueWidgetBuilder<ValueListenable<double>> builder,
    this.child,
  }) : builder = null,
       listenableBuilder = builder;

  final double target;

  /// Used when the target rises.
  final OcSpring spring;

  /// Used when the target falls. Defaults to [spring].
  final OcSpring? falling;

  /// How long a rise waits before it starts. A spring already moving when the
  /// target changes carries on as it was until then, and turns from there.
  final Duration delay;

  /// The same as [delay], for a fall.
  final Duration fallingDelay;

  /// A new target is reached at once, and stops a spring that was running:
  /// for a value the pointer is moving.
  final bool immediate;

  final ValueWidgetBuilder<double>? builder;
  final ValueWidgetBuilder<ValueListenable<double>>? listenableBuilder;
  final Widget? child;

  @override
  State<SpringValue> createState() => _SpringValueState();
}

class _SpringValueState extends State<SpringValue>
    with SingleTickerProviderStateMixin {
  late final _controller = AnimationController.unbounded(
    vsync: this,
    value: widget.target,
  );

  /// What [_controller] was last given, for a delay to carry on with.
  Simulation? _running;

  bool get _reduced => MediaQuery.disableAnimationsOf(context);

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Reduced motion may have come on in the middle of a spring.
    if (_reduced) _controller.value = widget.target;
  }

  @override
  void didUpdateWidget(SpringValue oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.target == oldWidget.target) return;
    if (_reduced || widget.immediate) {
      _controller.value = widget.target;
      return;
    }
    final rising = widget.target > oldWidget.target;
    final simulation = _towards(
      widget.target,
      rising ? widget.spring : widget.falling ?? widget.spring,
      rising ? widget.delay : widget.fallingDelay,
    );
    _running = simulation;
    _controller.animateWith(simulation);
  }

  Simulation _towards(double target, OcSpring spring, Duration delay) {
    final value = _controller.value;
    if (delay == Duration.zero) {
      return spring.simulation(value, target, _controller.velocity);
    }
    final running = _controller.isAnimating ? _running : null;
    return _Delayed(
      // Not moving, the spring from here to here stays here.
      running ?? spring.simulation(value, value, 0),
      running == null ? 0 : _seconds(_controller.lastElapsedDuration!),
      _seconds(delay),
      spring,
      target,
    );
  }

  static double _seconds(Duration time) =>
      time.inMicroseconds / Duration.microsecondsPerSecond;

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final listenableBuilder = widget.listenableBuilder;
    if (listenableBuilder != null) {
      return listenableBuilder(context, _controller, widget.child);
    }
    return ValueListenableBuilder<double>(
      valueListenable: _controller,
      builder: widget.builder!,
      child: widget.child,
    );
  }
}

/// Whatever was happening carries on for a delay, and a spring then takes over
/// from the value and speed that leaves.
class _Delayed extends Simulation {
  _Delayed(
    this._before,
    this._elapsed,
    this._delay,
    OcSpring spring,
    double target,
  ) : _after = spring.simulation(
        _before.x(_elapsed + _delay),
        target,
        _before.dx(_elapsed + _delay),
      );

  final Simulation _before;

  /// How far into [_before] this starts, in seconds.
  final double _elapsed;

  /// In seconds.
  final double _delay;

  final Simulation _after;

  @override
  double x(double time) =>
      time < _delay ? _before.x(_elapsed + time) : _after.x(time - _delay);

  @override
  double dx(double time) =>
      time < _delay ? _before.dx(_elapsed + time) : _after.dx(time - _delay);

  @override
  bool isDone(double time) => time >= _delay && _after.isDone(time - _delay);
}
