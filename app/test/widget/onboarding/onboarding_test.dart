import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/onboarding/onboarding_view.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';
import '../../support/clipboard.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _field(String label) => find.bySemanticsLabel(label);

void main() {
  testWidgets('without an identity the app starts with onboarding (§9.1)', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, withIdentity: false);

    expect(find.byType(OnboardingView), findsOneWidget);
    expect(find.byType(DesktopShell), findsNothing);
    expect(find.text('Create a new identity'), findsOneWidget);
    expect(find.text('I have a backup'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('a new identity shows its fingerprint, then opens the app', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, withIdentity: false);
    await tester.tap(find.text('Create a new identity'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    final fingerprint = find.textContaining(
      RegExp(r'^[A-Z0-9]{4} [A-Z0-9]{4}'),
    );
    expect(fingerprint, findsOneWidget);
    final continueButton = find.widgetWithText(OcButton, 'Continue');
    expect(tester.widget<OcButton>(continueButton).onPressed, isNull);

    await tester.enterText(_field('Display name'), 'Alex');
    await tester.pump();
    await tester.tap(continueButton);
    await _pumpFor(tester, const Duration(milliseconds: 800));

    expect(find.byType(DesktopShell), findsOneWidget);
    expect(app.repository.identity?.displayName, 'Alex');
    await app.dispose(tester);
  });

  testWidgets('the backup can be copied before going on', (tester) async {
    final clipboard = ClipboardSpy(tester);
    final app = await MockApp.pump(tester, withIdentity: false);
    await tester.tap(find.text('Create a new identity'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    await tester.tap(find.widgetWithText(OcButton, 'Copy backup'));
    await tester.pump();

    expect(clipboard.copied, startsWith('opencord-identity-v1:'));
    await app.dispose(tester);
  });

  testWidgets('a backup brings an existing identity here', (tester) async {
    final app = await MockApp.pump(tester, withIdentity: false);
    final backup = (await app.repository.generateIdentity()).backup;
    await tester.tap(find.text('I have a backup'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    await tester.enterText(_field('Identity backup'), 'not a backup');
    await tester.enterText(_field('Display name'), 'Alex');
    await tester.pump();
    await tester.tap(find.widgetWithText(OcButton, 'Continue'));
    await _pumpFor(tester, const Duration(milliseconds: 800));
    expect(
      find.text('That is not an Opencord identity backup.'),
      findsOneWidget,
    );

    await tester.enterText(_field('Identity backup'), backup);
    await tester.pump();
    await tester.tap(find.widgetWithText(OcButton, 'Continue'));
    await _pumpFor(tester, const Duration(milliseconds: 800));

    expect(find.byType(DesktopShell), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('Back returns to the choice', (tester) async {
    final app = await MockApp.pump(tester, withIdentity: false);
    await tester.tap(find.text('I have a backup'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    await tester.tap(find.widgetWithText(OcButton, 'Back'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(find.text('Create a new identity'), findsOneWidget);
    expect(find.byType(TextField), findsNothing);
    await app.dispose(tester);
  });
}
