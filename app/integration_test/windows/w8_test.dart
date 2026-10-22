import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Windows plan W8, step 1 — the §15 hardware checklist, the parts that need
// a person at the PC. `check_hardware.ps1` sets the room up and says what
// each measurement means; this test holds a real call and watches what the
// app does when the hardware moves under it.
//
// One phase per run:
//   headset  the chosen headset goes away mid-call and comes back. The app
//            must fall back to the default device with a toast (§7.6, "if
//            the selected device disappears, switch to the default device
//            and show a toast"), keep playing, and go back to the headset
//            when it returns (media.rs `switch_back`).
//   sleep    the PC sleeps mid-call and is woken. The call must come back
//            and audio must flow again.
//
// Nothing is audible except in the headset itself: the fallback device is
// the cable, which is also how the measurements are taken — CABLE Output
// hears CABLE Input back, so "the app is playing to the fallback" is a
// number and not an impression.
//
// No prompts: a script cannot ask the person anything here, because its
// output goes to the session and not to the screen in front of them
// (windows.md §7, W8). Every step waits for the hardware event itself — a
// device vanishing, a device returning, the clock jumping over a sleep — so
// the person can take as long as they like.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

const _phase = String.fromEnvironment(
  'OPENCORD_E2E_PHASE',
  defaultValue: 'headset',
);

/// Part of the name of the headset to use and then take away.
const _headset = String.fromEnvironment('OPENCORD_E2E_HEADSET');

/// The app's fallback speaker: the cable, so nothing is heard and what it
/// plays can be measured. The script makes this the system default too.
const _outputId = String.fromEnvironment('OPENCORD_E2E_OUTPUT_ID');

/// The app's microphone when the headset is gone: something silent.
const _inputId = String.fromEnvironment('OPENCORD_E2E_INPUT_ID');

/// The dshow capture that hears [_outputId] back: CABLE Output.
const _recordDevice = String.fromEnvironment('OPENCORD_E2E_RECORD');

/// Written when the call is up, so the script knows where the run is.
const _marker = String.fromEnvironment('OPENCORD_E2E_MARKER');

/// How long to wait for the person to move the hardware.
const _patience = Duration(minutes: 4);

const _profile = 'e2e_w8';

const _rate = 48000;

/// 100 ms: a whole number of cycles of the Talker's 440 Hz.
const _block = 4800;

/// dshow takes a moment to start delivering.
const _leadIn = Duration(milliseconds: 600);

Future<void> _hold(WidgetTester tester, Duration total) async {
  final until = DateTime.now().add(total);
  while (DateTime.now().isBefore(until)) {
    await tester.pump(const Duration(milliseconds: 50));
    await Future<void>.delayed(const Duration(milliseconds: 50));
  }
}

/// Records what the app plays into the cable for [length]. The same shape as
/// `user_audio_test.dart`'s: ffmpeg is given `-t` and waited for, because a
/// killed ffmpeg loses whatever it had not yet flushed.
Future<Float32List> _record(WidgetTester tester, Duration length) async {
  final total = length + _leadIn;
  final ffmpeg = await Process.start('ffmpeg', [
    '-hide_banner',
    '-loglevel',
    'error',
    '-f',
    'dshow',
    '-audio_buffer_size',
    '50',
    '-i',
    'audio=$_recordDevice',
    '-t',
    '${total.inMilliseconds / 1000}',
    '-f',
    'f32le',
    '-ar',
    '$_rate',
    '-ac',
    '1',
    '-',
  ]);
  final bytes = BytesBuilder(copy: false);
  final collected = ffmpeg.stdout.listen(bytes.add).asFuture<void>();
  final problems = StringBuffer();
  final complaints = ffmpeg.stderr
      .listen((chunk) => problems.write(String.fromCharCodes(chunk)))
      .asFuture<void>();

  // The widget tree has to keep running while ffmpeg records, or the app
  // stops playing what we are trying to measure.
  final end = DateTime.now().add(total + const Duration(seconds: 2));
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 50));
    await Future<void>.delayed(const Duration(milliseconds: 50));
    if (bytes.length >= total.inMilliseconds * _rate * 4 ~/ 1000) break;
  }
  await collected;
  await complaints;
  final status = await ffmpeg.exitCode;
  if (status != 0 && bytes.isEmpty) {
    fail('ffmpeg could not record $_recordDevice (exit $status): $problems');
  }

  final data = bytes.takeBytes();
  final all = data.buffer.asFloat32List(
    data.offsetInBytes,
    data.lengthInBytes ~/ 4,
  );
  final skip = math.min(_leadIn.inMilliseconds * _rate ~/ 1000, all.length);
  return Float32List.sublistView(all, skip);
}

/// Each 100 ms block's level of [hz] in dBFS (a full-scale sine is 0).
List<double> _levels(Float32List samples, double hz) {
  final coefficient = 2 * math.cos(2 * math.pi * hz / _rate);
  return [
    for (var start = 0; start + _block <= samples.length; start += _block)
      () {
        var (s1, s2) = (0.0, 0.0);
        for (var n = start; n < start + _block; n++) {
          (s1, s2) = (samples[n] + coefficient * s1 - s2, s1);
        }
        final power = s1 * s1 + s2 * s2 - coefficient * s1 * s2;
        final amplitude = 2 * math.sqrt(math.max(power, 0)) / _block;
        return amplitude <= 0 ? -200.0 : 20 * math.log(amplitude) / math.ln10;
      }(),
  ];
}

double _median(List<double> levels) {
  if (levels.isEmpty) return -200.0;
  final sorted = [...levels]..sort();
  return sorted[sorted.length ~/ 2];
}

/// The Talker's tone in the cable, in dBFS, as a median over [length].
Future<double> _talker(WidgetTester tester, Duration length) async =>
    _median(_levels(await _record(tester, length), 440));

String _db(double level) => '${level.toStringAsFixed(1)} dBFS';

/// What the app says is connected, for the log.
void _printDevices(AudioDeviceList devices, String when) {
  // ignore: avoid_print
  print(
    'W8 devices $when: outputs '
    '[${devices.outputs.map((device) => device.name).join(' | ')}] '
    'inputs [${devices.inputs.map((device) => device.name).join(' | ')}]',
  );
}

AudioDevice? _named(List<AudioDevice> devices, String part) =>
    devices.where((device) => device.name.contains(part)).firstOrNull;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'the $_phase checklist item',
    skip: _invite.isEmpty,
    timeout: const Timeout(Duration(minutes: 25)),
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      try {
        await createIdentity(tester, 'Checker');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));

        // The quiet devices: the cable and something silent. They stay the
        // app's choice through the sleep phase; the headset phase puts the
        // headset in their place and gets them back by falling back.
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio
                  .withInput(AudioDevice(id: _inputId, name: 'Silent'))
                  .withOutput(AudioDevice(id: _outputId, name: 'Cable')),
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
        if (_marker.isNotEmpty) File(_marker).writeAsStringSync('in call');

        // Voice media pushes the device list every 2 s while it is open
        // (media.rs DEVICE_POLL). That poll is what the headset phase tests
        // on Windows, so everything below reads the pushed list and never
        // asks again.
        AudioDeviceList devices() => container.read(audioDeviceListProvider);
        _printDevices(devices(), 'in the call');

        if (_phase == 'headset') {
          // 1. The headset. It may be connected already; if not, wait for
          //    the person to connect it.
          if (_named(devices().outputs, _headset) == null) {
            await waitUntil(
              tester,
              () => _named(devices().outputs, _headset) != null,
              reason: 'the headset ($_headset) to be connected',
              timeout: _patience,
            );
            _printDevices(devices(), 'with the headset connected');
          }
          final speaker = _named(devices().outputs, _headset)!;
          final microphone = _named(devices().inputs, _headset);
          // ignore: avoid_print
          print(
            'W8: the headset is "${speaker.name}", microphone '
            '${microphone == null ? 'none' : '"${microphone.name}"'}',
          );

          // 2. Use it for the call. A Bluetooth headset switches profile
          //    when the microphone opens, which Windows shows as the device
          //    list changing; print it either way (§15, "note what
          //    happens").
          container
              .read(audioSettingsProvider.notifier)
              .update(
                (audio) => microphone == null
                    ? audio.withOutput(speaker)
                    : audio.withOutput(speaker).withInput(microphone),
              );
          await _hold(tester, const Duration(seconds: 5));
          _printDevices(devices(), 'with the headset in the call');

          // The call is in the headset now, so the cable must go quiet.
          final onHeadset = await _talker(tester, const Duration(seconds: 2));
          // ignore: avoid_print
          print('W8: the cable while the headset plays: ${_db(onHeadset)}');
          expect(
            onHeadset,
            lessThan(-50.0),
            reason: 'the call should be in the headset, not the cable',
          );

          // 3. The person takes the headset away.
          await waitUntil(
            tester,
            () => _named(devices().outputs, _headset) == null,
            reason: 'the headset to be turned off or unplugged',
            timeout: _patience,
          );
          final noticed = DateTime.now();
          _printDevices(devices(), 'with the headset gone');

          // The toast, which names the device taken over.
          await waitUntil(
            tester,
            () =>
                container
                    .read(audioNoticeProvider)
                    ?.message
                    .contains('unavailable') ??
                false,
            reason: 'the fallback toast',
            timeout: const Duration(seconds: 20),
          );
          final notice = container.read(audioNoticeProvider)!.message;
          // ignore: avoid_print
          print(
            'W8: toast "$notice", '
            '${DateTime.now().difference(noticed).inMilliseconds} ms after '
            'the device list lost the headset — the engine hears the '
            'loss from its own callback and not from that list, so this can '
            'be 0.',
          );

          // 4. And the call keeps playing, on the device it fell back to.
          final onFallback = await _talker(tester, const Duration(seconds: 3));
          // ignore: avoid_print
          print('W8: the fallback device plays the call: ${_db(onFallback)}');
          expect(
            onFallback,
            greaterThan(-50.0),
            reason: 'the call went silent instead of falling back',
          );

          // 5. The person brings it back: media.rs `switch_back` returns to
          //    it, which the cable going quiet again proves.
          await waitUntil(
            tester,
            () => _named(devices().outputs, _headset) != null,
            reason: 'the headset to come back',
            timeout: _patience,
          );
          // Two device polls, so the switch back has happened.
          await _hold(tester, const Duration(seconds: 6));
          final backOnHeadset = await _talker(
            tester,
            const Duration(seconds: 2),
          );
          // ignore: avoid_print
          print(
            'W8: the cable after the headset came back: ${_db(backOnHeadset)}',
          );
          expect(
            backOnHeadset,
            lessThan(onFallback - 15.0),
            reason: 'the call did not go back to the headset',
          );
        } else if (_phase == 'sleep') {
          // The call before the PC sleeps.
          final before = await _talker(tester, const Duration(seconds: 2));
          // ignore: avoid_print
          print('W8: the call before sleeping: ${_db(before)}');
          expect(before, greaterThan(-50.0), reason: 'nothing to measure');

          // The script suspends the PC when this appears, and writes
          // `.woke` with how long the sleep lasted — SetSuspendState only
          // returns once the system is back. This PC has Modern Standby
          // (S0 Low Power Idle), where a desktop app is throttled rather
          // than stopped, so the app's own clock is not a reliable witness
          // that the PC slept; the script's is (windows.md §7, W8).
          final sleptBy = DateTime.now();
          if (_marker.isNotEmpty) File('$_marker.sleep').writeAsStringSync('');
          final woke = File('$_marker.woke');
          final giveUp = DateTime.now().add(const Duration(minutes: 10));
          while (!woke.existsSync()) {
            if (DateTime.now().isAfter(giveUp)) fail('the PC never slept');
            await tester.pump(const Duration(milliseconds: 100));
            await Future<void>.delayed(const Duration(milliseconds: 200));
          }
          final last = DateTime.now();
          // ignore: avoid_print
          print(
            'W8: the PC slept for ${woke.readAsStringSync().trim()} s; the '
            'app saw ${last.difference(sleptBy).inSeconds} s pass',
          );

          // What it looks like the moment it wakes, and what it settles to.
          final atWake = container.read(voiceConnectionProvider)?.phase;
          _printDevices(devices(), 'just after waking');
          await waitUntil(
            tester,
            () =>
                container.read(voiceConnectionProvider)?.phase ==
                VoiceConnectionPhase.connected,
            reason: 'the call to come back after waking',
            timeout: const Duration(seconds: 60),
          );
          final backAt = DateTime.now();
          await _hold(tester, const Duration(seconds: 5));
          final after = await _talker(tester, const Duration(seconds: 3));
          // ignore: avoid_print
          print(
            'W8: at the wake the call was $atWake, connected again '
            '${backAt.difference(last).inMilliseconds} ms later, and plays '
            'at ${_db(after)}',
          );
          _printDevices(devices(), 'after waking');
          // ignore: avoid_print
          print(
            'W8: the last toast after waking: '
            '${container.read(audioNoticeProvider)?.message ?? 'none'}',
          );
          expect(
            after,
            greaterThan(-50.0),
            reason: 'the call was silent after waking',
          );
        } else {
          fail('unknown phase $_phase');
        }

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
