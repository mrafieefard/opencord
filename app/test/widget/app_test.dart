import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:opencord/app.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/ui/theme/oc_colors.dart';

/// Records what the app gives its screens.
class _Probe extends StatelessWidget {
  const _Probe();

  static late OcColors colors;
  static late MediaQueryData media;

  @override
  Widget build(BuildContext context) {
    colors = context.oc;
    media = MediaQuery.of(context);
    return const SizedBox();
  }
}

Future<ProviderContainer> pumpApp(
  WidgetTester tester, {
  AppSettings Function(AppSettings)? settings,
}) async {
  final container = ProviderContainer(
    overrides: [
      keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore()),
      platformProvider.overrideWithValue(TargetPlatform.linux),
      routerProvider.overrideWithValue(
        GoRouter(
          routes: [
            GoRoute(path: '/', builder: (context, state) => const _Probe()),
          ],
        ),
      ),
    ],
  );
  addTearDown(container.dispose);
  if (settings != null) {
    container.read(appSettingsProvider.notifier).update(settings);
  }
  await tester.pumpWidget(
    UncontrolledProviderScope(container: container, child: const OpencordApp()),
  );
  return container;
}

void main() {
  testWidgets('follows the system brightness by default', (tester) async {
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
    addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);

    await pumpApp(tester);

    expect(_Probe.colors, OcColors.dark);
  });

  testWidgets('a chosen theme wins over the system', (tester) async {
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
    addTearDown(tester.platformDispatcher.clearPlatformBrightnessTestValue);

    await pumpApp(
      tester,
      settings: (s) => s.copyWith(theme: ThemePreference.light),
    );

    expect(_Probe.colors, OcColors.light);
  });

  testWidgets('switching the theme applies on the next frame', (tester) async {
    final container = await pumpApp(
      tester,
      settings: (s) => s.copyWith(theme: ThemePreference.dark),
    );

    container
        .read(appSettingsProvider.notifier)
        .update((s) => s.copyWith(theme: ThemePreference.light));
    await tester.pump();

    expect(_Probe.colors, OcColors.light);
  });

  testWidgets('the font size setting scales all text', (tester) async {
    await pumpApp(tester, settings: (s) => s.copyWith(fontSize: 18));

    expect(_Probe.media.textScaler.scale(14), closeTo(18, 0.001));
  });

  testWidgets('reduce motion turns animations off', (tester) async {
    await pumpApp(tester, settings: (s) => s.copyWith(reduceMotion: true));

    expect(_Probe.media.disableAnimations, isTrue);
  });
}
