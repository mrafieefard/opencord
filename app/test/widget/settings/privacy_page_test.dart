import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/settings/user/privacy_page.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

import '../../support/app.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _openSettings(WidgetTester tester) async {
  await tester.tap(
    find.byWidgetPredicate(
      (widget) => widget is OcIconButton && widget.tooltip == 'User settings',
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  expect(find.byType(SettingsDialog), findsOneWidget);
}

Future<void> _page(WidgetTester tester, String label) async {
  await tester.tap(
    find.descendant(
      of: find.byType(SettingsDialog),
      matching: find.text(label),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

Finder _inDialog(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

void main() {
  testWidgets('privacy_changes_wait_for_the_save_bar', (tester) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Privacy');

    expect(find.byType(PrivacyPage), findsOneWidget);
    expect(_inDialog(find.text('Friend requests')), findsOneWidget);
    expect(
      _inDialog(find.text('Let people send you friend requests.')),
      findsOneWidget,
    );
    expect(_inDialog(find.text('Message requests')), findsOneWidget);
    expect(
      _inDialog(find.text("Let people who aren't your friends message you.")),
      findsOneWidget,
    );

    // Initial state: both are true, save bar is not dirty
    expect(
      find.byWidgetPredicate((w) => w is SaveBar && w.changes.dirty),
      findsNothing,
    );

    // Toggle friend requests switch
    final friendSwitch = find.descendant(
      of: find.byType(PrivacyPage),
      matching: find.byType(OcSwitch).first,
    );
    await tester.tap(friendSwitch);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Save bar is now dirty
    expect(
      find.byWidgetPredicate((w) => w is SaveBar && w.changes.dirty),
      findsOneWidget,
    );

    final saveButton = find.descendant(
      of: find.byType(SaveBar),
      matching: find.widgetWithText(OcButton, 'Save changes'),
    );
    expect(saveButton, findsOneWidget);

    // Discard changes
    final discardButton = find.descendant(
      of: find.byType(SaveBar),
      matching: find.widgetWithText(OcButton, 'Discard'),
    );
    await tester.tap(discardButton);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Save bar is no longer dirty
    expect(
      find.byWidgetPredicate((w) => w is SaveBar && w.changes.dirty),
      findsNothing,
    );

    // Toggle again and save
    await tester.tap(friendSwitch);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(saveButton);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // Saved in repository
    final settings = await app.read(repositoryProvider).privacySettings();
    expect(settings.allowFriendRequests, isFalse);
    expect(settings.allowMessageRequests, isTrue);

    await app.dispose(tester);
  });

  testWidgets('a_failed_privacy_save_is_shown', (tester) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Privacy');

    // Simulate failure on next save
    app.repository.savePrivacySettingsFailure = const RepoException(
      RepoErrorKind.other,
      'Privacy settings failed to save',
    );

    final friendSwitch = find.descendant(
      of: find.byType(PrivacyPage),
      matching: find.byType(OcSwitch).first,
    );
    await tester.tap(friendSwitch);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final saveButton = find.descendant(
      of: find.byType(SaveBar),
      matching: find.widgetWithText(OcButton, 'Save changes'),
    );
    await tester.tap(saveButton);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // Error is shown as toast
    expect(
      find.text('Privacy settings failed to save'),
      findsOneWidget,
    );

    await app.dispose(tester);
  });
}
