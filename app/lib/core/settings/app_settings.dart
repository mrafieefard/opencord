import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/settings/key_value_store.dart';

enum ThemePreference { system, dark, light }

enum MessageDensity { comfortable, compact }

/// §3.1: Auto picks per platform and desktop; System uses the native frame.
enum WindowFramePreference { auto, custom, system }

/// User settings that are not about a particular server (§8.1).
@immutable
class AppSettings {
  const AppSettings({
    required this.theme,
    required this.fontSize,
    required this.reduceMotion,
    required this.density,
    required this.windowFrame,
    required this.closeToTray,
    required this.startMinimized,
    required this.launchAtLogin,
    required this.restoreWindowPosition,
    required this.desktopNotifications,
    required this.mentionsOnly,
    required this.messageSound,
    required this.flashTaskbar,
  });

  factory AppSettings.defaults(TargetPlatform platform) => AppSettings(
    theme: ThemePreference.system,
    fontSize: defaultFontSize,
    reduceMotion: false,
    density: MessageDensity.comfortable,
    windowFrame: WindowFramePreference.auto,
    closeToTray: platform != TargetPlatform.macOS,
    startMinimized: false,
    launchAtLogin: false,
    restoreWindowPosition: true,
    desktopNotifications: true,
    mentionsOnly: false,
    messageSound: true,
    flashTaskbar: true,
  );

  /// Reads saved settings; anything missing or unreadable keeps its default.
  factory AppSettings.fromJson(
    Map<String, Object?> json,
    TargetPlatform platform,
  ) {
    final defaults = AppSettings.defaults(platform);
    T pick<T>(String key, T fallback) {
      final value = json[key];
      return value is T ? value : fallback;
    }

    E pickEnum<E extends Enum>(String key, List<E> values, E fallback) {
      final name = json[key];
      return values.where((value) => value.name == name).firstOrNull ??
          fallback;
    }

    final fontSize = json['fontSize'];
    return AppSettings(
      theme: pickEnum('theme', ThemePreference.values, defaults.theme),
      fontSize: fontSize is num
          ? clampFontSize(fontSize.toDouble())
          : defaults.fontSize,
      reduceMotion: pick('reduceMotion', defaults.reduceMotion),
      density: pickEnum('density', MessageDensity.values, defaults.density),
      windowFrame: pickEnum(
        'windowFrame',
        WindowFramePreference.values,
        defaults.windowFrame,
      ),
      closeToTray: pick('closeToTray', defaults.closeToTray),
      startMinimized: pick('startMinimized', defaults.startMinimized),
      launchAtLogin: pick('launchAtLogin', defaults.launchAtLogin),
      restoreWindowPosition: pick(
        'restoreWindowPosition',
        defaults.restoreWindowPosition,
      ),
      desktopNotifications: pick(
        'desktopNotifications',
        defaults.desktopNotifications,
      ),
      mentionsOnly: pick('mentionsOnly', defaults.mentionsOnly),
      messageSound: pick('messageSound', defaults.messageSound),
      flashTaskbar: pick('flashTaskbar', defaults.flashTaskbar),
    );
  }

  static const double defaultFontSize = 14;
  static const double minFontSize = 12;
  static const double maxFontSize = 18;

  static double clampFontSize(double size) =>
      size.clamp(minFontSize, maxFontSize);

  final ThemePreference theme;

  /// Body text size in logical pixels; scales all text (§2.2).
  final double fontSize;
  final bool reduceMotion;
  final MessageDensity density;
  final WindowFramePreference windowFrame;
  final bool closeToTray;
  final bool startMinimized;
  final bool launchAtLogin;
  final bool restoreWindowPosition;
  final bool desktopNotifications;
  final bool mentionsOnly;
  final bool messageSound;
  final bool flashTaskbar;

  /// Factor applied to every text style.
  double get textScale => fontSize / defaultFontSize;

  AppSettings copyWith({
    ThemePreference? theme,
    double? fontSize,
    bool? reduceMotion,
    MessageDensity? density,
    WindowFramePreference? windowFrame,
    bool? closeToTray,
    bool? startMinimized,
    bool? launchAtLogin,
    bool? restoreWindowPosition,
    bool? desktopNotifications,
    bool? mentionsOnly,
    bool? messageSound,
    bool? flashTaskbar,
  }) {
    return AppSettings(
      theme: theme ?? this.theme,
      fontSize: clampFontSize(fontSize ?? this.fontSize),
      reduceMotion: reduceMotion ?? this.reduceMotion,
      density: density ?? this.density,
      windowFrame: windowFrame ?? this.windowFrame,
      closeToTray: closeToTray ?? this.closeToTray,
      startMinimized: startMinimized ?? this.startMinimized,
      launchAtLogin: launchAtLogin ?? this.launchAtLogin,
      restoreWindowPosition:
          restoreWindowPosition ?? this.restoreWindowPosition,
      desktopNotifications: desktopNotifications ?? this.desktopNotifications,
      mentionsOnly: mentionsOnly ?? this.mentionsOnly,
      messageSound: messageSound ?? this.messageSound,
      flashTaskbar: flashTaskbar ?? this.flashTaskbar,
    );
  }

  Map<String, Object?> toJson() => {
    'theme': theme.name,
    'fontSize': fontSize,
    'reduceMotion': reduceMotion,
    'density': density.name,
    'windowFrame': windowFrame.name,
    'closeToTray': closeToTray,
    'startMinimized': startMinimized,
    'launchAtLogin': launchAtLogin,
    'restoreWindowPosition': restoreWindowPosition,
    'desktopNotifications': desktopNotifications,
    'mentionsOnly': mentionsOnly,
    'messageSound': messageSound,
    'flashTaskbar': flashTaskbar,
  };

  @override
  bool operator ==(Object other) =>
      other is AppSettings && mapEquals(other.toJson(), toJson());

  @override
  int get hashCode => Object.hashAll(toJson().values);
}

const appSettingsKey = 'ui.settings';

class AppSettingsNotifier extends Notifier<AppSettings> {
  @override
  AppSettings build() {
    final platform = ref.watch(platformProvider);
    final saved = ref.watch(keyValueStoreProvider).read(appSettingsKey);
    if (saved == null) return AppSettings.defaults(platform);
    try {
      final json = jsonDecode(saved);
      if (json is Map<String, Object?>) {
        return AppSettings.fromJson(json, platform);
      }
    } on FormatException {
      // A damaged file starts over from the defaults.
    }
    return AppSettings.defaults(platform);
  }

  void update(AppSettings Function(AppSettings settings) change) {
    state = change(state);
    ref
        .read(keyValueStoreProvider)
        .write(appSettingsKey, jsonEncode(state.toJson()));
  }
}

final appSettingsProvider = NotifierProvider<AppSettingsNotifier, AppSettings>(
  AppSettingsNotifier.new,
);
