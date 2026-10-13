import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/settings/keybinds.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/settings/user/keybind_recorder.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../test/support/app.dart';

// Windows plan W2, step 6: what the app's key recorder (shared Dart) makes of
// the keys Windows reports oddly. The recorder is driven here with keys really
// pressed, by app/tool/windows/hold_key.ps1, because the question is exactly
// what Windows and Flutter hand it — `sendKeyDownEvent`, which the shared
// widget test uses, would answer a different question.
//
// The app runs on the mock repository: nothing here needs a server, and the
// recorder is the whole subject. One key per run, named by the phase:
//   super      the left Windows key — also opens the Start menu, see §7
//   print      Print Screen
//   right-alt  the right Alt, which on a layout with AltGr is AltGr
//
// It measures and reports; app/tool/windows/check_key_recorder.ps1 decides.
const _phase = String.fromEnvironment('OPENCORD_E2E_PHASE');

/// The virtual-key code to press.
const _virtualKey = int.fromEnvironment('OPENCORD_E2E_VIRTUAL_KEY');

/// Whether it needs the extended-key flag (the right Alt does).
const _extended = bool.fromEnvironment('OPENCORD_E2E_EXTENDED');

/// PowerShell 7, which runs the injector.
const _pwsh = String.fromEnvironment('OPENCORD_E2E_PWSH', defaultValue: 'pwsh');

/// The repository root, to find the injector script.
const _repo = String.fromEnvironment('OPENCORD_E2E_REPO');

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
    await Future<void>.delayed(step);
  }
}

Finder _inPage(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

/// The text the recorder is showing.
String _shown() => find
    .descendant(of: find.byType(KeybindRecorder), matching: find.byType(Text))
    .evaluate()
    .map((element) => (element.widget as Text).data)
    .nonNulls
    .join(' / ');

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the key recorder on $_phase', skip: _phase.isEmpty, (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.windows);
    var recorded = '';
    var shown = '';
    try {
      await tester.tap(
        find.byWidgetPredicate(
          (widget) =>
              widget is OcIconButton && widget.tooltip == 'User settings',
        ),
      );
      await _pumpFor(tester, const Duration(milliseconds: 400));
      await tester.tap(_inPage(find.text('Keybinds')));
      await _pumpFor(tester, const Duration(milliseconds: 400));
      // A new keybind records at once, like Discord's.
      await tester.tap(_inPage(find.text('Add a keybind')));
      await _pumpFor(tester, const Duration(milliseconds: 400));

      final injector = await Process.start(_pwsh, [
        '-NoLogo',
        '-NoProfile',
        '-File',
        '$_repo\\app\\tool\\windows\\hold_key.ps1',
        '-VirtualKey',
        '$_virtualKey',
        '-Seconds',
        '0.3',
        if (_extended) '-Extended',
      ]);
      await _pumpFor(tester, const Duration(seconds: 3));
      await injector.exitCode;
      await _pumpFor(tester, const Duration(milliseconds: 500));

      shown = _shown();
      final rows = app.read(keybindsProvider).list;
      recorded = rows.isEmpty ? '<no row>' : rows.last.keys;
    } finally {
      // ignore: avoid_print
      print(
        'RECORDER: phase=$_phase key=0x${_virtualKey.toRadixString(16)} '
        'recorded="$recorded" shown="$shown"',
      );
      await app.dispose(tester);
    }
  });
}
