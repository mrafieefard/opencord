import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:url_launcher/url_launcher.dart';

import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/src/rust/api/logs.dart';

/// This launch's session log (D82, docs/logs.md), once started: what
/// Settings → About exports.
@immutable
class SessionLog {
  const SessionLog({this.current, this.previousCrashed, this.dir});

  /// This session's file.
  final String? current;

  /// The last session's file, when it ended unexpectedly.
  final String? previousCrashed;

  /// The logs folder.
  final String? dir;
}

/// Empty until the app starts the log (desktop only).
final sessionLogProvider = Provider<SessionLog>((ref) => const SessionLog());

/// What the About page does with a log; tests replace it.
class SessionLogActions {
  const SessionLogActions();

  Future<void> export(String source, String destination) =>
      sessionLogExport(source: source, destination: destination);

  Future<void> openFolder(String dir) async {
    if (!await launchUrl(Uri.file(dir))) {
      throw Exception("The logs folder didn't open");
    }
  }
}

final sessionLogActionsProvider = Provider<SessionLogActions>(
  (ref) => const SessionLogActions(),
);

/// `…/opencord-2026-10-30T14-32-05.log` → `opencord-log-2026-10-30-1432.txt`.
String exportName(String logPath) {
  final name = logPath.split(RegExp(r'[/\\]')).last;
  final match = RegExp(
    r'^opencord-(\d{4}-\d{2}-\d{2})T(\d{2})-(\d{2})',
  ).firstMatch(name);
  if (match == null) return 'opencord-log.txt';
  return 'opencord-log-${match[1]}-${match[2]}${match[3]}.txt';
}

/// Starts the session log in [dir] with [facts] at the top, and sends
/// [appLog] lines and uncaught errors to it from now on. Without it (no
/// writable folder), the app carries on and logs only to the debugger.
SessionLog startSessionLog(String dir, List<LogFact> facts) {
  final SessionLogFiles files;
  try {
    files = sessionLogStart(dir: dir, facts: facts);
  } on Object catch (error) {
    appLog('no session log', name: 'log', error: error, level: LogLevel.warn);
    return const SessionLog();
  }
  appLogSink = (level, area, message) =>
      sessionLogWrite(level: level, area: area, message: message);
  endSessionLog = sessionLogEnd;

  final flutterErrors = FlutterError.onError;
  FlutterError.onError = (details) {
    appLog(
      details.exceptionAsString(),
      name: 'flutter',
      error: details.context?.toDescription(),
      stackTrace: details.stack,
      level: LogLevel.error,
    );
    flutterErrors?.call(details);
  };
  final platformErrors = PlatformDispatcher.instance.onError;
  PlatformDispatcher.instance.onError = (error, stack) {
    appLog(
      'uncaught',
      name: 'dart',
      error: error,
      stackTrace: stack,
      level: LogLevel.error,
    );
    return platformErrors?.call(error, stack) ?? false;
  };
  return SessionLog(
    current: files.current,
    previousCrashed: files.previousCrashed,
    dir: dir,
  );
}

/// The displays as the engine reports them, one fact each.
List<LogFact> displayFacts() {
  final displays = PlatformDispatcher.instance.displays.toList();
  if (displays.isEmpty) {
    return const [LogFact(key: 'displays', value: 'none reported')];
  }
  return [
    for (final (index, display) in displays.indexed)
      LogFact(
        key: 'display ${index + 1}',
        value:
            '${display.size.width.round()}x${display.size.height.round()}, '
            'scale ${display.devicePixelRatio}, '
            '${display.refreshRate.round()} Hz',
      ),
  ];
}

/// The last [count] lines of the launcher's log, when there is one.
String? launcherLogTail(String path, {int count = 40}) {
  final file = File(path);
  if (!file.existsSync()) return null;
  try {
    final lines = file.readAsLinesSync();
    return lines
        .skip(lines.length > count ? lines.length - count : 0)
        .join('\n');
  } on FileSystemException {
    return null;
  }
}
