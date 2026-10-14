import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Windows plan W2, step 7: push-to-talk on a global keybind, while another
// app has the focus. A tone plays into CABLE Input, the app's microphone is
// CABLE Output, so the tone is what the app would send; push-to-talk decides
// whether it does. Notepad is opened and takes the focus, and the keybind's
// key is injected from outside the app with
// app/tool/windows/hold_key.ps1 — a low-level hook sees an injected key
// exactly as it sees a typed one, so holding it proves the hook is watching
// the whole desktop and not just this window.
//
// A Listener voicebot measures 440 Hz in what arrives, and
// app/tool/windows/check_keybinds.ps1 reads that from the bot's log. As in
// W1's microphone check, the acceptance is a comparison between the phases,
// one phase per run:
//   held      the key is held throughout; the tone should go out
//   released  the key is never pressed; nothing should go out
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

/// The app's microphone: CABLE Output, which hears CABLE Input.
const _inputId = String.fromEnvironment('OPENCORD_E2E_INPUT_ID');

/// The app's speaker; deliberately *not* the cable, or what the app plays
/// would land in its own microphone.
const _outputId = String.fromEnvironment('OPENCORD_E2E_OUTPUT_ID');

const _phase = String.fromEnvironment('OPENCORD_E2E_PHASE');

/// The keybind, as [Accelerator] writes it. F13 by default: a key the
/// keyboard has a code for and no app uses, so holding it disturbs nothing.
const _keys = String.fromEnvironment('OPENCORD_E2E_KEYS', defaultValue: 'F13');

/// Its virtual-key code, for the injector. 0x7C is F13.
const _virtualKey = int.fromEnvironment(
  'OPENCORD_E2E_VIRTUAL_KEY',
  defaultValue: 0x7c,
);

/// How long to hold the app in the phase's state while the bot listens.
const _holdSeconds = int.fromEnvironment('OPENCORD_E2E_HOLD', defaultValue: 12);

/// PowerShell 7, which runs the injector.
const _pwsh = String.fromEnvironment('OPENCORD_E2E_PWSH', defaultValue: 'pwsh');

/// The repository root, to find the injector script.
const _repo = String.fromEnvironment('OPENCORD_E2E_REPO');

const _profile = 'e2e_keybinds';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'a global keybind holds push-to-talk, in $_phase',
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      Process? notepad;
      Process? injector;
      var foreground = 'not checked';
      try {
        await createIdentity(tester, 'Binder');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));

        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio
                  .withInput(AudioDevice(id: _inputId, name: 'Cable'))
                  .withOutput(AudioDevice(id: _outputId, name: 'Speakers'))
                  .copyWith(
                    inputMode: InputMode.pushToTalk,
                    // The device path, as in W1 step 5: a pure tone is not
                    // speech, so leave the processing out of the question and
                    // let push-to-talk be the only thing that decides.
                    noiseSuppression: NoiseSuppression.off,
                    automaticSensitivity: false,
                    echoCancellation: false,
                    automaticGain: false,
                  ),
            );

        // The keybind. Adding it binds it: the provider the app watches is the
        // one the Keybinds page writes to.
        container
            .read(keybindsProvider.notifier)
            .add(HotkeyAction.pushToTalk, keys: _keys);
        await waitUntil(
          tester,
          () => container.read(hotkeySupportProvider) != null,
          reason: 'the core answering whether the keybind is global',
        );

        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        await waitFor(tester, inSidebar('Listener'));

        // Somewhere else to type: the keybind has to work while this window is
        // not the one with the focus.
        notepad = await Process.start('notepad.exe', const []);
        for (var waited = 0; waited < 40; waited++) {
          await tester.pump(const Duration(milliseconds: 50));
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }

        if (_phase == 'held') {
          // Held a little past the measurement: the bot's level is the last
          // five seconds it decoded, so the key must still be down when the
          // app stops sending.
          injector = await Process.start(_pwsh, [
            '-NoLogo',
            '-NoProfile',
            '-File',
            '$_repo\\app\\tool\\windows\\hold_key.ps1',
            '-VirtualKey',
            '$_virtualKey',
            '-Seconds',
            '${_holdSeconds + 3}',
          ]);
          // The script says where the focus was before it pressed anything.
          foreground = await injector.stdout
              .map(String.fromCharCodes)
              .expand((text) => text.split(RegExp(r'[\r\n]+')))
              .firstWhere(
                (line) => line.startsWith('foreground:'),
                orElse: () => 'foreground: not reported',
              )
              .then((line) => line.split(': ').last.trim());
        }

        // Hold the state while the bot listens. Pumping keeps capture running.
        final until = DateTime.now().add(Duration(seconds: _holdSeconds));
        while (DateTime.now().isBefore(until)) {
          await tester.pump(const Duration(milliseconds: 50));
          await Future<void>.delayed(const Duration(milliseconds: 50));
        }

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        final support = switch (container.read(hotkeySupportProvider)) {
          HotkeysGlobal(:final method) => 'global ($method)',
          HotkeysFocusedOnly(:final reason) => 'focused-only ($reason)',
          null => 'unanswered',
        };
        // ignore: avoid_print
        print(
          'KEYBINDS: phase=$_phase keys=$_keys support=$support '
          'foreground=$foreground',
        );
        notepad?.kill();
        await injector?.exitCode;
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
