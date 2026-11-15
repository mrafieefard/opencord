import 'package:flutter/scheduler.dart';

import 'controller.dart';

/// Sends a view's `frame` inputs while its frames are on (`app.on_frame`,
/// design §12.6): one per vsync, each naming the last batch seen, and none
/// again until a batch newer than the one the last frame named arrives, so
/// a slow script never has frames queued for it. `dt` is the time since the
/// frame before, in seconds; the first frame after frames turn on says 0.
///
/// A frame the script answers without changing anything brings no batch, so
/// after [patience] with none the next vsync sends a frame anyway, naming the
/// same batch. The ticker is muted while the view is hidden ([TickerMode]),
/// so a hidden view gets no frames.
class FrameLoop {
  FrameLoop(this.controller, TickerProvider vsync) {
    _ticker = vsync.createTicker(_tick);
    controller.addListener(_sync);
    _sync();
  }

  /// A handler's time limit (`opencord_plugins::limits::VIEW_HANDLER_TIME`):
  /// a frame still unanswered after it drew nothing, or its script is far
  /// over the frame budget.
  static const patience = Duration(milliseconds: 50);

  final BlocksController controller;
  late final Ticker _ticker;
  Duration? _sentAt;
  var _named = 0;

  void _sync() {
    final on = controller.framesOn;
    if (on == _ticker.isActive) return;
    if (on) {
      _sentAt = null;
      _ticker.start();
    } else {
      _ticker.stop();
    }
  }

  void _tick(Duration elapsed) {
    final sentAt = _sentAt;
    final seen = controller.seq;
    if (sentAt != null && seen <= _named && elapsed - sentAt < patience) {
      return;
    }
    _sentAt = elapsed;
    _named = seen;
    final dt = sentAt == null ? 0.0 : (elapsed - sentAt).inMicroseconds / 1e6;
    controller.send({'kind': 'frame', 'dt': dt, 'seq': seen});
  }

  void dispose() {
    controller.removeListener(_sync);
    _ticker.dispose();
  }
}
