import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/settings/server/music/lavalink_section.dart';
import 'package:opencord/features/settings/server/music/music_page.dart';
import 'package:opencord/features/settings/server/music/node_dialog.dart';
import 'package:opencord/features/settings/server/music/node_test_dialog.dart';
import 'package:opencord/features/settings/server_settings.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/settings.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _berlin = 'rust-berlin.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inSettings(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

Future<void> _openMusic(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('Opencord Dev'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text('Server settings'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(_inSettings(find.text('Music')).first);
  await _pumpFor(tester, const Duration(milliseconds: 400));
  expect(find.byType(ServerMusicPage), findsOneWidget);
}

Future<void> _tapVisible(WidgetTester tester, Finder finder) async {
  await tester.ensureVisible(finder);
  await tester.pump();
  await tester.tap(finder);
  await tester.pump();
}

/// The text field with [label] in the dialog on top.
Finder _field(String label) => find.descendant(
  of: find.bySemanticsLabel(label),
  matching: find.byType(EditableText),
);

Finder _row(String title) => find
    .ancestor(
      of: _inSettings(find.text(title)),
      matching: find.byType(SettingsRow),
    )
    .first;

String _subtitleOf(WidgetTester tester, String title) =>
    tester.widget<SettingsRow>(_row(title)).subtitle ?? '';

Future<void> _addNode(
  WidgetTester tester, {
  required String name,
  required String host,
  String password = 'youshallnotpass',
}) async {
  await _tapVisible(
    tester,
    _inSettings(find.widgetWithText(OcButton, 'Add node')),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  expect(find.byType(LavalinkNodeDialog), findsOneWidget);
  await tester.enterText(_field('Name'), name);
  await tester.enterText(_field('Host'), host);
  await tester.enterText(_field('Password'), password);
  await tester.pump();
  await tester.tap(find.widgetWithText(OcButton, 'Add node').last);
  await _pumpFor(tester, const Duration(milliseconds: 500));
  expect(find.byType(LavalinkNodeDialog), findsNothing);
}

void main() {
  testWidgets('Music settings need Manage server', (tester) async {
    final app = await MockApp.pump(tester);
    app.read(navigationProvider.notifier).openServer(_berlin);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    final owned = app.read(serverProvider(_dev)).data!;
    final berlin = app.read(serverProvider(_berlin)).data!;
    expect(
      serverSettingsPages(owned, _dev).map((page) => page.id),
      contains('music'),
    );
    expect(
      serverSettingsPages(berlin, _berlin).map((page) => page.id),
      isNot(contains('music')),
    );
    await app.dispose(tester);
  });

  testWidgets('a node is added, connects, and its events show', (tester) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);

    expect(_inSettings(find.text('No nodes yet')), findsOneWidget);
    expect(
      _inSettings(
        find.text('Lavalink connects to wss://chat.example.com:7710'),
      ),
      findsOneWidget,
    );
    expect(
      _inSettings(find.text('Sends audio to UDP 203.0.113.10:7712')),
      findsOneWidget,
    );

    await _addNode(tester, name: 'Home', host: 'lavalink.example.com');
    expect(_subtitleOf(tester, 'Home'), contains('Connecting'));
    await _pumpFor(tester, const Duration(milliseconds: 800));

    final subtitle = _subtitleOf(tester, 'Home');
    expect(subtitle, contains('lavalink.example.com:2333'));
    expect(subtitle, contains('Lavalink 4.2.2 · Healthy'));
    expect(subtitle, contains('Sources: youtube, soundcloud, bandcamp'));
    expect(
      _inSettings(find.text('Home connected (Lavalink 4.2.2)')),
      findsOneWidget,
    );
    expect(
      _inSettings(find.textContaining('1 node · 1 healthy · 0 channels')),
      findsOneWidget,
    );
    expect(_inSettings(find.text('Add another node')), findsOneWidget);

    // The page let go of the nodes when it closed.
    await tester.tapAt(const Offset(4, 4));
    await _pumpFor(tester, const Duration(milliseconds: 400));
    expect(app.container.exists(lavalinkProvider(_dev)), isFalse);
    await app.dispose(tester);
  });

  testWidgets('a wrong password is explained and replacing it reconnects', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _addNode(
      tester,
      name: 'Public EU',
      host: 'lava.example.net',
      password: 'wrong',
    );
    await _pumpFor(tester, const Duration(milliseconds: 800));
    expect(
      _subtitleOf(tester, 'Public EU'),
      allOf(contains('Unavailable'), contains('Lavalink refused the password')),
    );
    expect(
      _inSettings(find.textContaining('Music server unavailable, retrying')),
      findsOneWidget,
      reason: 'every node is down',
    );

    await _tapVisible(
      tester,
      find.descendant(
        of: _row('Public EU'),
        matching: find.bySemanticsLabel('More for Public EU'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(find.text('Edit'));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.text('Edit Public EU'), findsOneWidget);
    expect(find.text('Saved: leave empty to keep it'), findsOneWidget);
    await tester.enterText(_field('Password'), 'youshallnotpass');
    await tester.pump();
    await tester.tap(find.widgetWithText(OcButton, 'Save'));
    await _pumpFor(tester, const Duration(milliseconds: 1200));

    expect(_subtitleOf(tester, 'Public EU'), contains('Healthy'));
    expect(
      _inSettings(find.textContaining('Music server unavailable')),
      findsNothing,
    );
    await app.dispose(tester);
  });

  testWidgets('the connection test shows each step', (tester) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _addNode(tester, name: 'Home', host: 'lavalink.example.com');
    await _addNode(tester, name: 'Away', host: 'unreachable.example.com');
    await _pumpFor(tester, const Duration(milliseconds: 800));

    await _tapVisible(
      tester,
      find.descendant(
        of: _row('Home'),
        matching: find.widgetWithText(OcButton, 'Test'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 100));
    expect(find.byType(LavalinkTestDialog), findsOneWidget);
    expect(
      find.bySemanticsLabel('Reach Lavalink and sign in: Waiting'),
      findsOneWidget,
    );
    await _pumpFor(tester, const Duration(milliseconds: 2000));
    for (final kind in LavalinkTestStepKind.values) {
      expect(find.bySemanticsLabel('${kind.label}: Passed'), findsOneWidget);
    }
    expect(find.text('This node is ready to play music.'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Close'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    await _tapVisible(
      tester,
      find.descendant(
        of: _row('Away'),
        matching: find.widgetWithText(OcButton, 'Test'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 1500));
    expect(
      find.bySemanticsLabel('Reach Lavalink and sign in: Failed'),
      findsOneWidget,
    );
    expect(
      find.textContaining('Nothing is listening on unreachable.example.com'),
      findsWidgets,
    );
    expect(
      find.bySemanticsLabel('Open its WebSocket: Not run'),
      findsOneWidget,
    );
    expect(
      find.bySemanticsLabel('Lavalink reaches this server: Not run'),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  testWidgets('draining and disabling show on the row', (tester) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _addNode(tester, name: 'Home', host: 'lavalink.example.com');
    await _pumpFor(tester, const Duration(milliseconds: 800));

    await _tapVisible(
      tester,
      find.descendant(
        of: _row('Home'),
        matching: find.bySemanticsLabel('More for Home'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(find.text('Drain'));
    await _pumpFor(tester, const Duration(milliseconds: 500));
    expect(_subtitleOf(tester, 'Home'), contains('Draining'));

    await _tapVisible(
      tester,
      find.descendant(of: _row('Home'), matching: find.byType(OcSwitch)),
    );
    await _pumpFor(tester, const Duration(milliseconds: 500));
    expect(_subtitleOf(tester, 'Home'), contains('Disabled'));
    expect(
      tester
          .widget<OcButton>(
            find.descendant(
              of: _row('Home'),
              matching: find.widgetWithText(OcButton, 'Test'),
            ),
          )
          .onPressed,
      isNull,
    );
    await app.dispose(tester);
  });

  testWidgets('a pasted URL fills the port and TLS', (tester) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _tapVisible(
      tester,
      _inSettings(find.widgetWithText(OcButton, 'Add node')),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));

    await tester.enterText(_field('Host'), 'https://lava.example.org');
    await tester.pump();
    expect(
      tester.widget<EditableText>(_field('Host')).controller.text,
      'lava.example.org',
    );
    expect(tester.widget<EditableText>(_field('Port')).controller.text, '443');
    expect(
      tester
          .widget<Semantics>(
            find
                .ancestor(
                  of: find.byType(OcSwitch).first,
                  matching: find.byType(Semantics),
                )
                .first,
          )
          .properties
          .toggled,
      isTrue,
    );

    await tester.enterText(_field('Host'), 'lava example');
    await tester.pump();
    expect(
      find.text('Enter a name like lavalink.example.com or an IP address.'),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  test('addresses are read from what was typed or pasted', () {
    expect(parseNodeAddress('Lava.Example.com'), (
      host: 'lava.example.com',
      port: null,
      secure: null,
    ));
    expect(parseNodeAddress('http://10.0.0.5:2333'), (
      host: '10.0.0.5',
      port: 2333,
      secure: false,
    ));
    expect(parseNodeAddress('wss://lava.example.com'), (
      host: 'lava.example.com',
      port: null,
      secure: true,
    ));
    expect(nodeHostProblem('lava.example.com'), isNull);
    expect(nodeHostProblem('2001:db8::1'), isNull);
    expect(nodeHostProblem('lava.example.com/v4'), isNotNull);
    expect(nodeHostProblem('-lava'), isNotNull);
  });

  test('the node list keeps its summary and at most 50 events', () {
    const settings = LavalinkNodeSettings(name: 'A', host: 'a');
    var nodes = const LavalinkNodes(channelsPlaying: 7)
        .withNode(
          const LavalinkNode(
            id: 1,
            settings: settings,
            health: LavalinkHealth.healthy,
          ),
        )
        .withNode(
          const LavalinkNode(
            id: 2,
            settings: settings,
            health: LavalinkHealth.unavailable,
          ),
        )
        .withNode(
          const LavalinkNode(
            id: 3,
            settings: settings,
            health: LavalinkHealth.healthy,
          ),
        );
    expect(nodes.summary, '3 nodes · 2 healthy · 7 channels playing');
    expect(nodes.allDown, isFalse);
    nodes = nodes.without(1).without(3, channelsPlaying: 0);
    expect(nodes.summary, '1 node · 0 healthy · 0 channels playing');
    expect(nodes.allDown, isTrue);
    for (var i = 0; i < 60; i++) {
      nodes = nodes.withEvent(
        LavalinkNodeEvent(
          nodeId: 2,
          kind: LavalinkEventKind.connected,
          text: '$i',
          at: DateTime(2026),
        ),
      );
    }
    expect(nodes.events, hasLength(lavalinkEventsKept));
    expect(nodes.events.first.text, '59');
  });

  testWidgets('a status change rebuilds only its row', (tester) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _addNode(tester, name: 'One', host: 'one.example.com');
    await _addNode(tester, name: 'Two', host: 'two.example.com');
    await _pumpFor(tester, const Duration(milliseconds: 800));

    final notifier = app.read(lavalinkProvider(_dev).notifier);
    final nodes = app.read(lavalinkProvider(_dev)).value!;
    final two = nodes.nodes.firstWhere((node) => node.settings.name == 'Two');
    final before = tester.element(_row('One'));
    notifier.set(
      nodes.withNode(
        LavalinkNode(
          id: two.id,
          settings: two.settings,
          health: LavalinkHealth.degraded,
          reason: 'CPU load is 90 %',
        ),
      ),
    );
    await tester.pump();
    expect(_subtitleOf(tester, 'Two'), contains('CPU load is 90 %'));
    expect(tester.element(_row('One')), same(before));
    expect(find.byType(LavalinkNodeRow), findsNWidgets(2));
    await app.dispose(tester);
  });
}
