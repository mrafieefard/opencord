import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/plugins/activity_picker.dart';
import 'package:opencord/features/plugins/activity_tile.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/voice/voice_focus.dart';
import 'package:opencord/features/voice/voice_view.dart';

import '../support/activities.dart';
import '../support/app.dart';
import '../support/floating_scenes.dart';

Finder _dock() => find.byKey(ShellKeys.dock);

Finder _activitiesButton() =>
    find.descendant(of: _dock(), matching: find.bySemanticsLabel('Activities'));

Finder _activity(int session) => find.byWidgetPredicate(
  (widget) => widget is ActivityTile && widget.session.id == session,
);

Finder _inActivity(int session, Finder finder) =>
    find.descendant(of: _activity(session), matching: finder);

Future<void> _settle(WidgetTester tester) async {
  for (var waited = 0; waited < 700; waited += 50) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

Future<void> _reload(MockApp app, WidgetTester tester) async {
  final server = app.repository.mockServer(activityServer);
  app.repository.emit(Ready(activityServer, server.snapshot()));
  await tester.pumpAndSettle();
}

void _setActivities(MockServer server, int count) {
  server.pluginActivities.clear();
  for (var i = 1; i <= count; i++) {
    server.pluginActivities.add(
      PluginActivity(
        pluginId: 'plugin$i',
        activityKey: 'activity$i',
        name: i == 1 ? 'Paint together' : 'Activity $i',
        description: 'Interactive activity number $i',
        pluginName: 'Plugin $i',
      ),
    );
  }
}

Future<void> _openPicker(WidgetTester tester) async {
  await tester.pump();
  await tester.pump();
  await tester.tap(_activitiesButton());
  await tester.pumpAndSettle();
}

Future<void> _makeLarge(WidgetTester tester, int sessionId) async {
  final byText = _inActivity(sessionId, find.text('Make large'));
  if (byText.evaluate().isNotEmpty) {
    await tester.tap(byText);
  } else {
    final byTooltip = _inActivity(sessionId, find.byTooltip('Make large'));
    await tester.tap(byTooltip);
  }
  await _settle(tester);
}

void main() {
  const normalSize = Size(1440, 900);
  const narrowSize = Size(940, 560);

  final configs = [
    (ThemePreference.dark, 'dark', normalSize, '1440'),
    (ThemePreference.dark, 'dark', narrowSize, '940'),
    (ThemePreference.light, 'light', normalSize, '1440'),
    (ThemePreference.light, 'light', narrowSize, '940'),
  ];

  for (final (themePref, themeName, winSize, widthLabel) in configs) {
    group('$themeName $widthLabel', () {
      Future<MockApp> initVoice(
        WidgetTester tester, {
        void Function(MockServer server)? setupServer,
      }) async {
        final app = await MockApp.pump(
          tester,
          size: winSize,
          settings: (s) => s.copyWith(theme: themePref),
        );
        recordPaint(app);
        if (setupServer != null) {
          setupServer(app.repository.mockServer(activityServer));
          await _reload(app, tester);
        }
        await openVoicePage(SceneDriver(tester));
        await tester.pump();
        await tester.pump();
        return app;
      }

      // 1. The dock with the Activities button
      testWidgets('dock with activities button', (tester) async {
        final app = await initVoice(tester);
        expect(_activitiesButton(), findsOneWidget);
        await expectLater(
          _dock(),
          matchesGoldenFile('goldens/dock_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });

      // 2a. Picker with 1 activity
      testWidgets('picker 1 activity', (tester) async {
        final app = await initVoice(
          tester,
          setupServer: (s) => _setActivities(s, 1),
        );
        await _openPicker(tester);
        expect(find.byType(ActivityPicker), findsOneWidget);
        await expectLater(
          find.byType(ActivityPicker),
          matchesGoldenFile('goldens/picker_1_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });

      // 2b. Picker with 6 activities
      testWidgets('picker 6 activities', (tester) async {
        final app = await initVoice(
          tester,
          setupServer: (s) => _setActivities(s, 6),
        );
        await _openPicker(tester);
        expect(find.byType(ActivityPicker), findsOneWidget);
        await expectLater(
          find.byType(ActivityPicker),
          matchesGoldenFile('goldens/picker_6_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });

      // 2c. Picker with 1 running activity
      testWidgets('picker 1 running', (tester) async {
        final app = await initVoice(
          tester,
          setupServer: (s) => _setActivities(s, 3),
        );
        final call = callOf(app);
        await app.repository.startActivity(
          call.server,
          call.channel,
          'plugin1',
          'activity1',
        );
        // Two pumps keep the dock up without triggering the idle autohide timer
        await tester.pump();
        await tester.pump();
        await _openPicker(tester);
        expect(find.byType(ActivityPicker), findsOneWidget);
        await expectLater(
          find.byType(ActivityPicker),
          matchesGoldenFile('goldens/picker_running_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });

      // 2d. Picker at cap of 4
      testWidgets('picker cap of 4', (tester) async {
        final app = await initVoice(
          tester,
          setupServer: (s) => _setActivities(s, 6),
        );
        final call = callOf(app);
        for (var i = 1; i <= 4; i++) {
          await app.repository.startActivity(
            call.server,
            call.channel,
            'plugin$i',
            'activity$i',
          );
        }
        // Two pumps keep the dock up without triggering the idle autohide timer
        await tester.pump();
        await tester.pump();
        await _openPicker(tester);
        expect(find.byType(ActivityPicker), findsOneWidget);
        await expectLater(
          find.byType(ActivityPicker),
          matchesGoldenFile('goldens/picker_cap_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });

      // 3a. Stage with 1 activity in grid
      testWidgets('stage 1 activity grid', (tester) async {
        final app = await initVoice(tester);
        final session = await startPaint(tester, app);
        await _settle(tester);
        await tester.tap(_inActivity(session.id, find.text('Open')));
        await _settle(tester);
        await expectLater(
          find.byType(VoiceView),
          matchesGoldenFile('goldens/stage_1_grid_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });

      // 3b. Stage with 1 activity in focus mode
      testWidgets('stage 1 activity focus', (tester) async {
        final app = await initVoice(tester);
        final session = await startPaint(tester, app);
        await _settle(tester);
        await tester.tap(_inActivity(session.id, find.text('Open')));
        await _settle(tester);
        await _makeLarge(tester, session.id);
        await expectLater(
          find.byType(VoiceView),
          matchesGoldenFile('goldens/stage_1_focus_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });

      // 3c. Stage with 3 activities in grid
      testWidgets('stage 3 activities grid', (tester) async {
        final app = await initVoice(
          tester,
          setupServer: (s) => _setActivities(s, 3),
        );
        final call = callOf(app);
        for (var i = 1; i <= 3; i++) {
          await app.repository.startActivity(
            call.server,
            call.channel,
            'plugin$i',
            'activity$i',
          );
        }
        await _settle(tester);
        await expectLater(
          find.byType(VoiceView),
          matchesGoldenFile('goldens/stage_3_grid_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });

      // 3d. Stage with 3 activities in focus mode
      testWidgets('stage 3 activities focus', (tester) async {
        final app = await initVoice(
          tester,
          setupServer: (s) => _setActivities(s, 3),
        );
        final call = callOf(app);
        final sessions = <ActivitySession>[];
        for (var i = 1; i <= 3; i++) {
          final s = await app.repository.startActivity(
            call.server,
            call.channel,
            'plugin$i',
            'activity$i',
          );
          sessions.add(s);
        }
        await _settle(tester);
        // Focus the first activity to make it large, then open it
        app.read(voiceFocusProvider((server: call.server, channel: call.channel)).notifier).focus(
          (userId: 0, screen: false, session: sessions.first.id),
        );
        await _settle(tester);
        await tester.tap(_inActivity(sessions.first.id, find.text('Open')));
        await _settle(tester);
        await expectLater(
          find.byType(VoiceView),
          matchesGoldenFile('goldens/stage_3_focus_${themeName}_$widthLabel.png'),
        );
        await app.dispose(tester);
      });
    });
  }
}
