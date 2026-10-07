import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/app.dart';
import 'package:opencord/app_start.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

// Phase 1 M5 acceptance against a real server, in two runs of the app:
// a fresh install creates an identity and joins (owner claim, TOFU), then a
// restart is still connected. tool/check_m5.sh starts the server and runs
// both; without it this test skips.
const _server = String.fromEnvironment('OPENCORD_E2E_SERVER');
const _claim = String.fromEnvironment('OPENCORD_E2E_CLAIM');
const _phase = String.fromEnvironment('OPENCORD_E2E_PHASE');
const _profile = 'e2e';
const _message = 'Hello from the M5 check';

/// Real time passes here: the core talks to a real server.
Future<void> _waitFor(
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

Future<ProviderContainer> _launch(WidgetTester tester) async {
  final container = await startApp(
    profile: _profile,
    mock: false,
    desktop: false,
  );
  await tester.pumpWidget(
    UncontrolledProviderScope(container: container, child: const OpencordApp()),
  );
  return container;
}

Finder _inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

Finder _sent(String text) => find.byWidgetPredicate(
  (widget) => widget is MessageBubble && widget.message.content == text,
);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  final skip = _server.isEmpty ? 'Run through tool/check_m5.sh' : null;

  testWidgets(
    'a fresh install creates an identity and joins a server',
    skip: skip != null || _phase != 'join',
    (tester) async {
      // A run that failed half way may have left an identity behind.
      await SecureIdentityStore(profile: _profile).clear();
      await _launch(tester);

      await _waitFor(tester, find.text('Create a new identity'));
      await tester.tap(find.text('Create a new identity'));
      await _waitFor(tester, find.bySemanticsLabel('Display name'));
      await tester.enterText(find.bySemanticsLabel('Display name'), 'Owner');
      await tester.pump();
      await tester.tap(find.widgetWithText(OcButton, 'Continue'));
      await _waitFor(tester, find.text('No servers yet'));

      await tester.tap(find.widgetWithText(OcButton, 'Add server'));
      await _waitFor(tester, find.bySemanticsLabel('Invite link or address'));
      await tester.enterText(
        find.bySemanticsLabel('Invite link or address'),
        _server,
      );
      await tester.tap(find.text("I'm the owner"));
      await tester.pump();
      await tester.enterText(
        find.bySemanticsLabel('Owner claim token'),
        _claim,
      );
      await tester.pump();
      await tester.tap(find.widgetWithText(OcButton, 'Join'));
      // A self-signed server: the fingerprint is shown and trusted (TOFU).
      await _waitFor(
        tester,
        find.widgetWithText(OcButton, 'Trust and connect'),
      );
      await tester.tap(find.widgetWithText(OcButton, 'Trust and connect'));

      await _waitFor(tester, _inSidebar('general'));
      await tester.tap(_inSidebar('general'));
      final composer = find.descendant(
        of: find.byType(Composer),
        matching: find.byType(TextField),
      );
      await _waitFor(tester, composer);
      await tester.enterText(composer, _message);
      await tester.pump();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      await _waitFor(tester, _sent(_message));
    },
  );

  testWidgets(
    'after a restart it is still there and connected',
    skip: skip != null || _phase != 'restart',
    (tester) async {
      final container = await _launch(tester);
      try {
        // No onboarding: the identity came back from the keychain.
        await _waitFor(tester, find.byType(DesktopShell));
        final deadline = DateTime.now().add(const Duration(seconds: 30));
        while (!container
            .read(serverProvider(_server))
            .connection
            .isConnected) {
          if (DateTime.now().isAfter(deadline)) {
            fail('not connected after the restart');
          }
          await tester.pump(const Duration(milliseconds: 50));
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
        await tester.tap(_inSidebar('general'));
        await _waitFor(tester, _sent(_message));
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
