import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/plugins.dart';
import 'package:opencord/core/providers/providers.dart';

import 'app.dart';

/// The mock server every activity test runs on.
const activityServer = 'opencord.example:7710';

/// What the mock's `paint` view heard, input by input, as the core would
/// hand them to the plugin's client script.
class PaintRecorder {
  final inputs = <Map<String, Object?>>[];

  /// `pause` and `resume`, in order; `focus` and `blur` are left out.
  List<String> get pauses => [
    for (final input in inputs)
      if (input['kind'] == 'lifecycle' &&
          (input['event'] == 'pause' || input['event'] == 'resume'))
        input['event']! as String,
  ];

  /// Every `call` input, in order.
  List<Map<String, Object?>> get calls => [
    for (final input in inputs)
      if (input['kind'] == 'call') input,
  ];
}

/// Registers the mock's `paint` script: it draws the counter the mock's
/// default view draws (a `Count: n` text and a `Count` button that adds
/// one) and records every input it gets.
PaintRecorder recordPaint(MockApp app) {
  final recorder = PaintRecorder();
  app.repository.registerMockView('paint', (viewId) {
    var count = 0;
    var seq = 0;
    return (json) {
      final input = (jsonDecode(json) as Map).cast<String, Object?>();
      recorder.inputs.add(input);
      switch (input) {
        case {'kind': 'init'}:
          seq++;
          return jsonEncode({
            'view': viewId,
            'seq': seq,
            'ops': [
              {
                'op': 'create',
                'id': 2,
                'block': 'text',
                'props': {'text': 'Count: $count'},
                'events': <String>[],
                'children': <int>[],
              },
              {
                'op': 'create',
                'id': 3,
                'block': 'button',
                'props': {'text': 'Count'},
                'events': ['press'],
                'children': <int>[],
              },
              {
                'op': 'create',
                'id': 1,
                'block': 'column',
                'props': <String, Object?>{},
                'events': <String>[],
                'children': [2, 3],
              },
              {'op': 'roots', 'main': 1, 'modals': <Object?>[]},
            ],
          });
        case {'kind': 'event', 'event': 'press'}:
          count++;
          seq++;
          return jsonEncode({
            'view': viewId,
            'seq': seq,
            'ops': [
              {
                'op': 'props',
                'id': 2,
                'set': {'text': 'Count: $count'},
                'unset': <String>[],
              },
            ],
          });
        default:
          return null;
      }
    };
  });
  return recorder;
}

/// The call this device is in.
({String server, int channel}) callOf(MockApp app) {
  final voice = app.read(voiceSessionProvider);
  return (server: voice.serverKey!, channel: voice.channelId!);
}

/// Starts the mock's registered `paint` activity in this device's call, as
/// the picker does, and lets its started event reach the providers.
Future<ActivitySession> startPaint(WidgetTester tester, MockApp app) async {
  final call = callOf(app);
  final session = await app.repository.startActivity(
    call.server,
    call.channel,
    'paint',
    'paint',
  );
  await tester.pump();
  return session;
}
