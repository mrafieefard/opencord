import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../support/e2e.dart';

// The macOS version of integration_test/user_audio_test.dart
// (app/tool/macos/check_user_audio.sh): the same steps, with BlackHole 2ch
// in place of PipeWire's null sinks. The app plays into BlackHole and uses
// it as its microphone too (what it sends isn't measured here). The app
// sandbox may not start sox, so the script records BlackHole for the whole
// test; the app prints each window to measure ("WINDOW: <name> <start>
// <end>", in milliseconds since the epoch) and the script measures them
// against the shared test's numbers. Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

/// The cpal ids of the devices the app plays into and records from.
const _output = String.fromEnvironment('OPENCORD_E2E_OUTPUT');
const _input = String.fromEnvironment('OPENCORD_E2E_INPUT');
const _profile = 'e2e_user_audio';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "the voice menu's volume, Mute and Mute soundboard, heard at the speaker",
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      var heard = false;
      final subscription = container.read(repositoryProvider).events.listen((
        event,
      ) {
        if (event is SpeakingChanged && event.speaking.isNotEmpty) {
          heard = true;
        }
      });
      try {
        await createIdentity(tester, 'Listener');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio
                  .withInput(const AudioDevice(id: _input, name: 'BlackHole'))
                  .withOutput(const AudioDevice(id: _output, name: 'Measure')),
            );
        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        await waitFor(tester, inSidebar('Talker'));
        await waitFor(tester, inSidebar('Player'));

        /// Lets [length] pass, printed as the window [name] when it has
        /// one.
        Future<void> window(String? name, Duration length) async {
          final start = DateTime.now();
          final end = start.add(length);
          while (DateTime.now().isBefore(end)) {
            await tester.pump(const Duration(milliseconds: 50));
            await Future<void>.delayed(const Duration(milliseconds: 50));
          }
          if (name == null) return;
          // ignore: avoid_print
          print(
            'WINDOW: $name ${start.millisecondsSinceEpoch} '
            '${DateTime.now().millisecondsSinceEpoch}',
          );
        }

        Future<void> voice(String name) =>
            window(name, const Duration(seconds: 2));
        // Player plays every 3 s; 4 s holds at least one whole sound.
        Future<void> sounds(String name) =>
            window(name, const Duration(seconds: 4));

        Future<void> openMenu(String name) async {
          await tester.tap(inSidebar(name), buttons: kSecondaryButton);
          await waitFor(tester, find.byType(OcMenuPanel));
        }

        Future<void> closeMenu() async {
          // A slider keeps focus at times and an Escape goes nowhere: press
          // until the menu is gone.
          for (var tries = 0; tries < 5; tries++) {
            if (find.byType(OcMenuPanel).evaluate().isEmpty) return;
            await tester.sendKeyEvent(LogicalKeyboardKey.escape);
            await tester.pump(const Duration(milliseconds: 300));
          }
          fail('the menu did not close');
        }

        Finder inMenu(Finder finder) =>
            find.descendant(of: find.byType(OcMenuPanel), matching: finder);

        Future<void> setVolume(int percent) async {
          await openMenu('Talker');
          tester.widget<Slider>(inMenu(find.byType(Slider))).onChanged!(
            percent.toDouble(),
          );
          await tester.pump();
          expect(inMenu(find.text('$percent%')), findsOneWidget);
          await closeMenu();
        }

        Future<void> toggle(String name, String label) async {
          await openMenu(name);
          await tester.tap(inMenu(find.text(label)));
          await tester.pump();
          await closeMenu();
        }

        // Talker's tone arrives once their audio flows.
        await waitUntil(tester, () => heard, reason: "Talker's audio flowing");
        await window(null, const Duration(seconds: 2));

        await voice('at100');
        await setVolume(50);
        await voice('at50');
        await setVolume(200);
        await voice('at200');
        await setVolume(100);
        await toggle('Talker', 'Mute');
        await voice('muted');
        await toggle('Talker', 'Mute');
        await voice('unmuted');

        // Talker muted here, so 880 Hz can only be Player's sound (Opus
        // gives the 440 Hz tone a faint 880 Hz harmonic).
        await toggle('Talker', 'Mute');
        await sounds('soundBefore');
        await toggle('Player', 'Mute soundboard');
        // A sound already playing ends within its 1.5 s.
        await window(null, const Duration(seconds: 2));
        await sounds('soundMuted');
        await toggle('Talker', 'Mute');
        await voice('voiceMeanwhile');
        await toggle('Talker', 'Mute');
        await toggle('Player', 'Mute soundboard');
        await window(null, const Duration(seconds: 2));
        await sounds('soundAfter');
        await toggle('Talker', 'Mute');

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await subscription.cancel();
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
