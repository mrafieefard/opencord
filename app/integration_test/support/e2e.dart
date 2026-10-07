import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/app.dart';
import 'package:opencord/app_start.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

// What the acceptance checks against a real server share. Real time passes
// in them: the core talks to a real server.

/// Pumps until [finder] finds something.
Future<void> waitFor(
  WidgetTester tester,
  Finder finder, {
  Duration timeout = const Duration(seconds: 30),
}) async {
  final end = DateTime.now().add(timeout);
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 50));
    if (finder.evaluate().isNotEmpty) return;
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  throw TestFailure('Timed out waiting for $finder');
}

/// Pumps until [condition] holds.
Future<void> waitUntil(
  WidgetTester tester,
  bool Function() condition, {
  required String reason,
  Duration timeout = const Duration(seconds: 30),
}) async {
  final end = DateTime.now().add(timeout);
  while (!condition()) {
    if (DateTime.now().isAfter(end)) fail('Timed out: $reason');
    await tester.pump(const Duration(milliseconds: 50));
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
}

/// The app on the Rust core, as [profile]: its own data and keychain
/// entries.
Future<ProviderContainer> launch(
  WidgetTester tester, {
  required String profile,
}) async {
  final container = await startApp(
    profile: profile,
    mock: false,
    desktop: false,
  );
  await tester.pumpWidget(
    UncontrolledProviderScope(container: container, child: const OpencordApp()),
  );
  return container;
}

/// Onboarding with a new identity called [name].
Future<void> createIdentity(WidgetTester tester, String name) async {
  await waitFor(tester, find.text('Create a new identity'));
  await tester.tap(find.text('Create a new identity'));
  await waitFor(tester, find.bySemanticsLabel('Display name'));
  await tester.enterText(find.bySemanticsLabel('Display name'), name);
  await tester.pump();
  await tester.tap(find.widgetWithText(OcButton, 'Continue'));
  await waitFor(tester, find.text('No servers yet'));
}

/// Fills in Add server with [linkOrAddress]; the caller presses Join.
Future<void> startAddingServer(
  WidgetTester tester,
  String linkOrAddress,
) async {
  await tester.tap(find.widgetWithText(OcButton, 'Add server'));
  await waitFor(tester, find.bySemanticsLabel('Invite link or address'));
  await tester.enterText(
    find.bySemanticsLabel('Invite link or address'),
    linkOrAddress,
  );
  await tester.pump();
}

Finder inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

Finder messageBubble(String text) => find.byWidgetPredicate(
  (widget) => widget is MessageBubble && widget.message.content == text,
);

/// Types [text] into the open channel's composer and sends it.
Future<void> send(WidgetTester tester, String text) async {
  final composer = find.descendant(
    of: find.byType(Composer),
    matching: find.byType(TextField),
  );
  await waitFor(tester, composer);
  await tester.enterText(composer, text);
  await tester.pump();
  await tester.sendKeyEvent(LogicalKeyboardKey.enter);
  await tester.pump();
  await waitFor(tester, messageBubble(text));
}
