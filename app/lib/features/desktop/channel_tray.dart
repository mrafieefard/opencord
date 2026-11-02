import 'dart:async';

import 'package:flutter/services.dart';

import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/features/desktop/tray.dart';
import 'package:opencord/features/desktop/tray_icon.dart';
import 'package:opencord/features/desktop/tray_menu.dart';
import 'package:opencord/src/rust/api/logs.dart' show LogLevel;

/// The tray on Windows (the notification area) and macOS (the menu bar),
/// drawn by the runner over the "dev.opencord/tray" channel (§15). The app
/// draws the icon and lays out the menu; the runner shows them, and sends
/// back "activate" for a click on the icon and "menu" with an item's id.
class ChannelTray implements TrayService {
  ChannelTray({
    required this.platform,
    MethodChannel channel = const MethodChannel('dev.opencord/tray'),
  }) : _channel = channel {
    channel.setMethodCallHandler(_fromRunner);
  }

  final TargetPlatform platform;
  final MethodChannel _channel;
  final _actions = StreamController<TrayAction>.broadcast();
  var _available = false;
  TrayState? _shown;
  var _menu = const <TrayMenuItem>[];
  // Talking flips the icon many times a second: each look is drawn once.
  final _drawn = <(Color, bool, TrayVoice), List<Map<String, Object>>>{};

  /// The macOS menu bar takes a template image and tints it itself.
  bool get _template => platform == TargetPlatform.macOS;

  /// Windows picks 16 or 32 px by the display's scale (and 20, 24, 40, 48
  /// in between); the menu bar is 18 pt, at 1× or 2×.
  List<int> get _sizes =>
      _template ? const [18, 36] : const [16, 20, 24, 32, 40, 48];

  @override
  bool get available => _available;

  @override
  Stream<TrayAction> get actions => _actions.stream;

  @override
  Future<void> show(TrayState state) async {
    if (state == _shown) return;
    _shown = state;
    _menu = trayMenu(state);
    // A template image is tinted by macOS from its alpha alone.
    final color = trayMarkColor(dark: !_template && state.dark);
    final dot = state.unread > 0;
    final look = (color, dot, state.voice);
    final icons = _drawn[look] ??= [
      for (final size in _sizes)
        {
          'size': size,
          'rgba': await renderTrayIconRgba(
            size,
            color: color,
            dot: dot,
            voice: state.voice,
          ),
        },
    ];
    try {
      final shown = await _channel.invokeMethod<bool>('show', {
        'icons': icons,
        'template': _template,
        'tooltip': state.tooltip,
        'menu': [for (final item in _menu) _item(item)],
      });
      _available = shown ?? false;
    } on MissingPluginException {
      _available = false;
    } on PlatformException catch (error) {
      appLog(
        'The tray could not show: ${error.message}',
        name: 'tray',
        level: LogLevel.warn,
      );
      _available = false;
    }
  }

  static Map<String, Object?> _item(TrayMenuItem item) => {
    'id': item.id,
    'label': item.label,
    'separator': item.separator,
    'checked': item.checked,
    'radio': item.radio,
    'children': [for (final child in item.children) _item(child)],
  };

  Future<Object?> _fromRunner(MethodCall call) async {
    switch (call.method) {
      case 'activate':
        _actions.add(const OpenWindow());
      case 'menu':
        if (call.arguments case final int id) {
          if (_find(id, _menu)?.action case final action?) {
            _actions.add(action);
          }
        }
    }
    return null;
  }

  static TrayMenuItem? _find(int id, List<TrayMenuItem> items) {
    for (final item in items) {
      if (item.id == id) return item;
      if (_find(id, item.children) case final found?) return found;
    }
    return null;
  }

  @override
  Future<void> close() async {
    _channel.setMethodCallHandler(null);
    try {
      await _channel.invokeMethod<void>('close');
    } on MissingPluginException {
      // No runner tray to take down.
    }
    await _actions.close();
  }
}
