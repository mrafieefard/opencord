import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

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

int _roleId(MockApp app, String name) => app
    .read(serverProvider(_dev))
    .data!
    .roles
    .values
    .firstWhere((role) => role.name == name)
    .id;

void main() {
  testWidgets('roles run highest first with @everyone at the bottom', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openRoles(tester);

    final maintainer = tester.getTopLeft(_in(find.text('Maintainer')).first);
    final contributor = tester.getTopLeft(_in(find.text('Contributor')).first);
    final everyone = tester.getTopLeft(_in(find.text('@everyone')).first);
    expect(maintainer.dy, lessThan(contributor.dy));
    expect(contributor.dy, lessThan(everyone.dy));
    await app.dispose(tester);
  });

  testWidgets('a new role is created and selected', (tester) async {
    final app = await MockApp.pump(tester);
    await _openRoles(tester);

    await tester.tap(_in(find.widgetWithText(OcButton, 'Create role')));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(
      app.read(serverProvider(_dev)).data!.roles.values.map((r) => r.name),
      contains('New role'),
    );
    final field = tester.widget<TextField>(_in(find.byType(TextField)).first);
    expect(field.controller!.text, 'New role');
    await app.dispose(tester);
  });

  testWidgets('a permission switch changes the role at once', (tester) async {
    final app = await MockApp.pump(tester);
    await _openRoles(tester);
    await tester.tap(_in(find.text('Contributor')).first);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    final row = _in(find.text('Manage messages'));
    await tester.scrollUntilVisible(
      row,
      200,
      scrollable: _in(find.byType(Scrollable)).last,
    );

    await tester.tap(row);
    await _pumpFor(tester, const Duration(milliseconds: 600));

    final role = app
        .read(serverProvider(_dev))
        .data!
        .roles[_roleId(app, 'Contributor')]!;
    expect(role.permissions.has(Permissions.manageMessages), isTrue);
    await app.dispose(tester);
  });

  testWidgets('a renamed role is saved with Save', (tester) async {
    final app = await MockApp.pump(tester);
    await _openRoles(tester);
    await tester.tap(_in(find.text('Contributor')).first);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    await tester.enterText(_in(find.byType(TextField)).first, 'Helpers');
    await tester.pump();
    await tester.tap(_in(find.widgetWithText(OcButton, 'Save')));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(
      app.read(serverProvider(_dev)).data!.roles.values.map((r) => r.name),
      contains('Helpers'),
    );
    await app.dispose(tester);
  });

  testWidgets('closing with an unsaved name asks first', (tester) async {
    final app = await MockApp.pump(tester);
    await _openRoles(tester);
    await tester.tap(_in(find.text('Contributor')).first);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.enterText(_in(find.byType(TextField)).first, 'Unsaved');
    await tester.pump();

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text('Discard your changes?'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Cancel'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.byType(SettingsDialog), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Discard'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.byType(SettingsDialog), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('dragging a role reorders it', (tester) async {
    final app = await MockApp.pump(tester);
    await _openRoles(tester);
    final contributor = _in(find.text('Contributor')).first;
    final maintainer = _in(find.text('Maintainer')).first;

    final start = tester.getCenter(contributor);
    final end = tester.getCenter(maintainer) - const Offset(0, 24);
    final gesture = await tester.startGesture(start);
    await tester.pump(const Duration(milliseconds: 100));
    // Reorderable lists follow the pointer in small steps.
    for (var step = 1; step <= 10; step++) {
      await gesture.moveTo(Offset.lerp(start, end, step / 10)!);
      await tester.pump(const Duration(milliseconds: 30));
    }
    await gesture.up();
    await _pumpFor(tester, const Duration(milliseconds: 800));

    final roles = app.read(serverProvider(_dev)).data!.roles;
    expect(
      roles[_roleId(app, 'Contributor')]!.position,
      greaterThan(roles[_roleId(app, 'Maintainer')]!.position),
    );
    await app.dispose(tester);
  });
}
