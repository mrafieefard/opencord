import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/members/member_actions.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _in(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

Future<void> _openRoles(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('Opencord Dev'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
  await tester.tap(find.text('Server settings'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(_in(find.text('Roles')).first);
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

void main() {
  testWidgets('plugin_roles_are_locked', (tester) async {
    final app = await MockApp.pump(tester);
    await _openRoles(tester);

    // The Welcome Bot role tile shows the lock icon.
    final roleItem = _in(find.text('Welcome Bot')).first;
    expect(roleItem, findsOneWidget);

    // Tap the Welcome Bot role to open its editor.
    await tester.tap(roleItem);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // Notice line: "Belongs to the plugin Welcome Bot. Its permissions follow the plugin's."
    expect(
      _in(
        find.text(
          "Belongs to the plugin Welcome Bot. Its permissions follow the plugin's.",
        ),
      ),
      findsOneWidget,
    );

    // Lock icon is displayed in editor header.
    expect(_in(find.byIcon(OcIcons.lock)), findsWidgets);

    // Name field is disabled.
    final textField = tester.widget<TextField>(
      _in(find.byType(TextField)).first,
    );
    expect(textField.enabled, isFalse);

    // Delete role button is not present.
    expect(_in(find.text('Delete role')), findsNothing);

    // Permission switches are all off and disabled.
    final switches = tester.widgetList<OcSwitch>(_in(find.byType(OcSwitch)));
    expect(switches, isNotEmpty);
    for (final s in switches) {
      if (s.semanticLabel == 'Not set') continue;
      expect(s.value, isFalse);
      expect(s.onChanged, isNull);
    }

    await app.dispose(tester);
  });

  testWidgets('plugin_roles_are_not_offered_to_members', (tester) async {
    final app = await MockApp.pump(tester);

    final element = tester.element(find.byType(DesktopShell));
    final actions = MemberActions(
      context: element,
      ref: tester.element(find.byType(DesktopShell)) as dynamic,
      serverKey: _dev,
    );

    final manageable = actions.manageableRoles();
    final names = manageable.map((r) => r.name).toList();
    expect(names, isNot(contains('Welcome Bot')));
    for (final role in manageable) {
      expect(role.pluginId, isNull);
    }

    await app.dispose(tester);
  });
}
