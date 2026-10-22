import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/src/rust/api/client.dart' as core;

// Measuring the app from inside it, in the load checks (Phase 2 V5, V10):
// pictures each video texture got, CPU by thread, memory.

/// Pictures handed to [texture] so far.
int presented(int texture) =>
    core.videoTextureStats(textureId: texture)?.presented.toInt() ?? 0;

/// Pictures Flutter drew from [texture] so far.
int drawn(int texture) =>
    core.videoTextureStats(textureId: texture)?.drawn.toInt() ?? 0;

/// Pictures each texture got a second, the CPU this process used (by
/// thread, too) and its memory, over [duration].
Future<Map<String, Object>> measure(
  WidgetTester tester,
  Map<String, int> textures,
  Duration duration,
) async {
  final seconds = {for (final name in textures.keys) name: <int>[]};
  final firstPresented = textures.map(
    (name, id) => MapEntry(name, presented(id)),
  );
  final firstDrawn = textures.map((name, id) => MapEntry(name, drawn(id)));
  var last = Map.of(firstPresented);
  final memory = <int>[];
  final cpuBefore = _cpu();
  final threadsBefore = _threads();
  final clock = Stopwatch()..start();
  for (var second = 0; second < duration.inSeconds; second++) {
    final next = Duration(seconds: second + 1);
    while (clock.elapsed < next) {
      await tester.pump();
      await Future<void>.delayed(const Duration(milliseconds: 50));
    }
    memory.add(rssMegabytes());
    for (final MapEntry(:key, :value) in textures.entries) {
      final now = presented(value);
      seconds[key]!.add(now - last[key]!);
      last[key] = now;
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
    'rss_mb': rssMegabytes(),
    'rss_mb_each_second': memory,
    'busiest_threads': {
      for (final entry in busiest.take(10)) entry.key: round(entry.value),
    },
    'tiles': {
      for (final MapEntry(:key, :value) in textures.entries)
        key: <String, num>{
          'fps': round((last[key]! - firstPresented[key]!) / elapsed),
          'drawn_fps': round((drawn(value) - firstDrawn[key]!) / elapsed),
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

/// This process's resident memory.
int rssMegabytes() {
  final line = File(
    '/proc/self/status',
  ).readAsLinesSync().firstWhere((line) => line.startsWith('VmRSS:'));
  return int.parse(line.split(RegExp(r'\s+'))[1]) ~/ 1024;
}

/// This process's largest resident mappings, in megabytes, by name
/// (anonymous ones by their label, or "anon").
Map<String, int> largestMappings({int count = 12}) {
  final byName = <String, int>{};
  var name = 'anon';
  for (final line in File('/proc/self/smaps').readAsLinesSync()) {
    final fields = line.split(RegExp(r'\s+'));
    if (RegExp(r'^[0-9a-f]+-[0-9a-f]+$').hasMatch(fields.first)) {
      name = fields.length > 5 && fields[5].isNotEmpty
          ? fields.sublist(5).join(' ').split('/').last
          : 'anon';
    } else if (fields.first == 'Rss:') {
      byName[name] = (byName[name] ?? 0) + int.parse(fields[1]);
    }
  }
  final sorted = byName.entries.toList()
    ..sort((a, b) => b.value.compareTo(a.value));
  return {
    for (final entry in sorted.take(count)) entry.key: entry.value ~/ 1024,
  };
}
