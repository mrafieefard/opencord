import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_plugin_area/src/area.dart';
import 'package:opencord_plugin_area/src/channel.dart';

import 'support.dart';

const _codec = StandardMethodCodec();

void main() {
  // Each control is tapped 18 points below its centre: outside what it
  // draws (a 22-point checkbox, a 34-point tab, a dense button), inside the
  // 44-point target a finger needs. Fails if the press is lost.
  testWidgets('controls take a 44-point touch on the phone', (tester) async {
    tester.view.physicalSize = const Size(390, 760);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final inputs = <Map<String, Object?>>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(const MethodChannel(AreaChannel.name), (call) async {
      if (call.method == 'input') {
        inputs.add(jsonDecode((call.arguments as Map)['json'] as String) as Map<String, Object?>);
      }
      return null;
    });
    addTearDown(() => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(const MethodChannel(AreaChannel.name), null));
    await tester.pumpWidget(PluginAreaApp(channel: AreaChannel()));
    Future<void> send(String method, String json) => tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      AreaChannel.name,
      _codec.encodeMethodCall(MethodCall(method, {'json': json})),
      (_) {},
    );
    await send('setup', setupJson());
    await send(
      'batch',
      batch(1, [
        create(2, 'checkbox', props: {'text': 'Subscribed', 'value': false}, events: ['change']),
        create(3, 'segmented', props: {
          'value': 'a',
          'options': [
            {'value': 'a', 'label': 'Low'},
            {'value': 'b', 'label': 'High'},
          ],
        }, events: ['change']),
        create(4, 'button', props: {'text': 'Claim', 'size': 'small'}, events: ['press']),
        create(1, 'column', props: {'gap': 40, 'padding': 40}, children: [2, 3, 4]),
        roots(1),
      ]),
    );
    await tester.pumpAndSettle();

    Future<void> below(Finder finder) async {
      final center = tester.getCenter(finder);
      await tester.tapAt(center + const Offset(0, 18));
      await tester.pump();
    }

    await below(find.text('Subscribed'));
    await below(find.text('High'));
    await below(find.text('Claim'));
    final events = [for (final input in inputs) if (input['kind'] == 'event') (input['node'], input['event'])];
    expect(events, [(2, 'change'), (3, 'change'), (4, 'press')]);
  });
}
