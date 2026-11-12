import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/plugins/activity_picker.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

import '../../support/app.dart';
import '../../support/floating_scenes.dart';

const _serverKey = 'opencord.example:7710';

/// A second registered activity, from a plugin whose name sorts before
/// "Paint", with no description (the row then shows the plugin's name).
const _poker = PluginActivity(
  pluginId: 'cards',
  activityKey: 'poker',
  name: 'Poker night',
  description: '',
  pluginName: 'Cards',
);

Finder _dock() => find.byKey(ShellKeys.dock);

Finder _activitiesButton() =>
    find.descendant(of: _dock(), matching: find.bySemanticsLabel('Activities'));

Finder _row(String name) =>
    find.ancestor(of: find.text(name), matching: find.byType(Hoverable)).first;

/// Changes the mock world before the app shows it, then sends the Ready
/// the server would send: the road is mock world → `serverProvider` /
/// `activitySessionsProvider` → the dock.
Future<void> _reload(MockApp app, WidgetTester tester) async {
  final server = app.repository.mockServer(_serverKey);
  app.repository.emit(Ready(_serverKey, server.snapshot()));
  await tester.pumpAndSettle();
}

/// The member stops being the owner and holds every permission but
/// [without], through @everyone only.
void _withoutPermission(MockServer server, Permissions without) {
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
}

Future<MockApp> _onVoice(
  WidgetTester tester, {
  void Function(MockServer server)? world,
}) async {
  final app = await MockApp.pump(tester, size: const Size(1280, 800));
  if (world != null) {
    world(app.repository.mockServer(_serverKey));
    await _reload(app, tester);
  }
  await openVoicePage(SceneDriver(tester));
  return app;
}

/// The call this device is in, as the dock sees it.
({String server, int channel}) _call(MockApp app) {
  final voice = app.read(voiceSessionProvider);
  return (server: voice.serverKey!, channel: voice.channelId!);
}

Future<void> _openPicker(WidgetTester tester) async {
  await tester.tap(_activitiesButton());
  await tester.pumpAndSettle();
  expect(find.byType(ActivityPicker), findsOneWidget);
}

void main() {
  // Road: mock world → serverProvider (pluginActivities, channel
  // permissions) → VoiceDock. Fails if the button ignores either condition.
  group('the button shows only with an activity and the permission', () {
    testWidgets('no registered activity: no button', (tester) async {
      final app = await _onVoice(
        tester,
        world: (server) => server.pluginActivities.clear(),
      );
      // The soundboard's button shows, so the dock is there.
      expect(
        find.descendant(
          of: _dock(),
          matching: find.bySemanticsLabel('Soundboard'),
        ),
        findsOneWidget,
      );
      expect(_activitiesButton(), findsNothing);
      await app.dispose(tester);
    });

    testWidgets('an activity without the permission: no button', (
      tester,
    ) async {
      final app = await _onVoice(
        tester,
        world: (server) =>
            _withoutPermission(server, Permissions.startActivities),
      );
      expect(app.read(voiceSessionProvider).channelId, isNotNull);
      expect(
        app.read(serverProvider(_serverKey)).data!.pluginActivities,
        isNotEmpty,
      );
      expect(_activitiesButton(), findsNothing);
      await app.dispose(tester);
    });

    testWidgets('both: the button', (tester) async {
      final app = await _onVoice(tester);
      expect(_activitiesButton(), findsOneWidget);
      await app.dispose(tester);
    });
  });

  // Road: dock → picker → MockRepository.startActivity → the started event →
  // activitySessionsProvider. Two activities, so a row that starts the other
  // one's key fails.
  testWidgets('pressing a row starts it in the call\'s channel', (
    tester,
  ) async {
    final app = await _onVoice(
      tester,
      world: (server) => server.pluginActivities.add(_poker),
    );
    final call = _call(app);
    await _openPicker(tester);

    // By plugin name: Cards before Paint.
    expect(
      tester.getTopLeft(find.text('Poker night')).dy,
      lessThan(tester.getTopLeft(find.text('Paint together')).dy),
    );
    // No description: the plugin's name instead.
    expect(
      find.descendant(of: _row('Poker night'), matching: find.text('Cards')),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: _row('Paint together'),
        matching: find.text('A shared whiteboard'),
      ),
      findsOneWidget,
    );

    await tester.tap(find.text('Poker night'));
    await tester.pumpAndSettle();

    expect(find.byType(ActivityPicker), findsNothing);
    final sessions = app
        .read(activitySessionsProvider(call.server))
        .values
        .toList();
    expect(sessions, hasLength(1));
    expect(sessions.single.channelId, call.channel);
    expect(sessions.single.pluginId, 'cards');
    expect(sessions.single.activityKey, 'poker');
    await app.dispose(tester);
  });

  // Road: MockRepository.startActivity → started event →
  // activitySessionsProvider → picker row.
  testWidgets('a running activity says Running', (tester) async {
    final app = await _onVoice(
      tester,
      world: (server) => server.pluginActivities.add(_poker),
    );
    final call = _call(app);
    await app.repository.startActivity(
      call.server,
      call.channel,
      'paint',
      'paint',
    );
    await tester.pumpAndSettle();
    await _openPicker(tester);

    expect(
      find.descendant(
        of: _row('Paint together'),
        matching: find.text('Running'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(of: _row('Poker night'), matching: find.text('Running')),
      findsNothing,
    );

    // Pressing it again lands on the running session: still one.
    await tester.tap(find.text('Paint together'));
    await tester.pumpAndSettle();
    expect(find.byType(ActivityPicker), findsNothing);
    expect(app.read(activitySessionsProvider(call.server)), hasLength(1));
    await app.dispose(tester);
  });

  // Road: four sessions through MockRepository.startActivity →
  // activitySessionsProvider → picker. Fails if the cap isn't read from the
  // call's sessions.
  testWidgets('a full call disables the rest', (tester) async {
    final extra = [
      for (var i = 1; i <= 4; i++)
        PluginActivity(
          pluginId: 'games',
          activityKey: 'game$i',
          name: 'Game $i',
          description: 'Game number $i',
          pluginName: 'Games',
        ),
    ];
    final app = await _onVoice(
      tester,
      world: (server) => server.pluginActivities.addAll(extra),
    );
    final call = _call(app);
    for (final activity in extra) {
      await app.repository.startActivity(
        call.server,
        call.channel,
        activity.pluginId,
        activity.activityKey,
      );
    }
    await tester.pumpAndSettle();
    await _openPicker(tester);

    expect(find.text('This call already has 4 activities.'), findsOneWidget);
    expect(tester.widget<Hoverable>(_row('Paint together')).onTap, isNull);
    expect(tester.widget<Hoverable>(_row('Game 1')).onTap, isNotNull);

    // The disabled row starts nothing and keeps the picker open.
    await tester.tap(find.text('Paint together'));
    await tester.pumpAndSettle();
    expect(find.byType(ActivityPicker), findsOneWidget);
    expect(app.read(activitySessionsProvider(call.server)), hasLength(4));
    await app.dispose(tester);
  });

  // Road: picker → MockRepository.startActivity refusing as the server does
  // → toast. The world changes without a Ready, so the app still offers the
  // row and only the server knows.
  for (final (why, refuse, toast)
      in <(String, void Function(MockServer), String)>[
        (
          'unavailable',
          (server) => server.pluginActivities.clear(),
          "This activity isn't available right now.",
        ),
        (
          'limit',
          (server) {
            final call = server.channels.values.firstWhere(
              (channel) =>
                  channel.kind == ChannelKind.voice &&
                  channel.name == 'General',
            );
            for (var i = 1; i <= 4; i++) {
              server.activitySessions[900 + i] = ActivitySession(
                id: 900 + i,
                channelId: call.id,
                pluginId: 'games',
                activityKey: 'game$i',
                name: 'Game $i',
                pluginName: 'Games',
                starterId: server.selfId,
                startedAt: DateTime.utc(2026, 11, 12),
              );
            }
          },
          'This call already has 4 activities.',
        ),
        (
          'forbidden',
          (server) => _withoutPermission(server, Permissions.startActivities),
          "You don't have permission to start activities here.",
        ),
      ]) {
    testWidgets('a refused start says why ($why)', (tester) async {
      final app = await _onVoice(tester);
      await _openPicker(tester);
      refuse(app.repository.mockServer(_serverKey));

      await tester.tap(find.text('Paint together'));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 300));

      expect(find.byType(ActivityPicker), findsNothing);
      expect(find.text(toast), findsOneWidget);
      // Lets the toast's timer run out.
      for (var waited = 0; waited < 3000; waited += 100) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      await app.dispose(tester);
    });
  }

  // Road: dock button → VoiceDockActivityNotifier.hold while the popover is
  // open → release when it closes.
  testWidgets('the dock stays while the picker is open', (tester) async {
    final app = await _onVoice(tester);
    await _openPicker(tester);

    for (var waited = 0; waited < 5000; waited += 50) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    final ignore = tester.widget<IgnorePointer>(
      find.ancestor(of: _dock(), matching: find.byType(IgnorePointer)).first,
    );
    expect(ignore.ignoring, isFalse);

    await tester.tapAt(const Offset(50, 50));
    await tester.pumpAndSettle();
    expect(find.byType(ActivityPicker), findsNothing);
    for (var waited = 0; waited < 2700; waited += 50) {
      await tester.pump(const Duration(milliseconds: 50));
    }
    await tester.pumpAndSettle();
    final after = tester.widget<IgnorePointer>(
      find.ancestor(of: _dock(), matching: find.byType(IgnorePointer)).first,
    );
    expect(after.ignoring, isTrue);
    await app.dispose(tester);
  });
}
