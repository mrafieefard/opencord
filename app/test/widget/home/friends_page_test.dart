import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

import '../../support/app.dart';

Future<void> _pumpFriends(WidgetTester tester, MockApp app) async {
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: app.container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: const Scaffold(body: FriendsPage()),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('each_tab_lists_its_people_with_a_count', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    // Default tab is Online. Kai and Mira are online; Noah is offline.
    expect(find.text('ONLINE — 2'), findsOneWidget);
    expect(find.text('Kai Nakamura'), findsOneWidget);
    expect(find.text('Mira Okafor'), findsOneWidget);
    expect(find.text('Noah Kim'), findsNothing);

    // Switch to All tab.
    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();
    expect(find.text('ALL — 3'), findsOneWidget);
    expect(find.text('Kai Nakamura'), findsOneWidget);
    expect(find.text('Mira Okafor'), findsOneWidget);
    expect(find.text('Noah Kim'), findsOneWidget);

    // Switch to Pending tab.
    // Non-spam pending: Jonas Weber (incoming), Tomás Ruiz (outgoing).
    // Plus Spam section with Lena Fischer.
    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();
    expect(find.text('PENDING — 2'), findsOneWidget);
    expect(find.text('Jonas Weber'), findsOneWidget);
    expect(find.text('Tomás Ruiz'), findsOneWidget);
    expect(find.textContaining('Spam — 1'), findsOneWidget);

    // Switch to Message requests tab.
    await tester.tap(find.text('Message requests'));
    await tester.pumpAndSettle();
    expect(find.text('MESSAGE REQUESTS — 2'), findsOneWidget);
    expect(find.text('Priya Shah'), findsOneWidget);
    expect(find.text('Greta Hoffmann'), findsOneWidget);

    // Switch to Blocked tab.
    await tester.tap(find.text('Blocked'));
    await tester.pumpAndSettle();
    expect(find.text('BLOCKED — 1'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('empty_tabs_say_so', (tester) async {
    final app = await MockApp.pump(tester);

    // Clear all contacts to test empty states.
    app.repository.privateWorld.contacts.clear();
    app.container.read(privateContactsProvider.notifier).onStateChanged();
    await _pumpFriends(tester, app);

    // Online
    expect(find.text("No one's online right now."), findsOneWidget);

    // All
    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();
    expect(find.text("You haven't added any friends yet."), findsOneWidget);

    // Pending
    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();
    expect(find.text('No pending requests.'), findsOneWidget);

    // Message requests
    await tester.tap(find.text('Message requests'));
    await tester.pumpAndSettle();
    expect(find.text('No message requests.'), findsOneWidget);

    // Blocked
    await tester.tap(find.text('Blocked'));
    await tester.pumpAndSettle();
    expect(find.text("You haven't blocked anyone."), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('accepting_and_declining_update_pending', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();

    expect(find.text('Jonas Weber'), findsOneWidget);

    // Find Accept button for Jonas Weber.
    final jonasRow = find.ancestor(
      of: find.text('Jonas Weber'),
      matching: find.byType(Row),
    );
    final acceptBtn = find.descendant(
      of: jonasRow,
      matching: find.byTooltip('Accept'),
    );
    await tester.tap(acceptBtn);
    await tester.pumpAndSettle();

    // Jonas is now friends, no longer in Pending.
    expect(find.text('Jonas Weber'), findsNothing);
    expect(find.text('PENDING — 1'), findsOneWidget);

    // Now switch to All tab; Jonas should be listed there.
    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();
    expect(find.text('Jonas Weber'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('cancelling_an_outgoing_request_removes_it', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();

    expect(find.text('Tomás Ruiz'), findsOneWidget);

    // Cancel Tomás Ruiz's outgoing request.
    final tomasRow = find.ancestor(
      of: find.text('Tomás Ruiz'),
      matching: find.byType(Row),
    );
    final cancelBtn = find.descendant(
      of: tomasRow,
      matching: find.byTooltip('Cancel'),
    );
    await tester.tap(cancelBtn);
    await tester.pumpAndSettle();

    expect(find.text('Tomás Ruiz'), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('spam_requests_wait_in_a_collapsed_section', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();

    // Spam row is visible and collapsed: Lena Fischer is not visible.
    expect(find.textContaining('Spam — 1'), findsOneWidget);
    expect(find.text('Lena Fischer'), findsNothing);

    // Expand spam.
    await tester.tap(find.textContaining('Spam — 1'));
    await tester.pumpAndSettle();

    // Now Lena Fischer is visible with Accept and Decline buttons.
    expect(find.text('Lena Fischer'), findsOneWidget);
    final lenaRow = find.ancestor(
      of: find.text('Lena Fischer'),
      matching: find.byType(Row),
    );
    expect(
      find.descendant(of: lenaRow, matching: find.byTooltip('Accept')),
      findsOneWidget,
    );
    expect(
      find.descendant(of: lenaRow, matching: find.byTooltip('Decline')),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('removing_a_friend_and_blocking_ask_first', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('All'));
    await tester.pumpAndSettle();

    // Open Kai's more menu.
    final kaiRow = find.ancestor(
      of: find.text('Kai Nakamura'),
      matching: find.byType(Row),
    );
    final moreBtn = find.descendant(
      of: kaiRow,
      matching: find.byTooltip('More options'),
    );
    await tester.tap(moreBtn);
    await tester.pumpAndSettle();

    // Tap Remove friend.
    await tester.tap(find.text('Remove friend'));
    await tester.pumpAndSettle();

    // Dialog asks first.
    expect(find.byType(OcDialog), findsOneWidget);
    expect(find.text('Remove Kai Nakamura?'), findsOneWidget);

    // Cancel.
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(find.text('Kai Nakamura'), findsOneWidget);

    // Open menu again and tap Block.
    await tester.tap(moreBtn);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Block'));
    await tester.pumpAndSettle();

    // Dialog asks first.
    expect(find.byType(OcDialog), findsOneWidget);
    expect(find.text('Block Kai Nakamura?'), findsOneWidget);

    // Confirm Block.
    await tester.tap(
      find.descendant(of: find.byType(OcDialog), matching: find.text('Block')),
    );
    await tester.pumpAndSettle();

    // Kai is removed from friends and blocked.
    expect(find.text('Kai Nakamura'), findsNothing);

    // Switch to Blocked tab; Kai is there.
    await tester.tap(find.text('Blocked'));
    await tester.pumpAndSettle();
    expect(find.text('Kai Nakamura'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('unblocking_moves_them_out_of_blocked', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Blocked'));
    await tester.pumpAndSettle();

    expect(find.text('Unblock'), findsOneWidget);
    await tester.tap(find.text('Unblock'));
    await tester.pumpAndSettle();

    // Blocked list is now empty.
    expect(find.text("You haven't blocked anyone."), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('a_failed_action_shows_its_dialog', (tester) async {
    final app = await MockApp.pump(tester);
    await _pumpFriends(tester, app);

    await tester.tap(find.text('Pending'));
    await tester.pumpAndSettle();

    // Configure repository to fail when answering Jonas Weber's friend request.
    app.repository.answerFriendRequestFailure = const RepoException(
      RepoErrorKind.privateMessagesUnavailable,
      "Couldn't answer: Opencord Dev isn't connected right now.",
    );

    final jonasRow = find.ancestor(
      of: find.text('Jonas Weber'),
      matching: find.byType(Row),
    );
    final acceptBtn = find.descendant(
      of: jonasRow,
      matching: find.byTooltip('Accept'),
    );
    await tester.tap(acceptBtn);
    await tester.pumpAndSettle();

    // An OcDialog is shown explaining the failure.
    expect(find.byType(OcDialog), findsOneWidget);
    expect(find.text("Couldn't answer friend request"), findsOneWidget);

    await app.dispose(tester);
  });
}
