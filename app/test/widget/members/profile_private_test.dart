import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/home/private_chat.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/members/member_profile.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _openChannel(WidgetTester tester, MockApp app, String name) async {
  final id = app
      .read(serverProvider(_server))
      .data!
      .channels
      .values
      .firstWhere((channel) => channel.name == name)
      .id;
  app.read(navigationProvider.notifier).openChannel(_server, id);
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

Finder _row(String name) => find.byWidgetPredicate(
  (widget) => widget is MemberRow && widget.line.member.displayName == name,
);

void main() {
  testWidgets('your_own_profile_has_no_private_buttons', (tester) async {
    final app = await MockApp.pump(tester);
    await _openChannel(tester, app, 'general');

    // Alex Rivera is self (userId 1000).
    await tester.tap(_row('Alex Rivera'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.byType(MemberProfileDialog), findsOneWidget);
    expect(
      find.descendant(
        of: find.byType(MemberProfileDialog),
        matching: find.widgetWithText(OcButton, 'Message'),
      ),
      findsNothing,
    );
    expect(
      find.descendant(
        of: find.byType(MemberProfileDialog),
        matching: find.widgetWithText(OcButton, 'Add friend'),
      ),
      findsNothing,
    );
    expect(
      find.descendant(
        of: find.byType(MemberProfileDialog),
        matching: find.widgetWithText(OcButton, 'Friends'),
      ),
      findsNothing,
    );
    expect(
      find.descendant(
        of: find.byType(MemberProfileDialog),
        matching: find.byIcon(OcIcons.moreHoriz),
      ),
      findsNothing,
    );

    await app.dispose(tester);
  });

  testWidgets('profile_buttons_follow_the_friend_state', (tester) async {
    final app = await MockApp.pump(tester);
    await _openChannel(tester, app, 'general');

    // 1. Friends: Kai Nakamura (1001)
    await tester.tap(_row('Kai Nakamura'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final friendsBtn = find.descendant(
      of: find.byType(MemberProfileDialog),
      matching: find.widgetWithText(OcButton, 'Friends'),
    );
    expect(friendsBtn, findsOneWidget);
    final friendsWidget = tester.widget<OcButton>(friendsBtn);
    expect(friendsWidget.onPressed, isNull);
    expect(friendsWidget.icon, OcIcons.check);

    // Close Kai's profile
    await tester.tapAt(const Offset(10, 10));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.byType(MemberProfileDialog), findsNothing);

    // 2. Incoming request: Jonas Weber (1003)
    await tester.tap(_row('Jonas Weber'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final acceptBtn = find.descendant(
      of: find.byType(MemberProfileDialog),
      matching: find.widgetWithText(OcButton, 'Accept request'),
    );
    expect(acceptBtn, findsOneWidget);
    await tester.tap(acceptBtn);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // After accepting, Jonas is now a friend
    expect(
      find.descendant(
        of: find.byType(MemberProfileDialog),
        matching: find.widgetWithText(OcButton, 'Friends'),
      ),
      findsOneWidget,
    );

    // Close Jonas's profile
    await tester.tapAt(const Offset(10, 10));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // 3. Outgoing request: Tomás Ruiz (1006)
    await tester.tap(_row('Tomás Ruiz'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final cancelBtn = find.descendant(
      of: find.byType(MemberProfileDialog),
      matching: find.widgetWithText(OcButton, 'Request sent'),
    );
    expect(cancelBtn, findsOneWidget);
    await tester.tap(cancelBtn);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Cancelling shows toast and changes button to "Add friend"
    expect(find.text('Friend request cancelled'), findsOneWidget);
    expect(
      find.descendant(
        of: find.byType(MemberProfileDialog),
        matching: find.widgetWithText(OcButton, 'Add friend'),
      ),
      findsOneWidget,
    );

    // Close Tomás's profile
    await tester.tapAt(const Offset(10, 10));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // 4. FriendStatus.none: Ethan Brooks
    await tester.tap(_row('Ethan Brooks'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final addBtn = find.descendant(
      of: find.byType(MemberProfileDialog),
      matching: find.widgetWithText(OcButton, 'Add friend'),
    );
    expect(addBtn, findsOneWidget);
    final addWidget = tester.widget<OcButton>(addBtn);
    expect(addWidget.icon, OcIcons.personAdd);

    await tester.tap(addBtn);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // After sending request, button becomes "Request sent"
    expect(
      find.descendant(
        of: find.byType(MemberProfileDialog),
        matching: find.widgetWithText(OcButton, 'Request sent'),
      ),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets('blocking_from_a_profile_asks_first', (tester) async {
    final app = await MockApp.pump(tester);
    await _openChannel(tester, app, 'general');

    await tester.tap(_row('Kai Nakamura'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final moreBtn = find.descendant(
      of: find.byType(MemberProfileDialog),
      matching: find.byIcon(OcIcons.moreHoriz),
    );
    expect(moreBtn, findsOneWidget);
    await tester.tap(moreBtn);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Menu shows Block
    expect(find.text('Block'), findsOneWidget);
    await tester.tap(find.text('Block'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Confirm dialog appears
    expect(find.text('Block Kai Nakamura?'), findsOneWidget);
    // Cancel first
    await tester.tap(find.widgetWithText(OcButton, 'Cancel'));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.text('Block Kai Nakamura?'), findsNothing);

    // Open menu again and confirm Block
    await tester.tap(moreBtn);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(find.text('Block'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final confirmBtn = find.descendant(
      of: find.byType(OcDialog),
      matching: find.widgetWithText(OcButton, 'Block'),
    );
    await tester.tap(confirmBtn);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Now menu shows Unblock
    await tester.tap(moreBtn);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.text('Unblock'), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('message_opens_the_conversation', (tester) async {
    final app = await MockApp.pump(tester);
    await _openChannel(tester, app, 'general');

    await tester.tap(_row('Kai Nakamura'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final messageBtn = find.descendant(
      of: find.byType(MemberProfileDialog),
      matching: find.widgetWithText(OcButton, 'Message'),
    );
    expect(messageBtn, findsOneWidget);

    await tester.tap(messageBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // Dialog closed, conversation open
    expect(find.byType(MemberProfileDialog), findsNothing);
    expect(find.byType(PrivateChat), findsOneWidget);

    await app.dispose(tester);
  });
}
