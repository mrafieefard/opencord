import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_plugin_area/src/area.dart';
import 'package:opencord_plugin_area/src/channel.dart';

import 'support.dart';

const _codec = StandardMethodCodec();

/// The native end of `dev.opencord/plugin_area`: records what Dart calls,
/// and sends what native code sends.
class _Native {
  _Native(this.tester) {
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      const MethodChannel(AreaChannel.name),
      (call) async {
        calls.add(call);
        return null;
      },
    );
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        const MethodChannel(AreaChannel.name),
        null,
      ),
    );
  }

  final WidgetTester tester;
  final calls = <MethodCall>[];

  Future<void> send(String method, [Object? arguments]) async {
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      AreaChannel.name,
      _codec.encodeMethodCall(MethodCall(method, arguments)),
      (_) {},
    );
    await tester.pump();
  }

  Future<void> setup() => send('setup', {'json': setupJson()});

  Future<void> batch(String json) => send('batch', {'json': json});

  List<String> named(String method) => [
    for (final call in calls)
      if (call.method == method) (call.arguments as Map)['json'] as String,
  ];

  List<Map<String, Object?>> statuses() => [
    for (final json in named('status')) jsonDecode(json) as Map<String, Object?>,
  ];

  List<Map<String, Object?>> inputs(String kind) => [
    for (final json in named('input'))
      if ((jsonDecode(json) as Map)['kind'] == kind)
        jsonDecode(json) as Map<String, Object?>,
  ];
}

Future<_Native> _start(WidgetTester tester) async {
  final native = _Native(tester);
  await tester.pumpWidget(PluginAreaApp(channel: AreaChannel()));
  await tester.pump();
  return native;
}

final _hello = batch(1, [
  create(2, 'text', props: {'text': 'Hello from the plugin'}),
  create(3, 'button', props: {'text': 'Claim'}, events: ['press']),
  create(1, 'column', children: [2, 3]),
  roots(1),
]);

void main() {
  // Fails if the area draws before it knows the theme (a flash of the
  // wrong colours), or drops a batch that came before setup.
  testWidgets('nothing is drawn before setup', (tester) async {
    final native = await _start(tester);
    await native.batch(_hello);
    expect(find.text('Hello from the plugin'), findsNothing);
    await native.setup();
    await tester.pump();
    expect(find.text('Hello from the plugin'), findsOneWidget);
  });

  // Fails if a batch isn't applied to the renderer.
  testWidgets('a batch after setup draws its text', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await tester.pump();
    expect(find.text('Hello from the plugin'), findsOneWidget);
  });

  // Fails if the renderer's inputs don't reach native code, or change on
  // the way.
  testWidgets('pressing the button sends its input', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await tester.pump();
    await tester.tap(find.text('Claim'));
    await tester.pump();
    expect(native.inputs('event'), [
      {'kind': 'event', 'node': 3, 'event': 'press', 'data': <String, Object?>{}},
    ]);
    // The renderer's own context input, sent once it has a size.
    expect(native.inputs('context'), hasLength(1));
  });

  // Fails if React Native never hears the view is running, or hears it
  // twice.
  testWidgets('the status goes loading, then running', (tester) async {
    final native = await _start(tester);
    await native.setup();
    expect(native.statuses().map((s) => s['state']), ['loading']);
    await native.batch(_hello);
    await tester.pump();
    expect(native.statuses().map((s) => s['state']), ['loading', 'running']);
  });

  // Fails if a closed op isn't reported with its reason.
  testWidgets('a closed op reports closed with its reason', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await native.batch(batch(2, [{'op': 'closed', 'reason': 'plugin_stopped'}]));
    await tester.pump();
    expect(native.statuses().last, {
      'state': 'closed',
      'reason': 'plugin_stopped',
      'detail': '',
      'overlays': 0,
    });
  });

  // Fails if an error op isn't reported, or its details are lost.
  testWidgets('an error op reports error', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await native.batch(
      batch(2, [
        {'op': 'error', 'message': 'This plugin hit an error', 'details': 'boom at line 3'},
      ]),
    );
    await tester.pump();
    expect(native.statuses().last['state'], 'error');
    expect(native.statuses().last['detail'], 'boom at line 3');
  });

  // Fails if Dart says ready more than once (native code would send held
  // batches twice) or never.
  testWidgets('ready is sent once', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await tester.pump();
    expect(native.calls.where((c) => c.method == 'ready'), hasLength(1));
  });
}
