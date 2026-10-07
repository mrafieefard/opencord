import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/features/desktop/tray_menu.dart';

/// The system tray icon and its menu (§15).
abstract interface class TrayService {
  /// Whether a tray shows the icon now. Closing the window hides it to
  /// the tray only then; otherwise the app could not be reached again.
  bool get available;

  /// What the user picked in the tray.
  Stream<TrayAction> get actions;

  Future<void> show(TrayState state);

  Future<void> close();
}

/// Where there is no tray: tests, and desktops that have none yet.
class NoTray implements TrayService {
  const NoTray();

  @override
  bool get available => false;

  @override
  Stream<TrayAction> get actions => const Stream.empty();

  @override
  Future<void> show(TrayState state) async {}

  @override
  Future<void> close() async {}
}

/// Overridden in `main` where the platform has a tray.
final trayServiceProvider = Provider<TrayService>((ref) => const NoTray());

/// Whether the desktop is dark, which the tray icon follows rather than
/// the app's theme.
class SystemBrightnessNotifier extends Notifier<Brightness>
    with WidgetsBindingObserver {
  @override
  Brightness build() {
    final binding = WidgetsBinding.instance;
    binding.addObserver(this);
    ref.onDispose(() => binding.removeObserver(this));
    return binding.platformDispatcher.platformBrightness;
  }

  @override
  void didChangePlatformBrightness() =>
      state = WidgetsBinding.instance.platformDispatcher.platformBrightness;
}

final systemBrightnessProvider =
    NotifierProvider<SystemBrightnessNotifier, Brightness>(
      SystemBrightnessNotifier.new,
    );
