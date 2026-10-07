import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import 'package:opencord/features/window/window_mode.dart';

enum ResizeEdge {
  top,
  bottom,
  left,
  right,
  topLeft,
  topRight,
  bottomLeft,
  bottomRight,
}

/// The Windows maximize button's pointer state. Over that button the
/// window manager handles the pointer (for snap layouts), so the runner
/// reports hover and press instead of Flutter seeing them.
enum CaptionHover { none, hover, pressed }

/// The window's state as the platform reports it.
@immutable
class WindowStatus {
  const WindowStatus({
    this.maximized = false,
    this.fullscreen = false,
    this.tiled = false,
    this.focused = true,
  });

  factory WindowStatus.fromMap(Object? map) {
    if (map is! Map) return const WindowStatus();
    bool flag(String key, bool fallback) =>
        map[key] is bool ? map[key]! as bool : fallback;
    return WindowStatus(
      maximized: flag('maximized', false),
      fullscreen: flag('fullscreen', false),
      tiled: flag('tiled', false),
      focused: flag('focused', true),
    );
  }

  final bool maximized;
  final bool fullscreen;
  final bool tiled;
  final bool focused;

  /// Normal windows get rounded corners and resize borders; maximized,
  /// tiled and fullscreen ones do not (§3.1).
  bool get floating => !maximized && !fullscreen && !tiled;

  @override
  bool operator ==(Object other) =>
      other is WindowStatus &&
      other.maximized == maximized &&
      other.fullscreen == fullscreen &&
      other.tiled == tiled &&
      other.focused == focused;

  @override
  int get hashCode => Object.hash(maximized, fullscreen, tiled, focused);
}

/// Size and place of the visible window, for restoring it (§3.1).
@immutable
class WindowGeometry {
  const WindowGeometry({
    required this.width,
    required this.height,
    this.x,
    this.y,
    this.maximized = false,
  });

  static WindowGeometry? fromJson(Object? json) {
    if (json is! Map) return null;
    final width = json['width'];
    final height = json['height'];
    if (width is! int || height is! int) return null;
    return WindowGeometry(
      width: width,
      height: height,
      x: json['x'] is int ? json['x'] as int : null,
      y: json['y'] is int ? json['y'] as int : null,
      maximized: json['maximized'] == true,
    );
  }

  final int width;
  final int height;

  /// Unknown on Wayland, where windows cannot see their position.
  final int? x;
  final int? y;
  final bool maximized;

  WindowGeometry copyWith({
    int? width,
    int? height,
    int? Function()? x,
    int? Function()? y,
    bool? maximized,
  }) => WindowGeometry(
    width: width ?? this.width,
    height: height ?? this.height,
    x: x == null ? this.x : x(),
    y: y == null ? this.y : y(),
    maximized: maximized ?? this.maximized,
  );

  Map<String, Object?> toJson() => {
    'width': width,
    'height': height,
    'x': ?x,
    'y': ?y,
    'maximized': maximized,
  };

  @override
  bool operator ==(Object other) =>
      other is WindowGeometry &&
      other.width == width &&
      other.height == height &&
      other.x == x &&
      other.y == y &&
      other.maximized == maximized;

  @override
  int get hashCode => Object.hash(width, height, x, y, maximized);
}

/// What the native window was created with.
@immutable
class WindowInfo {
  const WindowInfo({
    this.chrome = WindowChrome.system,
    this.transparent = false,
    this.frameMargin = 0,
    this.buttonLayout = ButtonLayout.gnomeDefault,
    this.status = const WindowStatus(),
  });

  /// Used where there is no native window to talk to (tests, unsupported
  /// platforms): the platform draws everything.
  static const none = WindowInfo();

  final WindowChrome chrome;

  /// Whether the window can have transparent, rounded corners.
  final bool transparent;

  /// Transparent space around the visible window for the frame's shadow.
  final double frameMargin;
  final ButtonLayout buttonLayout;
  final WindowStatus status;
}

/// The platform window (§3.1, §15): title, drag, resize, controls, state.
abstract interface class NativeWindow {
  Stream<WindowStatus> get status;
  Stream<WindowGeometry> get geometry;
  Stream<ButtonLayout> get buttonLayout;
  Stream<void> get closeRequests;

  Stream<CaptionHover> get maximizeHover;

  /// `opencord://` links from later launches of the app (§15), which hand
  /// them over and quit.
  Stream<String> get links;

  /// [chrome] is what the app wants. Windows applies it at once; on Linux
  /// it takes effect at the next start, and the returned info says what the
  /// window has now. [hidden] keeps the window from showing with the first
  /// frame (start minimized to the tray).
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
  });

  Future<void> setTitle(String title);

  Future<void> startDrag();

  Future<void> startResize(ResizeEdge edge);

  Future<void> showWindowMenu();

  Future<void> minimize();

  Future<void> toggleMaximize();

  Future<void> setFullscreen(bool fullscreen);

  /// Asks to close; the app may keep running in the tray.
  Future<void> close();

  /// Closes for good.
  Future<void> quit();

  Future<void> hide();

  Future<void> show();

  Future<void> setUrgent(bool urgent);

  /// Starts the app at login, on Windows (the Run key) and macOS (a login
  /// item). Linux writes an autostart entry instead.
  Future<void> setLaunchAtLogin(bool enabled);
}

/// [NativeWindow] over the `dev.opencord/window` method channel of the
/// platform runners. Calls do nothing where a runner does not implement
/// them.
class ChannelNativeWindow implements NativeWindow {
  ChannelNativeWindow([
    this._channel = const MethodChannel('dev.opencord/window'),
  ]) {
    _channel.setMethodCallHandler(_handle);
  }

  final MethodChannel _channel;
  final _status = StreamController<WindowStatus>.broadcast();
  final _geometry = StreamController<WindowGeometry>.broadcast();
  final _layout = StreamController<ButtonLayout>.broadcast();
  final _close = StreamController<void>.broadcast();
  final _caption = StreamController<CaptionHover>.broadcast();
  late final _links = StreamController<String>.broadcast(
    onListen: () => scheduleMicrotask(_flushLinks),
  );

  /// Links that came before anyone listened (macOS hands over the launch
  /// link as soon as the window is configured).
  final _unheardLinks = <String>[];

  void _flushLinks() {
    for (final link in _unheardLinks) {
      _links.add(link);
    }
    _unheardLinks.clear();
  }

  Future<void> _handle(MethodCall call) async {
    switch (call.method) {
      case 'status':
        _status.add(WindowStatus.fromMap(call.arguments));
      case 'geometry':
        final geometry = WindowGeometry.fromJson(call.arguments);
        if (geometry != null) _geometry.add(geometry);
      case 'buttonLayout':
        _layout.add(ButtonLayout.parse('${call.arguments}'));
      case 'openLink':
        final link = '${call.arguments}';
        _links.hasListener ? _links.add(link) : _unheardLinks.add(link);
      case 'closeRequested':
        _close.add(null);
      case 'caption':
        _caption.add(
          CaptionHover.values
                  .where((c) => c.name == call.arguments)
                  .firstOrNull ??
              CaptionHover.none,
        );
    }
  }

  Future<T?> _invoke<T>(String method, [Object? arguments]) async {
    try {
      return await _channel.invokeMethod<T>(method, arguments);
    } on MissingPluginException {
      return null;
    }
  }

  @override
  Stream<WindowStatus> get status => _status.stream;

  @override
  Stream<WindowGeometry> get geometry => _geometry.stream;

  @override
  Stream<ButtonLayout> get buttonLayout => _layout.stream;

  @override
  Stream<void> get closeRequests => _close.stream;

  @override
  Stream<CaptionHover> get maximizeHover => _caption.stream;

  @override
  Stream<String> get links => _links.stream;

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
    final info = await _invoke<Map<Object?, Object?>>('configure', {
      'hidden': hidden,
      'chrome': chrome.name,
      'background': background,
      'minWidth': minWidth.round(),
      'minHeight': minHeight.round(),
      'frameMargin': frameMargin.round(),
      'resizeBand': resizeBand.round(),
      'interceptClose': interceptClose,
      ...?restore?.toJson(),
    });
    if (info == null) return WindowInfo.none;
    final applied = WindowChrome.values.where((c) => c.name == info['chrome']);
    return WindowInfo(
      chrome: applied.firstOrNull ?? WindowChrome.system,
      transparent: info['transparent'] == true,
      frameMargin: (info['frameMargin'] as int? ?? 0).toDouble(),
      buttonLayout: ButtonLayout.parse('${info['buttonLayout'] ?? ''}'),
      status: WindowStatus.fromMap(info['status']),
    );
  }

  @override
  Future<void> setTitle(String title) => _invoke<void>('setTitle', title);

  @override
  Future<void> startDrag() => _invoke<void>('startDrag');

  @override
  Future<void> startResize(ResizeEdge edge) =>
      _invoke<void>('startResize', edge.name);

  @override
  Future<void> showWindowMenu() => _invoke<bool>('showWindowMenu');

  @override
  Future<void> minimize() => _invoke<void>('minimize');

  @override
  Future<void> toggleMaximize() => _invoke<void>('toggleMaximize');

  @override
  Future<void> setFullscreen(bool fullscreen) =>
      _invoke<void>('setFullscreen', fullscreen);

  @override
  Future<void> close() => _invoke<void>('close');

  @override
  Future<void> quit() => _invoke<void>('quit');

  @override
  Future<void> hide() => _invoke<void>('hide');

  @override
  Future<void> show() => _invoke<void>('show');

  @override
  Future<void> setUrgent(bool urgent) => _invoke<void>('setUrgent', urgent);

  @override
  Future<void> setLaunchAtLogin(bool enabled) =>
      _invoke<void>('setLaunchAtLogin', enabled);
}

/// A window nobody draws for: where there is no native side, as in widget
/// tests.
class NullNativeWindow implements NativeWindow {
  const NullNativeWindow();

  @override
  Stream<WindowStatus> get status => const Stream.empty();

  @override
  Stream<WindowGeometry> get geometry => const Stream.empty();

  @override
  Stream<ButtonLayout> get buttonLayout => const Stream.empty();

  @override
  Stream<void> get closeRequests => const Stream.empty();

  @override
  Stream<CaptionHover> get maximizeHover => const Stream.empty();

  @override
  Stream<String> get links => const Stream.empty();

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
  }) async => WindowInfo.none;

  @override
  Future<void> setTitle(String title) async {}

  @override
  Future<void> startDrag() async {}

  @override
  Future<void> startResize(ResizeEdge edge) async {}

  @override
  Future<void> showWindowMenu() async {}

  @override
  Future<void> minimize() async {}

  @override
  Future<void> toggleMaximize() async {}

  @override
  Future<void> setFullscreen(bool fullscreen) async {}

  @override
  Future<void> close() async {}

  @override
  Future<void> quit() async {}

  @override
  Future<void> hide() async {}

  @override
  Future<void> show() async {}

  @override
  Future<void> setUrgent(bool urgent) async {}

  @override
  Future<void> setLaunchAtLogin(bool enabled) async {}
}
