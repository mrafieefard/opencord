import 'dart:async';

import 'package:opencord/features/media_window/media_window_service.dart';
import 'package:opencord/features/window/native_window.dart';

/// A [MediaWindowService] that records what the app asks of it, as
/// `method` or `method:argument`, and lets a test play the runner's part.
class FakeMediaWindow implements MediaWindowService {
  final calls = <String>[];

  /// What the next [open] answers; null means there is no window.
  int? nextViewId = 7;

  /// Runs when the app closes the window, before it is recorded.
  void Function()? onClose;

  final _close = StreamController<void>.broadcast();
  final _status = StreamController<MediaWindowStatus>.broadcast();
  final _geometry = StreamController<WindowGeometry>.broadcast();

  void requestClose() => _close.add(null);

  void sendStatus(MediaWindowStatus status) => _status.add(status);

  void sendGeometry(WindowGeometry geometry) => _geometry.add(geometry);

  @override
  Future<int?> open(MediaWindowPlacement placement) async {
    calls.add('open:${placement.mode.name}');
    return nextViewId;
  }

  @override
  Future<void> close() async {
    onClose?.call();
    calls.add('close');
  }

  @override
  Future<void> setMode(MediaWindowMode mode) async =>
      calls.add('setMode:${mode.name}');

  @override
  Future<void> minimize() async => calls.add('minimize');

  @override
  Future<void> focus() async => calls.add('focus');

  @override
  Future<void> startDrag() async => calls.add('startDrag');

  @override
  Future<void> startResize(ResizeEdge edge) async =>
      calls.add('startResize:${edge.name}');

  @override
  Stream<void> get closeRequests => _close.stream;

  @override
  Stream<MediaWindowStatus> get status => _status.stream;

  @override
  Stream<WindowGeometry> get geometry => _geometry.stream;
}
