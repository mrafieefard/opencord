import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/chat/popup/composer_popup.dart';
import 'package:opencord/features/chat/popup/popup_rail.dart';
import 'package:opencord/features/chat/popup/popup_rows.dart';
import 'package:opencord/features/chat/suggestions.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

import '../../support/app.dart';

const _serverKey = 'opencord.example:7710';

CommandSuggestion _row(
  String name,
  String feature, {
  String desc = 'Does it',
}) => CommandSuggestion(
  CommandSpec(name: name, description: desc, feature: feature),
);

PopupGroup _group(
  String id,
  String name,
  List<CommandSuggestion> rows,
  int at,
) => PopupGroup(
  source: PopupSource(id: id, name: name),
  rows: rows,
  firstIndex: at,
);

/// [sources] groups of [each] rows.
List<PopupGroup> _groups(int sources, int each) => [
  for (var s = 0; s < sources; s++)
    _group(s == 0 ? '' : 'plugin:p$s', s == 0 ? 'Opencord' : 'Plugin $s', [
      for (var r = 0; r < each; r++)
        _row('cmd-$s-$r', s == 0 ? 'music' : 'plugin:p$s'),
    ], s * each),
];

Future<void> _pump(
  WidgetTester tester,
  List<PopupGroup> groups, {
  int selected = 0,
  ValueChanged<int>? onHover,
  ValueChanged<Suggestion>? onPick,
  double maxHeight = PopupMetrics.cap,
  GlobalKey<ComposerPopupState>? key,
  OcColors colors = OcColors.dark,
  bool reducedMotion = false,
  String? emptyText,
}) async {
  tester.view.physicalSize = const Size(800, 600);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      child: MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: buildTheme(colors),
        home: MediaQuery(
          data: MediaQueryData(
            size: const Size(800, 600),
            disableAnimations: reducedMotion,
          ),
          child: ColoredBox(
            color: colors.chat,
            child: Align(
              alignment: Alignment.bottomLeft,
              child: ComposerPopup(
                key: key,
                serverKey: _serverKey,
                width: 600,
                maxHeight: maxHeight,
                groups: groups,
                selected: selected,
                onHover: onHover ?? (_) {},
                onPick: onPick ?? (_) {},
                emptyText: emptyText,
              ),
            ),
          ),
        ),
      ),
    ),
  );
}

double _scrollOf(WidgetTester tester) => tester
    .state<ScrollableState>(
      find.descendant(
        of: find.byType(CustomScrollView),
        matching: find.byType(Scrollable),
      ),
    )
    .position
    .pixels;

int _railIndex(WidgetTester tester) =>
    tester.widget<PopupRail>(find.byType(PopupRail)).selected;

// Through the real composer ---------------------------------------------------

const _plugins = [
  CommandSpec(name: 'roll', description: 'Roll a die', feature: 'plugin:dice'),
  CommandSpec(name: 'coin', description: 'Flip it', feature: 'plugin:dice'),
  CommandSpec(name: 'roll', description: 'Raffle', feature: 'plugin:tickets'),
];

Future<void> _settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 400));
}

Future<MockApp> _openChat(WidgetTester tester) async {
  final app = await MockApp.pump(tester);
  final server = app.repository.mockServer(_serverKey);
  server.commands = CommandList(
    commands: [...server.commands.commands, ..._plugins],
  );
  server.pluginUsers[900] = const PluginUser(
    userId: 900,
    pluginId: 'dice',
    name: 'Dice Bot',
  );
  server.pluginUsers[901] = const PluginUser(
    userId: 901,
    pluginId: 'tickets',
    name: 'Tickets',
  );
  app.repository.emit(
    PluginUsersUpdated(_serverKey, server.pluginUsers.values.toList()),
  );
  app.repository.emit(CommandsChanged(_serverKey, server.availableCommands));
  await _settle(tester);
  final data = app.read(serverProvider(_serverKey)).data!;
  final general = data.channels.values.firstWhere(
    (channel) => channel.name == 'general' && channel.kind == ChannelKind.text,
  );
  app.read(navigationProvider.notifier).openChannel(_serverKey, general.id);
  await _settle(tester);
  return app;
}

Finder get _input => find.descendant(
  of: find.byType(Composer),
  matching: find.byType(TextField),
);

Future<void> _type(WidgetTester tester, String text) async {
  await tester.enterText(_input, text);
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 300));
}

Future<void> _key(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyEvent(key);
  await tester.pump();
}

Future<void> _ctrl(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await tester.sendKeyEvent(key);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await tester.pump();
}

ComposerPopup _popup(WidgetTester tester) =>
    tester.widget<ComposerPopup>(find.byType(ComposerPopup));

String _selectedName(WidgetTester tester) {
  final popup = _popup(tester);
  return flatRows(popup.groups)[popup.selected].spec.name;
}

String _selectedSource(WidgetTester tester) {
  final popup = _popup(tester);
  return flatRows(popup.groups)[popup.selected].spec.source;
}

void main() {
  testWidgets('the_rail_shows_only_with_two_sources_or_more', (tester) async {
    await _pump(tester, _groups(1, 3));
    await tester.pumpAndSettle();
    expect(find.byType(PopupRail), findsNothing);

    await _pump(tester, _groups(2, 3));
    await tester.pumpAndSettle();
    expect(find.byType(PopupRail), findsOneWidget);
  });

  testWidgets('clicking_a_rail_icon_scrolls_to_its_group', (tester) async {
    final groups = _groups(4, 20);
    int? hovered;
    await _pump(tester, groups, onHover: (index) => hovered = index);
    await tester.pumpAndSettle();

    await tester.tap(find.byTooltip('Plugin 2'));
    await tester.pumpAndSettle();

    expect(_scrollOf(tester), 2 * (30 + 20 * 36.0));
    expect(_railIndex(tester), 2);
    expect(hovered, 40);
  });

  testWidgets('scrolling_moves_the_rail_highlight', (tester) async {
    await _pump(tester, _groups(4, 20));
    await tester.pumpAndSettle();

    await tester.drag(
      find.byType(CustomScrollView),
      const Offset(0, -(30 + 20 * 36.0 + 50)),
    );
    await tester.pumpAndSettle();

    expect(_railIndex(tester), 1);
  });

  testWidgets('the_selected_row_stays_in_view', (tester) async {
    final key = GlobalKey<ComposerPopupState>();
    await _pump(tester, _groups(1, 100), key: key);
    await tester.pumpAndSettle();

    key.currentState!.scrollToRow(50);
    await tester.pump();

    final viewport = PopupMetrics.cap - 2;
    final top = 50 * 36.0;
    final offset = _scrollOf(tester);
    expect(offset, greaterThanOrEqualTo(top + 30 + 36 + 4 - viewport - 1));
    expect(offset, lessThanOrEqualTo(top));
    expect(find.text('/cmd-0-50'), findsOneWidget);
  });

  testWidgets('a_header_sticks_while_its_commands_scroll', (tester) async {
    await _pump(tester, _groups(2, 40));
    await tester.pumpAndSettle();
    final top = tester.getTopLeft(find.byType(CustomScrollView)).dy;

    await tester.drag(find.byType(CustomScrollView), const Offset(0, -400));
    await tester.pumpAndSettle();

    expect(tester.getTopLeft(find.text('OPENCORD')).dy, lessThan(top + 30));
    expect(find.text('/cmd-0-0'), findsNothing);
  });

  testWidgets('nothing_matching_shows_the_empty_row', (tester) async {
    await _pump(tester, const [], emptyText: 'No commands match /xyz');
    await tester.pumpAndSettle();

    expect(find.text('No commands match /xyz'), findsOneWidget);
    expect(find.byType(PopupRail), findsNothing);
  });

  testWidgets('5000_rows_build_only_what_is_visible', (tester) async {
    await _pump(tester, _groups(50, 100));
    await tester.pumpAndSettle();

    final built = find.byType(ComposerPopupRow).evaluate().length;
    expect(built, lessThan(40));
    expect(tester.getSize(find.byType(ComposerPopup)).height, PopupMetrics.cap);
  });

  testWidgets('the_popup_opens_on_pop_and_closes_at_once', (tester) async {
    await _pump(tester, _groups(2, 3));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));

    final fade = tester.widget<FadeTransition>(
      find
          .descendant(
            of: find.byType(ComposerPopup),
            matching: find.byType(FadeTransition),
          )
          .first,
    );
    final expected = OcSpring.pop.valueAt(const Duration(milliseconds: 40));
    expect(fade.opacity.value, closeTo(expected.clamp(0, 1), 0.08));
    expect(fade.opacity.value, lessThan(1));

    await tester.pumpWidget(const SizedBox());
    expect(find.byType(ComposerPopup), findsNothing);
  });

  testWidgets('reduced_motion_has_no_animation', (tester) async {
    await _pump(tester, _groups(2, 3), reducedMotion: true);

    expect(
      find.descendant(
        of: find.byType(ComposerPopup),
        matching: find.byType(FadeTransition),
      ),
      findsNothing,
    );
  });

  testWidgets('long_or_right_to_left_descriptions_stay_on_one_line', (
    tester,
  ) async {
    final long = 'long ' * 200;
    const rtl = 'این یک توضیح فارسی برای دستور است';
    await _pump(tester, [
      _group('', 'Opencord', [
        _row('one', 'music', desc: long),
        _row('two', 'music', desc: rtl),
      ], 0),
    ]);
    await tester.pumpAndSettle();

    expect(tester.takeException(), isNull);
    for (final text in [long, rtl]) {
      expect(tester.getSize(find.text(text)).height, lessThan(20));
    }
  });

  testWidgets('arrows_cross_groups_and_wrap', (tester) async {
    final app = await _openChat(tester);
    await _type(tester, '/');
    final count = flatRows(_popup(tester).groups).length;
    expect(_popup(tester).selected, 0);

    await _key(tester, LogicalKeyboardKey.arrowUp);
    expect(_popup(tester).selected, count - 1);
    await _key(tester, LogicalKeyboardKey.arrowDown);
    expect(_popup(tester).selected, 0);

    await app.dispose(tester);
  });

  testWidgets('ctrl_arrows_jump_groups', (tester) async {
    final app = await _openChat(tester);
    await _type(tester, '/');
    expect(_selectedSource(tester), '');

    await _ctrl(tester, LogicalKeyboardKey.arrowDown);
    expect(_selectedSource(tester), 'plugin:dice');
    await _ctrl(tester, LogicalKeyboardKey.arrowDown);
    expect(_selectedSource(tester), 'plugin:tickets');
    await _ctrl(tester, LogicalKeyboardKey.arrowUp);
    expect(_selectedSource(tester), 'plugin:dice');

    await app.dispose(tester);
  });

  testWidgets('enter_and_tab_pick_escape_closes', (tester) async {
    final app = await _openChat(tester);
    await _type(tester, '/ro');
    expect(_selectedName(tester), 'roll');

    await _key(tester, LogicalKeyboardKey.tab);
    await _settle(tester);
    expect(find.text('/roll'), findsOneWidget);
    expect(find.byType(ComposerPopup), findsNothing);

    await _key(tester, LogicalKeyboardKey.escape);
    await _settle(tester);
    await _type(tester, '/ro');
    await _key(tester, LogicalKeyboardKey.escape);
    await tester.pump();
    expect(find.byType(ComposerPopup), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('typing_filters_and_empty_groups_lose_their_rail_icon', (
    tester,
  ) async {
    final app = await _openChat(tester);
    await _type(tester, '/');
    expect(find.byTooltip('Tickets'), findsOneWidget);

    await _type(tester, '/co');

    expect(_popup(tester).groups.map((g) => g.source.name), ['Dice Bot']);
    expect(find.byTooltip('Tickets'), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('nothing_matching_shows_the_empty_row_and_stays_open', (
    tester,
  ) async {
    final app = await _openChat(tester);
    await _type(tester, '/');
    await _type(tester, '/xyz');

    expect(find.byType(ComposerPopup), findsOneWidget);
    expect(find.text('No commands match /xyz'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets(
    'the_message_list_keeps_size_and_offset_while_the_popup_opens_filters_and_closes',
    (tester) async {
      final app = await _openChat(tester);
      Rect listRect() => tester.getRect(find.byType(MessageList));
      final before = listRect();

      await _type(tester, '/');
      expect(listRect(), before);
      await _type(tester, '/ro');
      expect(listRect(), before);
      await _key(tester, LogicalKeyboardKey.escape);
      await tester.pump();
      expect(listRect(), before);

      await app.dispose(tester);
    },
  );

  testWidgets('the_bottom_edge_stays_put_while_filtering', (tester) async {
    final app = await _openChat(tester);
    await _type(tester, '/');
    final bottom = tester.getBottomLeft(find.byType(ComposerPopup)).dy;
    final top = tester.getTopLeft(find.byType(ComposerPopup)).dy;

    await _type(tester, '/co');

    expect(tester.getBottomLeft(find.byType(ComposerPopup)).dy, bottom);
    expect(tester.getTopLeft(find.byType(ComposerPopup)).dy, greaterThan(top));
    await app.dispose(tester);
  });

  testWidgets('the_popup_sits_above_the_composer_and_as_wide', (tester) async {
    final app = await _openChat(tester);
    await _type(tester, '/');

    final composer = tester.getRect(find.byType(Composer));
    final popup = tester.getRect(find.byType(ComposerPopup));
    expect(popup.left, composer.left);
    expect(popup.width, composer.width);
    expect(popup.bottom, composer.top - PopupMetrics.gap);
    await app.dispose(tester);
  });

  testWidgets('at_mentions_channels_and_emoji_use_it_without_rail_or_groups', (
    tester,
  ) async {
    final app = await _openChat(tester);

    for (final (text, title) in [
      ('hi @', 'MEMBERS'),
      ('hi #', 'CHANNELS'),
      ('hi :smi', 'EMOJI'),
    ]) {
      await _type(tester, text);
      expect(find.byType(ComposerPopup), findsOneWidget, reason: text);
      expect(find.text(title), findsOneWidget, reason: text);
      expect(find.byType(PopupRail), findsNothing, reason: text);
    }
    await app.dispose(tester);
  });

  testWidgets('commands_changing_while_open_keep_a_real_selection', (
    tester,
  ) async {
    final app = await _openChat(tester);
    await _type(tester, '/ro');
    await _key(tester, LogicalKeyboardKey.arrowDown);
    expect(_selectedSource(tester), 'plugin:tickets');

    final server = app.repository.mockServer(_serverKey);
    server.commands = CommandList(
      commands: [
        for (final spec in server.commands.commands)
          if (spec.source != 'plugin:tickets') spec,
      ],
    );
    app.repository.emit(CommandsChanged(_serverKey, server.availableCommands));
    await _settle(tester);

    final popup = _popup(tester);
    expect(
      popup.selected,
      inInclusiveRange(0, flatRows(popup.groups).length - 1),
    );
    expect(_selectedSource(tester), 'plugin:dice');
    await app.dispose(tester);
  });

  testWidgets('a_name_typed_without_picking_runs_it', (tester) async {
    final app = await _openChat(tester);
    await _type(tester, '/coin');
    await _key(tester, LogicalKeyboardKey.escape);
    await tester.pump();
    await _key(tester, LogicalKeyboardKey.enter);
    await _settle(tester);

    expect(app.repository.commandsRun, ['coin']);
    await app.dispose(tester);
  });

  testWidgets('two_plugins_typed_without_picking_ask_to_pick', (tester) async {
    final app = await _openChat(tester);
    await _type(tester, '/roll');
    await _key(tester, LogicalKeyboardKey.escape);
    await tester.pump();
    await _key(tester, LogicalKeyboardKey.enter);
    await _settle(tester);

    expect(
      find.text('Two commands are called /roll. Pick one from the list.'),
      findsOneWidget,
    );
    expect(app.repository.commandsRun, isEmpty);
    await app.dispose(tester);
  });

  for (final (name, colors) in [
    ('dark', OcColors.dark),
    ('light', OcColors.light),
  ]) {
    testWidgets('the popup looks right in $name', (tester) async {
      await _pump(
        tester,
        [
          _group('', 'Opencord', [
            _row('play', 'music', desc: 'Play a song or playlist'),
            CommandSuggestion(
              const CommandSpec(
                name: 'skip',
                description: 'Skip this song',
                feature: 'music',
              ),
              unavailable: 'Needs Control playback',
            ),
          ], 0),
          _group('plugin:dice', 'Dice Bot', [
            _row('roll', 'plugin:dice', desc: 'Roll a die'),
          ], 2),
        ],
        selected: 0,
        colors: colors,
        reducedMotion: true,
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(ComposerPopup),
        matchesGoldenFile('goldens/composer_popup_$name.png'),
      );
    });
  }
}
