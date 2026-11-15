import 'dart:ui' as ui;

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

const _shot = ValueKey('shot');
const _red = Color(0xFFFF0000);
const _blue = Color(0xFF0000FF);
const _green = Color(0xFF00FF00);

Map<String, Object?> _canvas(
  int id, {
  Map<String, Object?> props = const {'width': 200, 'height': 100},
  List<String> events = const [],
}) => create(id, 'canvas', props: props, events: events);

Map<String, Object?> _draw(
  int id, {
  List<Map<String, Object?>> set = const [],
  List<String> remove = const [],
  List<String>? order,
}) => {'op': 'draw', 'id': id, 'set': set, 'remove': remove, 'order': ?order};

Map<String, Object?> _rect(String id, double x, double y, String fill) => {
  'id': id,
  'type': 'rect',
  'x': x,
  'y': y,
  'w': 50,
  'h': 50,
  'fill': fill,
};

/// The view at the top left of the window, inside a boundary [_pixels]
/// reads.
Future<BlocksController> _show(
  WidgetTester tester,
  List<Map<String, Object?>> ops, {
  BlocksHost? host,
}) async {
  final controller = BlocksController();
  addTearDown(controller.dispose);
  controller.applyBatch(batch(1, ops));
  tester.view.physicalSize = const Size(400, 300);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    MaterialApp(
      home: RepaintBoundary(
        key: _shot,
        child: ColoredBox(
          color: darkTheme.colors['background']!,
          child: BlocksView(
            controller: controller,
            host: host ?? FakeHost(),
            theme: darkTheme,
          ),
        ),
      ),
    ),
  );
  return controller;
}

/// What's on screen now, read pixel by pixel.
Future<Color Function(double x, double y)> _pixels(WidgetTester tester) async {
  final boundary = tester.renderObject<RenderRepaintBoundary>(
    find.byKey(_shot),
  );
  final (width, bytes) = (await tester.runAsync(() async {
    final image = await boundary.toImage();
    final bytes = await image.toByteData();
    final width = image.width;
    image.dispose();
    return (width, bytes!);
  }))!;
  return (x, y) {
    final at = (y.toInt() * width + x.toInt()) * 4;
    return Color.fromARGB(
      bytes.getUint8(at + 3),
      bytes.getUint8(at),
      bytes.getUint8(at + 1),
      bytes.getUint8(at + 2),
    );
  };
}

/// The picture a canvas's items were last painted into; a repaint makes a
/// new one.
Layer? _itemsPicture(WidgetTester tester, int id) => tester
    .renderObject<RenderRepaintBoundary>(
      find
          .descendant(
            of: find.byKey(ValueKey<int>(id)),
            matching: find.byType(RepaintBoundary),
          )
          .first,
    )
    .debugLayer
    ?.firstChild;

/// A host whose images are ready at once: a 2 × 2 checker, blue and green.
class _ImageHost extends FakeHost {
  _ImageHost() {
    final recorder = ui.PictureRecorder();
    final canvas = Canvas(recorder);
    canvas.drawRect(const Rect.fromLTWH(0, 0, 2, 2), Paint()..color = _blue);
    canvas.drawRect(const Rect.fromLTWH(1, 0, 1, 1), Paint()..color = _green);
    canvas.drawRect(const Rect.fromLTWH(0, 1, 1, 1), Paint()..color = _green);
    final picture = recorder.endRecording();
    _image = picture.toImageSync(2, 2);
    picture.dispose();
  }

  late final ui.Image _image;
  final sources = <Map<String, Object?>>[];

  @override
  Future<ImageProvider?> image(Map<String, Object?> source) async {
    sources.add(source);
    return _ReadyImage(_image);
  }
}

class _ReadyImage extends ImageProvider<_ReadyImage> {
  const _ReadyImage(this.image);
  final ui.Image image;

  @override
  Future<_ReadyImage> obtainKey(ImageConfiguration configuration) =>
      SynchronousFuture(this);

  @override
  ImageStreamCompleter loadImage(
    _ReadyImage key,
    ImageDecoderCallback decode,
  ) => OneFrameImageStreamCompleter(
    SynchronousFuture(ImageInfo(image: image.clone())),
  );
}

void main() {
  // Fails if items draw in another order than the list's, if an `order`
  // isn't followed, or if a kind of item draws nothing or in the wrong
  // place: a line, a closed path, an ellipse, an image, a transform.
  testWidgets('items_draw_in_order', (tester) async {
    final host = _ImageHost();
    final controller = await _show(tester, host: host, [
      _canvas(1, props: {'width': 300, 'height': 200}),
      _draw(
        1,
        set: [
          _rect('a', 0, 0, '#FF0000'),
          _rect('b', 25, 25, '#0000FF'),
          {
            'id': 'line',
            'type': 'line',
            'x1': 100,
            'y1': 10,
            'x2': 190,
            'y2': 10,
            'stroke': {'color': '#00FF00', 'width': 4},
          },
          {
            'id': 'tri',
            'type': 'path',
            'points': [100, 100, 160, 100, 100, 160],
            'closed': true,
            'fill': '#FF0000',
          },
          {
            'id': 'oval',
            'type': 'ellipse',
            'x': 200,
            'y': 20,
            'w': 40,
            'h': 40,
            'fill': '#0000FF',
          },
          {
            'id': 'pic',
            'type': 'image',
            'x': 200,
            'y': 100,
            'w': 40,
            'h': 40,
            'asset': 'assets/a.png',
          },
          {
            ..._rect('moved', 0, 140, '#00FF00'),
            'transform': {
              'translate': {'x': 50, 'y': 0},
            },
          },
        ],
      ),
      roots(1),
    ]);
    await tester.pump();
    var pixel = await _pixels(tester);
    final background = darkTheme.colors['background'];
    expect(pixel(37, 37), _blue, reason: 'b is drawn after a');
    expect(pixel(10, 10), _red);
    expect(pixel(150, 10), _green, reason: 'the line');
    expect(pixel(110, 110), _red, reason: 'inside the triangle');
    expect(pixel(155, 155), background, reason: 'outside the triangle');
    expect(pixel(220, 40), _blue, reason: 'the ellipse middle');
    expect(pixel(201, 21), background, reason: 'outside the ellipse');
    expect(pixel(205, 105), _blue, reason: "the image's top left");
    expect(pixel(235, 105), _green, reason: "the image's top right");
    expect(host.sources, [
      {'asset': 'assets/a.png'},
    ]);
    expect(pixel(25, 165), background, reason: 'moved away');
    expect(pixel(75, 165), _green, reason: 'moved here');

    controller.applyBatch(
      batch(2, [
        _draw(1, order: ['b', 'a', 'line', 'tri', 'oval', 'pic', 'moved']),
      ]),
    );
    await tester.pump();
    pixel = await _pixels(tester);
    expect(pixel(37, 37), _red, reason: 'a is drawn after b now');
  });

  // Fails if a draw op on one canvas repaints another, if a canvas's
  // items repaint on a change that isn't a draw op (its painter's
  // `shouldRepaint`), or if the items share a layer with what's around.
  testWidgets('a_draw_op_repaints_only_its_canvas', (tester) async {
    final controller = await _show(tester, [
      _canvas(2),
      _canvas(3),
      create(1, 'column', props: {'gap': 10}, children: [2, 3]),
      _draw(2, set: [_rect('a', 0, 0, '#FF0000')]),
      _draw(3, set: [_rect('a', 0, 0, '#0000FF')]),
      roots(1),
    ]);
    await tester.pump();
    final two = _itemsPicture(tester, 2);
    final three = _itemsPicture(tester, 3);
    expect(two, isNotNull);
    expect(three, isNotNull);

    controller.applyBatch(
      batch(2, [
        _draw(2, set: [_rect('b', 50, 0, '#00FF00')]),
        // A new prop rebuilds canvas 3, but its items didn't change.
        {
          'op': 'props',
          'id': 3,
          'set': {
            'ink': {'color': 'accent', 'width': 2},
          },
          'unset': <String>[],
        },
      ]),
    );
    await tester.pump();
    expect(_itemsPicture(tester, 2), isNot(same(two)));
    expect(_itemsPicture(tester, 3), same(three));
  });

  // Fails if every move is sent (100 in a second), if the cap starves
  // moves well below 60, or if the last move before the pointer lifts is
  // dropped.
  testWidgets('pointer_moves_are_capped_at_60_a_second', (tester) async {
    final controller = await _show(tester, [
      _canvas(1, events: ['pointer_down', 'pointer_move', 'pointer_up']),
      roots(1),
    ]);
    final inputs = Inputs(controller);
    addTearDown(inputs.cancel);
    final gesture = await tester.startGesture(const Offset(10, 10));
    for (var i = 1; i <= 100; i++) {
      await gesture.moveTo(Offset(10 + i.toDouble(), 10));
      await tester.pump(const Duration(milliseconds: 10));
    }
    await gesture.moveTo(const Offset(150, 20));
    await gesture.up();
    await tester.pump();

    final events = [for (final input in inputs.of('event')) input['event']];
    final moves = events.where((event) => event == 'pointer_move').length;
    expect(moves, inInclusiveRange(55, 61));
    expect(events.first, 'pointer_down');
    expect(events.last, 'pointer_up');
    final lastMove = inputs
        .of('event')
        .lastWhere((input) => input['event'] == 'pointer_move');
    expect((lastMove['data']! as Map)['x'], 150.0);
    expect((lastMove['data']! as Map)['y'], 20.0);
  });

  // Fails if pointer events are sent without their handler, if their data
  // lacks a field or names the kind wrongly, if a pressure the device
  // doesn't report is made up, or if wheel and pan don't come through.
  testWidgets('pointer_wheel_and_pan_events_carry_their_data', (tester) async {
    final controller = await _show(tester, [
      _canvas(1, events: ['pointer_up', 'wheel', 'pan']),
      roots(1),
    ]);
    final inputs = Inputs(controller);
    addTearDown(inputs.cancel);

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(20, 30));
    await mouse.down(const Offset(20, 30));
    await mouse.moveTo(const Offset(80, 30));
    await mouse.up();
    await tester.sendEventToBinding(
      const PointerScrollEvent(
        position: Offset(40, 40),
        scrollDelta: Offset(0, 12),
      ),
    );
    await tester.pump();
    List<Object?> data(String name) => [
      for (final input in inputs.of('event'))
        if (input['event'] == name) input['data'],
    ];
    expect(
      inputs.of('event').map((input) => input['event']).toSet(),
      {'pan', 'pointer_up', 'wheel'},
      reason: 'no pointer_down or pointer_move: no handler for them',
    );
    final pans = data('pan').cast<Map<String, Object?>>();
    expect(pans.first, {
      'x': 20.0,
      'y': 30.0,
      'dx': 0.0,
      'dy': 0.0,
      'phase': 'start',
    });
    expect(pans.where((pan) => pan['phase'] == 'update'), [
      {'x': 80.0, 'y': 30.0, 'dx': 60.0, 'dy': 0.0, 'phase': 'update'},
    ]);
    expect(pans.last['phase'], 'end');
    expect(data('pointer_up'), [
      {
        'x': 80.0,
        'y': 30.0,
        'pointer': isA<int>(),
        'kind': 'mouse',
        'buttons': 0,
      },
    ]);
    expect(data('wheel'), [
      {'x': 40.0, 'y': 40.0, 'dx': 0.0, 'dy': 12.0},
    ]);
    await mouse.removePointer();
  });

  // Fails if ink isn't drawn under the pointer, if points go one by one
  // instead of 64 at a time or every 50 ms, if the end isn't marked, if
  // the local stroke stays over the item the plugin set with its id, or
  // if an unanswered stroke stays longer than 2 s.
  testWidgets('ink_draws_locally_and_sends_batched_points', (tester) async {
    final controller = await _show(tester, [
      _canvas(
        1,
        props: {
          'width': 200,
          'height': 100,
          'ink': {'color': '#FF0000', 'width': 6},
        },
        events: ['stroke'],
      ),
      roots(1),
    ]);
    final inputs = Inputs(controller);
    addTearDown(inputs.cancel);
    List<Map<String, Object?>> strokes() => [
      for (final input in inputs.of('event'))
        input['data']! as Map<String, Object?>,
    ];

    final gesture = await tester.startGesture(const Offset(10, 50));
    for (var x = 11; x <= 73; x++) {
      await gesture.moveTo(Offset(x.toDouble(), 50));
    }
    expect(strokes(), hasLength(1), reason: '64 points went at once');
    final first = strokes().single;
    final id = first['stroke']! as String;
    expect(first['done'], isFalse);
    final points = first['points']! as List<Object?>;
    expect(points, hasLength(64 * 3));
    // Touch in tests reports no pressure: 0.5 stands in for it.
    expect(points.take(6), [10.0, 50.0, 0.5, 11.0, 50.0, 0.5]);

    for (var x = 74; x <= 80; x++) {
      await gesture.moveTo(Offset(x.toDouble(), 50));
    }
    expect(strokes(), hasLength(1));
    await tester.pump(const Duration(milliseconds: 50));
    expect(strokes(), hasLength(2));
    expect(strokes()[1]['points'], hasLength(7 * 3));
    var pixel = await _pixels(tester);
    expect(pixel(40, 50), _red, reason: 'drawn while the pointer moves');

    await gesture.up();
    await tester.pump();
    expect(strokes()[2], {'stroke': id, 'points': <Object?>[], 'done': true});
    pixel = await _pixels(tester);
    expect(pixel(40, 50), _red, reason: 'kept until the plugin draws it');

    controller.applyBatch(
      batch(2, [
        _draw(
          1,
          set: [
            {
              'id': id,
              'type': 'path',
              'points': [10, 50, 80, 50],
              'stroke': {'color': '#0000FF', 'width': 6},
            },
          ],
        ),
      ]),
    );
    await tester.pump();
    pixel = await _pixels(tester);
    expect(pixel(40, 50), _blue, reason: "the plugin's item, not the ink");

    final second = await tester.startGesture(const Offset(10, 80));
    await second.moveTo(const Offset(80, 80));
    await second.up();
    await tester.pump(const Duration(milliseconds: 1900));
    pixel = await _pixels(tester);
    expect(pixel(40, 80), _red);
    await tester.pump(const Duration(milliseconds: 200));
    pixel = await _pixels(tester);
    expect(pixel(40, 80), darkTheme.colors['background']);
  });

  // Fails if a frame goes every vsync while the core hasn't answered the
  // last, if a frame names the wrong batch or the wrong time since the
  // last, if a frame that brought no batch stops the loop, or if frames
  // go on after they're turned off.
  testWidgets('frames_wait_for_the_previous_batch', (tester) async {
    final controller = BlocksController();
    addTearDown(controller.dispose);
    final inputs = Inputs(controller);
    addTearDown(inputs.cancel);
    controller.applyBatch(
      batch(1, [
        _canvas(1),
        roots(1),
        {'op': 'frames', 'on': true},
      ]),
    );
    await pumpView(tester, controller);
    await tester.pump(const Duration(milliseconds: 16));
    List<Object?> frames() => [
      for (final frame in inputs.of('frame')) (frame['seq'], frame['dt']),
    ];
    expect(frames(), [(1, 0.0)]);

    await tester.pump(const Duration(milliseconds: 16));
    await tester.pump(const Duration(milliseconds: 16));
    expect(frames(), hasLength(1), reason: 'batch 1 is still the newest');

    controller.applyBatch(
      batch(2, [
        _draw(1, set: [_rect('a', 0, 0, '#FF0000')]),
      ]),
    );
    await tester.pump(const Duration(milliseconds: 16));
    expect(frames(), [(1, 0.0), (2, 0.048)]);

    // The script drew nothing for that frame, so no batch comes.
    await tester.pump(const Duration(milliseconds: 16));
    await tester.pump(const Duration(milliseconds: 16));
    await tester.pump(const Duration(milliseconds: 16));
    expect(frames(), hasLength(2), reason: '48 ms is within the patience');
    await tester.pump(const Duration(milliseconds: 16));
    expect(frames(), [(1, 0.0), (2, 0.048), (2, 0.064)]);

    controller.applyBatch(
      batch(3, [
        {'op': 'frames', 'on': false},
      ]),
    );
    for (var i = 0; i < 10; i++) {
      await tester.pump(const Duration(milliseconds: 16));
    }
    expect(frames(), hasLength(3));
  });

  // Fails if an item the renderer can't draw stops the others, or is
  // reported more than once, or reported with its content.
  testWidgets('an_item_it_cant_draw_is_left_out_and_reported_once', (
    tester,
  ) async {
    final reported = <FlutterErrorDetails>[];
    final previous = FlutterError.onError;
    FlutterError.onError = reported.add;
    try {
      await _show(tester, [
        _canvas(1),
        _draw(
          1,
          set: [
            {'id': 'x', 'type': 'star', 'secret': 'secret'},
            {'id': 'y', 'type': 'rect', 'x': 0, 'y': 0, 'w': 'wide'},
            _rect('a', 0, 0, '#FF0000'),
          ],
        ),
        roots(1),
      ]);
      await tester.pump();
      final pixel = await _pixels(tester);
      expect(pixel(10, 10), _red);
    } finally {
      FlutterError.onError = previous;
    }
    expect(reported, hasLength(1));
    expect(
      reported.single.exceptionAsString(),
      'Canvas item: items[0]: unknown type',
    );
  });

  for (final (name, theme) in themes) {
    for (final width in [800.0, 360.0]) {
      // Fails if any kind of item draws differently: a colour, a gradient,
      // a stroke, a radius, an opacity, a transform, text, an image.
      testWidgets('the_canvas_${name}_${width.toInt()}', (tester) async {
        final controller = BlocksController();
        addTearDown(controller.dispose);
        controller.applyBatch(batch(1, _goldenPage()));
        await pumpView(
          tester,
          controller,
          host: _ImageHost(),
          theme: theme,
          width: width,
          height: 280,
          window: Size(width, 280),
        );
        await tester.pump();
        await expectLater(
          find.byType(BlocksView),
          matchesGoldenFile('goldens/canvas_${name}_${width.toInt()}.png'),
        );
      });
    }
  }
}

List<Map<String, Object?>> _goldenPage() => [
  _canvas(
    2,
    props: {
      'width': 'fill',
      'height': 248,
      'ink': {'color': 'accent', 'width': 3},
    },
  ),
  _draw(
    2,
    set: [
      for (var x = 0; x <= 320; x += 40)
        {
          'id': 'v$x',
          'type': 'line',
          'x1': x,
          'y1': 0,
          'x2': x,
          'y2': 248,
          'stroke': {'color': 'border', 'width': 1},
        },
      {
        'id': 'card',
        'type': 'rect',
        'x': 16,
        'y': 16,
        'w': 140,
        'h': 88,
        'radius': 12,
        'fill': 'surface',
        'stroke': {'color': 'border', 'width': 1},
      },
      {
        'id': 'title',
        'type': 'text',
        'x': 32,
        'y': 30,
        'text': 'Canvas',
        'size': 18,
      },
      {
        'id': 'caption',
        'type': 'text',
        'x': 32,
        'y': 58,
        'text': 'Items, in order',
        'color': 'text_secondary',
      },
      {
        'id': 'orb',
        'type': 'ellipse',
        'x': 180,
        'y': 16,
        'w': 88,
        'h': 88,
        'fill': {'from': 'accent', 'to': '#4C8DFF', 'angle': 135},
      },
      {
        'id': 'tri',
        'type': 'path',
        'points': [16, 228, 76, 128, 136, 228],
        'closed': true,
        'fill': 'danger',
        'opacity': 0.8,
      },
      {
        'id': 'zig',
        'type': 'path',
        'points': [150, 220, 180, 140, 210, 220, 240, 140, 270, 220],
        'stroke': {'color': 'accent', 'width': 3},
      },
      {
        'id': 'tilt',
        'type': 'rect',
        'x': 282,
        'y': 34,
        'w': 32,
        'h': 32,
        'radius': 6,
        'fill': '#4C8DFF',
        'opacity': 0.6,
        'transform': {'rotate': 20, 'scale': 1.1},
      },
      {
        'id': 'pic',
        'type': 'image',
        'x': 286,
        'y': 150,
        'w': 40,
        'h': 40,
        'asset': 'assets/checker.png',
      },
    ],
  ),
  create(
    1,
    'column',
    props: {
      'padding': 16,
      'width': 'fill',
      'height': 'fill',
      'background': 'background',
    },
    children: [2],
  ),
  roots(1),
];
