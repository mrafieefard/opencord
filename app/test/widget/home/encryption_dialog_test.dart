import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/encryption_dialog.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/home/home_main.dart';
import 'package:opencord/features/home/private_chat.dart';
import 'package:opencord/features/home/private_composer.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

import '../../support/app.dart';

Future<void> _pump(WidgetTester tester, MockApp app, Widget body) async {
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: app.container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: Scaffold(body: body),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

String _peer(MockApp app, String name) => app
    .repository
    .privateWorld
    .contacts
    .values
    .firstWhere((c) => c.name == name)
    .peer;

/// The header's lock button, whatever its state.
Finder get _lock => find.byWidgetPredicate(
  (widget) =>
      widget is Icon &&
      (widget.icon == OcIcons.lock ||
          widget.icon == OcIcons.verifiedUser ||
          widget.icon == OcIcons.warning),
);

void main() {
  testWidgets('the dialog shows 16 words in 4 rows', (tester) async {
    final app = await MockApp.pump(tester);
    await _pump(tester, app, PrivateChat(peer: _peer(app, 'Mira Okafor')));

    await tester.tap(_lock.first);
    await tester.pumpAndSettle();

    expect(find.text('End-to-end encryption'), findsOneWidget);
    expect(find.text('Verified'), findsOneWidget);
    final grid = find.byType(WordGrid);
    expect(grid, findsOneWidget);
    expect(
      find.descendant(of: grid, matching: find.byType(Row)),
      findsNWidgets(4),
    );
    expect(
      find.descendant(of: grid, matching: find.byType(Text)),
      findsNWidgets(16),
    );
    expect(
      find.textContaining('Compare these words with Mira Okafor'),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('They match turns the lock verified', (tester) async {
    final app = await MockApp.pump(tester);
    final kai = _peer(app, 'Kai Nakamura');
    await _pump(tester, app, PrivateChat(peer: kai));
    expect(
      find.byTooltip(
        'End-to-end encrypted. Not verified: compare the words with Kai Nakamura.',
      ),
      findsOneWidget,
    );

    await tester.tap(_lock.first);
    await tester.pumpAndSettle();
    await tester.tap(find.text('They match'));
    await tester.pumpAndSettle();

    expect(
      find.byTooltip('End-to-end encrypted. Verified with Kai Nakamura.'),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets("They don't match shows the warning banner", (tester) async {
    final app = await MockApp.pump(tester);
    await _pump(tester, app, PrivateChat(peer: _peer(app, 'Mira Okafor')));

    await tester.tap(_lock.first);
    await tester.pumpAndSettle();
    await tester.tap(find.text("They don't match"));
    await tester.pumpAndSettle();

    expect(
      find.text(
        "The words didn't match. Someone may be able to read this chat. "
        "Don't send anything private.",
      ),
      findsOneWidget,
    );
    expect(
      find.byTooltip("End-to-end encrypted. The words didn't match."),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('a key change shows the banner until the new keys are used', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _pump(tester, app, PrivateChat(peer: _peer(app, 'Kai Nakamura')));
    const banner =
        "Kai Nakamura's keys changed. They may have reinstalled, or someone "
        "may be trying to read this chat. Your messages won't reach the new "
        'keys until you compare the words.';
    expect(find.text(banner), findsOneWidget);
    expect(find.text('Compare words'), findsOneWidget);

    await tester.tap(find.text('Use new keys anyway'));
    await tester.pumpAndSettle();

    expect(find.text(banner), findsNothing);
    expect(find.text("Kai Nakamura's keys changed."), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('the composer is disabled when nothing is reachable', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final kai = _peer(app, 'Kai Nakamura');
    final world = app.repository.privateWorld;
    final unreachable = world.contacts[kai]!.copyWith(reachable: false);
    world.contacts[kai] = unreachable;
    app.repository.emit(PrivateContactChanged(unreachable));
    await _pump(tester, app, PrivateChat(peer: kai));

    expect(find.byType(PrivateComposer), findsNothing);
    expect(
      find.text(
        "Nothing you write would reach Kai Nakamura until you compare the "
        'words or use the new keys.',
      ),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('the home view offers to approve another device of yours', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _pump(tester, app, const FriendsPage());
    const notice =
        "A device signed in as you with different keys. Approve it only if "
        "it's yours.";
    expect(find.text(notice), findsOneWidget);

    await tester.tap(find.text('Approve'));
    await tester.pumpAndSettle();
    expect(find.byType(WordGrid), findsOneWidget);
    await tester.tap(find.text('They match'));
    await tester.pumpAndSettle();

    expect(find.text(notice), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('a device without chats asks to be approved elsewhere', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app.repository.privateWorld.ownHasChats = false;
    await _pump(tester, app, const FriendsPage());
    const notice =
        'Your other devices use different keys. Approve this device on one '
        'of them so they share your chats.';
    expect(find.text(notice), findsOneWidget);

    await tester.tap(find.text('Keep new keys'));
    await tester.pumpAndSettle();

    expect(find.text(notice), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('an offered key asks for They match', (tester) async {
    final app = await MockApp.pump(tester);
    await _pump(tester, app, const FriendsPage());
    const notice = 'Your device F9E8 wants to give this device its keys.';
    expect(find.text(notice), findsOneWidget);

    await tester.tap(find.text('Compare words'));
    await tester.pumpAndSettle();
    expect(find.byType(WordGrid), findsOneWidget);
    await tester.tap(find.text('They match'));
    await tester.pumpAndSettle();

    expect(find.text(notice), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('the old chats notice shows once', (tester) async {
    final app = await MockApp.pump(tester);
    app.repository.debugSetPrivateKeysReset(true);
    const notice =
        'Private messages now use new encryption. Your old private chats '
        'were removed from this device.';

    await _pump(tester, app, const HomeMain());
    expect(find.text(notice), findsOneWidget);
    await tester.tap(find.text('OK'));
    await tester.pumpAndSettle();
    await _pump(tester, app, const SizedBox());
    await _pump(tester, app, const HomeMain());

    expect(find.text(notice), findsNothing);
    expect(app.repository.privateKeysReset, isFalse);

    await app.dispose(tester);
  });
}
