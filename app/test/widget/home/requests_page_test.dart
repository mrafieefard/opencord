import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/private.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/home/person_card.dart';
import 'package:opencord/features/home/requests_page.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

import '../../support/app.dart';

Future<void> _pumpRequests(
  WidgetTester tester,
  MockApp app, {
  Size size = const Size(1280, 800),
}) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(() {
    tester.view.resetPhysicalSize();
    tester.view.resetDevicePixelRatio();
  });
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: app.container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: Scaffold(
          body: SizedBox.fromSize(
            size: size,
            child: const RequestsPage(),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('requests_page_is_cards', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpRequests(tester, app, size: const Size(1280, 800));

    // At 1280: 2 columns in the grid
    final requestCards = find.byType(RequestCard);
    expect(requestCards, findsNWidgets(2)); // Priya Shah and Greta Hoffmann

    final firstCardRect = tester.getRect(requestCards.at(0));
    final secondCardRect = tester.getRect(requestCards.at(1));
    // 2 columns means they are side by side: top coordinates match within 1 px
    expect(firstCardRect.top, closeTo(secondCardRect.top, 1.0));
    expect(firstCardRect.width, greaterThanOrEqualTo(280.0));
    expect(secondCardRect.width, greaterThanOrEqualTo(280.0));

    // Card container numbers: padding 12, radius 14, background sidebar, 1 px border
    final cardContainer = tester.widget<Container>(
      find.descendant(
        of: requestCards.first,
        matching: find.byWidgetPredicate((w) =>
          w is Container &&
          w.padding == const EdgeInsets.all(12)),
      ).first,
    );
    final cardDeco = cardContainer.decoration as BoxDecoration;
    expect(cardDeco.color, OcColors.dark.sidebar);
    expect(cardDeco.borderRadius, BorderRadius.circular(14));
    expect(cardDeco.border?.top.width, 1.0);

    // Top row: avatar 40, name 14/600, via relay name 12 textMuted, time 11 textMuted
    expect(find.text('Priya Shah'), findsOneWidget);
    final priyaText = tester.widget<Text>(find.text('Priya Shah'));
    expect(priyaText.style?.fontSize, 14);
    expect(priyaText.style?.fontWeight, FontWeight.w600);
    expect(priyaText.style?.color, OcColors.dark.text);

    // via Opencord Dev (relayed via opencord.example:7710)
    expect(find.text('via Opencord Dev'), findsOneWidget);
    final viaText = tester.widget<Text>(find.text('via Opencord Dev'));
    expect(viaText.style?.fontSize, 12);
    expect(viaText.style?.color, OcColors.dark.textMuted);

    // Time text exists with fontSize 11 textMuted
    final timeFinder = find.descendant(
      of: requestCards.first,
      matching: find.byWidgetPredicate((w) =>
        w is Text &&
        w.style?.fontSize == 11 &&
        w.style?.color == OcColors.dark.textMuted),
    );
    expect(timeFinder, findsOneWidget);

    // Message bubble: padding 8 12, radii 16 16 16 5, bubbleIn, 1 px border
    final bubbleContainer = tester.widget<Container>(
      find.descendant(
        of: requestCards.first,
        matching: find.byWidgetPredicate((w) =>
          w is Container &&
          w.padding == const EdgeInsets.symmetric(horizontal: 12, vertical: 8)),
      ).first,
    );
    final bubbleDeco = bubbleContainer.decoration as BoxDecoration;
    expect(bubbleDeco.color, OcColors.dark.bubbleIn);
    expect(
      bubbleDeco.borderRadius,
      const BorderRadius.only(
        topLeft: Radius.circular(16),
        topRight: Radius.circular(16),
        bottomRight: Radius.circular(16),
        bottomLeft: Radius.circular(5),
      ),
    );
    expect(bubbleDeco.border?.top.width, 1.0);

    // Priya has a preview in mock data; clear it to test fallback "Wants to message you"
    final priyaKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Priya Shah')
        .peer;
    final emptyPreviewPriya = app.repository.privateWorld.contacts[priyaKey]!.copyWith(preview: '');
    app.repository.privateWorld.contacts[priyaKey] = emptyPreviewPriya;
    app.repository.emit(PrivateContactChanged(emptyPreviewPriya));
    await tester.pumpAndSettle();

    // Greta Hoffmann has a preview in mock_private
    expect(
      find.text('Hello! We would love to have you speak at the next Berlin meetup.'),
      findsOneWidget,
    );
    // Priya Shah now has empty preview, so shows 'Wants to message you'
    expect(find.text('Wants to message you'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('accept_opens_the_chat', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpRequests(tester, app);

    final priyaKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Priya Shah')
        .peer;

    final priyaCard = find.ancestor(
      of: find.text('Priya Shah'),
      matching: find.byType(RequestCard),
    );
    final acceptBtn = find.descendant(
      of: priyaCard,
      matching: find.widgetWithText(PersonActionButton, 'Accept'),
    );
    expect(acceptBtn, findsOneWidget);

    await tester.tap(acceptBtn);
    await tester.pumpAndSettle();

    // Conversation is accepted and chat is opened
    expect(app.container.read(currentPeerProvider), priyaKey);
    final contact = app.repository.privateWorld.contacts[priyaKey]!;
    expect(contact.conversation, ConversationStatus.accepted);

    await app.dispose(tester);
  });

  testWidgets('decline_asks_first_then_removes', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpRequests(tester, app);

    final priyaKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Priya Shah')
        .peer;

    final priyaCard = find.ancestor(
      of: find.text('Priya Shah'),
      matching: find.byType(RequestCard),
    );
    final declineBtn = find.descendant(
      of: priyaCard,
      matching: find.widgetWithText(PersonActionButton, 'Decline'),
    );
    expect(declineBtn, findsOneWidget);

    await tester.tap(declineBtn);
    await tester.pumpAndSettle();

    // Dialog asks first
    expect(find.byType(OcDialog), findsOneWidget);
    expect(find.text('Ignore request?'), findsOneWidget);
    expect(find.text('This removes the conversation.'), findsOneWidget);

    // Confirm Ignore
    await tester.tap(
      find.descendant(of: find.byType(OcDialog), matching: find.text('Ignore')),
    );
    await tester.pumpAndSettle();

    // Request is removed
    expect(find.text('Priya Shah'), findsNothing);
    final contact = app.repository.privateWorld.contacts[priyaKey];
    expect(contact == null || contact.conversation == ConversationStatus.none, isTrue);

    await app.dispose(tester);
  });

  testWidgets('decline_cancelled_keeps_it', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpRequests(tester, app);

    final priyaCard = find.ancestor(
      of: find.text('Priya Shah'),
      matching: find.byType(RequestCard),
    );
    final declineBtn = find.descendant(
      of: priyaCard,
      matching: find.widgetWithText(PersonActionButton, 'Decline'),
    );

    await tester.tap(declineBtn);
    await tester.pumpAndSettle();

    expect(find.byType(OcDialog), findsOneWidget);

    // Cancel
    await tester.tap(
      find.descendant(of: find.byType(OcDialog), matching: find.text('Cancel')),
    );
    await tester.pumpAndSettle();

    // Request is kept
    expect(find.text('Priya Shah'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('a_click_on_the_card_opens_the_chat', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpRequests(tester, app);

    final gretaKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Greta Hoffmann')
        .peer;

    // Click on Greta's name
    await tester.tap(find.text('Greta Hoffmann'));
    await tester.pumpAndSettle();

    expect(app.container.read(currentPeerProvider), gretaKey);

    await app.dispose(tester);
  });

  testWidgets('empty_says_so', (tester) async {
    final app = await MockApp.pump(tester);

    // Clear message requests
    final world = app.repository.privateWorld;
    for (final c in world.contacts.values.toList()) {
      if (c.conversation == ConversationStatus.request) {
        world.contacts[c.peer] = c.copyWith(conversation: ConversationStatus.none);
      }
    }
    app.container.read(privateContactsProvider.notifier).onStateChanged();

    await _pumpRequests(tester, app);

    // Mail icon 32 textMuted
    final iconFinder = find.byWidgetPredicate((w) =>
      w is Icon &&
      w.icon == OcIcons.mail &&
      w.size == 32 &&
      w.color == OcColors.dark.textMuted);
    expect(iconFinder, findsOneWidget);

    // Text 'No one is waiting for an answer' in textSecondary
    final textFinder = find.byWidgetPredicate((w) =>
      w is Text &&
      w.data == 'No one is waiting for an answer' &&
      w.style?.color == OcColors.dark.textSecondary);
    expect(textFinder, findsOneWidget);

    await app.dispose(tester);
  });
}
