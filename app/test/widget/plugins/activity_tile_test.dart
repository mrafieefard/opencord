import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/plugins/activity_tile.dart';

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

Future<MockApp> _onStage(WidgetTester tester) async {
  final app = await MockApp.pump(tester, size: const Size(1440, 900));
  recordPaint(app);
  await openVoicePage(SceneDriver(tester));
  return app;
}

/// Opens the tile's menu with a secondary press.
Future<void> _menu(WidgetTester tester, int session) async {
  await tester.tapAt(
    tester.getRect(_activity(session)).topCenter + const Offset(0, 40),
    buttons: kSecondaryMouseButton,
  );
  await tester.pumpAndSettle();
}

/// The member stops being the owner and holds every permission but
/// [without], through @everyone only; the Ready the server would send
/// carries it to the app.
Future<void> _withoutPermission(
  WidgetTester tester,
  MockApp app,
  Permissions without,
) async {
  final server = app.repository.mockServer(activityServer);
  server.info = ServerInfo(
    serverIdHex: server.info.serverIdHex,
    name: server.info.name,
    ownerId: 999999,
    everyoneRoleId: server.info.everyoneRoleId,
  );
  server.roles[server.info.everyoneRoleId] = Role(
    id: server.info.everyoneRoleId,
    name: '@everyone',
    permissions: Permissions.all - Permissions.administrator - without,
    position: 0,
  );
  server.members[server.selfId] = server.members[server.selfId]!.copyWith(
    roleIds: [],
  );
  app.repository.emit(Ready(activityServer, server.snapshot()));
  await tester.pumpAndSettle();
}

void main() {
  // Road: MockRepository.startActivity, then ActivitySessionChanged as the
  // server sends it when someone opens or closes it → activitySessionsProvider
  // → the tile's lines.
  testWidgets('its tile says who started it and how many have it open', (
    tester,
  ) async {
    final app = await _onStage(tester);
    final server = app.repository.mockServer(activityServer);
    final session = await startPaint(tester, app);
    await _settle(tester);
    final self = server.members[server.selfId]!.displayName;

    expect(
      _inActivity(session.id, find.text('Paint together')),
      findsOneWidget,
    );
    expect(
      _inActivity(session.id, find.text('Started by $self')),
      findsOneWidget,
    );
    expect(
      _inActivity(session.id, find.text('No one has it open')),
      findsOneWidget,
    );

    final others = server.members.keys
        .where((id) => id != server.selfId)
        .take(3)
        .toList();
    for (final (viewers, line) in [
      (others.take(1).toList(), '1 person has it open'),
      (others, '3 people have it open'),
    ]) {
      app.repository.emit(
        ActivitySessionChanged(
          activityServer,
          session.copyWith(viewerIds: viewers),
        ),
      );
      await _settle(tester);
      expect(_inActivity(session.id, find.text(line)), findsOneWidget);
    }
    await app.dispose(tester);
  });

  group('End activity asks, then ends it for everyone', () {
    // Road: secondary press → OcMenu → confirmAction → endActivity →
    // ActivitySessionEnded → the tile goes. Cancel ends nothing.
    testWidgets('the starter', (tester) async {
      final app = await _onStage(tester);
      final session = await startPaint(tester, app);
      await _settle(tester);

      await _menu(tester, session.id);
      expect(find.text('Open'), findsWidgets);
      await tester.tap(find.text('End activity'));
      await tester.pumpAndSettle();
      expect(find.text('End Paint together for everyone?'), findsOneWidget);
      expect(find.text('It closes for everyone in the call.'), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(
        app.read(activitySessionsProvider(activityServer)),
        contains(session.id),
      );
      expect(_activity(session.id), findsOneWidget);

      await _menu(tester, session.id);
      await tester.tap(find.text('End activity'));
      await tester.pumpAndSettle();
      // The dialog's button, not the menu's (which has closed).
      await tester.tap(find.text('End activity'));
      await _settle(tester);

      expect(
        app.read(activitySessionsProvider(activityServer)),
        isNot(contains(session.id)),
      );
      expect(_activity(session.id), findsNothing);
      await app.dispose(tester);
    });

    // Someone else started it (Mira), and the member can't manage the
    // channel: no End activity. Then a member who manages the channel
    // and didn't start it: End activity is there.
    testWidgets('neither the starter nor a channel manager', (tester) async {
      final app = await _onStage(tester);
      final call = callOf(app);
      final server = app.repository.mockServer(activityServer);
      const mira = 1002;
      expect(server.members[mira]?.displayName, 'Mira Okafor');
      final session = ActivitySession(
        id: 777,
        channelId: call.channel,
        pluginId: 'paint',
        activityKey: 'paint',
        name: 'Paint together',
        pluginName: 'Paint',
        starterId: mira,
        startedAt: DateTime.utc(2026, 11, 12),
      );
      server.activitySessions[session.id] = session;
      app.repository.emit(ActivitySessionStarted(activityServer, session));
      await _settle(tester);
      await _withoutPermission(tester, app, Permissions.manageChannels);
      await _settle(tester);

      await _menu(tester, session.id);
      expect(find.text('Open'), findsWidgets);
      expect(find.text('End activity'), findsNothing);
      await tester.tapAt(const Offset(5, 5));
      await tester.pumpAndSettle();

      await _withoutPermission(tester, app, Permissions.none);
      await _settle(tester);
      await _menu(tester, session.id);
      expect(find.text('End activity'), findsOneWidget);
      await tester.tapAt(const Offset(5, 5));
      await tester.pumpAndSettle();
      await app.dispose(tester);
    });
  });
}
