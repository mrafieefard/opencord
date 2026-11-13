import 'dart:async';
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import 'status.dart';

/// How long a host call waits for React Native before it fails. A file
/// picker waits without limit: the viewer may take their time.
const hostCallTimeout = Duration(seconds: 30);

/// The theme, insets and scale React Native gives the area ("The plugin
/// area channel", `setup`).
@immutable
class AreaSetup {
  const AreaSetup({
    required this.dark,
    required this.colors,
    required this.textScale,
    required this.safeArea,
    required this.reduceMotion,
    required this.serverKey,
    required this.pluginId,
  });

  factory AreaSetup.fromJson(Map<String, Object?> json) {
    final safe = json['safe_area'] as Map<String, Object?>? ?? const {};
    double side(String name) => (safe[name] as num?)?.toDouble() ?? 0;
    return AreaSetup(
      dark: json['theme'] != 'light',
      colors: {
        for (final MapEntry(:key, :value)
            in (json['colors'] as Map<String, Object?>? ?? const {}).entries)
          if (value is String && parseColor(value) != null)
            key: parseColor(value)!,
      },
      textScale: (json['text_scale'] as num?)?.toDouble() ?? 1,
      safeArea: EdgeInsets.fromLTRB(
        side('left'),
        side('top'),
        side('right'),
        side('bottom'),
      ),
      reduceMotion: json['reduce_motion'] == true,
      serverKey: json['server_key'] as String? ?? '',
      pluginId: json['plugin_id'] as String? ?? '',
    );
  }

  final bool dark;
  final Map<String, Color> colors;
  final double textScale;
  final EdgeInsets safeArea;
  final bool reduceMotion;
  final String serverKey;
  final String pluginId;
}

/// `#RRGGBB` or `#AARRGGBB`; null for anything else.
Color? parseColor(String text) {
  final hex = text.startsWith('#') ? text.substring(1) : null;
  if (hex == null || (hex.length != 6 && hex.length != 8)) return null;
  final value = int.tryParse(hex, radix: 16);
  if (value == null) return null;
  return Color(hex.length == 6 ? 0xFF000000 | value : value);
}

/// A host call React Native refused: `{"error":"<kind>"}`, or no answer
/// within [hostCallTimeout].
class HostCallError implements Exception {
  const HostCallError(this.kind);
  final String kind;

  @override
  String toString() => 'HostCallError($kind)';
}

/// The Dart end of `dev.opencord/plugin_area` (plugins M4 plan, "The plugin
/// area channel"). Every JSON value crosses as a string.
class AreaChannel {
  AreaChannel({MethodChannel? channel})
    : _channel = channel ?? const MethodChannel(name) {
    _channel.setMethodCallHandler(_handle);
    unawaited(_invoke('ready'));
  }

  static const name = 'dev.opencord/plugin_area';

  final MethodChannel _channel;

  /// Batches wait here until the area listens.
  final _batches = StreamController<String>();
  final _setup = ValueNotifier<AreaSetup?>(null);
  final _waiting = <int, Completer<Object?>>{};
  var _nextCall = 0;
  String? _lastStatus;

  /// Answers the system back: true when the area closed something with it.
  bool Function()? onBack;

  Stream<String> get batches => _batches.stream;
  ValueListenable<AreaSetup?> get setup => _setup;

  void sendInput(String json) => unawaited(_invoke('input', {'json': json}));

  /// Sends [status] unless it is the one sent last.
  void sendStatus(AreaStatus status) {
    final json = jsonEncode(status.toJson());
    if (json == _lastStatus) return;
    _lastStatus = json;
    unawaited(_invoke('status', {'json': json}));
  }

  /// A `BlocksHost` call React Native answers. [wait] null waits without
  /// limit (the file picker).
  Future<Object?> hostCall(
    String method,
    Object? args, {
    Duration? wait = hostCallTimeout,
  }) {
    final id = ++_nextCall;
    final reply = Completer<Object?>();
    _waiting[id] = reply;
    unawaited(
      _invoke('hostCall', {
        'id': id,
        'method': method,
        'json': jsonEncode(args),
      }),
    );
    final answer = wait == null ? reply.future : reply.future.timeout(wait);
    return answer.whenComplete(() => _waiting.remove(id)).catchError(
      (Object _) => throw const HostCallError('timeout'),
      test: (error) => error is TimeoutException,
    );
  }

  Future<Object?> _handle(MethodCall call) async {
    final arguments = call.arguments;
    final json = arguments is Map ? arguments['json'] as String? : null;
    switch (call.method) {
      case 'setup':
        if (json != null) {
          _setup.value = AreaSetup.fromJson(
            jsonDecode(json) as Map<String, Object?>,
          );
        }
      case 'batch':
        if (json != null) _batches.add(json);
      case 'hostReply':
        final reply = _waiting.remove((arguments as Map)['id'] as int);
        if (reply == null || json == null) return null;
        final decoded = jsonDecode(json) as Map<String, Object?>;
        if (decoded.containsKey('error')) {
          reply.completeError(HostCallError('${decoded['error']}'));
        } else {
          reply.complete(decoded['ok']);
        }
      case 'back':
        return onBack?.call() ?? false;
    }
    return null;
  }

  Future<void> _invoke(String method, [Object? arguments]) async {
    try {
      await _channel.invokeMethod<Object?>(method, arguments);
    } on MissingPluginException {
      // No native end (a test without one, or the engine shutting down).
    } on PlatformException catch (error) {
      debugPrint('plugin area: $method failed: ${error.code}');
    }
  }
}
