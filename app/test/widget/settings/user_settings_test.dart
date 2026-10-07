import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/app_info.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/window/window_startup.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

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

Finder _inPage(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

void main() {
  testWidgets('Ctrl+, opens settings on My profile', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.comma);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.byType(SettingsDialog), findsOneWidget);
    expect(_inPage(find.text('Display name')), findsOneWidget);
    await app.dispose(tester);
  });

  for (final (platform, tray) in [
    (TargetPlatform.linux, true),
    (TargetPlatform.windows, false),
    (TargetPlatform.macOS, false),
  ]) {
    testWidgets(
      'tray switches show only where there is a tray (${platform.name})',
      (tester) async {
        final app = await MockApp.pump(tester, platform: platform);
        await _openSettings(tester);
        await _page(tester, 'Windows & behavior');

        final rows = [
          _inPage(find.textContaining('minimizes to the tray')),
          _inPage(find.textContaining('menu bar')),
          _inPage(find.text('Start minimized')),
        ];
        expect(
          rows.where((row) => row.evaluate().isNotEmpty),
          hasLength(tray ? 2 : 0),
        );
        expect(_inPage(find.text('Launch at login')), findsOneWidget);
        await app.dispose(tester);
      },
    );
  }

  testWidgets('the theme and density apply at once', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'Appearance');

    await tester.tap(_inPage(find.text('Light')));
    await tester.pump();
    await tester.tap(_inPage(find.text('Compact')));
    await tester.pump();

    final settings = app.read(appSettingsProvider);
    expect(settings.theme, ThemePreference.light);
    expect(settings.density, MessageDensity.compact);
    await app.dispose(tester);
  });

  testWidgets('a window frame choice is saved for the next start', (
    tester,
  ) async {
    final saved = <WindowFramePreference>[];
    final app = await MockApp.pump(
      tester,
      platform: TargetPlatform.linux,
      overrides: [
        frameChoiceSaverProvider.overrideWithValue((frame) async {
          saved.add(frame);
        }),
      ],
    );
    await _openSettings(tester);
    await _page(tester, 'Appearance');
    await tester.scrollUntilVisible(
      _inPage(find.text('System').last),
      200,
      scrollable: _inPage(find.byType(Scrollable)).last,
    );

    await tester.tap(_inPage(find.text('Your desktop draws the frame')));
    await tester.pump();

    expect(
      app.read(appSettingsProvider).windowFrame,
      WindowFramePreference.system,
    );
    expect(saved, [WindowFramePreference.system]);
    await app.dispose(tester);
  });

  testWidgets('the font size slider scales text', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'Appearance');
    final slider = _inPage(find.byType(Slider)).first;
    await tester.scrollUntilVisible(
      slider,
      200,
      scrollable: _inPage(find.byType(Scrollable)).last,
    );

    await tester.drag(slider, const Offset(400, 0));
    await tester.pump();

    expect(app.read(appSettingsProvider).fontSize, AppSettings.maxFontSize);
    await app.dispose(tester);
  });

  testWidgets('a new display name is saved', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);

    await tester.enterText(_inPage(find.byType(TextField)), 'Alex R.');
    await tester.pump();
    await tester.tap(_inPage(find.widgetWithText(OcButton, 'Save')));
    await _pumpFor(tester, const Duration(milliseconds: 500));

    expect(app.repository.identity?.displayName, 'Alex R.');
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('the identity backup goes out and comes back in', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    final before = app.repository.identity!.fingerprint;
    await _openSettings(tester);
    await _page(tester, 'Identity & keys');

    await tester.tap(_inPage(find.text('Export identity backup')));
    await _pumpFor(tester, const Duration(milliseconds: 500));
    final backup = tester
        .widget<SelectableText>(
          find.descendant(
            of: find.byType(OcDialog),
            matching: find.byType(SelectableText),
          ),
        )
        .data!;
    expect(backup, startsWith('opencord-identity-v1:'));
    await tester.tap(find.widgetWithText(OcButton, 'Done'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    await tester.tap(_inPage(find.text('Import identity')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Continue'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    final field = find.descendant(
      of: find.byType(OcDialog),
      matching: find.byType(TextField),
    );
    await tester.enterText(field, 'not a backup');
    await tester.tap(find.widgetWithText(OcButton, 'Import'));
    await _pumpFor(tester, const Duration(milliseconds: 500));
    expect(
      find.text('That is not an Opencord identity backup.'),
      findsOneWidget,
    );

    final other = 'opencord-identity-v1:${'ab' * 32}';
    await tester.enterText(field, other);
    await tester.tap(find.widgetWithText(OcButton, 'Import'));
    await _pumpFor(tester, const Duration(milliseconds: 500));

    expect(app.repository.identity!.fingerprint, isNot(before));
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('keybinds show this platform’s keys', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.macOS);
    await _openSettings(tester);
    await _page(tester, 'Keybinds');

    final hints = [
      for (final hint in tester.widgetList<KeyHint>(
        _inPage(find.byType(KeyHint)),
      ))
        shortcutLabel(hint.shortcut, TargetPlatform.macOS),
    ];
    expect(hints, contains('⌘K'));
    await app.dispose(tester);
  });

  testWidgets('turning notifications off disables "only for mentions"', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'Notifications');

    await tester.tap(_inPage(find.text('Desktop notifications')));
    await tester.pump();
    await tester.tap(_inPage(find.text('Only for mentions')));
    await tester.pump();

    final settings = app.read(appSettingsProvider);
    expect(settings.desktopNotifications, isFalse);
    expect(settings.mentionsOnly, isFalse);
    await app.dispose(tester);
  });

  testWidgets('a trusted server can be forgotten', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'Trusted servers');

    await tester.tap(_inPage(find.widgetWithText(OcButton, 'Forget')).last);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Forget server'));
    await _pumpFor(tester, const Duration(milliseconds: 500));

    expect(app.read(serverListProvider), hasLength(2));
    await app.dispose(tester);
  });

  testWidgets('About shows the versions', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'About');

    expect(_inPage(find.text(appVersion)), findsOneWidget);
    expect(
      _inPage(find.text('App: MPL-2.0 · Server: AGPL-3.0')),
      findsOneWidget,
    );
    await app.dispose(tester);
  });
}
