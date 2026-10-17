import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:font_awesome_flutter/font_awesome_flutter.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/music/music_search.dart';
import 'package:opencord/features/music/music_source_icon.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _settle(WidgetTester tester) =>
    _pumpFor(tester, const Duration(milliseconds: 600));

Finder _inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

Finder _inPanel(Finder finder) =>
    find.descendant(of: find.byType(MusicPanel), matching: finder);

Finder _inMenu(Finder finder) =>
    find.descendant(of: find.byType(OcMenuPanel), matching: finder);

Finder _headerButton(String tooltip) => find.descendant(
  of: find.byType(ChatHeader),
  matching: find.byWidgetPredicate(
    (widget) => widget is OcIconButton && widget.tooltip == tooltip,
  ),
);

Future<void> _openMusic(WidgetTester tester) async {
  await tester.tap(_inSidebar('General'));
  await _settle(tester);
  await tester.tap(_headerButton('Music'));
  await _settle(tester);
}

void main() {
  group('MusicSourceIcon', () {
    testWidgets('renders FontAwesome brand icons for known music providers and fallback', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: const Scaffold(
            body: SingleChildScrollView(
              child: Column(
                children: [
                  MusicSourceIcon('youtube_music'),
                  MusicSourceIcon('youtube'),
                  MusicSourceIcon('soundcloud'),
                  MusicSourceIcon('bandcamp'),
                  MusicSourceIcon('spotify'),
                  MusicSourceIcon('applemusic'),
                  MusicSourceIcon('deezer'),
                  MusicSourceIcon('unknown_provider'),
                ],
              ),
            ),
          ),
        ),
      );

      expect(find.byType(MusicSourceIcon), findsNWidgets(8));
      // 7 FontAwesome FaIcon widgets, 1 fallback Icon(OcIcons.musicNote)
      expect(find.byType(FaIcon), findsNWidgets(7));
      expect(find.byIcon(OcIcons.musicNote), findsOneWidget);
    });
  });

  group('MusicSearchField', () {
    testWidgets('source selector is on the left and search button is removed', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      final searchFieldFinder = _inPanel(find.byType(MusicSearchField));
      expect(searchFieldFinder, findsOneWidget);

      // Selector on the left
      final selectorIconFinder = _inPanel(
        find.descendant(
          of: searchFieldFinder,
          matching: find.byType(MusicSourceIcon),
        ),
      );
      expect(selectorIconFinder, findsOneWidget);

      // Search button on the right is removed
      final searchButtonFinder = _inPanel(
        find.descendant(
          of: searchFieldFinder,
          matching: find.bySemanticsLabel('Search'),
        ),
      );
      expect(searchButtonFinder, findsNothing);

      await app.dispose(tester);
    });

    testWidgets('debounces search by 1 second when typing', (tester) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      // Type search query
      await tester.enterText(
        _inPanel(find.bySemanticsLabel('Search for music')).first,
        'lofi',
      );

      // After 500 ms, search has not triggered yet
      await tester.pump(const Duration(milliseconds: 500));
      expect(_inPanel(find.text('lofi 1')), findsNothing);

      // Advance past the 1-second debounce timer and settle the search async results
      await tester.pump(const Duration(milliseconds: 600));
      await _settle(tester);
      expect(_inPanel(find.text('lofi 1')), findsOneWidget);

      await app.dispose(tester);
    });

    testWidgets('tapping source selector opens vertical menu with provider names and icons', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      // Find the selector pill
      final selectorPill = _inPanel(
        find.bySemanticsLabel('Search YouTube Music'),
      );
      expect(selectorPill, findsOneWidget);

      // Tap selector to open vertical menu
      await tester.tap(selectorPill);
      await _settle(tester);

      // Vertical menu should be open
      expect(find.byType(OcMenuPanel), findsOneWidget);

      // Vertical menu lists provider names and icons
      expect(_inMenu(find.text('YouTube Music')), findsOneWidget);
      expect(_inMenu(find.text('YouTube')), findsOneWidget);
      expect(_inMenu(find.text('SoundCloud')), findsOneWidget);
      expect(_inMenu(find.text('Bandcamp')), findsOneWidget);
      expect(_inMenu(find.byType(MusicSourceIcon)), findsWidgets);

      // Tap SoundCloud in the vertical menu
      await tester.tap(_inMenu(find.text('SoundCloud')));
      await _settle(tester);

      // Menu is dismissed
      expect(find.byType(OcMenuPanel), findsNothing);

      // Search field selector now reflects SoundCloud
      expect(
        _inPanel(find.bySemanticsLabel('Search SoundCloud')),
        findsOneWidget,
      );

      await app.dispose(tester);
    });
  });
}
