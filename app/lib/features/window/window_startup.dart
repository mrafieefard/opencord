import 'package:flutter/widgets.dart';

import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_frame.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';

/// Sets the window up before the first frame (§3.1): the app's own title
/// bar on every platform, the saved size and place, the minimum size and a
/// background in the theme's color. Keeps the geometry saved from then on.
///
/// The app always asks before the window closes, and decides then whether
/// it hides to the tray. [hidden] starts it minimized to the tray.
Future<WindowInfo> startWindow({
  required NativeWindow window,
  required KeyValueStore store,
  required AppSettings settings,
  bool hidden = false,
}) async {
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

/// Whether the window starts hidden in the tray (§8.1 Start minimized):
/// at login only, as Telegram does, and only with a tray to come back from.
/// Launched by hand, the person wants the window.
bool startsHidden({
  required bool atLogin,
  required AppSettings settings,
  required bool tray,
}) => atLogin && settings.startMinimized && tray;
