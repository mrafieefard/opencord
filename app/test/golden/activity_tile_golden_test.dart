import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/plugins/activity_tile.dart';
import 'package:opencord/features/voice/voice_tile.dart';

import '../support/activities.dart';
import '../support/app.dart';
import '../support/floating_scenes.dart';

Future<void> _settle(WidgetTester tester) async {
  for (var waited = 0; waited < 700; waited += 50) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

void main() {
  for (final theme in [ThemePreference.dark, ThemePreference.light]) {
    final name = theme.name;

    Future<MockApp> onStage(WidgetTester tester) async {
      final app = await MockApp.pump(
        tester,
        size: const Size(1440, 900),
        settings: (s) => s.copyWith(theme: theme),
      );
      recordPaint(app);
      await openVoicePage(SceneDriver(tester));
      await startPaint(tester, app);
      await _settle(tester);
      return app;
    }

    testWidgets('activity tile not opened ($name)', (tester) async {
      final app = await onStage(tester);
      await expectLater(
        find.byType(ActivityTile),
        matchesGoldenFile('goldens/activity_tile_closed_$name.png'),
      );
      await app.dispose(tester);
    });

    testWidgets('activity tile opened ($name)', (tester) async {
      final app = await onStage(tester);
      await tester.tap(
        find.descendant(
          of: find.byType(ActivityTile),
          matching: find.text('Open'),
        ),
      );
      await _settle(tester);
      await expectLater(
        find.byType(ActivityTile),
        matchesGoldenFile('goldens/activity_tile_opened_$name.png'),
      );
      await app.dispose(tester);
    });

    testWidgets('activity tile small ($name)', (tester) async {
      final app = await onStage(tester);
      await tester.tap(
        find.descendant(
          of: find.byType(ActivityTile),
          matching: find.text('Open'),
        ),
      );
      await _settle(tester);
      final person = find.byType(VoiceTileView).first;
      await tester.tapAt(tester.getRect(person).topCenter + const Offset(0, 3));
      await _settle(tester);
      await expectLater(
        find.byType(ActivityTile),
        matchesGoldenFile('goldens/activity_tile_small_$name.png'),
      );
      await app.dispose(tester);
    });
  }
}
