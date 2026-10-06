import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path_provider/path_provider.dart';

import 'package:opencord/app.dart';
import 'package:opencord/core/app_info.dart';
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
  runApp(
    ProviderScope(
      overrides: [
        keyValueStoreProvider.overrideWithValue(const CoreKeyValueStore()),
        coreVersionProvider.overrideWithValue(coreVersion()),
      ],
      child: const OpencordApp(),
    ),
  );
}
