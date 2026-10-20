import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

/// The settings' save bar button labelled [label].
Finder saveBarButton(String label) => find.descendant(
  of: find.byType(SaveBar),
  matching: find.widgetWithText(OcButton, label),
);

/// Presses Save changes on the settings' save bar once it has slid in,
/// and waits for the changes to arrive.
Future<void> saveSettings(WidgetTester tester) async {
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(saveBarButton('Save changes'));
  await _pumpFor(tester, const Duration(milliseconds: 800));
}
