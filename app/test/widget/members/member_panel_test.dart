import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/members/member_profile.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/section_label.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';
const _kai = 1001;

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _open(WidgetTester tester, MockApp app, String name) async {
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

List<String> _sections(WidgetTester tester) => [
  for (final label in tester.widgetList<SectionLabel>(
    find.descendant(
      of: find.byType(MemberPanel),
      matching: find.byType(SectionLabel),
    ),
  ))
    label.text,
];

List<String> _menuLabels(WidgetTester tester) => [
  for (final panel in tester.widgetList<OcMenuPanel>(find.byType(OcMenuPanel)))
    for (final entry in panel.entries)
      if (entry is OcMenuItem) entry.label,
];

Future<void> _rightClick(WidgetTester tester, Finder finder) async {
  await tester.tap(
    finder,
    buttons: kSecondaryMouseButton,
    kind: PointerDeviceKind.mouse,
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

void main() {
  testWidgets('members come in hoisted roles, then online, then offline', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    final sections = _sections(tester);
    expect(sections.first, startsWith('Maintainer — '));
    expect(sections, contains(startsWith('Contributor — ')));
    expect(sections[sections.length - 2], startsWith('Online — '));
    expect(sections.last, startsWith('Offline — '));
    // The owner carries a star.
    expect(
      find.descendant(
        of: _row('Alex Rivera'),
        matching: find.byIcon(OcIcons.star),
      ),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  testWidgets('a private channel lists only who can see it', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    final everyone = find.byType(MemberRow).evaluate().length;

    await _open(tester, app, 'maintainers');

    expect(find.byType(MemberRow).evaluate().length, lessThan(everyone));
    await app.dispose(tester);
  });

  testWidgets('search keeps the matching members', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    await tester.enterText(
      find.descendant(
        of: find.byType(MemberPanel),
        matching: find.byType(TextField),
      ),
      'nakam',
    );
    await tester.pump();

    expect(find.byType(MemberRow), findsOneWidget);
    expect(_row('Kai Nakamura'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('a click opens the profile', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    await tester.tap(_row('Kai Nakamura'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    final profile = find.byType(MemberProfileDialog);
    expect(profile, findsOneWidget);
    for (final text in ['IDENTITY KEY', 'ROLES', 'MEMBER SINCE']) {
      expect(
        find.descendant(of: profile, matching: find.text(text)),
        findsOneWidget,
      );
    }
    expect(
      find.descendant(of: profile, matching: find.text('Maintainer')),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  testWidgets('the owner’s menu offers roles, kick and ban', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    await _rightClick(tester, _row('Kai Nakamura'));

    expect(_menuLabels(tester), [
      'Profile',
      'Mention',
      'Copy identity fingerprint',
      'Roles',
      'Kick',
      'Ban',
    ]);
    await app.dispose(tester);
  });

  testWidgets('a role is given from the menu', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    final member = app.read(serverProvider(_server)).data!.members[_kai]!;
    expect(member.roleIds, isNot(contains(2)));

    await _rightClick(tester, _row('Kai Nakamura'));
    await tester.tap(find.text('Roles'));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(find.text('Contributor').last);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(
      app.read(serverProvider(_server)).data!.members[_kai]!.roleIds,
      contains(2),
    );
    await app.dispose(tester);
  });

  testWidgets('kicking asks first, with an optional reason', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    await _rightClick(tester, _row('Kai Nakamura'));
    await tester.tap(find.text('Kick'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text('Kick Kai Nakamura?'), findsOneWidget);
    await tester.enterText(find.byType(TextField).last, 'Spam');
    await tester.tap(find.widgetWithText(OcButton, 'Kick'));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(app.read(serverProvider(_server)).data!.members[_kai], isNull);
    expect(_row('Kai Nakamura'), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('Mention types @name into the composer', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    await _rightClick(tester, _row('Mira Okafor'));
    await tester.tap(find.text('Mention'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final input = tester.widget<TextField>(
      find.descendant(
        of: find.byType(Composer),
        matching: find.byType(TextField),
      ),
    );
    expect(input.controller!.text, '@Mira Okafor ');
    expect(input.focusNode!.hasFocus, isTrue);
    await app.dispose(tester);
  });

  testWidgets('an author’s name in the chat opens their profile', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    final name = find.descendant(
      of: find.byType(MessageBubble),
      matching: find.textContaining('Priya Shah', findRichText: true),
    );

    await tester.tap(name.last);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    final profile = find.byType(MemberProfileDialog);
    expect(profile, findsOneWidget);
    expect(tester.widget<MemberProfileDialog>(profile).userId, 1005);
    await app.dispose(tester);
  });

  testWidgets('a mention in a message opens that profile', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    await tester.tapOnText(find.textRange.ofSubstring('@Alex Rivera').last);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    final profile = find.byType(MemberProfileDialog);
    expect(profile, findsOneWidget);
    expect(tester.widget<MemberProfileDialog>(profile).userId, 1000);
    await app.dispose(tester);
  });
}
