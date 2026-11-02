import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/desktop/tray.dart';
import 'package:opencord/features/desktop/tray_menu.dart';
import 'package:opencord/features/window/window_providers.dart';

/// The tray icon's voice state (§15): out of voice, quiet, talking, or
/// muted. Talking changes many times a second; this changes only with it.
final trayVoiceProvider = Provider<TrayVoice>((ref) {
  final session = ref.watch(voiceSessionProvider);
  final serverKey = session.serverKey;
  if (!session.connected || serverKey == null) return TrayVoice.none;
  if (session.muted || session.deafened) return TrayVoice.muted;
  final me = ref.watch(
    serverProvider(serverKey).select((server) => server.data?.self.id),
  );
  final talking = ref.watch(
    speakingProvider(serverKey).select((ids) => me != null && ids.contains(me)),
  );
  return talking ? TrayVoice.speaking : TrayVoice.quiet;
});

/// Everything the tray shows, in one value, so it never shows half of a
/// change.
final trayStateProvider = Provider<TrayState>((ref) {
  final voice = ref.watch(voiceSessionProvider);
  return TrayState(
    unread: ref.watch(unreadTotalProvider),
    mentions: ref.watch(mentionTotalProvider),
    muted: voice.muted,
    deafened: voice.deafened,
    presence: ref.watch(selfPresenceProvider),
    dark: ref.watch(systemBrightnessProvider) == Brightness.dark,
    voice: ref.watch(trayVoiceProvider),
  );
});

/// Keeps the tray in step with the app, carries out what is picked in it,
/// and keeps the app running in the background when the window is closed,
/// unless the user turned that off (§15, §8.1).
final trayBindingProvider = Provider<void>((ref) {
  final tray = ref.watch(trayServiceProvider);
  final window = ref.watch(nativeWindowProvider);

  ref.listen(
    trayStateProvider,
    (_, state) => unawaited(tray.show(state)),
    fireImmediately: true,
  );

  final actions = tray.actions.listen((action) {
    switch (action) {
      case OpenWindow(:final activationToken):
        window.show(activationToken: activationToken);
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
    // from there; ⌘Q quits (§7). Elsewhere it keeps running for calls and
    // messages: the tray icon brings it back, and so does launching it
    // again, even on a desktop with no tray.
    final mac = defaultTargetPlatform == TargetPlatform.macOS;
    if (mac || ref.read(appSettingsProvider).closeToTray) {
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
