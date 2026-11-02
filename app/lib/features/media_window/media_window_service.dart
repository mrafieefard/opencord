import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/window/native_window.dart';

/// How the media viewer's window sits on the screen, as Telegram's viewer
/// does (D86): covering the whole monitor, or a window of its own size.
enum MediaWindowMode { fullscreen, normal }

/// The viewer window's last mode and normal size, kept between openings.
@immutable
class MediaWindowPlacement {
  const MediaWindowPlacement({
    this.mode = MediaWindowMode.fullscreen,
    this.normal,
  });

  /// Anything unreadable opens full screen, as the first time does.
  static MediaWindowPlacement fromJson(Object? json) {
    if (json is! Map) return const MediaWindowPlacement();
    final mode = MediaWindowMode.values
        .where((mode) => mode.name == json['mode'])
        .firstOrNull;
    if (mode == null) return const MediaWindowPlacement();
    return MediaWindowPlacement(
      mode: mode,
      normal: WindowGeometry.fromJson(json),
    );
  }

  final MediaWindowMode mode;

  /// The normal window's size and place, once it has had one.
  final WindowGeometry? normal;

  MediaWindowPlacement copyWith({
    MediaWindowMode? mode,
    WindowGeometry? normal,
  }) => MediaWindowPlacement(
    mode: mode ?? this.mode,
    normal: normal ?? this.normal,
  );

  Map<String, Object?> toJson() => {
    'mode': mode.name,
    if (normal case final normal?) ...{
      'width': normal.width,
      'height': normal.height,
      'x': ?normal.x,
      'y': ?normal.y,
    },
  };

  @override
  bool operator ==(Object other) =>
      other is MediaWindowPlacement &&
      other.mode == mode &&
      other.normal == normal;

  @override
  int get hashCode => Object.hash(mode, normal);
}

/// The viewer window's state as the runner reports it.
@immutable
class MediaWindowStatus {
  const MediaWindowStatus({
    this.mode = MediaWindowMode.fullscreen,
    this.maximized = false,
    this.focused = true,
  });

  /// Linux reports "maximized" for a window the desktop maximized: a
  /// normal window, as far as the viewer's rules go.
  factory MediaWindowStatus.fromMap(Object? map) {
    if (map is! Map) return const MediaWindowStatus();
    final mode = map['mode'];
    return MediaWindowStatus(
      mode: mode == 'fullscreen'
          ? MediaWindowMode.fullscreen
          : MediaWindowMode.normal,
      maximized: mode == 'maximized',
      focused: map['focused'] != false,
    );
  }

  final MediaWindowMode mode;
  final bool maximized;
  final bool focused;

  @override
  bool operator ==(Object other) =>
      other is MediaWindowStatus &&
      other.mode == mode &&
      other.maximized == maximized &&
      other.focused == focused;

  @override
  int get hashCode => Object.hash(mode, maximized, focused);
}

/// The media viewer's own OS window (D86): a second Flutter view on the
/// running engine, in a see-through window the runner makes.
abstract interface class MediaWindowService {
  /// Opens the window; answers the new Flutter view's id, or null where
  /// there is no such window (the viewer then shows inside the app).
  Future<int?> open(MediaWindowPlacement placement);

  Future<void> close();

  Future<void> setMode(MediaWindowMode mode);

  Future<void> minimize();

  /// Brings the window forward and gives it the keyboard.
  Future<void> focus();

  Future<void> startDrag();

  Future<void> startResize(ResizeEdge edge);

  /// The system's close (Alt+F4, the taskbar's or Dock's Close).
  Stream<void> get closeRequests;

  Stream<MediaWindowStatus> get status;

  /// The normal window's size and place as it moves.
  Stream<WindowGeometry> get geometry;
}

/// [MediaWindowService] over the runners' `dev.opencord/media_window`
/// channel.
class ChannelMediaWindow implements MediaWindowService {
  ChannelMediaWindow([
    this._channel = const MethodChannel('dev.opencord/media_window'),
  ]) {
    _channel.setMethodCallHandler(_handle);
  }

  final MethodChannel _channel;
  final _close = StreamController<void>.broadcast();
  final _status = StreamController<MediaWindowStatus>.broadcast();
  final _geometry = StreamController<WindowGeometry>.broadcast();

  Future<void> _handle(MethodCall call) async {
    switch (call.method) {
      case 'closeRequested':
        _close.add(null);
      case 'status':
        _status.add(MediaWindowStatus.fromMap(call.arguments));
      case 'geometry':
        final geometry = WindowGeometry.fromJson(call.arguments);
        if (geometry != null) _geometry.add(geometry);
    }
  }

  Future<T?> _invoke<T>(String method, [Object? arguments]) async {
    try {
      return await _channel.invokeMethod<T>(method, arguments);
    } on MissingPluginException {
      return null;
    } on PlatformException {
      return null;
    }
  }

  @override
  Future<int?> open(MediaWindowPlacement placement) => _invoke<int>('open', {
    ...placement.toJson(),
    // Windows finds the engine to add the view to by its id.
    'engineId': PlatformDispatcher.instance.engineId,
  });

  @override
  Future<void> close() => _invoke<void>('close');

  @override
  Future<void> setMode(MediaWindowMode mode) =>
      _invoke<void>('setMode', mode.name);

  @override
  Future<void> minimize() => _invoke<void>('minimize');

  @override
  Future<void> focus() => _invoke<void>('focus');

  @override
  Future<void> startDrag() => _invoke<void>('startDrag');

  @override
  Future<void> startResize(ResizeEdge edge) =>
      _invoke<void>('startResize', edge.name);

  @override
  Stream<void> get closeRequests => _close.stream;

  @override
  Stream<MediaWindowStatus> get status => _status.stream;

  @override
  Stream<WindowGeometry> get geometry => _geometry.stream;
}

/// No window of its own: tests, and where the runner has none.
class NullMediaWindow implements MediaWindowService {
  const NullMediaWindow();

  @override
  Future<int?> open(MediaWindowPlacement placement) async => null;

  @override
  Future<void> close() async {}

  @override
  Future<void> setMode(MediaWindowMode mode) async {}

  @override
  Future<void> minimize() async {}

  @override
  Future<void> focus() async {}

  @override
  Future<void> startDrag() async {}

  @override
  Future<void> startResize(ResizeEdge edge) async {}

  @override
  Stream<void> get closeRequests => const Stream.empty();

  @override
  Stream<MediaWindowStatus> get status => const Stream.empty();

  @override
  Stream<WindowGeometry> get geometry => const Stream.empty();
}

/// The viewer's window; the channel version at startup on the desktop.
final mediaWindowServiceProvider = Provider<MediaWindowService>(
  (ref) => const NullMediaWindow(),
);
