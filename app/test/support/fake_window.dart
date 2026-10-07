import 'dart:async';

import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_mode.dart';

/// Records what the app asks of the native window.
class FakeNativeWindow implements NativeWindow {
  final calls = <String>[];
  final statusEvents = StreamController<WindowStatus>.broadcast();
  final geometryEvents = StreamController<WindowGeometry>.broadcast();
  final layoutEvents = StreamController<ButtonLayout>.broadcast();
  final closeEvents = StreamController<void>.broadcast();
  final captionEvents = StreamController<CaptionHover>.broadcast();
  final linkEvents = StreamController<String>.broadcast();

  @override
  Stream<WindowStatus> get status => statusEvents.stream;

  @override
  Stream<WindowGeometry> get geometry => geometryEvents.stream;

  @override
  Stream<ButtonLayout> get buttonLayout => layoutEvents.stream;

  @override
  Stream<void> get closeRequests => closeEvents.stream;

  @override
  Stream<String> get links => linkEvents.stream;

  @override
  Stream<CaptionHover> get maximizeHover => captionEvents.stream;

  @override
  Future<WindowInfo> configure({
    required WindowChrome chrome,
    required int background,
    required double minWidth,
    required double minHeight,
    required double frameMargin,
    required double resizeBand,
    WindowGeometry? restore,
    bool interceptClose = false,
    bool hidden = false,
  }) async {
    calls.add('configure:${chrome.name}${hidden ? ':hidden' : ''}');
    return WindowInfo.none;
  }

  @override
  Future<void> setTitle(String title) async => calls.add('setTitle:$title');

  @override
  Future<void> startDrag() async => calls.add('startDrag');

  @override
  Future<void> startResize(ResizeEdge edge) async =>
      calls.add('startResize:${edge.name}');

  @override
  Future<void> showWindowMenu() async => calls.add('showWindowMenu');

  @override
  Future<void> minimize() async => calls.add('minimize');

  @override
  Future<void> toggleMaximize() async => calls.add('toggleMaximize');

  @override
  Future<void> setFullscreen(bool fullscreen) async =>
      calls.add('setFullscreen:$fullscreen');

  @override
  Future<void> close() async => calls.add('close');

  @override
  Future<void> quit() async => calls.add('quit');

  @override
  Future<void> hide() async => calls.add('hide');

  @override
  Future<void> show() async => calls.add('show');

  @override
  Future<void> setUrgent(bool urgent) async => calls.add('setUrgent:$urgent');
}
