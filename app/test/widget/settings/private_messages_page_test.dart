import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/private_messages_page.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _openPage(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('Opencord Dev'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text('Server settings'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(
    find
        .descendant(
          of: find.byType(SettingsDialog),
          matching: find.text('Private messages'),
        )
        .first,
  );
  await _pumpFor(tester, const Duration(milliseconds: 400));
  expect(find.byType(ServerPrivateMessagesPage), findsOneWidget);
}

List<OcSwitch> _switches(WidgetTester tester) => tester
    .widgetList<OcSwitch>(
      find.descendant(
        of: find.byType(ServerPrivateMessagesPage),
        matching: find.byType(OcSwitch),
      ),
    )
    .toList();

Future<void> _save(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byType(SaveBar),
      matching: find.widgetWithText(OcButton, 'Save changes'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 400));
}

void main() {
  testWidgets('the owner turns on friends only, then private messages off', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openPage(tester);
    ServerPrivateSettings settings() =>
        app.read(serverProvider(_dev)).data!.privateSettings;
    expect(settings().enabled, isTrue);
    expect(settings().friendsOnly, isFalse);

    await tester.tap(find.byWidget(_switches(tester)[1]));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(settings().friendsOnly, isFalse, reason: 'waits for Save');
    await _save(tester);
    expect(settings().friendsOnly, isTrue);

    await tester.tap(find.byWidget(_switches(tester)[0]));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    // Friends only means nothing with private messages off.
    expect(_switches(tester)[1].onChanged, isNull);
    await _save(tester);
    expect(settings().enabled, isFalse);
    expect(app.read(serverProvider(_dev)).data!.privateMessages, isFalse);
    await app.dispose(tester);
  });

  testWidgets("the config file's choice can't be changed here", (tester) async {
    final app = await MockApp.pump(tester);
    app.repository.debugSetPrivateSettings(
      _dev,
      const ServerPrivateSettings(allowedByConfig: false),
    );
    await _openPage(tester);

    expect(
      find.descendant(
        of: find.byType(ServerPrivateMessagesPage),
        matching: find.byType(InlineError),
      ),
      findsOneWidget,
    );
    expect(_switches(tester).map((s) => s.onChanged), everyElement(isNull));
    expect(_switches(tester).first.value, isFalse);
    await app.dispose(tester);
  });

  testWidgets('a change made elsewhere shows while the page is open', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openPage(tester);
    expect(_switches(tester)[1].value, isFalse);

    app.repository.debugSetPrivateSettings(
      _dev,
      const ServerPrivateSettings(enabled: true, friendsOnly: true),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(_switches(tester)[1].value, isTrue);
    await app.dispose(tester);
  });
}
