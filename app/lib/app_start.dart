import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import 'package:opencord/core/app_info.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/core_api.dart';
import 'package:opencord/core/rust/core_key_value_store.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/rust/rust_repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/desktop/linux_notifications.dart';
import 'package:opencord/features/desktop/linux_tray.dart';
import 'package:opencord/features/desktop/login_item.dart';
import 'package:opencord/features/desktop/notification_binding.dart';
import 'package:opencord/features/desktop/notifications.dart';
import 'package:opencord/features/desktop/tray.dart';
import 'package:opencord/features/desktop/tray_binding.dart';
import 'package:opencord/features/links/app_links.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/features/window/window_startup.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/src/rust/api/system.dart';
import 'package:opencord/src/rust/frb_generated.dart';
import 'package:opencord/ui/theme/font_licenses.dart';
import 'package:opencord/ui/widgets/error_box.dart';

/// The app's data folder; a profile gets its own inside it.
Future<Directory> _dataDir(String profile) async {
  final base = await getApplicationSupportDirectory();
  if (profile.isEmpty) return base;
  return Directory('${base.path}/profiles/$profile')
    ..createSync(recursive: true);
}

/// Starts what the app runs on: the Rust core (or the mock), the window,
/// desktop integration and the providers. `main` and the end-to-end tests
/// share it; [desktop] off leaves out the tray, notifications and launch
/// at login.
Future<ProviderContainer> startApp({
  required String profile,
  required bool mock,
  List<String> links = const [],
  bool desktop = true,
}) async {
  await RustLib.init();
  final environment = Platform.environment;
  final dataDir = await _dataDir(profile);
  core.init(appDataDir: dataDir.path);
  registerFontLicenses();
  limitErrorBoxes();

  const store = CoreKeyValueStore();
  final settings = loadAppSettings(store, defaultTargetPlatform);
  // The tray before the window: starting minimized needs one (§15).
  final linux = desktop && defaultTargetPlatform == TargetPlatform.linux;
  final tray = linux ? await LinuxTray.start() : null;
  final notifications = linux ? await LinuxNotifications.start() : null;
  final window = ChannelNativeWindow();
  final windowInfo = await startWindow(
    window: window,
    store: store,
    settings: settings,
    dataDir: dataDir,
    hidden: settings.startMinimized && (tray?.available ?? false),
  );

  // The Rust core, or the mock for working on the UI without a server.
  final OpencordRepository repository = mock
      ? MockRepository()
      : await RustRepository.open(
          core: const FrbCoreApi(),
          identities: SecureIdentityStore(profile: profile),
          store: store,
        );
  final container = ProviderContainer(
    overrides: [
      keyValueStoreProvider.overrideWithValue(store),
      coreVersionProvider.overrideWithValue(coreVersion()),
      repositoryProvider.overrideWithValue(repository),
      nativeWindowProvider.overrideWithValue(window),
      windowInfoProvider.overrideWithValue(windowInfo),
      frameChoiceSaverProvider.overrideWithValue(frameChoiceSaver(dataDir)),
      if (mock) defaultMutedServersProvider.overrideWithValue(mockMutedServers),
      if (tray != null) trayServiceProvider.overrideWithValue(tray),
      if (notifications != null)
        notificationServiceProvider.overrideWithValue(notifications),
      // Profiles are for testing and leave the login item alone.
      loginItemProvider.overrideWithValue(switch (defaultTargetPlatform) {
        _ when profile.isNotEmpty || !desktop => const NoLoginItem(),
        TargetPlatform.linux => XdgAutostart(
          configHome: xdgConfigHome(environment),
          executable: Platform.resolvedExecutable,
        ),
        TargetPlatform.windows ||
        TargetPlatform.macOS => RunnerLoginItem(window),
        _ => const NoLoginItem(),
      }),
    ],
  );
  container.read(eventPumpProvider);
  container.read(trayBindingProvider);
  container.read(notificationBindingProvider);
  container.read(loginItemBindingProvider);
  // Links the app was started with (§15); later launches pass theirs on
  // through the window, which the inbox listens to from now.
  final inbox = container.read(appLinkInboxProvider.notifier);
  links.forEach(inbox.add);
  repository.start();
  repository.updatePresence(container.read(selfPresenceProvider));
  return container;
}
