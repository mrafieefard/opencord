import 'dart:async';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/composer_bar.dart';
import 'package:opencord/features/chat/typing_dots.dart';
import 'package:opencord/features/home/move_card.dart';
import 'package:opencord/features/home/private_chat.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/home/relay_divider.dart';
import 'package:opencord/features/home/request_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

Future<void> _pumpChat(WidgetTester tester, MockApp app, String peer) async {
  await tester.pumpWidget(
    UncontrolledProviderScope(
      container: app.container,
      child: MaterialApp(
        theme: buildTheme(OcColors.dark),
        home: Scaffold(body: PrivateChat(peer: peer)),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _rightClick(WidgetTester tester, Finder finder) async {
  await tester.tapAt(
    tester.getCenter(finder),
    buttons: kSecondaryMouseButton,
    kind: PointerDeviceKind.mouse,
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('a_dividers_server_name_opens_that_server', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;
    app.container.read(navigationProvider.notifier).openConversation(kaiKey);

    await _pumpChat(tester, app, kaiKey);
    await tester.tap(
      find.descendant(
        of: find.byType(RelayDivider),
        matching: find.text('Opencord Dev'),
      ),
    );
    await tester.pumpAndSettle();

    final selection = app.container.read(navigationProvider);
    expect(selection.home, isFalse);
    expect(selection.server, 'opencord.example:7710');

    await app.dispose(tester);
  });

  testWidgets('a_server_this_device_lacks_is_named_but_not_a_link', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: app.container,
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: const Scaffold(
            body: RelayDivider(
              serverName: 'Elsewhere',
              serverKey: 'elsewhere.example:7710',
            ),
          ),
        ),
      ),
    );

    expect(find.text('Elsewhere'), findsOneWidget);
    expect(
      find.descendant(
        of: find.byType(RelayDivider),
        matching: find.byType(Hoverable),
      ),
      findsNothing,
    );

    await app.dispose(tester);
  });

  testWidgets('messages_show_in_order_under_the_first_divider', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    await _pumpChat(tester, app, kaiKey);

    // Divider at top
    expect(find.byType(RelayDivider), findsOneWidget);
    expect(find.text('Opencord Dev'), findsAtLeastNWidgets(1));

    // Messages in order
    expect(
      find.textContaining(
        'Hey Alex, are you working on private messages?',
        findRichText: true,
      ),
      findsOneWidget,
    );
    expect(
      find.textContaining(
        'Yeah, wrapping up Milestone E4 right now!',
        findRichText: true,
      ),
      findsOneWidget,
    );

    // Move card at bottom of Kai's messages
    expect(find.byType(MoveCard), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('own_messages_can_be_edited_and_deleted', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    await _pumpChat(tester, app, kaiKey);

    // Right click on our own message
    final ownMsg = find.textContaining(
      'Yeah, wrapping up Milestone E4 right now!',
      findRichText: true,
    );
    expect(ownMsg, findsOneWidget);
    await _rightClick(tester, ownMsg);

    // Edit in menu
    expect(find.text('Edit'), findsOneWidget);
    await tester.tap(find.text('Edit'));
    await tester.pumpAndSettle();

    // Context bar shows editing
    expect(find.byType(ComposerContextBar), findsOneWidget);
    expect(find.text('Edit message'), findsOneWidget);

    // Change text and press enter
    final textField = find.byType(TextField);
    await tester.enterText(textField, 'Updated message text!');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Updated message text!', findRichText: true),
      findsOneWidget,
    );

    // Now delete our message
    await _rightClick(
      tester,
      find.textContaining('Updated message text!', findRichText: true),
    );

    await tester.tap(find.text('Delete'));
    await tester.pumpAndSettle();

    // Confirm dialog
    expect(find.byType(OcDialog), findsOneWidget);
    expect(find.text('Delete message?'), findsOneWidget);
    await tester.tap(
      find.descendant(of: find.byType(OcDialog), matching: find.text('Delete')),
    );
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Updated message text!', findRichText: true),
      findsNothing,
    );

    await app.dispose(tester);
  });

  testWidgets('a_failed_message_retries', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    // Add a failed message
    final failedMsg = PrivateMessage(
      id: 'failed1001',
      peer: kaiKey,
      outgoing: true,
      kind: PrivateMessageKind.text,
      content: 'This send failed',
      sentAt: DateTime.now(),
      state: PrivateSendState.failed,
    );
    app.repository.privateWorld.messages[kaiKey]!.add(failedMsg);
    app.container
        .read(privateConversationProvider(kaiKey).notifier)
        .onMessageAdded(failedMsg);

    await _pumpChat(tester, app, kaiKey);
    final failed = find.textContaining('This send failed', findRichText: true);
    expect(failed, findsOneWidget);

    // Right click the failed message
    await _rightClick(tester, failed);

    expect(find.text('Retry'), findsNWidgets(2));
    await tester.tap(find.text('Retry').last);
    await tester.pumpAndSettle();

    // Retry marks it sending/sent
    final state = app.container.read(privateConversationProvider(kaiKey));
    final msg = state.messages.firstWhere((m) => m.id == 'failed1001');
    expect(msg.state != PrivateSendState.failed, isTrue);

    await app.dispose(tester);
  });

  testWidgets('move_too_appears_only_when_it_can_work', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    await _pumpChat(tester, app, kaiKey);

    // Kai is on Rust Berlin and Rust Berlin is connected in mock world, so Move too appears.
    expect(find.text('Move too'), findsOneWidget);
    expect(
      find.textContaining('Kai Nakamura moved this chat to Rust Berlin'),
      findsOneWidget,
    );

    // Tap Move too
    await tester.tap(find.text('Move too'));
    await tester.pumpAndSettle();

    final contact = app.container.read(privateContactsProvider).byPeer[kaiKey]!;
    expect(contact.relayServerKey, 'rust-berlin.example:7710');
    // Once there, the card stays but has nothing more to offer.
    expect(find.text('Move too'), findsNothing);
    expect(
      find.textContaining('Kai Nakamura moved this chat to Rust Berlin'),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('the_request_bar_accepts_ignores_and_blocks', (tester) async {
    final app = await MockApp.pump(tester);
    final priyaKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Priya Shah')
        .peer;

    await _pumpChat(tester, app, priyaKey);

    // In request mode
    expect(find.byType(RequestBar), findsOneWidget);
    expect(find.text('Priya Shah wants to message you.'), findsOneWidget);
    expect(find.text('Accept'), findsOneWidget);
    expect(find.text('Ignore'), findsOneWidget);
    expect(find.text('Block'), findsOneWidget);

    // Tap Accept
    await tester.tap(find.text('Accept'));
    await tester.pumpAndSettle();

    // Now accepted: RequestBar disappears and composer appears
    expect(find.byType(RequestBar), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('each_disabled_state_says_why', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    // 1. Blocked
    app.repository.privateWorld.blockUser(kaiKey);
    await _pumpChat(tester, app, kaiKey);
    expect(find.text('You blocked Kai Nakamura.'), findsOneWidget);
    expect(find.text('Unblock'), findsOneWidget);

    // Unblock
    await tester.tap(find.text('Unblock'));
    await tester.pumpAndSettle();
    expect(find.text('You blocked Kai Nakamura.'), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('the_relay_chip_lists_shared_servers_and_moves', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    await _pumpChat(tester, app, kaiKey);

    // Tap relay chip
    await tester.tap(find.textContaining('via '));
    await tester.pumpAndSettle();

    // Lists shared servers: Opencord Dev and Rust Berlin
    expect(find.text('Opencord Dev'), findsAtLeastNWidgets(1));
    expect(find.text('Rust Berlin'), findsAtLeastNWidgets(1));

    // Tap Opencord Dev
    await tester.tap(find.text('Opencord Dev').last);
    await tester.pumpAndSettle();

    final contact = app.container.read(privateContactsProvider).byPeer[kaiKey]!;
    expect(contact.relayServerKey, 'opencord.example:7710');

    await app.dispose(tester);
  });

  testWidgets('typing_shows_and_goes_away', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    await _pumpChat(tester, app, kaiKey);

    // Not typing initially
    expect(find.byType(TypingDots), findsNothing);

    // Emit typing event
    app.repository.emit(PrivateTypingStarted(kaiKey));
    await tester.pump();
    await tester.pump();

    // Shows typing
    expect(find.byType(TypingDots), findsOneWidget);
    expect(find.text('Kai Nakamura is typing…'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('enter_sends_and_shift_enter_adds_a_line', (tester) async {
    final app = await MockApp.pump(tester);
    final kaiKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Kai Nakamura')
        .peer;

    await _pumpChat(tester, app, kaiKey);

    final input = find.byType(TextField);
    await tester.enterText(input, 'Testing enter key');
    await tester.pump();

    // Enter key sends
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(
      find.textContaining('Testing enter key', findRichText: true),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('opening_the_chat_marks_it_read', (tester) async {
    final app = await MockApp.pump(tester);
    // Mira Okafor starts with unread = 1
    final miraKey = app.repository.privateWorld.contacts.values
        .firstWhere((c) => c.name == 'Mira Okafor')
        .peer;

    expect(
      app.container.read(privateContactsProvider).byPeer[miraKey]!.unread,
      1,
    );

    await _pumpChat(tester, app, miraKey);
    // Wait for 500ms debounce
    await tester.pump(const Duration(milliseconds: 600));

    expect(
      app.container.read(privateContactsProvider).byPeer[miraKey]!.unread,
      0,
    );

    await app.dispose(tester);
  });

  testWidgets('a_friends_only_server_is_offered_only_for_friends', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final contacts = app.repository.privateWorld.contacts.values;
    final kai = contacts.firstWhere((c) => c.name == 'Kai Nakamura').peer;
    final priya = contacts.firstWhere((c) => c.name == 'Priya Shah').peer;
    List<String> shared(String peer) => [
      for (final server in app.read(sharedServersProvider(peer)))
        server.serverKey,
    ];
    expect(shared(kai), contains(_dev));
    expect(shared(priya), contains(_dev));

    app.repository.debugSetPrivateSettings(
      _dev,
      const ServerPrivateSettings(enabled: true, friendsOnly: true),
    );
    await tester.pump(const Duration(milliseconds: 100));

    expect(shared(kai), contains(_dev));
    expect(shared(priya), isNot(contains(_dev)));
    await app.dispose(tester);
  });

  testWidgets('a_friends_only_refusal_says_what_to_do', (tester) async {
    final app = await MockApp.pump(tester);
    final context = tester.element(find.byType(Navigator).first);
    unawaited(
      showPrivateFailure(
        context,
        const RepoException(RepoErrorKind.friendsOnly, 'friends only'),
        name: 'Priya Shah',
        server: 'Opencord Dev',
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byType(OcDialog), findsOneWidget);
    expect(find.text("Couldn't send message"), findsOneWidget);
    expect(
      find.textContaining(
        'Opencord Dev only carries private messages between friends',
      ),
      findsOneWidget,
    );
    await app.dispose(tester);
  });
}
