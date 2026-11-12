import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/plugins/activity_tile.dart';
import 'package:opencord/features/plugins/plugin_area.dart';
import 'package:opencord/features/voice/voice_focus.dart';
import 'package:opencord/features/voice/voice_tile.dart';

import '../../support/activities.dart';
import '../../support/app.dart';
import '../../support/floating_scenes.dart';

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

/// On the voice page in General, joined, with [recordPaint]'s script for
/// the `paint` plugin.
Future<(MockApp, PaintRecorder)> _onStage(WidgetTester tester) async {
  final app = await MockApp.pump(tester, size: const Size(1440, 900));
  final paint = recordPaint(app);
  await openVoicePage(SceneDriver(tester));
  return (app, paint);
}

/// Presses the tile's Open: the road is ActivityTile → openActivitiesProvider
/// → pluginViewProvider(ActivityPlace) → MockRepository.openActivityView.
Future<void> _open(WidgetTester tester, ActivitySession session) async {
  await tester.tap(_inActivity(session.id, find.text('Open')));
  await _settle(tester);
}

String _viewIdOf(MockApp app, ActivitySession session) => app
    .read(pluginViewProvider(ActivityPlace(activityServer, session)))
    .viewId
    .value!;

void main() {
  // Road: MockRepository.startActivity → ActivitySessionStarted →
  // activitySessionsProvider → voiceTiles → the stage. Fails if the tile
  // goes before the people or doesn't come.
  testWidgets('a session adds a tile after the people', (tester) async {
    final (app, _) = await _onStage(tester);
    final people = find.byType(VoiceTileView).evaluate().length;
    expect(people, greaterThan(1));

    final session = await startPaint(tester, app);
    await _settle(tester);

    expect(_activity(session.id), findsOneWidget);
    expect(find.byType(VoiceTileView), findsNWidgets(people));
    // Last in reading order: below, or right of, every person and screen.
    final activity = tester.getRect(_activity(session.id));
    for (final element in find.byType(VoiceTileView).evaluate()) {
      final tile = tester.getRect(find.byWidget(element.widget));
      expect(
        tile.top < activity.top - 1 ||
            ((tile.top - activity.top).abs() < 1 && tile.left < activity.left),
        isTrue,
        reason: '$tile comes before $activity',
      );
    }
    await app.dispose(tester);
  });

  // Road: ActivityTile's Open → openActivitiesProvider → PluginArea in the
  // tile → the mock's view; its button's press goes to the mock's script
  // and its answer comes back drawn.
  testWidgets('Open draws the plugin\'s screen in the tile', (tester) async {
    final (app, _) = await _onStage(tester);
    final session = await startPaint(tester, app);
    await _settle(tester);
    expect(_inActivity(session.id, find.byType(PluginArea)), findsNothing);

    await _open(tester, session);

    expect(app.read(openActivitiesProvider), {session.id});
    expect(_inActivity(session.id, find.text('Count: 0')), findsOneWidget);
    await tester.tap(_inActivity(session.id, find.text('Count')));
    await _settle(tester);
    expect(_inActivity(session.id, find.text('Count: 1')), findsOneWidget);
    await app.dispose(tester);
  });

  // Road: the bar's Make large → voiceFocusProvider → the stage's focus
  // layout; Back to grid toggles it off.
  testWidgets('Make large focuses it and the strip shows the others', (
    tester,
  ) async {
    final (app, _) = await _onStage(tester);
    final session = await startPaint(tester, app);
    await _settle(tester);
    await _open(tester, session);
    final call = callOf(app);

    await tester.tap(_inActivity(session.id, find.text('Make large')));
    await _settle(tester);

    expect(app.read(voiceFocusProvider((server: call.server, channel: call.channel))), (
      userId: 0,
      screen: false,
      session: session.id,
    ));
    expect(tester.widget<ActivityTile>(_activity(session.id)).focused, isTrue);
    expect(tester.getSize(_activity(session.id)).height, greaterThan(500));
    for (final element in find.byType(VoiceTileView).evaluate()) {
      expect((element.widget as VoiceTileView).small, isTrue);
    }
    expect(_inActivity(session.id, find.text('Count: 0')), findsOneWidget);

    await tester.tap(_inActivity(session.id, find.text('Back to grid')));
    await _settle(tester);
    expect(
      app.read(voiceFocusProvider((server: call.server, channel: call.channel))),
      isNull,
    );
    expect(_inActivity(session.id, find.text('Make large')), findsOneWidget);
    await app.dispose(tester);
  });

  // Road: a person's tile focused → the activity's tile goes to the strip
  // with `small` → no PluginArea, and openActivitiesProvider's view is
  // told pause; back to the grid it's drawn again and told resume.
  testWidgets('in the strip an opened activity is only its name', (
    tester,
  ) async {
    final (app, paint) = await _onStage(tester);
    final session = await startPaint(tester, app);
    await _settle(tester);
    await _open(tester, session);
    final viewId = _viewIdOf(app, session);
    expect(paint.pauses, isEmpty);

    final person = find.byType(VoiceTileView).first;
    await tester.tapAt(tester.getRect(person).topCenter + const Offset(0, 3));
    await _settle(tester);

    final tile = tester.widget<ActivityTile>(_activity(session.id));
    expect(tile.small, isTrue);
    expect(_inActivity(session.id, find.text('Paint together')), findsOneWidget);
    expect(_inActivity(session.id, find.byType(PluginArea)), findsNothing);
    expect(_inActivity(session.id, find.text('Open')), findsNothing);
    expect(_inActivity(session.id, find.text('Close')), findsNothing);
    expect(paint.pauses, ['pause']);

    // Pressing it in the strip focuses it, drawn again and running. Near
    // its top: the control bar lies over the bottom of the stage.
    await tester.tapAt(
      tester.getRect(_activity(session.id)).topCenter + const Offset(0, 3),
    );
    await _settle(tester);
    expect(tester.widget<ActivityTile>(_activity(session.id)).focused, isTrue);
    expect(_inActivity(session.id, find.text('Count: 0')), findsOneWidget);
    expect(paint.pauses, ['pause', 'resume']);
    expect(_viewIdOf(app, session), viewId);
    expect(app.repository.closedViews, isEmpty);
    await app.dispose(tester);
  });

  // Road: the bar's Close → openActivitiesProvider without the id →
  // pluginViewProvider disposed → MockRepository.closeView. The session
  // stays in activitySessionsProvider and the tile offers Open again.
  testWidgets('Close closes my view and leaves the tile', (tester) async {
    final (app, _) = await _onStage(tester);
    final session = await startPaint(tester, app);
    await _settle(tester);
    await _open(tester, session);
    final viewId = _viewIdOf(app, session);

    await tester.tap(_inActivity(session.id, find.text('Close')));
    await _settle(tester);

    expect(app.repository.closedViews, [viewId]);
    expect(app.read(openActivitiesProvider), isEmpty);
    expect(
      app.read(activitySessionsProvider(activityServer)),
      contains(session.id),
    );
    expect(_inActivity(session.id, find.text('Open')), findsOneWidget);
    expect(_inActivity(session.id, find.byType(PluginArea)), findsNothing);
    await app.dispose(tester);
  });

  // Review focus 5. Road: openActivitiesProvider keeps the view while the
  // page shows #general (no PluginArea anywhere), tells it pause there and
  // resume when the voice page draws it again.
  testWidgets('an open activity survives leaving the voice page', (
    tester,
  ) async {
    final (app, paint) = await _onStage(tester);
    final scene = SceneDriver(tester);
    final session = await startPaint(tester, app);
    await _settle(tester);
    await _open(tester, session);
    final viewId = _viewIdOf(app, session);
    await tester.tap(_inActivity(session.id, find.text('Count')));
    await _settle(tester);
    await tester.tap(_inActivity(session.id, find.text('Count')));
    await _settle(tester);
    expect(_inActivity(session.id, find.text('Count: 2')), findsOneWidget);
    expect(paint.pauses, isEmpty);

    await scene.pickInLeft('general');
    await _settle(tester);
    expect(find.byType(PluginArea), findsNothing);
    expect(paint.pauses, ['pause']);

    await scene.pickInLeft('General');
    await _settle(tester);

    expect(_viewIdOf(app, session), viewId);
    expect(_inActivity(session.id, find.text('Count: 2')), findsOneWidget);
    expect(paint.pauses, ['pause', 'resume']);
    expect(app.repository.closedViews, isEmpty);
    await app.dispose(tester);
  });

  // Road: voiceProvider (VoiceChanged) and speakingProvider →
  // openActivitiesProvider → controller.send → the mock view's script.
  testWidgets('the call input follows who is in the call and who speaks', (
    tester,
  ) async {
    final (app, paint) = await _onStage(tester);
    final call = callOf(app);
    final session = await startPaint(tester, app);
    await _settle(tester);
    final before = app.read(voiceProvider(call.server))[call.channel]!;
    List<String> ids(Iterable<int> ids) => [
      for (final id in {...ids}.toList()..sort()) '$id',
    ];

    final members = before.map((participant) => participant.userId).toSet();
    app.read(speakingProvider(call.server).notifier).set({members.first});
    await tester.pump();

    await _open(tester, session);
    expect(paint.calls, [
      {
        'kind': 'call',
        'members': ids(members),
        'speaking': ids({members.first}),
      },
    ]);

    // Someone joins.
    const joining = 4242;
    app.repository.emit(
      VoiceChanged(call.server, call.channel, [
        ...before,
        const VoiceParticipant(userId: joining),
      ]),
    );
    await tester.pump(const Duration(milliseconds: 150));
    expect(paint.calls, hasLength(2));
    expect(paint.calls.last['members'], ids({...members, joining}));
    expect(paint.calls.last['speaking'], ids({members.first}));

    // Ten speaking changes in 50 ms: one input, with the last set.
    final speakers = members.toList();
    for (var i = 0; i < 10; i++) {
      app.read(speakingProvider(call.server).notifier).set({
        speakers[i % speakers.length],
        if (i.isOdd) joining,
      });
      await tester.pump(const Duration(milliseconds: 5));
    }
    await tester.pump(const Duration(milliseconds: 150));
    expect(paint.calls, hasLength(3));
    expect(paint.calls.last['speaking'], ids({speakers[9 % speakers.length], joining}));
    await app.dispose(tester);
  });

  // Road: VoiceSessionNotifier.leave → openActivitiesProvider emptied →
  // pluginViewProvider disposed → MockRepository.closeView.
  testWidgets('leaving the call closes the opened activities', (tester) async {
    final (app, _) = await _onStage(tester);
    final session = await startPaint(tester, app);
    await _settle(tester);
    await _open(tester, session);
    final viewId = _viewIdOf(app, session);

    await app.read(voiceSessionProvider.notifier).leave();
    await _settle(tester);

    expect(app.read(openActivitiesProvider), isEmpty);
    expect(app.repository.closedViews, contains(viewId));
    await app.dispose(tester);
  });

  // Road: MockRepository.endActivity → ActivitySessionEnded →
  // activitySessionsProvider → the stage drops the tile and its focus;
  // openActivitiesProvider prunes the id.
  testWidgets('an ended activity\'s tile goes, and focus with it', (
    tester,
  ) async {
    final (app, _) = await _onStage(tester);
    final session = await startPaint(tester, app);
    await _settle(tester);
    await _open(tester, session);
    await tester.tap(_inActivity(session.id, find.text('Make large')));
    await _settle(tester);
    expect(tester.widget<ActivityTile>(_activity(session.id)).focused, isTrue);

    await app.repository.endActivity(activityServer, session.id);
    await _settle(tester);

    expect(_activity(session.id), findsNothing);
    expect(app.read(openActivitiesProvider), isEmpty);
    for (final element in find.byType(VoiceTileView).evaluate()) {
      final tile = element.widget as VoiceTileView;
      expect(tile.small || tile.focused, isFalse);
    }
    await app.dispose(tester);
  });
}
