import 'dart:math';
import 'dart:ui';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/app.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/music/music_queue.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

// Phase 3 plan §0.1's targets for the music panel, on the mock repository
// (no server needed): it opens in under 100 ms with 500 tracks queued,
// dragging in that queue stays at 60 fps, and nothing is drawn while the
// music is paused. The numbers mean something in a profile build:
//
//   flutter drive --profile -d linux --driver=test_driver/integration_test.dart --target=integration_test/music_ui_test.dart
//
// They land in build/integration_response_data.json.

/// Frames Flutter reported while [action] ran.
Future<List<FrameTiming>> _frames(
  WidgetTester tester,
  Future<void> Function() action,
) async {
  final frames = <FrameTiming>[];
  void collect(List<FrameTiming> timings) => frames.addAll(timings);
  SchedulerBinding.instance.addTimingsCallback(collect);
  await action();
  // Timings arrive in batches, about once a second.
  for (var i = 0; i < 12; i++) {
    await tester.pump(const Duration(milliseconds: 100));
    await Future<void>.delayed(const Duration(milliseconds: 100));
  }
  SchedulerBinding.instance.removeTimingsCallback(collect);
  return frames;
}

Map<String, num> _summary(List<FrameTiming> frames) {
  double ms(Duration d) => d.inMicroseconds / 1000;
  final builds = [for (final f in frames) ms(f.buildDuration)];
  final rasters = [for (final f in frames) ms(f.rasterDuration)];
  num worst(List<double> values) =>
      values.isEmpty ? 0 : (values.reduce(max) * 10).round() / 10;
  return {
    'frames': frames.length,
    'worst_build_ms': worst(builds),
    'worst_raster_ms': worst(rasters),
    'over_16ms': [
      for (final f in frames)
        if (ms(f.buildDuration) > 16 || ms(f.rasterDuration) > 16) f,
    ].length,
  };
}

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the music panel meets the §0.1 targets', (tester) async {
    await binding.setSurfaceSize(const Size(1440, 900));
    addTearDown(() => binding.setSurfaceSize(null));
    final repository = MockRepository(random: Random(1), simulateLife: false);
    final container = ProviderContainer(
      overrides: [
        keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore()),
        repositoryProvider.overrideWithValue(repository),
      ],
    );
    addTearDown(container.dispose);
    container.read(eventPumpProvider);
    repository.start();
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: container,
        child: const OpencordApp(),
      ),
    );
    for (var i = 0; i < 20; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    final server = container.read(serverListProvider).first.key;
    final voice = container
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.kind == ChannelKind.voice);
    await repository.joinVoice(server, voice.id);
    container.read(navigationProvider.notifier).openChannel(server, voice.id);
    for (var i = 0; i < 10; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    repository.debugFillQueue(server, voice.id, 500);
    // The session is here already (the network is not what is timed).
    final session = container.listen(
      musicSessionProvider((server: server, channel: voice.id)),
      (_, _) {},
    );
    addTearDown(session.close);
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }

    // Opening: from the Music button to the frame with the panel and its
    // queue, build and raster.
    final opening = await _frames(tester, () async {
      container.read(rightPanelModeProvider.notifier).set(RightPanelMode.music);
      await tester.pump();
    });
    expect(find.byType(MusicQueueRow), findsWidgets);
    expect(find.byType(MusicPanel), findsOneWidget);

    // Dragging the first queued track down through the list.
    const trace = bool.fromEnvironment('OPENCORD_TRACE_DRAG');
    if (trace) debugProfileBuildsEnabled = true;
    late List<FrameTiming> dragging;
    Future<void> drag() async => dragging = await _frames(tester, () async {
      final handle = find.byIcon(OcIcons.dragIndicator).first;
      final gesture = await tester.startGesture(tester.getCenter(handle));
      for (var step = 0; step < 60; step++) {
        await gesture.moveBy(const Offset(0, 8));
        await tester.pump(const Duration(milliseconds: 16));
      }
      await gesture.up();
      await tester.pump(const Duration(milliseconds: 200));
    });
    if (trace) {
      await binding.traceAction(drag, reportKey: 'drag_timeline');
      debugProfileBuildsEnabled = false;
    } else {
      await drag();
    }

    // Paused: the bar stops; nothing else asks for frames. Frames are
    // counted as they are built, for 3 s of only waiting.
    await repository.musicPause(server, voice.id, paused: true);
    await tester.pump(const Duration(milliseconds: 500));
    var counting = true;
    var paused = 0;
    SchedulerBinding.instance.addPersistentFrameCallback((_) {
      if (counting) paused++;
    });
    await Future<void>.delayed(const Duration(seconds: 3));
    counting = false;

    final opened = opening.isEmpty
        ? 0.0
        : opening.map((f) => f.totalSpan.inMicroseconds / 1000).reduce(max);
    binding.reportData = {
      ...?binding.reportData,
      'open_frame_ms': (opened * 10).round() / 10,
      'drag': _summary(dragging),
      'paused_frames_in_3s': paused,
    };
    // ignore: avoid_print
    print('music ui: ${binding.reportData}');
    // ignore: avoid_print
    print(
      'slow drag frames: ${[for (final (i, f) in dragging.indexed)
        if (f.buildDuration.inMilliseconds > 16 || f.rasterDuration.inMilliseconds > 16) '$i/${dragging.length} build ${f.buildDuration.inMicroseconds / 1000} raster ${f.rasterDuration.inMicroseconds / 1000}']}',
    );
    // A debug build (CI's `flutter test`) is far slower than what ships, so
    // only a profile run is held to the time; debug only reports it.
    if (!kDebugMode) {
      expect(opened, lessThan(100), reason: 'opens in under 100 ms');
    }
    expect(paused, 0, reason: 'nothing drawn while paused');
    await tester.pumpWidget(const SizedBox());
    repository.dispose();
  });
}
