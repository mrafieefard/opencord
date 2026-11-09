// The plugin page where people see it: inside the settings dialog, whose
// body scrolls and so gives its pages an unbounded height.
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/plugins_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/plugins/plugin_page.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';

import '../../support/app.dart';

Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 8; i++) {
    await tester.pump(const Duration(milliseconds: 100));
  }
}

const _server = 'opencord.example:7710';

Future<MockApp> _openPlugin(WidgetTester tester) async {
  final app = await MockApp.pump(tester);
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('Opencord Dev'),
    ),
  );
  await _settle(tester);
  await tester.tap(find.text('Server settings'));
  await _settle(tester);
  await tester.tap(
    find
        .descendant(
          of: find.byType(SettingsDialog),
          matching: find.text('Plugins'),
        )
        .first,
  );
  await _settle(tester);
  final List<Plugin> plugins = app.read(pluginsProvider(_server)).value!;
  await tester.tap(find.text(plugins.first.name));
  await _settle(tester);
  return app;
}

void main() {
  testWidgets('a_plugin_opened_from_the_settings_dialog_shows_its_page', (
    tester,
  ) async {
    final app = await _openPlugin(tester);

    expect(tester.takeException(), isNull);
    expect(find.byType(PluginPage), findsOneWidget);
    expect(find.text('DETAILS'), findsOneWidget);
    expect(find.byTooltip('Back to plugins'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('every_tab_of_a_plugin_shows_in_the_settings_dialog', (
    tester,
  ) async {
    final app = await _openPlugin(tester);

    for (final tab in ['Permissions', 'Settings', 'Log', 'Overview']) {
      await tester.tap(
        find.descendant(of: find.byType(PluginPage), matching: find.text(tab)),
      );
      await _settle(tester);
      expect(tester.takeException(), isNull, reason: tab);
    }
    await app.dispose(tester);
  });
}
