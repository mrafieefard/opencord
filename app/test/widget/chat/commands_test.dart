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
import 'package:opencord/features/chat/popup/composer_popup.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/chat/popup/source_icon.dart';
import 'package:opencord/features/settings/server/commands_page.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

import '../../support/app.dart';
import '../../support/save_bar.dart';

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
    find.descendant(of: find.byType(ComposerPopup), matching: finder);

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

  testWidgets('Backspace at the start of a command takes it away', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _joinVoice(tester, app);
    await _choose(tester, 'play');
    final query = _option('query').first;
    await tester.enterText(query, 'ab');
    await _press(tester, LogicalKeyboardKey.backspace);
    expect(find.byType(CommandInput), findsOneWidget, reason: 'deletes "b"');
    expect(tester.widget<TextField>(query).controller!.text, 'a');

    await _press(tester, LogicalKeyboardKey.backspace);
    expect(find.byType(CommandInput), findsOneWidget, reason: 'deletes "a"');
    await _press(tester, LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.byType(CommandInput), findsNothing);
    expect(
      tester.widget<TextField>(_input).focusNode!.hasFocus,
      isTrue,
      reason: 'back to the message box',
    );
    await app.dispose(tester);
  });

  testWidgets('Ctrl+A selects the whole command and Backspace removes it', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _joinVoice(tester, app);
    await _choose(tester, 'play');
    await tester.enterText(_option('query').first, 'some song');
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await _press(tester, LogicalKeyboardKey.keyA);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    expect(find.byType(CommandInput), findsOneWidget);
    await _press(tester, LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.byType(CommandInput), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('a command without options goes with one Backspace', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await _choose(tester, 'music');
    await _press(tester, LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.byType(CommandInput), findsNothing);
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
    // Waits for the save bar; Discard puts it back.
    expect(
      app.read(serverProvider(_server)).data!.commands.find('clear'),
      isNotNull,
    );
    await tester.tap(saveBarButton('Discard'));
    await _settle(tester);
    expect(tester.widget<OcSwitch>(clear).value, isTrue);

    await tester.tap(clear);
    await tester.ensureVisible(find.text('Only the user'));
    await tester.tap(find.text('Only the user'));
    await saveSettings(tester);
    final commands = app.read(serverProvider(_server)).data!.commands;
    expect(commands.find('clear'), isNull);
    expect(commands.find('play'), isNotNull);
    expect(commands.resultsPublic, isFalse);
    await app.dispose(tester);
  });

  // The command being written (§26.6). --------------------------------------

  /// Adds a plugin's commands to the server the app shows.
  Future<void> addPluginCommands(
    WidgetTester tester,
    MockApp app,
    List<CommandSpec> specs,
  ) async {
    final server = app.repository.mockServer(_server);
    server.commands = CommandList(
      commands: [...server.commands.commands, ...specs],
    );
    server.pluginUsers[900] = const PluginUser(
      userId: 900,
      pluginId: 'dice',
      name: 'Dice Bot',
    );
    app.repository.emit(
      PluginUsersUpdated(_server, server.pluginUsers.values.toList()),
    );
    app.repository.emit(CommandsChanged(_server, server.availableCommands));
    await _settle(tester);
  }

  const roll = CommandSpec(
    name: 'roll',
    description: 'Roll a die',
    feature: 'plugin:dice',
    options: [
      CommandOptionSpec(
        name: 'sides',
        description: 'How many',
        kind: CommandOptionKind.text,
        autocomplete: true,
      ),
    ],
  );
  const assign = CommandSpec(
    name: 'assign',
    description: 'Give it away',
    feature: 'plugin:dice',
    options: [
      CommandOptionSpec(
        name: 'person',
        description: 'Who',
        kind: CommandOptionKind.text,
      ),
      CommandOptionSpec(
        name: 'note',
        description: 'Why',
        kind: CommandOptionKind.text,
      ),
    ],
  );
  const flag = CommandSpec(
    name: 'flag',
    description: 'A switch',
    feature: 'plugin:dice',
    options: [
      CommandOptionSpec(
        name: 'on',
        description: 'Yes or no',
        kind: CommandOptionKind.boolean,
      ),
    ],
  );

  Future<MockApp> withPlugin(WidgetTester tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    await addPluginCommands(tester, app, const [roll, assign, flag]);
    return app;
  }

  testWidgets('the_command_draws_no_box_of_its_own', (tester) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'roll');

    final boxes = tester
        .widgetList<Container>(
          find.descendant(
            of: find.byType(CommandInput),
            matching: find.byType(Container),
          ),
        )
        .where(
          (box) => switch (box.decoration) {
            BoxDecoration(:final border) => border != null,
            _ => false,
          },
        );
    expect(boxes, isEmpty);
    await app.dispose(tester);
  });

  testWidgets('the_chip_shows_the_plugins_icon', (tester) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'roll');
    expect(
      find.descendant(
        of: find.byType(CommandInput),
        matching: find.byType(CommandSourceIcon),
      ),
      findsOneWidget,
    );
    await _press(tester, LogicalKeyboardKey.escape);
    await _settle(tester);

    await _choose(tester, 'music');
    expect(
      find.descendant(
        of: find.byType(CommandInput),
        matching: find.byType(CommandSourceIcon),
      ),
      findsNothing,
    );
    await app.dispose(tester);
  });

  testWidgets('autocomplete_uses_the_popups_surface_and_place', (tester) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'roll');

    await tester.enterText(_option('sides').first, 'ab');
    await _settle(tester);
    await tester.pump(suggestAfter);
    await _settle(tester);

    expect(find.byType(ComposerPopup), findsOneWidget);
    expect(find.text('SIDES'), findsOneWidget);
    final composer = tester.getRect(find.byType(Composer));
    final popup = tester.getRect(find.byType(ComposerPopup));
    expect(popup.left, composer.left);
    expect(popup.width, composer.width);
    expect(popup.bottom, composer.top - PopupMetrics.gap);
    await app.dispose(tester);
  });

  testWidgets('a_picked_commands_source_is_sent', (tester) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'roll');
    await _press(tester, LogicalKeyboardKey.enter);
    await _settle(tester);

    expect(app.repository.commandSources, ['plugin:dice']);
    await app.dispose(tester);
  });

  testWidgets('autocomplete_sends_the_source', (tester) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'roll');
    await tester.enterText(_option('sides').first, 'ab');
    await _settle(tester);
    await tester.pump(suggestAfter);
    await _settle(tester);

    expect(app.repository.autocompleteSources, ['plugin:dice']);
    await app.dispose(tester);
  });

  testWidgets('a_removed_command_gives_a_plain_error_and_keeps_the_text', (
    tester,
  ) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'assign');
    await tester.enterText(_option('person').first, 'kept');
    final server = app.repository.mockServer(_server);
    server.commands = CommandList(
      commands: [
        for (final spec in server.commands.commands)
          if (spec.name != 'assign') spec,
      ],
    );
    app.repository.emit(CommandsChanged(_server, server.availableCommands));
    await _settle(tester);

    await _press(tester, LogicalKeyboardKey.enter);
    await _settle(tester);

    final note = tester.widget<EphemeralLine>(find.byType(EphemeralLine));
    expect(note.note.content, "/assign isn't a command here");
    expect(note.note.error, isTrue);
    expect(
      tester.widget<TextField>(_option('person').first).controller!.text,
      'kept',
    );
    await app.dispose(tester);
  });

  testWidgets(
    'backspace_with_the_popup_open_removes_the_command_and_closes_the_popup',
    (tester) async {
      final app = await withPlugin(tester);
      await _choose(tester, 'roll');
      final sides = _option('sides').first;
      await tester.enterText(sides, 'ab');
      await _settle(tester);
      await tester.pump(suggestAfter);
      await _settle(tester);
      expect(find.byType(ComposerPopup), findsOneWidget);
      tester.widget<TextField>(sides).controller!.selection =
          const TextSelection.collapsed(offset: 0);
      await tester.pump();

      await _press(tester, LogicalKeyboardKey.backspace);
      await tester.pump();

      expect(find.byType(CommandInput), findsNothing);
      expect(find.byType(ComposerPopup), findsNothing);
      await app.dispose(tester);
    },
  );

  testWidgets('holding_backspace_empties_the_field_then_removes_the_command', (
    tester,
  ) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'roll');
    await tester.enterText(_option('sides').first, 'ab');
    await tester.pump();

    await tester.sendKeyDownEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.byType(CommandInput), findsOneWidget);
    await tester.sendKeyRepeatEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.byType(CommandInput), findsOneWidget);
    await tester.sendKeyRepeatEvent(LogicalKeyboardKey.backspace);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    await tester.pump();

    expect(find.byType(CommandInput), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('backspace_steps_back_through_a_commands_fields', (tester) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'assign');
    await tester.tap(_option('note').at(1));
    await tester.pump();

    await _press(tester, LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.byType(CommandInput), findsOneWidget);
    expect(
      tester.widget<TextField>(_option('person').first).focusNode!.hasFocus,
      isTrue,
    );

    await _press(tester, LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.byType(CommandInput), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('backspace_on_a_choice_or_yes_no_option_removes_the_command', (
    tester,
  ) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'flag');

    await _press(tester, LogicalKeyboardKey.backspace);
    await tester.pump();

    expect(find.byType(CommandInput), findsNothing);
    await app.dispose(tester);
  });

  testWidgets(
    'clicking_the_composer_gives_the_focus_back_so_backspace_reaches_the_command',
    (tester) async {
      final app = await withPlugin(tester);
      await _choose(tester, 'roll');
      final field = tester.widget<TextField>(_option('sides').first);
      field.focusNode!.unfocus();
      await tester.pump();
      expect(field.focusNode!.hasFocus, isFalse);

      await tester.tapAt(
        tester.getTopRight(find.byType(CommandInput)) + const Offset(-20, 8),
      );
      await tester.pump();
      expect(field.focusNode!.hasFocus, isTrue);

      await _press(tester, LogicalKeyboardKey.backspace);
      await tester.pump();
      expect(find.byType(CommandInput), findsNothing);
      await app.dispose(tester);
    },
  );

  testWidgets('clicking_the_chip_then_backspace_removes_the_command', (
    tester,
  ) async {
    final app = await withPlugin(tester);
    await _choose(tester, 'roll');
    await tester.tap(find.text('/roll'));
    await tester.pump();

    await _press(tester, LogicalKeyboardKey.backspace);
    await tester.pump();

    expect(find.byType(CommandInput), findsNothing);
    await app.dispose(tester);
  });

  testWidgets(
    'after_removing_a_command_the_field_is_empty_focused_and_sends_plain_messages',
    (tester) async {
      final app = await withPlugin(tester);
      await _choose(tester, 'flag');
      await _press(tester, LogicalKeyboardKey.backspace);
      await tester.pump();
      await tester.pump();

      final field = tester.widget<TextField>(_input);
      expect(field.controller!.text, '');
      expect(field.focusNode!.hasFocus, isTrue);

      await tester.enterText(_input, 'just a message');
      await tester.pump();
      await _press(tester, LogicalKeyboardKey.enter);
      await _settle(tester);

      expect(find.text('just a message'), findsWidgets);
      expect(app.repository.commandsRun, isEmpty);
      await app.dispose(tester);
    },
  );
}
