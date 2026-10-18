// Chat commands in the composer (Phase 3 plan §17, §19.6): the command
// list with dimmed entries and their reasons, the chip with inline options,
// /play's suggestions, public results and "only you" replies, the app's own
// /volume and /music, and the Commands page.

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/command_input.dart';
import 'package:opencord/features/chat/command_rows.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/suggestion_list.dart';
import 'package:opencord/features/settings/server/commands_page.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _settle(WidgetTester tester) =>
    _pumpFor(tester, const Duration(milliseconds: 700));

int _channelId(MockApp app, String name, {ChannelKind? kind}) => app
    .read(serverProvider(_server))
    .data!
    .channels
    .values
    .firstWhere(
      (channel) =>
          channel.name == name && (kind == null || channel.kind == kind),
    )
    .id;

Future<void> _open(WidgetTester tester, MockApp app, String name) async {
  app
      .read(navigationProvider.notifier)
      .openChannel(_server, _channelId(app, name));
  await _settle(tester);
}

/// Joins General from the sidebar, then goes back to #general.
Future<void> _joinVoice(WidgetTester tester, MockApp app) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('General'),
    ),
  );
  await _settle(tester);
  await _open(tester, app, 'general');
}

Finder get _input => find.descendant(
  of: find.byType(Composer),
  matching: find.byType(TextField),
);

Finder _inList(Finder finder) =>
    find.descendant(of: find.byType(SuggestionList), matching: finder);

Future<void> _press(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyEvent(key);
  await tester.pump();
}

/// Types "/name", picks it, and waits for the chip.
Future<void> _choose(WidgetTester tester, String name) async {
  await tester.enterText(_input, '/$name');
  await tester.pump();
  await _press(tester, LogicalKeyboardKey.enter);
  await _settle(tester);
}

Finder _option(String name) => find.descendant(
  of: find.byType(CommandInput),
  matching: find.byType(TextField),
);

void main() {
  testWidgets('"/" lists the commands, dimmed with why where they fail', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await tester.enterText(_input, '/');
    await tester.pump();

    expect(_inList(find.text('/play')), findsOneWidget);
    // Not in voice: music commands say so.
    expect(_inList(find.text('Join a voice channel first')), findsWidgets);
    // The app's own work anywhere.
    await tester.enterText(_input, '/vo');
    await tester.pump();
    expect(_inList(find.text('/volume')), findsOneWidget);
    expect(
      _inList(find.text('Your music volume, on every server')),
      findsOneWidget,
    );

    await tester.enterText(_input, '/np');
    await tester.pump();
    expect(_inList(find.text('/nowplaying')), findsOneWidget, reason: 'alias');
    await app.dispose(tester);
  });

  testWidgets('/play takes a suggestion and posts its result for everyone', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _joinVoice(tester, app);
    await _choose(tester, 'play');
    expect(
      find.descendant(
        of: find.byType(CommandInput),
        matching: find.text('/play'),
      ),
      findsOneWidget,
    );
    expect(find.text('query:'), findsOneWidget);

    await tester.enterText(_option('query').first, 'lofi');
    // Suggestions after 400 ms without typing.
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.textContaining('lofi 2 — Mock Artist'), findsNothing);
    await _pumpFor(tester, const Duration(milliseconds: 500));
    expect(find.textContaining('lofi 2 — Mock Artist'), findsOneWidget);
    await tester.tap(find.textContaining('lofi 2 — Mock Artist'));
    await tester.pump();
    await _press(tester, LogicalKeyboardKey.enter);
    await _settle(tester);

    expect(app.repository.commandsRun, ['play']);
    expect(find.byType(CommandInput), findsNothing, reason: 'back to text');
    expect(find.byType(CommandResultLine), findsOneWidget);
    expect(find.textContaining('used'), findsWidgets);
    await app.dispose(tester);
  });

  testWidgets('"only you" replies show here alone and can be dismissed', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _joinVoice(tester, app);
    await _choose(tester, 'nowplaying');
    await _press(tester, LogicalKeyboardKey.enter);
    await _settle(tester);

    expect(find.byType(EphemeralLine), findsOneWidget);
    expect(find.text('Nothing is playing in General'), findsOneWidget);
    expect(find.textContaining('Only you can see this'), findsOneWidget);
    await tester.tap(find.text('Dismiss'));
    await tester.pump();
    expect(find.byType(EphemeralLine), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('/volume and /music stay in the app', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _joinVoice(tester, app);
    await _choose(tester, 'volume');
    await tester.enterText(_option('percent').first, '150');
    await _press(tester, LogicalKeyboardKey.enter);
    await _settle(tester);
    expect(app.read(musicListeningProvider).volume, 150);
    expect(app.repository.musicListening.volume, 150);

    await _choose(tester, 'music');
    await _press(tester, LogicalKeyboardKey.enter);
    await _settle(tester);
    expect(app.read(rightPanelModeProvider), RightPanelMode.music);
    expect(app.repository.commandsRun, isEmpty, reason: 'never sent');
    await app.dispose(tester);
  });

  testWidgets('Escape leaves a command; Tab moves between its options', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _joinVoice(tester, app);
    await _choose(tester, 'play');
    final fields = _option('query');
    expect(tester.widget<TextField>(fields.first).focusNode!.hasFocus, isTrue);
    await _press(tester, LogicalKeyboardKey.tab);
    expect(tester.widget<TextField>(fields.first).focusNode!.hasFocus, isFalse);
    await _press(tester, LogicalKeyboardKey.escape);
    await tester.pump();
    expect(find.byType(CommandInput), findsNothing);
    expect(_input, findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('what the server refuses is an error only for you', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _joinVoice(tester, app);
    await _choose(tester, 'pause');
    // Music goes away meanwhile: the server no longer has /pause.
    app.repository.debugSetMusicConfigured(_server, configured: false);
    await tester.pump();
    await _press(tester, LogicalKeyboardKey.enter);
    await _settle(tester);
    final note = tester.widget<EphemeralLine>(find.byType(EphemeralLine));
    expect(note.note.error, isTrue);
    expect(note.note.content, contains('/pause'));
    await app.dispose(tester);
  });

  testWidgets('the Commands page turns a command off for everyone', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.text('Opencord Dev'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(find.text('Server settings'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(
      find
          .descendant(
            of: find.byType(SettingsDialog),
            matching: find.text('Commands'),
          )
          .first,
    );
    await _settle(tester);
    expect(find.byType(ServerCommandsPage), findsOneWidget);
    expect(find.text('/clear'), findsOneWidget);
    expect(find.text('Allowed in: All channels'), findsWidgets);

    final clear = find.byWidgetPredicate(
      (widget) => widget is OcSwitch && widget.semanticLabel == '/clear',
    );
    await tester.ensureVisible(clear);
    await tester.tap(clear);
    await _settle(tester);
    final commands = app.read(serverProvider(_server)).data!.commands;
    expect(commands.find('clear'), isNull);
    expect(commands.find('play'), isNotNull);

    await tester.ensureVisible(find.text('Only the user'));
    await tester.tap(find.text('Only the user'));
    await _settle(tester);
    expect(
      app.read(serverProvider(_server)).data!.commands.resultsPublic,
      isFalse,
    );
    await app.dispose(tester);
  });
}
