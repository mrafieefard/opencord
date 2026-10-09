import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 2 V5's acceptance against a real server: nine people in a call
// with every camera on (plan §16 V5). Eight voicebots send cameras of real
// H.264 and watch everyone's (tool/check_v5_load.sh starts them, and a
// virtual PipeWire camera for this app, so this machine's real camera
// stays off). The app joins, turns its camera on, and for 20 seconds
// counts the pictures each tile's texture gets and the CPU and memory it
// uses; it also times turning the camera on (to the preview's first
// picture) and off. The numbers mean something in a profile build
// (`flutter drive --profile`, as the script runs it). Without the script
// this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _camera = String.fromEnvironment('OPENCORD_E2E_CAMERA');
const _profile = 'e2e_video_load';
const _others = 8;
const _measured = Duration(seconds: 20);

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'a call of nine with every camera on stays smooth',
    skip: _invite.isEmpty || _camera.isEmpty,
    (tester) async {
      // Typing in tests otherwise relies on a debug-only shortcut (client
      // id -1), and this test runs in profile builds.
      binding.testTextInput.register();
      addTearDown(binding.testTextInput.unregister);
      // Laid out as a maximized window on a 1080p laptop screen, drawn
      // scaled into the test's window: the tiles want what they would there.
      await binding.setSurfaceSize(const Size(1920, 1080));
      addTearDown(() => binding.setSurfaceSize(null));
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      try {
        await createIdentity(tester, 'Watcher');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        final cameras = Stopwatch()..start();
        while (container.read(videoFeedsProvider).cameras.length < _others) {
          if (cameras.elapsed > const Duration(seconds: 60)) {
            fail(
              "Timed out: the bots' cameras; "
              '${container.read(videoFeedsProvider).cameras.length} came',
            );
          }
          await tester.pump(const Duration(milliseconds: 50));
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
        final textures = {
          for (final MapEntry(:key, :value)
              in container.read(videoFeedsProvider).cameras.entries)
            'user $key': value.textureId!,
        };
        final drawn = Stopwatch()..start();
        while (!textures.values.every((texture) => _presented(texture) >= 30)) {
          if (drawn.elapsed > const Duration(seconds: 30)) {
            final counts = textures.map(
              (name, texture) => MapEntry(name, _presented(texture)),
            );
            final wants = container
                .read(videoWantsProvider)
                .map((want) => '${want.trackId} ${want.width}x${want.height}');
            final now = container
                .read(videoFeedsProvider)
                .cameras
                .map((user, feed) => MapEntry(user, feed.textureId));
            fail(
              'Timed out: every camera drawn; pictures $counts; wants $wants; '
              'textures now $now',
            );
          }
          await tester.pump(const Duration(milliseconds: 50));
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }

        container.read(repositoryProvider).chooseCamera(_camera);
        final on = Stopwatch()..start();
        await container
            .read(voiceSessionProvider.notifier)
            .toggleCamera()
            .timeout(const Duration(seconds: 10));
        await _timeUntil(tester, on, () {
          final own = container.read(videoFeedsProvider).own?.textureId;
          return own != null && _presented(own) >= 1;
        }, reason: "the camera's first picture in its preview");
        on.stop();
        final ownTexture = container.read(videoFeedsProvider).own!.textureId!;
        textures['own'] = ownTexture;
        // Others start watching it; its layers settle.
        await Future<void>.delayed(const Duration(seconds: 3));

        final measured = await _measure(tester, textures);

        final off = Stopwatch()..start();
        await container
            .read(voiceSessionProvider.notifier)
            .toggleCamera()
            .timeout(const Duration(seconds: 10));
        await _timeUntil(
          tester,
          off,
          () =>
              container.read(videoFeedsProvider).own == null &&
              core.videoTextureStats(textureId: ownTexture) == null,
          reason: 'the camera off and its preview released',
        );
        off.stop();

        final report = {
          'camera_on_ms': on.elapsedMilliseconds,
          'camera_off_ms': off.elapsedMilliseconds,
          ...measured,
        };
        debugPrint('V5 load: ${jsonEncode(report)}');

        final tiles = measured['tiles']! as Map<String, Map<String, num>>;
        for (final MapEntry(:key, :value) in tiles.entries) {
          expect(value['worst_second'], greaterThan(0), reason: '$key stalled');
          expect(value['fps'], greaterThanOrEqualTo(14), reason: key);
        }
        expect(on.elapsed, lessThan(const Duration(seconds: 1)));
        expect(off.elapsed, lessThan(const Duration(seconds: 1)));
        // Only now: the script takes reported numbers as a pass.
        binding.reportData = report;

        await container.read(voiceSessionProvider.notifier).leave();
        await waitUntil(
          tester,
          () => container.read(voiceConnectionProvider) == null,
          reason: 'leaving voice',
        );
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
    timeout: const Timeout(Duration(minutes: 5)),
  );
}

int _presented(int texture) =>
    core.videoTextureStats(textureId: texture)?.presented.toInt() ?? 0;

int _drawn(int texture) =>
    core.videoTextureStats(textureId: texture)?.drawn.toInt() ?? 0;

/// Waits, pumping frames, until [done]; the stopwatch keeps running.
Future<void> _timeUntil(
  WidgetTester tester,
  Stopwatch watch,
  bool Function() done, {
  required String reason,
}) async {
  while (!done()) {
    if (watch.elapsed > const Duration(seconds: 10)) fail('Timed out: $reason');
    await tester.pump();
    await Future<void>.delayed(const Duration(milliseconds: 5));
  }
}

/// Pictures each texture got a second, the CPU this process used (by
/// thread, too) and its memory, over [_measured].
Future<Map<String, Object>> _measure(
  WidgetTester tester,
  Map<String, int> textures,
) async {
  final seconds = {for (final name in textures.keys) name: <int>[]};
  final firstPresented = textures.map(
    (name, id) => MapEntry(name, _presented(id)),
  );
  final firstDrawn = textures.map((name, id) => MapEntry(name, _drawn(id)));
  var last = Map.of(firstPresented);
  final cpuBefore = _cpu();
  final threadsBefore = _threads();
  final clock = Stopwatch()..start();
  for (var second = 0; second < _measured.inSeconds; second++) {
    final next = Duration(seconds: second + 1);
    while (clock.elapsed < next) {
      await tester.pump();
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    for (final MapEntry(:key, :value) in textures.entries) {
      final presented = _presented(value);
      seconds[key]!.add(presented - last[key]!);
      last[key] = presented;
    }
  }
  final elapsed = clock.elapsed.inMicroseconds / 1e6;
  final cpu = (_cpu() - cpuBefore).inMicroseconds / 1e6 / elapsed * 100;
  final threads = _threads();
  final byThread = {
    for (final MapEntry(:key, :value) in threads.entries)
      key:
          ((value - (threadsBefore[key] ?? Duration.zero)).inMicroseconds /
                  1e6 /
                  elapsed *
                  100)
              .clamp(0, double.infinity),
  };
  final busiest = byThread.entries.toList()
    ..sort((a, b) => b.value.compareTo(a.value));
  num round(num value) => (value * 10).round() / 10;
  return {
    'seconds': elapsed,
    'cpu_percent_of_a_core': round(cpu),
    'cpu_percent_of_the_machine': round(cpu / Platform.numberOfProcessors),
    'rss_mb': _rssMegabytes(),
    'busiest_threads': {
      for (final entry in busiest.take(10)) entry.key: round(entry.value),
    },
    'tiles': {
      for (final MapEntry(:key, :value) in textures.entries)
        key: <String, num>{
          'fps': round((last[key]! - firstPresented[key]!) / elapsed),
          'drawn_fps': round((_drawn(value) - firstDrawn[key]!) / elapsed),
          'worst_second': seconds[key]!.reduce((a, b) => a < b ? a : b),
        },
    },
  };
}

/// CPU time from a /proc stat line: utime and stime, the 14th and 15th
/// fields, in hundredths of a second.
Duration _ticks(String stat) {
  final fields = stat.substring(stat.lastIndexOf(')') + 2).split(' ');
  final ticks = int.parse(fields[11]) + int.parse(fields[12]);
  return Duration(milliseconds: ticks * 10);
}

Duration _cpu() => _ticks(File('/proc/self/stat').readAsStringSync());

/// CPU time so far by thread name.
Map<String, Duration> _threads() {
  final times = <String, Duration>{};
  for (final task in Directory('/proc/self/task').listSync()) {
    try {
      final name = File('${task.path}/comm').readAsStringSync().trim();
      final used = _ticks(File('${task.path}/stat').readAsStringSync());
      times[name] = (times[name] ?? Duration.zero) + used;
    } on FileSystemException {
      // The thread ended.
    }
  }
  return times;
}

int _rssMegabytes() {
  final line = File(
    '/proc/self/status',
  ).readAsLinesSync().firstWhere((line) => line.startsWith('VmRSS:'));
  return int.parse(line.split(RegExp(r'\s+'))[1]) ~/ 1024;
}
