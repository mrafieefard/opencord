import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';

void main() {
  group('AppSettings', () {
    test('defaults follow the plan', () {
      final linux = AppSettings.defaults(TargetPlatform.linux);
      final mac = AppSettings.defaults(TargetPlatform.macOS);

      expect(linux.theme, ThemePreference.system);
      expect(linux.fontSize, 14);
      expect(linux.reduceMotion, isFalse);
      expect(linux.density, MessageDensity.comfortable);
      expect(linux.windowFrame, WindowFramePreference.auto);
      expect(linux.closeToTray, isTrue);
      expect(mac.closeToTray, isFalse);
      expect(linux.restoreWindowPosition, isTrue);
      expect(linux.desktopNotifications, isTrue);
    });

    test('survive a JSON round trip', () {
      final custom = AppSettings.defaults(TargetPlatform.linux).copyWith(
        theme: ThemePreference.light,
        fontSize: 16,
        reduceMotion: true,
        density: MessageDensity.compact,
        windowFrame: WindowFramePreference.system,
        closeToTray: false,
        startMinimized: true,
        launchAtLogin: true,
        mentionsOnly: true,
      );

      final restored = AppSettings.fromJson(
        custom.toJson(),
        TargetPlatform.linux,
      );

      expect(restored, custom);
    });

    test('unknown or broken values fall back to the defaults', () {
      final defaults = AppSettings.defaults(TargetPlatform.linux);

      final restored = AppSettings.fromJson({
        'theme': 'sepia',
        'fontSize': 'huge',
        'reduceMotion': 3,
      }, TargetPlatform.linux);

      expect(restored, defaults);
    });

    test('font size stays within 12–18', () {
      final defaults = AppSettings.defaults(TargetPlatform.linux);

      expect(defaults.copyWith(fontSize: 40).fontSize, 18);
      expect(defaults.copyWith(fontSize: 2).fontSize, 12);
      expect(
        AppSettings.fromJson({'fontSize': 99}, TargetPlatform.linux).fontSize,
        18,
      );
    });
  });

  group('appSettingsProvider', () {
    ProviderContainer containerWith(KeyValueStore store) {
      final container = ProviderContainer(
        overrides: [
          keyValueStoreProvider.overrideWithValue(store),
          platformProvider.overrideWithValue(TargetPlatform.linux),
        ],
      );
      addTearDown(container.dispose);
      return container;
    }

    test('changes are saved and read back by a new app', () {
      final store = MemoryKeyValueStore();
      final first = containerWith(store);

      first
          .read(appSettingsProvider.notifier)
          .update((settings) => settings.copyWith(theme: ThemePreference.dark));
      final second = containerWith(store);

      expect(second.read(appSettingsProvider).theme, ThemePreference.dark);
    });

    test('a corrupt saved value starts from the defaults', () {
      final store = MemoryKeyValueStore()..write(appSettingsKey, '{not json');

      final settings = containerWith(store).read(appSettingsProvider);

      expect(settings, AppSettings.defaults(TargetPlatform.linux));
    });
  });
}
