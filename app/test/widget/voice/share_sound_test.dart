import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/features/voice/voice_view.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/settings.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _tomas = 1006;

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _control(String label) => find.descendant(
  of: find.byType(VoiceControlBar),
  matching: find.bySemanticsLabel(label),
);

Finder _inMenu(Finder finder) =>
    find.descendant(of: find.byType(OcMenuPanel), matching: finder);

Finder _inDialog(Finder finder) =>
    find.descendant(of: find.byType(OcDialog), matching: finder);

Future<void> _joinGeneral(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('General'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

Future<void> _openShareDialog(WidgetTester tester) async {
  await tester.tap(_control('Share your screen'));
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

SettingsSwitchRow _soundSwitch(WidgetTester tester) =>
    tester.widget(find.widgetWithText(SettingsSwitchRow, 'Share sound'));

void main() {
  testWidgets('the sound goes by default, and its switch is remembered', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    await _joinGeneral(tester);
    await _openShareDialog(tester);

    expect(_soundSwitch(tester).value, isTrue);
    expect(_soundSwitch(tester).onChanged, isNotNull);
    await tester.tap(find.widgetWithText(OcButton, 'Go live'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(ownScreenShareProvider)?.audio, isTrue);

    // Turned off from the share menu, and back on.
    await tester.tap(_control('Screen share options'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(_inMenu(find.text('Share sound')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(ownScreenShareProvider)?.audio, isFalse);

    await tester.tap(_control('Screen share options'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(_inMenu(find.text('Stop sharing')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await _openShareDialog(tester);
    await tester.tap(find.widgetWithText(SettingsSwitchRow, 'Share sound'));
    await tester.pump();
    await tester.tap(find.widgetWithText(OcButton, 'Go live'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(ownScreenShareProvider)?.audio, isFalse);
    expect(app.read(shareSoundProvider), isFalse, reason: 'remembered');
    await app.dispose(tester);
  });

  testWidgets('without PipeWire the switch is off and says why', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    app.repository.screenAudio = (
      supported: false,
      reason: 'Sharing sound needs PipeWire',
    );
    await _joinGeneral(tester);
    await _openShareDialog(tester);

    expect(_soundSwitch(tester).value, isFalse);
    expect(_soundSwitch(tester).onChanged, isNull);
    expect(find.text('Sharing sound needs PipeWire'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Go live'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(ownScreenShareProvider)?.audio, isFalse);
    await app.dispose(tester);
  });

  testWidgets('a window asks which app its sound is from, none by default', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
    app.repository.debugShareSource = StreamSource.window;
    await _joinGeneral(tester);
    await _openShareDialog(tester);
    await tester.tap(find.widgetWithText(OcButton, 'Go live'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // Two apps playing: nothing is shared until one is chosen.
    expect(_inDialog(find.text('Sound from')), findsOneWidget);
    expect(_inDialog(find.text('Firefox')), findsOneWidget);
    expect(_inDialog(find.text('Spotify')), findsOneWidget);
    expect(app.read(ownScreenShareProvider)?.audioFrom, isNull);
    await tester.tap(_inDialog(find.text('Spotify')));
    await tester.pump();
    expect(app.read(ownScreenShareProvider)?.audioFrom, 'Spotify');
    await tester.tap(find.widgetWithText(OcButton, 'Done'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // Shared again: the app chosen before is chosen again.
    await tester.tap(_control('Screen share options'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(_inMenu(find.text('Sound from Spotify…')), findsOneWidget);
    await tester.tap(_inMenu(find.text('Stop sharing')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await _openShareDialog(tester);
    await tester.tap(find.widgetWithText(OcButton, 'Go live'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(ownScreenShareProvider)?.audioFrom, 'Spotify');
    await app.dispose(tester);
  });

  testWidgets("a window whose app the system names shares it without asking", (
    tester,
  ) async {
    final app = await MockApp.pump(tester, platform: TargetPlatform.windows);
    app.repository
      ..debugShareSource = StreamSource.window
      ..debugShareSound = 'Notes';
    await _joinGeneral(tester);
    await _openShareDialog(tester);
    await tester.tap(find.widgetWithText(OcButton, 'Go live'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(_inDialog(find.text('Sound from')), findsNothing);
    expect(app.read(ownScreenShareProvider)?.audioFrom, 'Notes');
    // Another app can still be chosen.
    await tester.tap(_control('Screen share options'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(_inMenu(find.text('Sound from Notes…')), findsOneWidget);
    await app.dispose(tester);
  });

  test('the app a window sound starts from', () {
    const firefox = (name: 'Firefox', icon: null);
    const spotify = (name: 'Spotify', icon: null);

    expect(preselectedSoundFrom([firefox, spotify], const []), isNull);
    expect(preselectedSoundFrom([firefox], const []), 'Firefox');
    expect(
      preselectedSoundFrom([firefox, spotify], ['mpv', 'Spotify']),
      'Spotify',
    );
    expect(preselectedSoundFrom([], ['Spotify']), isNull);
  });

  testWidgets("a watched stream's sound has its own volume and mute", (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: const Size(1440, 1000));
    await _joinGeneral(tester);
    await tester.tap(
      find.descendant(
        of: find.byType(VoiceView),
        matching: find.widgetWithText(OcButton, 'Watch'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    final screen = find.byWidgetPredicate(
      (widget) =>
          widget is VoiceTileView && widget.userId == _tomas && widget.screen,
    );

    await tester.tap(screen, buttons: kSecondaryButton);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(_inMenu(find.text('Stream volume')), findsOneWidget);
    tester.widget<Slider>(_inMenu(find.byType(Slider))).onChanged!(40);
    await tester.pump();
    await tester.tap(_inMenu(find.text('Mute stream')));
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(app.repository.streamListening[(_dev, _tomas)], (40, true));
    // His voice is untouched.
    expect(app.repository.listening[(_dev, _tomas)], (100, false));
    expect(tester.widget<VoiceTileView>(screen).mutedHere, isTrue);
    expect(
      find.descendant(of: screen, matching: find.byIcon(OcIcons.volumeOff)),
      findsOneWidget,
    );
    await app.dispose(tester);
  });
}
