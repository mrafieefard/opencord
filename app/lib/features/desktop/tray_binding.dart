import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/desktop/tray.dart';
import 'package:opencord/features/desktop/tray_menu.dart';
import 'package:opencord/features/window/window_providers.dart';

/// Keeps the tray in step with the app, carries out what is picked in it,
/// and hides the window to it on close when the user wants that and a tray
/// shows the icon (§15, §8.1).
final trayBindingProvider = Provider<void>((ref) {
  final tray = ref.watch(trayServiceProvider);
  final window = ref.watch(nativeWindowProvider);

  void update() {
    final voice = ref.read(voiceSessionProvider);
    unawaited(
      tray.show(
        TrayState(
          unread: ref.read(unreadTotalProvider),
          mentions: ref.read(mentionTotalProvider),
          muted: voice.muted,
          deafened: voice.deafened,
          presence: ref.read(selfPresenceProvider),
          dark: ref.read(systemBrightnessProvider) == Brightness.dark,
        ),
      ),
    );
  }

  ref
    ..listen(unreadTotalProvider, (_, _) => update())
    ..listen(mentionTotalProvider, (_, _) => update())
    ..listen(voiceSessionProvider, (_, _) => update())
    ..listen(selfPresenceProvider, (_, _) => update())
    ..listen(systemBrightnessProvider, (_, _) => update());
  update();

  final actions = tray.actions.listen((action) {
    switch (action) {
      case OpenWindow():
        window.show();
      case ToggleMute():
        ref.read(voiceSessionProvider.notifier).toggleMute();
      case ToggleDeafen():
        ref.read(voiceSessionProvider.notifier).toggleDeafen();
      case SetPresence(:final presence):
        ref.read(selfPresenceProvider.notifier).choose(presence);
      case QuitApp():
        window.quit();
    }
  });
  final closes = window.closeRequests.listen((_) {
    // A Mac app stays in the Dock when its window closes, and comes back
    // from there; ⌘Q quits (§7).
    final mac = defaultTargetPlatform == TargetPlatform.macOS;
    if (mac || (ref.read(appSettingsProvider).closeToTray && tray.available)) {
      window.hide();
    } else {
      window.quit();
    }
  });
  ref.onDispose(() {
    actions.cancel();
    closes.cancel();
  });
});
