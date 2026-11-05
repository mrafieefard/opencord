import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/home/person_card.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

import '../../support/app.dart';

Future<void> _pumpFriends(
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
            child: const FriendsPage(),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('tabs_have_counts', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    // Header has PanelHeader with ShellKeys.hd
    expect(find.byKey(ShellKeys.hd), findsOneWidget);

    // Message requests is NOT a tab in the header
    final headerFinder = find.byKey(ShellKeys.hd);
    expect(
      find.descendant(
        of: headerFinder,
        matching: find.text('Message requests'),
      ),
      findsNothing,
    );

    // Tabs: Online, All, Pending, Blocked with counts
    // Kai and Mira are online -> Online 2
    // Kai, Mira, Noah are friends -> All 3
    // Jonas (incoming) + Tomás (outgoing) -> Pending 2
    // Old Friend is blocked -> Blocked 1
    expect(find.descendant(of: headerFinder, matching: find.text('Online')), findsOneWidget);
    expect(find.text('2'), findsWidgets); // Online 2
    expect(find.descendant(of: headerFinder, matching: find.text('All')), findsOneWidget);
    expect(find.text('3'), findsWidgets); // All 3
    expect(find.descendant(of: headerFinder, matching: find.text('Pending')), findsOneWidget);
    expect(find.descendant(of: headerFinder, matching: find.text('Blocked')), findsOneWidget);

    // Tab button numbers: height 30, radius 15, on state accent/onAccent
    final onlineTabBtn = tester.widget<Container>(
      find.descendant(
        of: headerFinder,
        matching: find.byWidgetPredicate((w) =>
          w is Container &&
          w.constraints?.maxHeight == 30 &&
          w.padding == const EdgeInsets.symmetric(horizontal: 12)),
      ).first,
    );
    expect(onlineTabBtn.constraints?.maxHeight, 30);
    expect(onlineTabBtn.padding, const EdgeInsets.symmetric(horizontal: 12));
    final onlineDeco = onlineTabBtn.decoration as BoxDecoration;
    expect(onlineDeco.borderRadius, BorderRadius.circular(15));
    // Default selected tab is Online: on state is accent
    expect(onlineDeco.color, OcColors.dark.accent);

    // Text in on state is onAccent
    final onlineText = tester.widget<Text>(
      find.descendant(of: find.byWidget(onlineTabBtn), matching: find.text('Online')),
    );
    expect(onlineText.style?.color, OcColors.dark.onAccent);
    expect(onlineText.style?.fontSize, 13);
    expect(onlineText.style?.fontWeight, FontWeight.w500);

    // Count text has fontSize 11.5 and 0.65 opacity
    final countOpacity = tester.widget<Opacity>(
      find.descendant(
        of: find.byWidget(onlineTabBtn),
        matching: find.byWidgetPredicate((w) => w is Opacity && w.opacity == 0.65),
      ),
    );
    expect(countOpacity.opacity, 0.65);

    // Event changes count numbers: unblock Old Friend
    final oldFriendKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Old Friend')
        .peer;
    await app.repository.unblockUser(oldFriendKey);
    await tester.pumpAndSettle();

    // Blocked count is now 0
    final blockedBtn = find.ancestor(
      of: find.text('Blocked'),
      matching: find.byWidgetPredicate((w) => w is Container && w.constraints?.maxHeight == 30),
    );
    expect(
      find.descendant(of: blockedBtn, matching: find.text('0')),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('person_card_numbers', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    // Kai Nakamura is online on the Online tab
    final kaiCardFinder = find.ancestor(
      of: find.text('Kai Nakamura'),
      matching: find.byType(PersonCard),
    );
    expect(kaiCardFinder, findsOneWidget);

    // Card container numbers: min height 60, padding 8 10, radius 12, background sidebar, 1px border, 6 below, gap 12
    final cardContainer = tester.widget<Container>(
      find.descendant(
        of: kaiCardFinder,
        matching: find.byWidgetPredicate((w) =>
          w is Container &&
          w.constraints?.minHeight == 60 &&
          w.margin == const EdgeInsets.only(bottom: 6) &&
          w.padding == const EdgeInsets.symmetric(horizontal: 10, vertical: 8)),
      ),
    );
    expect(cardContainer.constraints?.minHeight, 60);
    expect(cardContainer.margin, const EdgeInsets.only(bottom: 6));
    expect(cardContainer.padding, const EdgeInsets.symmetric(horizontal: 10, vertical: 8));
    final deco = cardContainer.decoration as BoxDecoration;
    expect(deco.color, OcColors.dark.sidebar);
    expect(deco.borderRadius, BorderRadius.circular(12));
    expect(deco.border?.top.width, 1.0);

    // Text: name 14/600, line 12.5 textMuted
    final nameText = tester.widget<Text>(find.text('Kai Nakamura'));
    expect(nameText.style?.fontSize, 14);
    expect(nameText.style?.fontWeight, FontWeight.w600);
    expect(nameText.style?.color, OcColors.dark.text);

    final presenceText = tester.widget<Text>(
      find.descendant(of: kaiCardFinder, matching: find.text('Online')),
    );
    expect(presenceText.style?.fontSize, 12.5);
    expect(presenceText.style?.color, OcColors.dark.textMuted);

    // Switch to All tab to check offline person (Noah Kim is offline)
    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();

    final noahCardFinder = find.ancestor(
      of: find.text('Noah Kim'),
      matching: find.byType(PersonCard),
    );
    expect(noahCardFinder, findsOneWidget);

    // Offline fades: avatar and text at 0.55 opacity
    final offlineOpacity = tester.widget<Opacity>(
      find.descendant(
        of: noahCardFinder,
        matching: find.byWidgetPredicate((w) => w is Opacity && w.opacity == 0.55),
      ),
    );
    expect(offlineOpacity.opacity, 0.55);

    await app.dispose(tester);
  });

  testWidgets('friend_actions_work', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    // Kai has accepted conversation: tap Message button opens chat
    final kaiCard = find.ancestor(
      of: find.text('Kai Nakamura'),
      matching: find.byType(PersonCard),
    );
    final messageBtn = find.descendant(
      of: kaiCard,
      matching: find.byTooltip('Message'),
    );
    expect(messageBtn, findsOneWidget);

    await tester.tap(messageBtn);
    await tester.pumpAndSettle();
    expect(app.container.read(currentPeerProvider), kaiKey);

    // Reset current peer and go to All tab to test Noah (conversation status none)
    app.container.read(navigationProvider.notifier).openFriends();
    await _pumpFriends(tester, app);
    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();

    final noahKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Noah Kim')
        .peer;
    final noahCard = find.ancestor(
      of: find.text('Noah Kim'),
      matching: find.byType(PersonCard),
    );
    final noahMessageBtn = find.descendant(
      of: noahCard,
      matching: find.byTooltip('Message'),
    );
    await tester.tap(noahMessageBtn);
    await tester.pumpAndSettle();
    expect(app.container.read(currentPeerProvider), noahKey);

    // Reset and test More options menu
    app.container.read(navigationProvider.notifier).openFriends();
    await _pumpFriends(tester, app);
    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();

    final moreBtn = find.descendant(
      of: find.ancestor(of: find.text('Kai Nakamura'), matching: find.byType(PersonCard)),
      matching: find.byTooltip('More options'),
    );
    await tester.tap(moreBtn);
    await tester.pumpAndSettle();

    expect(find.text('Remove friend'), findsOneWidget);
    expect(find.text('Block'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('incoming_has_accept_and_decline', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();

    final jonasCard = find.ancestor(
      of: find.text('Jonas Weber'),
      matching: find.byType(PersonCard),
    );
    expect(jonasCard, findsOneWidget);

    // Has Accept and Decline action buttons
    final acceptBtn = find.descendant(
      of: jonasCard,
      matching: find.widgetWithText(PersonActionButton, 'Accept'),
    );
    final declineBtn = find.descendant(
      of: jonasCard,
      matching: find.widgetWithText(PersonActionButton, 'Decline'),
    );
    expect(acceptBtn, findsOneWidget);
    expect(declineBtn, findsOneWidget);

    // Configure failure: error dialog shows through showPrivateFailure and row stays
    app.repository.answerFriendRequestFailure = const RepoException(
      RepoErrorKind.privateMessagesUnavailable,
      "Couldn't answer: Opencord Dev isn't connected right now.",
    );

    await tester.tap(acceptBtn);
    await tester.pumpAndSettle();

    expect(find.byType(OcDialog), findsOneWidget);
    expect(find.text("Couldn't answer friend request"), findsOneWidget);

    // Dismiss dialog; row stays
    await tester.tap(find.text('OK'));
    await tester.pumpAndSettle();
    expect(find.text('Jonas Weber'), findsOneWidget);

    // Clear failure and tap Accept: now succeeds
    app.repository.answerFriendRequestFailure = null;
    await tester.tap(acceptBtn);
    await tester.pumpAndSettle();

    // Jonas is now a friend, no longer in Pending
    expect(find.text('Jonas Weber'), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('sent_has_cancel', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();

    final tomasCard = find.ancestor(
      of: find.text('Tomás Ruiz'),
      matching: find.byType(PersonCard),
    );
    expect(tomasCard, findsOneWidget);

    final cancelBtn = find.descendant(
      of: tomasCard,
      matching: find.widgetWithText(PersonActionButton, 'Cancel'),
    );
    expect(cancelBtn, findsOneWidget);

    await tester.tap(cancelBtn);
    await tester.pumpAndSettle();

    expect(find.text('Tomás Ruiz'), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('blocked_has_unblock', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Blocked'));
    await tester.pumpAndSettle();

    final unblockBtn = find.widgetWithText(PersonActionButton, 'Unblock');
    expect(unblockBtn, findsOneWidget);

    await tester.tap(unblockBtn);
    await tester.pumpAndSettle();

    expect(find.text("You haven't blocked anyone."), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('spam_fold_stays', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();

    // 1. Initially spam section is collapsed
    expect(find.textContaining('Spam — 1'), findsOneWidget);
    expect(find.text('Lena Fischer'), findsNothing);

    // First cycle: expand and collapse
    await tester.tap(find.textContaining('Spam — 1'));
    await tester.pumpAndSettle();
    expect(find.text('Lena Fischer'), findsOneWidget);

    await tester.tap(find.textContaining('Spam — 1'));
    await tester.pumpAndSettle();
    expect(find.text('Lena Fischer'), findsNothing);

    // Second cycle: expand and collapse again
    await tester.tap(find.textContaining('Spam — 1'));
    await tester.pumpAndSettle();
    expect(find.text('Lena Fischer'), findsOneWidget);

    await tester.tap(find.textContaining('Spam — 1'));
    await tester.pumpAndSettle();
    expect(find.text('Lena Fischer'), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('empty_says_so', (tester) async {
    final app = await MockApp.pump(tester);

    // Clear all contacts
    app.repository.privateWorld.contacts.clear();
    app.container.read(privateContactsProvider.notifier).onStateChanged();
    await _pumpFriends(tester, app);

    // Online empty
    expect(find.text("No one's online right now."), findsOneWidget);
    expect(
      find.byWidgetPredicate((w) => w is Icon && w.icon == OcIcons.group && w.size == 32),
      findsOneWidget,
    );

    // All empty
    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();
    expect(find.text("You haven't added any friends yet."), findsOneWidget);
    expect(
      find.byWidgetPredicate((w) => w is Icon && w.icon == OcIcons.group && w.size == 32),
      findsOneWidget,
    );

    // Pending empty
    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();
    expect(find.text('No pending requests.'), findsOneWidget);
    expect(
      find.byWidgetPredicate((w) => w is Icon && w.icon == OcIcons.mail && w.size == 32),
      findsOneWidget,
    );

    // Blocked empty
    await tester.tap(find.text('Blocked'));
    await tester.pumpAndSettle();
    expect(find.text("You haven't blocked anyone."), findsOneWidget);
    expect(
      find.byWidgetPredicate((w) => w is Icon && w.icon == OcIcons.block && w.size == 32),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('page_fades_in_tabs_do_not_slide', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    // AnimatedSwitcher with FadeTransition is used
    expect(find.byType(AnimatedSwitcher), findsOneWidget);

    // Tap All tab
    await tester.tap(find.text('All'));
    await tester.pump();
    // Pump half-way through the 100 ms fade
    await tester.pump(const Duration(milliseconds: 50));

    final fade = tester.widget<FadeTransition>(
      find.descendant(
        of: find.byType(AnimatedSwitcher),
        matching: find.byType(FadeTransition),
      ).first,
    );
    expect(fade.opacity.value, greaterThan(0.0));
    expect(fade.opacity.value, lessThan(1.0));

    await tester.pumpAndSettle();
    final finalFade = tester.widget<FadeTransition>(
      find.descendant(
        of: find.byType(AnimatedSwitcher),
        matching: find.byType(FadeTransition),
      ),
    );
    expect(finalFade.opacity.value, 1.0);

    // Verify there is no TabBarView or slide transition
    expect(find.byType(TabBarView), findsNothing);
    expect(find.byType(SlideTransition), findsNothing);

    await app.dispose(tester);
  });
}
