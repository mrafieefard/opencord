import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/window/window_startup.dart';

import '../../support/fake_window.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  // The Linux runner reads the file before the app runs, at every start,
  // and settles Auto itself for the desktop it finds then: Hyprland today
  // and GNOME tomorrow must not share a frame resolved once.
  test(
    'the frame kept for the runner is the choice, not what it came to',
    () async {
      final dir = await Directory.systemTemp.createTemp('opencord-frame');
      addTearDown(() => dir.delete(recursive: true));
      final file = File('${dir.path}/window-chrome');

      for (final choice in WindowFramePreference.values) {
        await startWindow(
          window: FakeNativeWindow(),
          store: MemoryKeyValueStore(),
          settings: AppSettings.defaults(
            TargetPlatform.linux,
          ).copyWith(windowFrame: choice),
          dataDir: dir,
        );
        expect(file.readAsStringSync(), choice.name);

        file.deleteSync();
        await frameChoiceSaver(dir)(choice);
        expect(file.readAsStringSync(), choice.name);
      }
    },
  );
}
