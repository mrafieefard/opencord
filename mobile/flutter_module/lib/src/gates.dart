import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/scheduler.dart';

/// Stage A's measurements (plugins M4 plan, Task 6), compiled in only with
/// `--dart-define=PLUGIN_GATES=true`. They print to the log as
/// `plugin-gate …` lines; nothing about a view's content.
const gatesOn = bool.fromEnvironment('PLUGIN_GATES');

class Gates {
  Gates._();

  static final _sent = Stopwatch();
  static var _waiting = false;
  static var _frames = 0;
  static var _started = false;

  /// An input went to native code: a press starts the round trip.
  static void inputSent(String json) {
    if (!gatesOn) return;
    final kind = (jsonDecode(json) as Map<String, Object?>)['kind'];
    if (kind != 'event') return;
    _sent
      ..reset()
      ..start();
    _waiting = true;
  }

  /// A batch was applied: ends the round trip a press started.
  static void batchApplied() {
    if (!gatesOn || !_waiting) return;
    _waiting = false;
    debugPrint('plugin-gate rtt_us=${_sent.elapsedMicroseconds}');
  }

  /// Counts frames, and prints the rate every ten seconds.
  static void startFrameCount() {
    if (!gatesOn || _started) return;
    _started = true;
    SchedulerBinding.instance.addTimingsCallback((List<FrameTiming> timings) {
      _frames += timings.length;
    });
    final clock = Stopwatch()..start();
    void report() {
      Future<void>.delayed(const Duration(seconds: 10), () {
        final seconds = clock.elapsedMilliseconds / 1000;
        debugPrint('plugin-gate fps=${(_frames / seconds).toStringAsFixed(1)} frames=$_frames');
        _frames = 0;
        clock.reset();
        report();
      });
    }

    report();
  }
}
