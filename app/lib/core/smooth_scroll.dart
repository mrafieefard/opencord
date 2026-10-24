import 'dart:math' as math;

import 'package:flutter/gestures.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

/// Flutter's binding, with mouse wheel scrolling animated everywhere in the
/// app (see [WheelSmoother]).
class OcBinding extends WidgetsFlutterBinding {
  static OcBinding? _created;

  static WidgetsBinding ensureInitialized() {
    _created ??= OcBinding();
    return WidgetsBinding.instance;
  }

  /// Off with reduce motion (the OS setting or the app's own): each wheel
  /// step then jumps.
  static set smoothWheel(bool on) => _created?._wheel.enabled = on;

  late final _wheel = WheelSmoother(dispatch: _dispatch);

  void _dispatch(PointerEvent event) => super.handlePointerEvent(event);

  @override
  void handlePointerEvent(PointerEvent event) {
    if (!_wheel.take(event)) super.handlePointerEvent(event);
  }
}

/// Spreads each mouse wheel step over a few frames, as browsers and
/// Discord do, instead of jumping the whole step in one. It works on the
/// wheel events themselves, so every scrollable gets it: each frame it
/// sends on a share of what is left, under the pointer of the latest step.
class WheelSmoother {
  WheelSmoother({required this.dispatch});

  final void Function(PointerEvent event) dispatch;

  /// False leaves every event to Flutter.
  var enabled = true;

  /// How fast what is left runs out: about 95% of a step is done after
  /// three times this.
  static const settle = Duration(milliseconds: 45);

  /// What the first frame of a glide counts as, having no frame before it.
  static const _firstFrame = Duration(milliseconds: 16);

  PointerScrollEvent? _latest;
  Offset _left = Offset.zero;
  Duration? _lastFrame;
  bool _scheduled = false;

  /// Takes [event] if it is a mouse wheel step, to send on bit by bit;
  /// false leaves it to the caller. Trackpads already scroll smoothly.
  bool take(PointerEvent event) {
    if (!enabled ||
        event is! PointerScrollEvent ||
        event.kind != PointerDeviceKind.mouse) {
      return false;
    }
    _left = Offset(
      _add(_left.dx, event.scrollDelta.dx),
      _add(_left.dy, event.scrollDelta.dy),
    );
    _latest = event;
    if (!_scheduled) {
      _scheduled = true;
      _lastFrame = null;
      SchedulerBinding.instance.scheduleFrameCallback(_frame);
    }
    return true;
  }

  /// A step the other way drops what is left, so turning back is at once.
  static double _add(double left, double delta) =>
      left * delta < 0 ? delta : left + delta;

  void _frame(Duration now) {
    _scheduled = false;
    final latest = _latest;
    if (latest == null) return;
    final elapsed = now - (_lastFrame ?? now - _firstFrame);
    _lastFrame = now;
    final share = 1 - math.exp(-elapsed.inMicroseconds / settle.inMicroseconds);
    var step = _left * share;
    if ((_left - step).distance < 0.5) step = _left;
    _left -= step;
    if (_left == Offset.zero) {
      _latest = null;
    } else {
      _scheduled = true;
      SchedulerBinding.instance.scheduleFrameCallback(_frame);
    }
    dispatch(
      PointerScrollEvent(
        viewId: latest.viewId,
        timeStamp: latest.timeStamp,
        kind: latest.kind,
        device: latest.device,
        position: latest.position,
        scrollDelta: step,
        embedderId: latest.embedderId,
      ),
    );
  }
}
