import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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
///
/// The app always asks before the window closes, and decides then whether
/// it hides to the tray. [hidden] starts it minimized to the tray.
Future<WindowInfo> startWindow({
  required NativeWindow window,
  required KeyValueStore store,
  required AppSettings settings,
  required Directory dataDir,
  bool hidden = false,
}) async {
  final chrome = resolveWindowChrome(
    platform: defaultTargetPlatform,
    environment: Platform.environment,
    preference: settings.windowFrame,
  );
  await saveWindowFrame(dataDir, settings.windowFrame);
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
    interceptClose: true,
    hidden: hidden,
  );
  WindowGeometryKeeper(window: window, store: store, initial: restore);
  return info;
}

/// Saves a new frame choice for the next start (§8.1 Window frame). The
/// window is configured once, when it is created, so the choice applies
/// after a restart on every platform. Overridden in `main` with the app's
/// data directory; tests keep this one, which does nothing.
final frameChoiceSaverProvider =
    Provider<Future<void> Function(WindowFramePreference preference)>(
      (ref) => (_) async {},
    );

/// The saver `main` installs.
Future<void> Function(WindowFramePreference) frameChoiceSaver(
  Directory dataDir,
) =>
    (preference) => saveWindowFrame(dataDir, preference);

/// The runner reads this file when it creates the window at the next start:
/// GTK has to know about decorations before the app runs. It holds the
/// choice (auto, custom or system), and the runner settles Auto for the
/// desktop it starts on, which can differ from this one.
Future<void> saveWindowFrame(
  Directory dataDir,
  WindowFramePreference preference,
) async {
  final file = File('${dataDir.path}/window-chrome');
  try {
    if (file.existsSync() &&
        file.readAsStringSync().trim() == preference.name) {
      return;
    }
    await file.writeAsString(preference.name);
  } on FileSystemException catch (error) {
    debugPrint('could not save the window frame choice: $error');
  }
}
