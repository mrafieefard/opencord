import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import 'support/e2e.dart';

// The per-user voice menu against a real server, measured at the speaker
// (app/tool/check_user_audio.sh): Talker sends a 440 Hz tone, Player plays
// an 880 Hz soundboard sound every 3 s. The app plays into a null sink of
// its own; its monitor is recorded with parec, and each frequency's level
// is measured while the menu changes Talker's volume and mute and Player's
// soundboard. Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _sink = String.fromEnvironment('OPENCORD_E2E_SINK');
const _silence = String.fromEnvironment('OPENCORD_E2E_SILENCE');
const _profile = 'e2e_user_audio';

const _rate = 48000;

/// 100 ms: a whole number of cycles of both tones.
const _block = 4800;

/// Below this nothing is heard.
const _silent = -70.0;

/// Records what the app plays for [length].
Future<Float32List> _record(WidgetTester tester, Duration length) async {
  final parec = await Process.start('parec', [
    '--device=$_sink.monitor',
    '--raw',
    '--format=float32le',
    '--rate=$_rate',
    '--channels=1',
    '--latency-msec=20',
  ]);
  final bytes = BytesBuilder(copy: false);
  final done = parec.stdout.listen(bytes.add).asFuture<void>();
  final end = DateTime.now().add(length);
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 50));
    await Future<void>.delayed(const Duration(milliseconds: 50));
  }
  parec.kill();
  await done;
  final data = bytes.takeBytes();
  return data.buffer.asFloat32List(data.offsetInBytes, data.lengthInBytes ~/ 4);
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
  final sorted = [...levels]..sort();
  return sorted[sorted.length ~/ 2];
}

double _loudest(List<double> levels) => levels.reduce(math.max);

String _db(double level) => '${level.toStringAsFixed(1)} dBFS';

String _change(double db) => '${db >= 0 ? '+' : ''}${db.toStringAsFixed(1)} dB';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "the voice menu's volume, Mute and Mute soundboard, heard at the speaker",
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      final report = <String>[];
      try {
        await createIdentity(tester, 'Listener');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio
                  .withInput(
                    AudioDevice(id: 'pipewire:$_silence', name: 'Silence'),
                  )
                  .withOutput(
                    AudioDevice(id: 'pipewire:$_sink', name: 'Measure'),
                  ),
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

        Future<double> voice() async => _median(
          _levels(await _record(tester, const Duration(seconds: 2)), 440),
        );
        // Player plays every 3 s; 4 s holds at least one whole sound.
        Future<double> sounds() async => _loudest(
          _levels(await _record(tester, const Duration(seconds: 4)), 880),
        );

        Future<void> openMenu(String name) async {
          await tester.tap(inSidebar(name), buttons: kSecondaryButton);
          await waitFor(tester, find.byType(OcMenuPanel));
        }

        Future<void> closeMenu() async {
          await tester.sendKeyEvent(LogicalKeyboardKey.escape);
          await tester.pump(const Duration(milliseconds: 300));
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
        final heard = DateTime.now().add(const Duration(seconds: 30));
        while (await voice() < -40) {
          if (DateTime.now().isAfter(heard)) fail("Talker's tone never came");
        }

        final at100 = await voice();
        await setVolume(50);
        final at50 = await voice();
        await setVolume(200);
        final at200 = await voice();
        await setVolume(100);
        await toggle('Talker', 'Mute');
        final muted = await voice();
        await toggle('Talker', 'Mute');
        final unmuted = await voice();
        report.addAll([
          'Talker 440 Hz at 100 %: ${_db(at100)}',
          'at 50 %: ${_db(at50)} (${_change(at50 - at100)})',
          'at 200 %: ${_db(at200)} (${_change(at200 - at100)})',
          'muted: ${_db(muted)}',
          'unmuted: ${_db(unmuted)}',
        ]);
        expect(at50 - at100, inInclusiveRange(-7, -5), reason: '50 % is -6 dB');
        expect(at200 - at100, inInclusiveRange(5, 7), reason: '200 % is +6 dB');
        expect(muted, lessThan(_silent));
        expect((unmuted - at100).abs(), lessThan(1));

        // Talker muted here, so 880 Hz can only be Player's sound (Opus
        // gives the 440 Hz tone a faint 880 Hz harmonic).
        await toggle('Talker', 'Mute');
        final soundBefore = await sounds();
        await toggle('Player', 'Mute soundboard');
        // A sound already playing ends within its 1.5 s.
        await _record(tester, const Duration(seconds: 2));
        final soundMuted = await sounds();
        await toggle('Talker', 'Mute');
        final voiceMeanwhile = await voice();
        await toggle('Talker', 'Mute');
        await toggle('Player', 'Mute soundboard');
        await _record(tester, const Duration(seconds: 2));
        final soundAfter = await sounds();
        await toggle('Talker', 'Mute');
        report.addAll([
          "Player's 880 Hz sound: ${_db(soundBefore)}",
          'soundboard muted: ${_db(soundMuted)}',
          'Talker with the soundboard muted: ${_db(voiceMeanwhile)}',
          'soundboard unmuted: ${_db(soundAfter)}',
        ]);
        expect(soundBefore, greaterThan(-30));
        expect(soundMuted, lessThan(_silent));
        expect((voiceMeanwhile - at100).abs(), lessThan(1));
        expect(soundAfter, greaterThan(-30));

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        // ignore: avoid_print
        print('USER AUDIO: ${report.join('; ')}');
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
