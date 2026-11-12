import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/providers/activity_sessions.dart';
import 'package:opencord/core/providers/plugin_views.dart';
import 'package:opencord/features/plugins/plugin_area.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

import '../../support/app.dart';

const _serverKey = 'opencord.example:7710';

/// Joins the mock server's General call and starts its registered `paint`
/// activity there, as the picker does: the session comes back from
/// `startActivity`, and the same session reaches [activitySessionsProvider]
/// through the started event.
Future<ActivitySession> _startPaint(WidgetTester tester, MockApp app) async {
  final server = app.repository.mockServer(_serverKey);
  final generalVoice = server.channels.values.firstWhere(
    (channel) =>
        channel.kind == ChannelKind.voice && channel.name == 'General',
  );
  final paint = server.pluginActivities.firstWhere(
    (activity) => activity.activityKey == 'paint',
  );
  await app.repository.joinVoice(_serverKey, generalVoice.id);
  final session = await app.repository.startActivity(
    _serverKey,
    generalVoice.id,
    paint.pluginId,
    paint.activityKey,
  );
  await tester.pump();
  return session;
}

/// One [PluginArea] for [place] in the app's container, with [paused]
/// driven from outside, as the stage's tile will drive it.
Future<void> _pumpArea(
  WidgetTester tester,
  MockApp app,
  ViewPlaceRef place, {
  ValueNotifier<bool>? paused,
}) async {
  final pausedValue = paused ?? ValueNotifier(false);
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: app.container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: Scaffold(
          body: ValueListenableBuilder<bool>(
            valueListenable: pausedValue,
            builder: (context, isPaused, _) =>
                PluginArea(place: place, paused: isPaused),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  // Road: mock repository (openActivityView) → pluginViewProvider
  // (ActivityPlace) → PluginArea. Fails if an activity place opens a
  // channel's view, or opens nothing.
  testWidgets('an activity place opens its session\'s view', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 900));
    final session = await _startPaint(tester, app);

    await _pumpArea(tester, app, ActivityPlace(_serverKey, session));

    expect(find.text('Count: 0'), findsOneWidget);
    await tester.tap(find.text('Count'));
    await tester.pumpAndSettle();
    expect(find.text('Count: 1'), findsOneWidget);
    // Opened as the session's view: the mock added us to its viewers.
    final self = app.repository.mockServer(_serverKey).selfId;
    expect(
      app.read(activitySessionsProvider(_serverKey))[session.id]?.viewerIds,
      [self],
    );

    await app.dispose(tester);
  });

  // Road: mock repository's ActivitySessionChanged (our own open adds us to
  // viewerIds) → activitySessionsProvider → a new ActivityPlace for the
  // changed session → pluginViewProvider. Fails if the place compares by
  // the whole session: the view would close and open again.
  testWidgets('a session whose viewers changed keeps its view', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 900));
    final session = await _startPaint(tester, app);
    expect(session.viewerIds, isEmpty);

    await _pumpArea(tester, app, ActivityPlace(_serverKey, session));
    final first = app.read(
      pluginViewProvider(ActivityPlace(_serverKey, session)),
    );
    final viewId = first.viewId.value;
    expect(viewId, isNotNull);
    await tester.tap(find.text('Count'));
    await tester.pumpAndSettle();

    final changed = app.read(activitySessionsProvider(_serverKey))[session.id]!;
    expect(changed.viewerIds, isNotEmpty);
    expect(changed, isNot(session));

    await _pumpArea(tester, app, ActivityPlace(_serverKey, changed));

    final again = app.read(
      pluginViewProvider(ActivityPlace(_serverKey, changed)),
    );
    expect(again, same(first));
    expect(again.viewId.value, viewId);
    expect(app.repository.closedViews, isEmpty);
    expect(find.text('Count: 1'), findsOneWidget);

    await app.dispose(tester);
  });

  // Road: endActivity → the mock's `closed` batch with `activity_ended`, as
  // the core sends it → controller.closed → PluginArea's message. Leaving
  // the call sends `left_call`, which shows nothing: the tile is going.
  testWidgets('an ended activity shows its message', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 900));
    final session = await _startPaint(tester, app);
    await _pumpArea(tester, app, ActivityPlace(_serverKey, session));
    expect(find.text('Count: 0'), findsOneWidget);

    await app.repository.endActivity(_serverKey, session.id);
    await tester.pumpAndSettle();

    expect(find.text('This activity ended'), findsOneWidget);
    expect(find.text('Count: 0'), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('leaving the call shows nothing', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 900));
    // A second member stays in the call, so leaving it doesn't end the
    // session: the view closes with `left_call`, not `activity_ended`.
    final server = app.repository.mockServer(_serverKey);
    final session = await _startPaint(tester, app);
    final inCall = server.voice[session.channelId]!;
    if (!inCall.any((participant) => participant.userId != server.selfId)) {
      final other = server.members.keys.firstWhere(
        (id) => id != server.selfId,
      );
      inCall.add(VoiceParticipant(userId: other));
    }
    await _pumpArea(tester, app, ActivityPlace(_serverKey, session));
    expect(find.text('Count: 0'), findsOneWidget);

    await app.repository.leaveVoice();
    await tester.pumpAndSettle();

    expect(
      app.read(pluginViewProvider(ActivityPlace(_serverKey, session))).closed,
      'left_call',
    );
    expect(find.text('Count: 0'), findsNothing);
    expect(find.text('This activity ended'), findsNothing);
    expect(find.text('This channel is unavailable'), findsNothing);

    await app.dispose(tester);
  });

  group('paused', () {
    /// Registers a `paint` script that records each lifecycle event it
    /// gets, then draws the counter as the default mock view does.
    List<String> recordLifecycle(MockApp app) {
      final heard = <String>[];
      app.repository.registerMockView('paint', (viewId) {
        return (input) {
          final decoded = jsonDecode(input);
          if (decoded is Map && decoded['kind'] == 'lifecycle') {
            heard.add(decoded['event'] as String);
            return null;
          }
          if (decoded is Map && decoded['kind'] == 'init') {
            return jsonEncode({
              'view': viewId,
              'seq': 1,
              'ops': [
                {
                  'op': 'create',
                  'id': 1,
                  'block': 'text',
                  'props': {'text': 'Painting'},
                  'events': <String>[],
                  'children': <int>[],
                },
                {'op': 'roots', 'main': 1, 'modals': <Object?>[]},
              ],
            });
          }
          return null;
        };
      });
      return heard;
    }

    void window(WidgetTester tester, List<AppLifecycleState> states) {
      for (final state in states) {
        tester.binding.handleAppLifecycleStateChanged(state);
      }
    }

    // Road: PluginArea(paused) → controller.send → repository.viewInput →
    // the view's script. Fails if a change sends twice, or none.
    testWidgets('paused sends pause once and resume once', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1440, 900));
      final heard = recordLifecycle(app);
      final session = await _startPaint(tester, app);
      final paused = ValueNotifier(false);
      await _pumpArea(
        tester,
        app,
        ActivityPlace(_serverKey, session),
        paused: paused,
      );
      expect(find.text('Painting'), findsOneWidget);
      heard.clear();

      paused.value = true;
      await tester.pumpAndSettle();
      expect(heard, ['pause']);

      // Rebuilt still paused: nothing new.
      await _pumpArea(
        tester,
        app,
        ActivityPlace(_serverKey, session),
        paused: paused,
      );
      expect(heard, ['pause']);

      paused.value = false;
      await tester.pumpAndSettle();
      expect(heard, ['pause', 'resume']);

      await app.dispose(tester);
    });

    // Fails if the window coming back resumes a view the stage paused, or
    // the window hiding tells a paused view `pause` a second time.
    testWidgets('the window hiding while paused sends nothing more', (
      tester,
    ) async {
      final app = await MockApp.pump(tester, size: const Size(1440, 900));
      final heard = recordLifecycle(app);
      final session = await _startPaint(tester, app);
      final paused = ValueNotifier(false);
      await _pumpArea(
        tester,
        app,
        ActivityPlace(_serverKey, session),
        paused: paused,
      );
      heard.clear();

      paused.value = true;
      await tester.pumpAndSettle();
      expect(heard, ['pause']);

      window(tester, [AppLifecycleState.inactive, AppLifecycleState.hidden]);
      await tester.pump();
      expect(heard, ['pause']);

      window(tester, [AppLifecycleState.inactive, AppLifecycleState.resumed]);
      await tester.pump();
      expect(heard, ['pause']);

      // The window hidden, then the stage shows it: still hidden, so no
      // resume until the window is back.
      window(tester, [AppLifecycleState.inactive, AppLifecycleState.hidden]);
      paused.value = false;
      await tester.pumpAndSettle();
      expect(heard, ['pause']);

      window(tester, [AppLifecycleState.inactive, AppLifecycleState.resumed]);
      await tester.pump();
      expect(heard, ['pause', 'resume']);

      // Not paused: the window alone still pauses and resumes it.
      window(tester, [AppLifecycleState.inactive, AppLifecycleState.hidden]);
      await tester.pump();
      window(tester, [AppLifecycleState.inactive, AppLifecycleState.resumed]);
      await tester.pump();
      expect(heard, ['pause', 'resume', 'pause', 'resume']);

      await app.dispose(tester);
    });
  });
}
