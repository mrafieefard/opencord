import 'dart:developer' as developer;

import 'package:opencord/src/rust/api/logs.dart' show LogLevel;

/// Where [appLog] writes besides the debugger: the session log once it
/// has started (D82), or a test's list.
typedef AppLogSink = void Function(LogLevel level, String area, String message);

AppLogSink? appLogSink;

/// Marks the session log as ended cleanly; set once it has started.
void Function()? endSessionLog;

/// Stack frames kept in a line.
const _frames = 40;

/// Logs [message] under [name] to the debugger and the session log, with
/// [error] and the top of [stackTrace] on the lines after it. The core
/// replaces addresses, servers and secrets before anything reaches disk.
void appLog(
  String message, {
  required String name,
  Object? error,
  StackTrace? stackTrace,
  LogLevel level = LogLevel.info,
}) {
  developer.log(message, name: name, error: error, stackTrace: stackTrace);
  final sink = appLogSink;
  if (sink == null) return;
  final text = StringBuffer(message);
  if (error != null) text.write('\n$error');
  if (stackTrace != null) {
    final frames = '$stackTrace'.trimRight().split('\n').take(_frames);
    text.write('\n${frames.join('\n')}');
  }
  sink(level, name, '$text');
}
