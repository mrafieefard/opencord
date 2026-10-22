import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/members/member_profile.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/switcher/quick_switcher.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _berlin = 'rust-berlin.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

int _channelId(MockApp app, String server, String name) => app
    .read(serverProvider(server))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

Future<void> _open(WidgetTester tester, MockApp app, String name) async {
  app
      .read(navigationProvider.notifier)
      .openChannel(_dev, _channelId(app, _dev, name));
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

Future<void> _ctrlK(WidgetTester tester) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  await tester.sendKeyEvent(LogicalKeyboardKey.keyK);
  await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

Finder get _input => find.descendant(
  of: find.byType(QuickSwitcher),
  matching: find.byType(TextField),
);

Future<void> _type(WidgetTester tester, String text) async {
  await tester.enterText(_input, text);
  await tester.pump();
}

Future<void> _press(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyEvent(key);
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

void main() {
  testWidgets('Ctrl+K opens it and Escape closes it', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _open(tester, app, 'general');

    await _ctrlK(tester);
    expect(find.byType(QuickSwitcher), findsOneWidget);
    expect(find.text('Jump to a channel, server or member'), findsOneWidget);

    await _press(tester, LogicalKeyboardKey.escape);
    expect(find.byType(QuickSwitcher), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('Enter opens a channel on another server', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _open(tester, app, 'general');

    await _ctrlK(tester);
    await _type(tester, 'show');
    expect(find.text('#show-and-tell'), findsOneWidget);
    expect(find.text('Rust Berlin'), findsWidgets);
    await _press(tester, LogicalKeyboardKey.enter);

    expect(app.read(currentServerProvider), _berlin);
    expect(
      app.read(currentChannelProvider),
      _channelId(app, _berlin, 'show-and-tell'),
    );
    await app.dispose(tester);
  });

  testWidgets('with nothing typed, recent channels lead', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _open(tester, app, 'general');
    await _open(tester, app, 'help');
    await _open(tester, app, 'off-topic');

    await _ctrlK(tester);
    // Down past the open channel to the one before it.
    await _press(tester, LogicalKeyboardKey.arrowDown);
    await _press(tester, LogicalKeyboardKey.enter);

    expect(app.read(currentChannelProvider), _channelId(app, _dev, 'help'));
    await app.dispose(tester);
  });

  testWidgets('@ finds members; Enter shows the profile', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _open(tester, app, 'general');

    await _ctrlK(tester);
    await _type(tester, '@mira');
    await _press(tester, LogicalKeyboardKey.enter);

    final profile = find.byType(MemberProfileDialog);
    expect(profile, findsOneWidget);
    expect(tester.widget<MemberProfileDialog>(profile).userId, 1002);
    await app.dispose(tester);
  });

  testWidgets('nothing found says so', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _open(tester, app, 'general');

    await _ctrlK(tester);
    await _type(tester, 'zzqxj');

    expect(find.text('Nothing matches "zzqxj".'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('the search pill in the sidebar opens it', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _open(tester, app, 'general');

    await tester.tap(find.text('Search'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.byType(QuickSwitcher), findsOneWidget);
    await app.dispose(tester);
  });
}
