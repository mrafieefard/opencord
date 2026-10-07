import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/app.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/features/links/app_links.dart';

/// The whole app on the mock repository, in a widget test.
class MockApp {
  MockApp._(this.container, this.repository, this.store);

  final ProviderContainer container;
  final MockRepository repository;
  final MemoryKeyValueStore store;

  /// Wraps the whole app, for pixel checks.
  static final boundaryKey = GlobalKey();

  static Future<MockApp> pump(
    WidgetTester tester, {
    Size size = const Size(1440, 900),
    double textScale = 1,
    MemoryKeyValueStore? store,
    AppSettings Function(AppSettings settings)? settings,
    NativeWindow? window,
    WindowInfo windowInfo = WindowInfo.none,
    TargetPlatform? platform,
    Set<String> mutedServers = const {},
    List<Override> overrides = const [],
    List<String> links = const [],
  }) async {
    debugDefaultTargetPlatformOverride = platform;
    tester.view.physicalSize = size;
    tester.view.devicePixelRatio = 1;
    tester.platformDispatcher.textScaleFactorTestValue = textScale;
    addTearDown(tester.view.reset);
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    final repository = MockRepository(random: Random(1), simulateLife: false);
    final keyValues = store ?? MemoryKeyValueStore();
    final container = ProviderContainer(
      overrides: [
        keyValueStoreProvider.overrideWithValue(keyValues),
        repositoryProvider.overrideWithValue(repository),
        nativeWindowProvider.overrideWithValue(
          window ?? const NullNativeWindow(),
        ),
        windowInfoProvider.overrideWithValue(windowInfo),
        defaultMutedServersProvider.overrideWithValue(mutedServers),
        ...overrides,
      ],
    );
    if (settings != null) {
      container.read(appSettingsProvider.notifier).update(settings);
    }
    container.read(eventPumpProvider);
    for (final link in links) {
      container.read(appLinkInboxProvider.notifier).add(link);
    }
    repository.start();
    await tester.pumpWidget(
      RepaintBoundary(
        key: boundaryKey,
        child: UncontrolledProviderScope(
          container: container,
          child: const OpencordApp(),
        ),
      ),
    );
    await tester.pump(const Duration(seconds: 1));
    await tester.pump();
    return MockApp._(container, repository, keyValues);
  }

  T read<T>(ProviderListenable<T> provider) => container.read(provider);

  /// Unmounts the app and stops the mock, so no timers are left behind.
  Future<void> dispose(WidgetTester tester) async {
    await tester.pumpWidget(const SizedBox());
    repository.dispose();
    container.dispose();
    debugDefaultTargetPlatformOverride = null;
  }
}
