import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_frame.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// Sets the window up before the first frame (§3.1): the frame the user
/// chose, the saved size and place, the minimum size and a background in
/// the theme's color. Keeps the geometry saved from then on.
Future<WindowInfo> startWindow({
  required NativeWindow window,
  required KeyValueStore store,
  required AppSettings settings,
  required Directory dataDir,
}) async {
  final chrome = resolveWindowChrome(
    platform: defaultTargetPlatform,
    environment: Platform.environment,
    preference: settings.windowFrame,
  );
  await saveWindowChrome(dataDir, chrome);
  final restore = settings.restoreWindowPosition
      ? savedWindowGeometry(store)
      : null;
  final brightness =
      WidgetsBinding.instance.platformDispatcher.platformBrightness;
  final dark =
      settings.theme == ThemePreference.dark ||
      (settings.theme == ThemePreference.system &&
          brightness == Brightness.dark);
  final info = await window.configure(
    chrome: chrome,
    background: (dark ? OcColors.dark : OcColors.light).chat.toARGB32(),
    minWidth: OcSize.minWindow.width,
    minHeight: OcSize.minWindow.height,
    frameMargin: windowFrameMargin,
    resizeBand: windowResizeBand,
    restore: restore,
  );
  WindowGeometryKeeper(window: window, store: store, initial: restore);
  return info;
}

/// The runner reads this file when it creates the window at the next start:
/// GTK has to know about decorations before the app runs.
Future<void> saveWindowChrome(Directory dataDir, WindowChrome chrome) async {
  final file = File('${dataDir.path}/window-chrome');
  try {
    if (file.existsSync() && file.readAsStringSync().trim() == chrome.name) {
      return;
    }
    await file.writeAsString(chrome.name);
  } on FileSystemException catch (error) {
    debugPrint('could not save the window frame choice: $error');
  }
}
