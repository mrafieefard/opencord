import 'dart:convert';

import 'dart:io';

import 'package:flutter/material.dart';
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

  Future<void> setup({String theme = 'dark'}) => send('setup', {'json': setupJson(theme: theme)});

  /// Native code asks Dart what back does.
  Future<bool> back() async {
    Object? answer;
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      AreaChannel.name,
      _codec.encodeMethodCall(const MethodCall('back')),
      (data) => answer = data == null ? null : _codec.decodeEnvelope(data),
    );
    await tester.pump();
    return answer == true;
  }

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

  Map<String, Object?> modal(String name, {bool sheet = false, bool dismissible = true, int id = 20}) => {
    'id': id,
    'name': name,
    'size': 'medium',
    // Left out unless set: at a phone's width the renderer then shows a sheet.
    if (sheet) 'sheet': true,
    'dismissible': dismissible,
  };

  String withModals(int seq, List<Map<String, Object?>> modals, {bool nodes = true}) => batch(seq, [
    if (nodes) ...[
      create(21, 'text', props: {'text': 'In the sheet'}),
      create(20, 'column', children: [21]),
      create(31, 'text', props: {'text': 'In the modal'}),
      create(30, 'column', children: [31]),
    ],
    {'op': 'roots', 'main': 1, 'modals': modals},
  ]);

  // Fails if back closes the wrong overlay (the sheet under the modal), or
  // leaves the screen while one is open.
  testWidgets('back closes the top overlay first', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await native.batch(withModals(2, [modal('filters', sheet: true), modal('edit', id: 30)]));
    await tester.pump();
    expect(await native.back(), isTrue);
    expect(native.inputs('modal_dismiss').last, {'kind': 'modal_dismiss', 'name': 'edit'});
    // The core closes it: the roots come back without it.
    await native.batch(withModals(3, [modal('filters', sheet: true)], nodes: false));
    expect(await native.back(), isTrue);
    expect(native.inputs('modal_dismiss').last, {'kind': 'modal_dismiss', 'name': 'filters'});
    await native.batch(withModals(4, [], nodes: false));
    expect(await native.back(), isFalse);
    expect(native.inputs('modal_dismiss'), hasLength(2));
  });

  // Fails if back dismisses a modal the plugin made undismissable.
  testWidgets('a modal that isn\'t dismissible keeps back', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await native.batch(withModals(2, [modal('locked', dismissible: false, id: 30)]));
    expect(await native.back(), isFalse);
    expect(native.inputs('modal_dismiss'), isEmpty);
    expect(find.text('In the modal'), findsOneWidget);
  });

  // Fails if back doesn't answer a waiting dialog first (confirm: no).
  testWidgets('back answers a dialog before a modal', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await native.batch(withModals(2, [modal('edit', id: 30)]));
    await native.batch(batch(3, [
      {'op': 'dialog', 'id': 7, 'kind': 'confirm', 'text': 'Delete it?'},
    ]));
    expect(await native.back(), isTrue);
    expect(native.inputs('dialog'), [
      {'kind': 'dialog', 'id': 7, 'result': false},
    ]);
    expect(native.inputs('modal_dismiss'), isEmpty);
  });

  // Fails if React Native isn't told how many overlays are open (it decides
  // what its own back does from this).
  testWidgets('the status counts overlays', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await native.batch(withModals(2, [modal('filters', sheet: true), modal('edit', id: 30)]));
    await tester.pump();
    expect(native.statuses().last['overlays'], 2);
  });

  // Fails if a theme change rebuilds the view as new (a second context
  // input, state lost) or doesn't recolour it.
  testWidgets('a new setup recolours without a new context input', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await tester.pump();
    final before = native.inputs('context').length;
    await native.setup(theme: 'light');
    await tester.pump();
    await tester.pump();
    final contexts = native.inputs('context');
    expect(contexts.length, before + 1, reason: 'the theme is part of the context');
    expect(contexts.last['theme'], 'light');
    expect(find.text('Hello from the plugin'), findsOneWidget);
  });

  // Fails if a resize sends no context input, or one per frame.
  testWidgets('a new size sends one context input', (tester) async {
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await tester.pump();
    final before = native.inputs('context').length;
    tester.view.physicalSize = const Size(820, 1180);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.pump();
    await tester.pump();
    await tester.pump();
    final contexts = native.inputs('context');
    expect(contexts.length, before + 1);
    expect(contexts.last['width'], 820);
  });

  // Fails if a modal at a phone's width shows as a centred card instead of
  // a sheet along the bottom (§12.5).
  testWidgets('a modal is a sheet at 390 wide', (tester) async {
    tester.view.physicalSize = const Size(390, 760);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final native = await _start(tester);
    await native.setup();
    await native.batch(_hello);
    await native.batch(withModals(2, [modal('edit', id: 30)]));
    await tester.pumpAndSettle();
    final card = tester.getRect(find.byKey(const ValueKey<String>('modal_card_edit')));
    expect(card.width, 390);
    expect(card.bottom, 760);
  });

  // The recorded tickets session (fixtures from the real core, Task 6) at a
  // phone's and a tablet's size, dark and light.
  for (final (width, height) in [(390.0, 760.0), (820.0, 1180.0)]) {
    for (final theme in ['dark', 'light']) {
      testWidgets('tickets at ${width.toInt()} $theme', (tester) async {
        tester.view.physicalSize = Size(width, height);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.reset);
        final native = await _start(tester);
        await native.setup(theme: theme);
        for (final step in [1, 2]) {
          final recorded = jsonDecode(
            File('../src/core/fake/fixtures/tickets-$step.json').readAsStringSync(),
          ) as Map<String, Object?>;
          for (final batch in recorded['batches']! as List<Object?>) {
            await native.batch(jsonEncode(batch));
          }
        }
        await tester.pumpAndSettle();
        await expectLater(
          find.byType(PluginAreaApp),
          matchesGoldenFile('goldens/tickets_${width.toInt()}_$theme.png'),
        );
      });
    }
  }
}

