import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/core/app_info.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/user_panel.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/features/desktop/installation.dart';
import 'package:opencord/features/desktop/tray_menu.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/settings/user/settings_draft.dart';
import 'package:opencord/features/voice/input_level_meter.dart';
import 'package:opencord/features/voice/mic_test.dart';
import 'package:opencord/features/voice/mic_test_panel.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

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

/// Client settings wait for the save bar, as in Discord.
Future<void> _saveChanges(WidgetTester tester) async {
  await _pumpFor(tester, const Duration(milliseconds: 300));
  await tester.tap(
    find.descendant(
      of: find.byType(SaveBar),
      matching: find.widgetWithText(OcButton, 'Save changes'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

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
    (TargetPlatform.windows, true),
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

  testWidgets('Appearance changes wait for the save bar', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'Appearance');
    final before = app.read(appSettingsProvider);

    await tester.tap(_inPage(find.text('Light')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(appSettingsProvider), before, reason: 'not yet');
    expect(find.text('Careful — you have unsaved changes!'), findsOneWidget);

    await tester.tap(find.widgetWithText(OcButton, 'Discard'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(appSettingsProvider), before);
    expect(find.text('Careful — you have unsaved changes!'), findsNothing);

    await tester.tap(_inPage(find.text('Light')));
    await _saveChanges(tester);
    final settings = app.read(appSettingsProvider);
    expect(settings.theme, ThemePreference.light);
    expect(find.text('Careful — you have unsaved changes!'), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('appearance_has_no_density', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'Appearance');

    expect(_inPage(find.text('Message density')), findsNothing);
    expect(_inPage(find.text('Comfortable')), findsNothing);
    expect(_inPage(find.text('Compact')), findsNothing);
    expect(_inPage(find.text('Font size')), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('Appearance offers no window frame choice', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'Appearance');

    expect(_inPage(find.text('Window frame')), findsNothing);
    expect(_inPage(find.text('Your desktop draws the frame')), findsNothing);
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
    await _saveChanges(tester);

    expect(app.read(appSettingsProvider).fontSize, AppSettings.maxFontSize);
    await app.dispose(tester);
  });

  testWidgets('a new display name is saved', (tester) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);

    await tester.enterText(_inPage(find.byType(TextField)), 'Alex R.');
    await tester.pump();
    await _saveChanges(tester);
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
    await _saveChanges(tester);

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

  testWidgets('a copy not installed by the launcher says it does not update', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _openSettings(tester);
    await _page(tester, 'About');

    expect(_inPage(find.text("This copy doesn't update itself")), findsOne);
    expect(_inPage(find.text('Beta')), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('switching tracks asks, saves the track and restarts', (
    tester,
  ) async {
    final installation = _FakeInstallation(ReleaseTrack.canary);
    final app = await MockApp.pump(
      tester,
      platform: TargetPlatform.linux,
      overrides: [installationProvider.overrideWithValue(installation)],
    );
    // The track is read once, as the app starts.
    app.read(releaseTrackProvider);
    await tester.pump();
    await _openSettings(tester);
    await _page(tester, 'About');
    expect(_inPage(find.text('0.0.2-canary.1')), findsOneWidget);

    // Cancelled: nothing changes.
    final stable = _inPage(find.text('Stable'));
    await tester.ensureVisible(stable);
    await tester.tap(stable);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text('Switch to Stable?'), findsOneWidget);
    expect(
      find.textContaining('It may be older than the one you have now'),
      findsOneWidget,
    );
    await tester.tap(find.widgetWithText(OcButton, 'Cancel'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(installation.written, isEmpty);
    expect(installation.restarted, isFalse);
    expect(app.read(releaseTrackProvider), ReleaseTrack.canary);

    // Confirmed: the track is saved, then the app restarts on it.
    await tester.tap(stable);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Restart').last);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(installation.written, [ReleaseTrack.stable]);
    expect(installation.restarted, isTrue);
    expect(app.read(releaseTrackProvider), ReleaseTrack.stable);
    await app.dispose(tester);
  });

  testWidgets('noise suppression can be switched off and on again', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    // Noise suppression's Off; Attenuation further down says Off too.
    final off = _inPage(find.text('Off')).first;
    await tester.ensureVisible(off);
    await tester.tap(off);
    await tester.pump();
    await _saveChanges(tester);
    final switchedOff = app.repository.audio?.noiseSuppression;
    await tester.tap(_inPage(find.text('Standard')));
    await tester.pump();
    await _saveChanges(tester);

    expect(switchedOff, NoiseSuppression.off);
    expect(app.repository.audio?.noiseSuppression, NoiseSuppression.standard);
    expect(
      app.container.read(audioSettingsProvider).noiseSuppression,
      NoiseSuppression.standard,
      reason: 'kept with the other audio choices',
    );
    await app.dispose(tester);
  });

  testWidgets('voice settings show the microphone level under its volume', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);

    await _page(tester, 'Voice & audio');
    final meters = _inPage(find.byType(InputLevelMeter)).evaluate().length;
    final on = app.repository.levelMeterOn;
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(meters, 1);
    expect(on, isTrue);
    expect(
      app.repository.levelMeterOn,
      isFalse,
      reason: 'closed with the page',
    );
    await app.dispose(tester);
  });

  testWidgets('attenuation lowers other apps by the share chosen, for whom', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    await tester.ensureVisible(_inPage(find.text('When I speak')));
    await tester.pump();
    expect(_inPage(find.text('Off')), findsWidgets, reason: 'off at first');
    await tester.tap(_inPage(find.text('When I speak')));
    await tester.pump();
    final lowerBy = find.descendant(
      of: find.ancestor(
        of: find.text('Lower other apps by'),
        matching: find.byType(SettingsSliderRow),
      ),
      matching: find.byType(Slider),
    );
    await tester.drag(lowerBy, const Offset(400, 0));
    await tester.pump();
    await _saveChanges(tester);

    final sent = app.repository.audio!;
    expect(sent.attenuationPercent, greaterThan(0));
    expect(sent.attenuateWhenISpeak, isTrue);
    expect(sent.attenuateWhenOthersSpeak, isTrue);
    await app.dispose(tester);
  });

  testWidgets('a presence chosen in settings waits for the save bar', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    // Settings open on My profile.
    await _openSettings(tester);

    final idle = _inPage(find.text('Idle'));
    await tester.ensureVisible(idle);
    await tester.tap(idle);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(app.read(selfPresenceProvider), SelfPresence.online);

    await _saveChanges(tester);
    expect(app.read(selfPresenceProvider), SelfPresence.idle);
    await app.dispose(tester);
  });

  testWidgets('a_saved_invisible_opens_as_online', (tester) async {
    final store = MemoryKeyValueStore()..write(selfPresenceKey, 'invisible');
    final app = await MockApp.pump(tester, store: store);

    expect(app.read(selfPresenceProvider), SelfPresence.online);
    expect(app.repository.lastPresence, SelfPresence.online);
    await app.dispose(tester);
  });

  testWidgets('the_choices_are_three', (tester) async {
    final app = await MockApp.pump(tester);

    // 1. Settings cards
    await _openSettings(tester);
    final cards = find.byType(SettingsChoiceCards<SelfPresence>);
    expect(cards, findsOneWidget);
    final cardWidget = tester.widget<SettingsChoiceCards<SelfPresence>>(cards);
    expect(cardWidget.options.map((o) => o.value.label).toList(), [
      'Online',
      'Idle',
      'Do not disturb',
    ]);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // 2. User panel menu
    await tester.tap(find.bySemanticsLabel(RegExp('Change status')));
    await tester.pumpAndSettle();
    final panel = find.byType(OcMenuPanel);
    expect(panel, findsOneWidget);
    expect(
      tester
          .widget<OcMenuPanel>(panel)
          .entries
          .whereType<OcMenuItem>()
          .map((item) => item.label)
          .toList(),
      ['Online', 'Idle', 'Do not disturb'],
    );
    // Three rows still open 8 above the panel, as four did.
    expect(
      tester.getRect(find.byType(UserPanel)).top - tester.getRect(panel).bottom,
      closeTo(8, 1),
    );
    // Dismiss menu by tapping outside
    await tester.tapAt(const Offset(500, 500));
    await tester.pumpAndSettle();

    // 3. Tray menu
    final tray = trayMenu(const TrayState());
    final status = tray.firstWhere((item) => item.label == 'Status');
    expect(status.children.map((item) => item.label).toList(), [
      'Online',
      'Idle',
      'Do not disturb',
    ]);

    await app.dispose(tester);
  });

  testWidgets('(Task 11) the_section_is_after_noise_suppression', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    final noiseSuppression = _inPage(
      find.byWidgetPredicate(
        (w) => w is SettingsSection && w.title == 'Noise suppression',
      ),
    );
    final micTest = _inPage(
      find.byWidgetPredicate(
        (w) => w is SettingsSection && w.title == 'Microphone test',
      ),
    );
    final volume = _inPage(
      find.byWidgetPredicate(
        (w) => w is SettingsSection && w.title == 'Volume',
      ),
    );

    expect(micTest, findsOneWidget);
    expect(
      find.descendant(of: micTest, matching: find.byType(MicTestPanel)),
      findsOneWidget,
    );

    final noiseTop = tester.getTopLeft(noiseSuppression).dy;
    final micTestTop = tester.getTopLeft(micTest).dy;
    final volumeTop = tester.getTopLeft(volume).dy;

    expect(noiseTop, lessThan(micTestTop));
    expect(micTestTop, lessThan(volumeTop));

    await app.dispose(tester);
  });

  testWidgets('(Task 11) out_of_a_call_it_tests_without_deafening', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    const expectedNote =
        'Hear what Opencord sends. Use headphones, or you may hear an echo.';
    expect(_inPage(find.text(expectedNote)), findsOneWidget);

    final testBtn = _inPage(find.widgetWithText(OcButton, 'Test'));
    await tester.ensureVisible(testBtn);
    await tester.tap(testBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.container.read(micTestProvider), isTrue);
    expect(app.repository.micTest, isTrue);
    expect(app.container.read(voiceSessionProvider).deafened, isFalse);
    expect(_inPage(find.text('Stop test')), findsOneWidget);
    expect(_inPage(find.byType(InputLevelMeter)), findsNWidgets(2));

    final stopBtn = _inPage(find.widgetWithText(OcButton, 'Stop test'));
    await tester.tap(stopBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.container.read(micTestProvider), isFalse);
    expect(app.repository.micTest, isFalse);
    expect(_inPage(find.text('Test')), findsOneWidget);

    await app.dispose(tester);
  });

  testWidgets('(Task 11) in_a_call_it_deafens_and_restores', (tester) async {
    final app = await MockApp.pump(tester);
    final server = app.repository.servers.first.key;
    final channel = app.container
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((c) => c.kind == ChannelKind.voice)
        .id;
    await app.read(voiceSessionProvider.notifier).join(server, channel);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.container.read(voiceSessionProvider).connected, isTrue);
    expect(app.container.read(voiceSessionProvider).deafened, isFalse);

    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    const expectedNote =
        "Hear what Opencord sends. You're deafened until you stop. Use headphones, or you may hear an echo.";
    expect(_inPage(find.text(expectedNote)), findsOneWidget);

    final testBtn = _inPage(find.widgetWithText(OcButton, 'Test'));
    await tester.ensureVisible(testBtn);
    await tester.tap(testBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.container.read(micTestProvider), isTrue);
    expect(app.repository.micTest, isTrue);
    expect(app.container.read(voiceSessionProvider).deafened, isTrue);

    final stopBtn = _inPage(find.widgetWithText(OcButton, 'Stop test'));
    await tester.tap(stopBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.container.read(micTestProvider), isFalse);
    expect(app.repository.micTest, isFalse);
    expect(app.container.read(voiceSessionProvider).deafened, isFalse);

    await app.dispose(tester);
  });

  testWidgets('(Task 11) leaving_the_page_ends_it', (tester) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    final testBtn = _inPage(find.widgetWithText(OcButton, 'Test'));
    await tester.ensureVisible(testBtn);
    await tester.tap(testBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.container.read(micTestProvider), isTrue);
    expect(app.repository.micTest, isTrue);

    await _page(tester, 'Appearance');
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.container.read(micTestProvider), isFalse);
    expect(app.repository.micTest, isFalse);

    await app.dispose(tester);
  });

  testWidgets('(Task 11) closing_settings_ends_it', (tester) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    final testBtn = _inPage(find.widgetWithText(OcButton, 'Test'));
    await tester.ensureVisible(testBtn);
    await tester.tap(testBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.container.read(micTestProvider), isTrue);
    expect(app.repository.micTest, isTrue);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(find.byType(SettingsDialog), findsNothing);
    expect(app.container.read(micTestProvider), isFalse);
    expect(app.repository.micTest, isFalse);

    await app.dispose(tester);
  });

  testWidgets('(Task 11) it_hears_the_draft', (tester) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    final savedNoise =
        app.container.read(audioSettingsProvider).noiseSuppression ??
        NoiseSuppression.standard;
    final savedVolume = app.container.read(audioSettingsProvider).inputVolume;

    final testBtn = _inPage(find.widgetWithText(OcButton, 'Test'));
    await tester.ensureVisible(testBtn);
    await tester.tap(testBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.repository.micTest, isTrue);

    // Change noise suppression to Off in draft
    final offCard = _inPage(find.text('Off')).first;
    await tester.ensureVisible(offCard);
    await tester.tap(offCard);
    await _pumpFor(tester, const Duration(milliseconds: 100));

    // Change input volume in draft
    final volumeSection = _inPage(
      find.byWidgetPredicate(
        (w) => w is SettingsSection && w.title == 'Volume',
      ),
    );
    final slider = find
        .descendant(of: volumeSection, matching: find.byType(Slider))
        .first;
    await tester.ensureVisible(slider);
    await tester.drag(slider, const Offset(-100, 0));
    await _pumpFor(tester, const Duration(milliseconds: 100));

    // The mock follows the draft unsaved
    expect(
      app.container.read(clientSettingsDraftProvider).audio?.noiseSuppression,
      NoiseSuppression.off,
    );
    expect(
      app.container.read(audioSettingsProvider).noiseSuppression,
      savedNoise,
    );
    expect(app.repository.audio?.noiseSuppression, NoiseSuppression.off);
    expect(app.repository.audio?.inputVolume, isNot(savedVolume));

    // Stop: it goes back to the saved settings
    final stopBtn = _inPage(find.widgetWithText(OcButton, 'Stop test'));
    await tester.ensureVisible(stopBtn);
    await tester.tap(stopBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.repository.audio?.noiseSuppression, savedNoise);
    expect(app.repository.audio?.inputVolume, savedVolume);

    await app.dispose(tester);
  });

  testWidgets('(Task 11) saving_during_a_test_keeps_the_saved', (tester) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    final testBtn = _inPage(find.widgetWithText(OcButton, 'Test'));
    await tester.ensureVisible(testBtn);
    await tester.tap(testBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // Change noise suppression to Off in draft
    final offCard = _inPage(find.text('Off')).first;
    await tester.ensureVisible(offCard);
    await tester.tap(offCard);
    await _pumpFor(tester, const Duration(milliseconds: 100));
    expect(app.repository.audio?.noiseSuppression, NoiseSuppression.off);

    // Save changes while test is still running
    await _saveChanges(tester);
    expect(
      app.container.read(audioSettingsProvider).noiseSuppression,
      NoiseSuppression.off,
    );
    expect(app.repository.audio?.noiseSuppression, NoiseSuppression.off);

    // Stop test: no flicker back to old settings, keeps saved
    final stopBtn = _inPage(find.widgetWithText(OcButton, 'Stop test'));
    await tester.ensureVisible(stopBtn);
    await tester.tap(stopBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.repository.audio?.noiseSuppression, NoiseSuppression.off);
    expect(
      app.container.read(audioSettingsProvider).noiseSuppression,
      NoiseSuppression.off,
    );

    await app.dispose(tester);
  });

  testWidgets('(Task 11) it_always_starts_off', (tester) async {
    final app = await MockApp.pump(tester);
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');

    final testBtn = _inPage(find.widgetWithText(OcButton, 'Test'));
    await tester.ensureVisible(testBtn);
    await tester.tap(testBtn);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.container.read(micTestProvider), isTrue);

    // Close settings
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.container.read(micTestProvider), isFalse);

    // Reopen settings
    await _openSettings(tester);
    await _page(tester, 'Voice & audio');
    expect(app.container.read(micTestProvider), isFalse);
    expect(app.repository.micTest, isFalse);
    expect(_inPage(find.text('Test')), findsOneWidget);
    expect(_inPage(find.text('Stop test')), findsNothing);

    await app.dispose(tester);
  });
}

class _FakeInstallation extends Installation {
  _FakeInstallation(this.track)
    : super(
        root: Directory('/nowhere'),
        version: '0.0.2-canary.1',
        launcher: '/nowhere/opencord',
      );

  ReleaseTrack track;
  final written = <ReleaseTrack>[];

  @override
  Future<ReleaseTrack> readTrack({
    ReleaseTrack fallback = ReleaseTrack.stable,
  }) async => track;

  @override
  Future<void> writeTrack(ReleaseTrack track) async {
    written.add(track);
    this.track = track;
  }

  var restarted = false;

  @override
  Future<void> restart() async => restarted = true;
}
