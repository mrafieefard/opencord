import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

BlocksController _show(List<Map<String, Object?>> creates, int root) {
  final controller = BlocksController();
  addTearDown(controller.dispose);
  controller.applyBatch(batch(1, [...creates, roots(root)]));
  return controller;
}

Map<String, Object?> _box(
  int id, {
  Object? width = 50,
  Object? height = 20,
  Map<String, Object?> props = const {},
  List<String> events = const [],
  List<int> children = const [],
}) => create(
  id,
  'box',
  props: {'width': ?width, 'height': ?height, ...props},
  events: events,
  children: children,
);

List<double> _lefts(WidgetTester tester, List<int> ids) => [
  for (final id in ids) blockRect(tester, id).left,
];

void main() {
  // Fails if a box listens for presses it has no handler for, if the event
  // isn't named without `on_`, if a secondary press doesn't carry where it
  // landed inside the box, or if hovering sends nothing.
  testWidgets('a_press_sends_its_event_input', (tester) async {
    final controller = _show([
      _box(
        2,
        width: 100,
        height: 40,
        events: ['press', 'secondary_press', 'hover_enter', 'hover_exit'],
      ),
      _box(3, width: 100, height: 40),
      create(1, 'column', props: {'gap': 10}, children: [2, 3]),
    ], 1);
    final inputs = Inputs(controller);
    addTearDown(inputs.cancel);
    await pumpView(tester, controller);

    // Nothing listens on a box without handlers, so the tap meets nothing.
    await tester.tap(find.byKey(const ValueKey<int>(3)), warnIfMissed: false);
    await tester.tap(find.byKey(const ValueKey<int>(2)));
    await tester.pump();
    expect(inputs.of('event'), [
      {
        'kind': 'event',
        'node': 2,
        'event': 'press',
        'data': <String, Object?>{},
      },
    ]);

    inputs.all.clear();
    final mouse = await tester.createGesture(
      kind: PointerDeviceKind.mouse,
      buttons: kSecondaryMouseButton,
    );
    await mouse.addPointer(location: const Offset(700, 500));
    await mouse.moveTo(const Offset(10, 5));
    await mouse.down(const Offset(10, 5));
    await mouse.up();
    await mouse.moveTo(const Offset(700, 500));
    await tester.pump();
    expect(inputs.of('event'), [
      {
        'kind': 'event',
        'node': 2,
        'event': 'hover_enter',
        'data': <String, Object?>{},
      },
      {
        'kind': 'event',
        'node': 2,
        'event': 'secondary_press',
        'data': {'x': 10.0, 'y': 5.0},
      },
      {
        'kind': 'event',
        'node': 2,
        'event': 'hover_exit',
        'data': <String, Object?>{},
      },
    ]);
    await mouse.removePointer();
  });

  // Fails if `gap` isn't put between children, if `align` or `cross` map to
  // the wrong side, or if `wrap` doesn't start a new line with the gap.
  testWidgets('row_gap_align_and_wrap', (tester) async {
    Future<void> row(Map<String, Object?> props, {int count = 3}) async {
      final controller = _show([
        for (var i = 0; i < count; i++) _box(10 + i, height: i == 1 ? 40 : 20),
        create(
          1,
          'row',
          props: props,
          children: [for (var i = 0; i < count; i++) 10 + i],
        ),
      ], 1);
      await pumpView(tester, controller);
    }

    await row({'gap': 10});
    expect(_lefts(tester, [10, 11, 12]), [0, 60, 120]);
    expect(blockRect(tester, 10).top, 0, reason: 'cross start by default');

    await row({'gap': 10, 'align': 'center', 'cross': 'center'});
    expect(_lefts(tester, [10, 11, 12]), [115, 175, 235]);
    expect(blockRect(tester, 10).top, 10);

    await row({'gap': 10, 'align': 'end', 'cross': 'end'});
    expect(_lefts(tester, [10, 11, 12]), [230, 290, 350]);
    expect(blockRect(tester, 10).top, 20);

    await row({'align': 'space_between'});
    expect(_lefts(tester, [10, 11, 12]), [0, 175, 350]);

    await row({'align': 'space_around'});
    final lefts = _lefts(tester, [10, 11, 12]);
    expect(lefts.first, closeTo(400 - 50 - lefts.last, 0.01));
    expect(lefts[1], closeTo(175, 0.01));

    await row({'cross': 'stretch'});
    expect(blockRect(tester, 10).height, 20, reason: 'its own height stays');
    // Five 50-wide boxes with gaps of 100 don't fit 400: the fourth wraps.
    await row({'gap': 100, 'wrap': true}, count: 5);
    expect(_lefts(tester, [10, 11, 12, 13]), [0, 150, 300, 0]);
    expect(blockRect(tester, 13).top, 40 + 100);

    final column = _show([
      _box(20),
      _box(21),
      create(1, 'column', props: {'gap': 8}, children: [20, 21]),
    ], 1);
    await pumpView(tester, column);
    expect(blockRect(tester, 21).top, 28);
  });

  // Fails if a stretched child without its own height keeps its content's
  // height, if one with a height is stretched over it, or if a row whose
  // height has no end (inside a column) breaks rather than taking its
  // tallest child's.
  testWidgets('cross_stretch_fills_only_children_without_a_size', (
    tester,
  ) async {
    final controller = _show([
      _box(2, height: null),
      _box(3, height: 20),
      _box(4, width: 30, height: 60),
      create(10, 'row', props: {'cross': 'stretch'}, children: [2, 3, 4]),
      _box(5, height: null),
      create(
        11,
        'row',
        props: {'cross': 'stretch', 'height': 80},
        children: [5],
      ),
      create(1, 'column', children: [10, 11]),
    ], 1);
    await pumpView(tester, controller);
    expect(tester.takeException(), isNull);
    expect(blockRect(tester, 2).height, 60);
    expect(blockRect(tester, 3).height, 20);
    expect(blockRect(tester, 5).height, 80);
  });

  // Fails if any common prop is dropped or misread: a number size, `fill`,
  // `fit`, max over width and min over max, padding sides, opacity, or a
  // hidden child still taking room (or a gap) in its column.
  testWidgets('common_props_size_padding_opacity_and_visible', (tester) async {
    final controller = _show([
      create(3, 'box', props: {'background': 'accent'}),
      _box(
        2,
        width: 120,
        height: 30,
        props: {
          'padding': {'top': 2, 'right': 4, 'bottom': 6, 'left': 8},
        },
        children: [3],
      ),
      _box(4, width: 'fill', height: 10),
      _box(5, width: 50, height: 10, props: {'max_width': 30}),
      _box(6, width: 50, height: 10, props: {'max_width': 30, 'min_width': 70}),
      _box(7, height: 10, props: {'visible': false}),
      _box(8, height: 10, props: {'opacity': 0.5}),
      _box(9, width: 40, height: 10),
      _box(10, width: 'fit', height: null, children: [9]),
      create(
        1,
        'column',
        props: {'gap': 10, 'cross': 'stretch'},
        children: [2, 4, 5, 6, 7, 8, 10],
      ),
    ], 1);
    await pumpView(tester, controller);

    expect(blockRect(tester, 2), const Rect.fromLTWH(0, 0, 120, 30));
    expect(blockRect(tester, 3), const Rect.fromLTWH(8, 2, 108, 22));
    expect(blockRect(tester, 4).width, 400);
    expect(blockRect(tester, 5).width, 30);
    expect(blockRect(tester, 6).width, 70);
    expect(find.byKey(const ValueKey<int>(7)), findsNothing);
    expect(blockRect(tester, 8).top, 30 + 10 + 10 + 10 + 10 + 10 + 10 + 10);
    final faded = tester.widget<Opacity>(
      find.descendant(
        of: find.byKey(const ValueKey<int>(8)),
        matching: find.byType(Opacity),
      ),
    );
    expect(faded.opacity, 0.5);
    expect(blockRect(tester, 10).width, 40, reason: 'fit beats stretch');

    // Shown again, it takes its place between 6 and 8.
    controller.applyBatch(
      batch(2, [
        {
          'op': 'props',
          'id': 7,
          'set': const <String, Object?>{},
          'unset': ['visible'],
        },
      ]),
    );
    await tester.pump();
    expect(blockRect(tester, 7).top, 30 + 10 + 10 + 10 + 10 + 10 + 10 + 10);
    expect(
      blockRect(tester, 8).top,
      30 + 10 + 10 + 10 + 10 + 10 + 10 + 10 + 10 + 10,
    );
  });

  // Fails if `flex` doesn't share the free space by its ratio, if `fill` on
  // the main axis doesn't take what's left, or if either breaks layout
  // where the main axis has no end (a row inside a row).
  testWidgets('flex_shares_the_room_and_never_breaks_an_endless_row', (
    tester,
  ) async {
    final controller = _show([
      _box(2, width: null, props: {'flex': 1}),
      _box(3, width: null, props: {'flex': 3}),
      create(1, 'row', children: [2, 3]),
    ], 1);
    await pumpView(tester, controller);
    expect(blockRect(tester, 2).width, 100);
    expect(blockRect(tester, 3).width, 300);

    final endless = _show([
      _box(4, width: null, props: {'flex': 1}),
      _box(5, width: 'fill'),
      create(3, 'row', children: [4, 5]),
      create(2, 'row', children: [3]),
      create(1, 'column', children: [2]),
    ], 1);
    await pumpView(tester, endless);
    expect(tester.takeException(), isNull);
    expect(blockRect(tester, 5).width, 0, reason: 'nothing to fill');
  });

  // Fails if a block the renderer doesn't know is drawn as nothing, or
  // takes the whole view down.
  testWidgets('an_unknown_block_says_so', (tester) async {
    final controller = _show([
      create(2, 'sparkle'),
      _box(3),
      create(1, 'column', children: [2, 3]),
    ], 1);
    await pumpView(tester, controller);
    expect(find.text('Unknown block: sparkle'), findsOneWidget);
    expect(find.byKey(const ValueKey<int>(3)), findsOneWidget);
  });

  // Fails if the view doesn't tell the core its size, theme, input and text
  // scale at first, or doesn't tell it again when the area is resized or
  // the theme changes, or repeats an unchanged context.
  testWidgets(
    'the_view_reports_its_context_when_it_changes',
    variant: TargetPlatformVariant.only(TargetPlatform.linux),
    (tester) async {
      final controller = _show([_box(1)], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);
      await pumpView(tester, controller);
      await tester.pump();
      expect(inputs.of('context'), [
        {
          'kind': 'context',
          'width': 400.0,
          'height': 300.0,
          'theme': 'dark',
          'input': 'mouse',
          'safe_area': {'top': 0.0, 'right': 0.0, 'bottom': 0.0, 'left': 0.0},
          'text_scale': 1.0,
        },
      ]);

      await pumpView(tester, controller);
      await tester.pump();
      expect(inputs.of('context'), hasLength(1), reason: 'nothing changed');

      await pumpView(tester, controller, width: 360, theme: lightTheme);
      await tester.pump();
      final last = inputs.of('context').last;
      expect((last['width'], last['theme']), (360.0, 'light'));
    },
  );

  // Fails if a side effect never reaches the host, or a sound's end never
  // reaches the core.
  testWidgets('side_effects_reach_the_host', (tester) async {
    final host = FakeHost();
    final controller = _show([_box(1)], 1);
    final inputs = Inputs(controller);
    addTearDown(inputs.cancel);
    await pumpView(tester, controller, host: host);
    controller.applyBatch(
      batch(2, [
        {'op': 'copy', 'text': 'hi'},
        {'op': 'open_link', 'url': 'https://example.com'},
        {'op': 'open_channel', 'channel_id': '42'},
        {'op': 'sound', 'id': 5, 'asset': 'assets/ding.ogg', 'volume': 0.5},
      ]),
    );
    await tester.pump();
    expect(host.calls, [
      'copy hi',
      'confirmLink https://example.com',
      'openChannel 42',
      'playSound assets/ding.ogg 0.5',
    ]);
    host.soundEnds.single();
    await tester.pump();
    expect(inputs.of('sound_end'), [
      {'kind': 'sound_end', 'id': 5},
    ]);
  });

  // Fails if a common `tooltip` draws in the app's overlay, where it would
  // hang below the plugin area, rather than the area's own.
  testWidgets('a_tooltip_stays_inside_the_area', (tester) async {
    final controller = _show([
      _box(2, width: 60, height: 20, props: {'tooltip': 'Hello'}),
      create(
        1,
        'column',
        props: {'height': 'fill', 'align': 'end'},
        children: [2],
      ),
    ], 1);
    await pumpView(tester, controller, width: 200, height: 120);
    expect(blockRect(tester, 2).bottom, 120);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: const Offset(30, 110));
    await tester.pump(const Duration(seconds: 1));
    await tester.pumpAndSettle();
    final tip = tester.getRect(find.text('Hello'));
    expect(const Rect.fromLTWH(0, 0, 200, 120).contains(tip.topLeft), isTrue);
    expect(
      const Rect.fromLTWH(0, 0, 200, 120).contains(tip.bottomRight),
      isTrue,
    );
    await mouse.removePointer();
  });
}
