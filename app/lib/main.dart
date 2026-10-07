import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import 'package:opencord/app.dart';
import 'package:opencord/core/app_info.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/core_key_value_store.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/links/app_links.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/features/window/window_startup.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/src/rust/api/system.dart';
import 'package:opencord/src/rust/frb_generated.dart';
import 'package:opencord/ui/theme/font_licenses.dart';

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final dataDir = await getApplicationSupportDirectory();
  core.init(appDataDir: dataDir.path);
  registerFontLicenses();

  const store = CoreKeyValueStore();
  final window = ChannelNativeWindow();
  final windowInfo = await startWindow(
    window: window,
    store: store,
    settings: loadAppSettings(store, defaultTargetPlatform),
    dataDir: dataDir,
  );

  // The desktop UI runs on the mock repository until it is switched to the
  // Rust core (desktop UI plan §12, step 15).
  final repository = MockRepository();
  final container = ProviderContainer(
    overrides: [
      keyValueStoreProvider.overrideWithValue(store),
      coreVersionProvider.overrideWithValue(coreVersion()),
      repositoryProvider.overrideWithValue(repository),
      nativeWindowProvider.overrideWithValue(window),
      windowInfoProvider.overrideWithValue(windowInfo),
      frameChoiceSaverProvider.overrideWithValue(frameChoiceSaver(dataDir)),
      defaultMutedServersProvider.overrideWithValue(mockMutedServers),
    ],
  );
  container.read(eventPumpProvider);
  // Links the app was started with (§15); later launches pass theirs on
  // through the window, which the inbox listens to from now.
  final inbox = container.read(appLinkInboxProvider.notifier);
  for (final arg in args.where((arg) => arg.startsWith('opencord:'))) {
    inbox.add(arg);
  }
  repository.start();
  repository.updatePresence(container.read(selfPresenceProvider));
  runApp(
    UncontrolledProviderScope(container: container, child: const OpencordApp()),
  );
}
