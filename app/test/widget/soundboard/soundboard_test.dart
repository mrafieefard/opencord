import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/soundboard/soundboard_picker.dart';
import 'package:opencord/features/soundboard/soundboard_ring.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _mira = 1002;

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inPicker(Finder finder) =>
    find.descendant(of: find.byType(SoundboardPicker), matching: finder);

Finder _inSettings(Finder finder) =>
    find.descendant(of: find.byType(SettingsDialog), matching: finder);

Finder _playingRing() => find.byWidgetPredicate(
  (widget) => widget is SoundboardRing && widget.playing,
  description: 'a soundboard ring',
);

Future<void> _joinGeneral(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('General'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

Future<void> _openPicker(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byType(VoiceControlBar),
      matching: find.bySemanticsLabel('Soundboard'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
  expect(find.byType(SoundboardPicker), findsOneWidget);
}

String _bruh(MockApp app) => app
    .read(soundsProvider(_dev))
    .values
    .firstWhere((sound) => sound.name == 'Bruh')
    .ref;

void main() {
  testWidgets('a sound from the picker plays for the call, then cools down', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    await _openPicker(tester);

    expect(_inPicker(find.text('THIS SERVER')), findsOneWidget);
    expect(_inPicker(find.text('DEFAULT')), findsOneWidget);
    expect(_inPicker(find.text('Bruh')), findsOneWidget);

    await tester.tap(_inPicker(find.text('Bruh')));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(app.repository.soundsPlayed, [_bruh(app)]);
    // Your own ring shows while it plays (§11.6).
    expect(_playingRing(), findsWidgets);

    // Three seconds of cooldown: the next tap does nothing.
    await tester.tap(_inPicker(find.text('Wow')));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(app.repository.soundsPlayed, hasLength(1));
    expect(_inPicker(find.byTooltip('Cooling down')), findsWidgets);

    await _pumpFor(tester, const Duration(seconds: 3));
    await tester.tap(_inPicker(find.text('Wow')));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(app.repository.soundsPlayed, hasLength(2));
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('sounds that cannot be played here say why', (tester) async {
    final app = await MockApp.pump(tester);
    final settings = app.read(serverProvider(_dev)).data!.voiceSettings;
    // The mock answers after a pretend round trip.
    final saved = app.repository.updateVoiceSettings(
      _dev,
      settings.copyWith(allowDefaultSounds: false),
    );
    await _pumpFor(tester, const Duration(milliseconds: 500));
    await saved;
    await _joinGeneral(tester);
    await _openPicker(tester);

    expect(
      _inPicker(find.byTooltip('This server turned off default sounds')),
      findsWidgets,
    );
    await tester.tap(_inPicker(find.text('Ta-da')));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(app.repository.soundsPlayed, isEmpty);

    // A favourite comes first, and the preview plays here only.
    final row = _inPicker(find.text('Bruh'));
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.addPointer(location: tester.getCenter(row));
    await tester.pump();
    await tester.tap(
      _inPicker(
        find.byWidgetPredicate(
          (widget) => widget is OcIconButton && widget.tooltip == 'Preview',
        ),
      ).first,
    );
    await tester.pump();
    expect(app.repository.soundsPreviewed, [_bruh(app)]);
    expect(app.repository.soundsPlayed, isEmpty);
    await tester.tap(
      _inPicker(
        find.byWidgetPredicate(
          (widget) =>
              widget is OcIconButton && widget.tooltip == 'Add to favorites',
        ),
      ).first,
    );
    await tester.pump();
    expect(_inPicker(find.text('FAVORITES')), findsOneWidget);
    await gesture.removePointer();
    await app.dispose(tester);
  });

  testWidgets('someone playing a sound gets the dashed ring for its length', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    final channel = app.read(voiceSessionProvider).channelId!;

    app.repository.debugSoundPlayed(
      _dev,
      SoundPlay(
        channelId: channel,
        userId: _mira,
        soundRef: 'default:boing',
        name: 'Boing',
        emoji: '🪀',
        duration: const Duration(seconds: 1),
        startedAt: DateTime(2026),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 100));

    expect(_playingRing(), findsWidgets);
    expect(find.text('🪀'), findsWidgets);
    await _pumpFor(tester, const Duration(milliseconds: 1000));
    expect(_playingRing(), findsNothing);
    expect(find.text('🪀'), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('server settings list the sounds and delete one', (tester) async {
    final app = await MockApp.pump(tester);
    await tester.tap(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.text('Opencord Dev'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    await tester.tap(find.text('Server settings'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(_inSettings(find.text('Soundboard')).first);
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(_inSettings(find.text('4 / 48')), findsOneWidget);
    expect(_inSettings(find.text('Drum roll')), findsOneWidget);
    final delete = _inSettings(
      find.byWidgetPredicate(
        (widget) => widget is OcIconButton && widget.tooltip == 'Delete',
      ),
    ).first;
    await tester.ensureVisible(delete);
    await tester.tap(delete);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Delete sound'));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(_inSettings(find.text('3 / 48')), findsOneWidget);
    expect(app.read(soundsProvider(_dev)), hasLength(3));
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('Sounds in user settings mutes every soundboard here', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(
      find.byWidgetPredicate(
        (widget) => widget is OcIconButton && widget.tooltip == 'User settings',
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(_inSettings(find.text('Sounds')).first);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    await tester.tap(_inSettings(find.text('Mute all soundboards')));
    await tester.pump();

    expect(app.repository.soundboardListening, (volume: 100, muted: true));
    expect(app.store.read('ui.soundboard'), contains('"muted":true'));
    await app.dispose(tester);
  });
}
