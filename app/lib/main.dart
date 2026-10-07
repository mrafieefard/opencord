import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import 'package:opencord/app.dart';
import 'package:opencord/core/app_info.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/core_key_value_store.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/src/rust/api/system.dart';
import 'package:opencord/src/rust/frb_generated.dart';
import 'package:opencord/ui/theme/font_licenses.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await RustLib.init();
  final dataDir = await getApplicationSupportDirectory();
  core.init(appDataDir: dataDir.path);
  registerFontLicenses();

  // The desktop UI runs on the mock repository until it is switched to the
  // Rust core (desktop UI plan §12, step 15).
  final repository = MockRepository();
  final container = ProviderContainer(
    overrides: [
      keyValueStoreProvider.overrideWithValue(const CoreKeyValueStore()),
      coreVersionProvider.overrideWithValue(coreVersion()),
      repositoryProvider.overrideWithValue(repository),
    ],
  );
  container.read(eventPumpProvider);
  repository.start();
  runApp(
    UncontrolledProviderScope(container: container, child: const OpencordApp()),
  );
}
