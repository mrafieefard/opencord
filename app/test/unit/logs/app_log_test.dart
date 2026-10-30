import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/logs/app_log.dart';
import 'package:opencord/core/logs/session_log.dart';
import 'package:opencord/src/rust/api/logs.dart' show LogLevel;

void main() {
  final lines = <(LogLevel, String, String)>[];

  setUp(() {
    lines.clear();
    appLogSink = (level, area, message) => lines.add((level, area, message));
  });

  tearDown(() => appLogSink = null);

  test('appLog forwards to the sink', () {
    appLog('picked a source', name: 'share');
    expect(lines.single, (LogLevel.info, 'share', 'picked a source'));
  });

  test('appLog adds the error and the top of the stack', () {
    final stack = StackTrace.fromString(
      List.generate(60, (index) => '#$index frame$index').join('\n'),
    );
    appLog(
      'could not start',
      name: 'start',
      error: StateError('broken'),
      stackTrace: stack,
      level: LogLevel.error,
    );
    final (level, area, message) = lines.single;
    expect(level, LogLevel.error);
    expect(area, 'start');
    expect(
      message,
      startsWith('could not start\nBad state: broken\n#0 frame0'),
    );
    expect(message, contains('#39 frame39'));
    expect(message, isNot(contains('#40 frame40')));
  });

  test('nothing is sent without a sink', () {
    appLogSink = null;
    appLog('quiet', name: 'test');
    expect(lines, isEmpty);
  });

  test('the export name comes from the session file', () {
    expect(
      exportName('/x/logs/opencord-2026-10-30T14-32-05.log'),
      'opencord-log-2026-10-30-1432.txt',
    );
    expect(
      exportName(r'C:\x\logs\opencord-2026-10-30T09-05-00-2.log'),
      'opencord-log-2026-10-30-0905.txt',
    );
    expect(exportName('/x/odd.log'), 'opencord-log.txt');
  });

  test('a failure gathering the facts never stops the app', () async {
    final log = await startSessionLog(
      dir: () async => '/x/logs',
      facts: () async => throw const FormatException('update.json is broken'),
    );
    expect(log.current, isNull);
    final (level, area, message) = lines.single;
    expect((level, area), (LogLevel.warn, 'log'));
    expect(message, contains('update.json is broken'));
  });
}
