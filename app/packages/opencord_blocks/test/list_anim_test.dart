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

void main() {
  group('list', () {
    // Fails if list does not request visible range plus padding items on viewport attach.
    testWidgets('only_the_visible_range_is_asked', (tester) async {
      final controller = _show([
        create(1, 'list', props: {'count': 100, 'item_height': 50}),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      // Viewport height 200 = 4 items visible (0, 1, 2, 3).
      // Plus 20 on each side: start = 0, end = 24.
      await pumpView(tester, controller, width: 300, height: 200);
      await tester.pumpAndSettle();

      final ranges = inputs.of('range');
      expect(ranges, isNotEmpty);
      final first = ranges.first;
      expect(first['node'], 1);
      expect(first['start'], 0);
      expect(first['end'], 24);
    });

    // Fails if requested list range exceeds the total count.
    testWidgets('a_range_never_passes_count', (tester) async {
      final controller = _show([
        create(1, 'list', props: {'count': 15, 'item_height': 50}),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      // Viewport height 400 = 8 items visible. 8 + 20 = 28 > 15.
      await pumpView(tester, controller, width: 300, height: 400);
      await tester.pumpAndSettle();

      final ranges = inputs.of('range');
      expect(ranges, isNotEmpty);
      for (final r in ranges) {
        expect((r['end'] as num).toInt(), lessThanOrEqualTo(15));
      }
    });

    // Fails if reversed list does not stay pinned to the bottom when new items arrive at bottom.
    testWidgets('a_reversed_list_stays_at_the_bottom', (tester) async {
      // 100 items of height 50; items 95..99 provided.
      final itemNodes = [
        for (var i = 95; i < 100; i++)
          create(100 + i, 'text', key: '#$i', props: {'text': 'Item $i'}),
      ];
      final controller = _show([
        ...itemNodes,
        create(
          1,
          'list',
          props: {
            'count': 100,
            'item_height': 50,
            'reverse': true,
            'start': 95,
          },
          children: [for (var i = 95; i < 100; i++) 100 + i],
        ),
      ], 1);

      // Viewport height 200 (shows 4 items: items 96..99).
      await pumpView(tester, controller, width: 300, height: 200);
      await tester.pumpAndSettle();

      expect(find.text('Item 99'), findsOneWidget);

      // Count increases to 105, new items 100..104 added with start: 99.
      controller.applyBatch(
        batch(2, [
          for (var i = 100; i <= 104; i++)
            create(200 + i, 'text', props: {'text': 'Item $i'}),
          {
            'op': 'props',
            'id': 1,
            'set': {'count': 105, 'start': 99},
            'unset': const <String>[],
          },
          {
            'op': 'children',
            'id': 1,
            'children': [199, for (var i = 100; i <= 104; i++) 200 + i],
          },
        ]),
      );
      await tester.pumpAndSettle();

      // Because user was at the bottom, list stays pinned to the bottom: Item 104 is visible.
      expect(find.text('Item 104'), findsOneWidget);
    });

    // Fails if reversed list jumps when scrolled up and new items are added at the bottom.
    testWidgets('scrolled_up_it_stays_put', (tester) async {
      final itemNodes = [
        for (var i = 70; i < 100; i++)
          create(100 + i, 'text', key: '#$i', props: {'text': 'Item $i'}),
      ];
      final controller = _show([
        ...itemNodes,
        create(
          1,
          'list',
          props: {
            'count': 100,
            'item_height': 50,
            'reverse': true,
            'start': 70,
          },
          children: [for (var i = 70; i < 100; i++) 100 + i],
        ),
      ], 1);

      await pumpView(tester, controller, width: 300, height: 200);
      await tester.pumpAndSettle();
      expect(find.text('Item 99'), findsOneWidget);

      // Scroll up (drag down with positive dy) by 250 pixels.
      await tester.drag(find.byType(ListView), const Offset(0, 120));
      await tester.pumpAndSettle();
      await tester.drag(find.byType(ListView), const Offset(0, 130));
      await tester.pumpAndSettle();

      // Item 99 is scrolled out of view, and an earlier item is visible.
      expect(find.text('Item 99'), findsNothing);
      expect(find.text('Item 85'), findsOneWidget);

      // Now new items arrive: count increases to 105.
      controller.applyBatch(
        batch(2, [
          {
            'op': 'create',
            'id': 204,
            'block': 'text',
            'key': '#104',
            'props': {'text': 'Item 104'},
            'events': const <String>[],
            'children': const <int>[],
          },
          {
            'op': 'props',
            'id': 1,
            'set': {'count': 105, 'start': 70},
            'unset': const <String>[],
          },
        ]),
      );
      await tester.pumpAndSettle();

      // Because the user scrolled up, the list stayed put: Item 85 is still visible, NOT Item 104.
      expect(find.text('Item 85'), findsOneWidget);
      expect(find.text('Item 104'), findsNothing);
    });

    // Fails if list child k is not placed at start + k using list node's start prop.
    testWidgets('list_places_children_by_start_prop_ignoring_keys', (
      tester,
    ) async {
      final itemNodes = [
        for (var i = 0; i < 5; i++)
          create(10 + i, 'text', key: 'm_$i', props: {'text': 'Msg ${40 + i}'}),
      ];
      final controller = _show([
        ...itemNodes,
        create(
          1,
          'list',
          props: {'count': 100, 'item_height': 50, 'start': 40},
          children: [for (var i = 0; i < 5; i++) 10 + i],
        ),
      ], 1);

      await pumpView(tester, controller, width: 300, height: 300);
      await tester.pumpAndSettle();

      // Scroll down to item 40 (offset 2000)
      await tester.drag(find.byType(ListView), const Offset(0, -2000));
      await tester.pumpAndSettle();

      expect(find.text('Msg 40'), findsOneWidget);
      expect(find.text('Msg 41'), findsOneWidget);
      expect(find.text('Msg 44'), findsOneWidget);
    });

    // Fails if list does not dispatch scroll event with offset and max when scrolled.
    testWidgets('list_sends_scroll_events', (tester) async {
      final controller = _show([
        create(
          1,
          'list',
          props: {'count': 100, 'item_height': 50},
          events: ['scroll'],
        ),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      await pumpView(tester, controller, width: 300, height: 200);
      await tester.pumpAndSettle();

      await tester.drag(find.byType(ListView), const Offset(0, -200));
      await tester.pumpAndSettle();

      final scrollEvents = inputs
          .of('event')
          .where((e) => e['event'] == 'scroll')
          .toList();
      expect(scrollEvents.isNotEmpty, isTrue);
      expect((scrollEvents.first['data'] as Map)['offset'], greaterThan(0));
    });
  });

  group('animation', () {
    // Fails if animate prop does not animate opacity/size or reduced motion does not complete immediately.
    testWidgets('an_animate_prop_animates_and_reduced_motion_does_not', (
      tester,
    ) async {
      final controller = _show([
        create(
          1,
          'box',
          props: {
            'width': 100,
            'opacity': 1.0,
            'animate': {'duration': 200, 'curve': 'linear'},
          },
          events: ['animation_end'],
        ),
      ], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      await pumpView(tester, controller, width: 300, height: 300);
      await tester.pumpAndSettle();

      // Change width to 200 and opacity to 0.0.
      controller.applyBatch(
        batch(2, [
          {
            'op': 'props',
            'id': 1,
            'set': {'width': 200, 'opacity': 0.0},
            'unset': const <String>[],
          },
        ]),
      );

      // Start animation frame
      await tester.pump();

      // Advance by 100 ms (halfway).
      await tester.pump(const Duration(milliseconds: 100));

      final animatedOpacity = tester.widget<AnimatedOpacity>(
        find.byType(AnimatedOpacity),
      );
      expect(animatedOpacity.opacity, 0.0); // target is 0.0
      // Check that animation_end hasn't fired yet.
      expect(inputs.of('event'), isEmpty);

      // Advance by remaining 100 ms to completion.
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 50));

      // animation_end has fired once.
      final ends = inputs.of('event');
      expect(ends.length, 1);
      expect(ends.first['event'], 'animation_end');

      // Now test reduced motion:
      tester.view.physicalSize = const Size(800, 600);
      tester.view.devicePixelRatio = 1;
      await tester.pumpWidget(
        MaterialApp(
          home: MediaQuery(
            data: const MediaQueryData(disableAnimations: true),
            child: BlocksView(
              controller: controller,
              host: FakeHost(),
              theme: darkTheme,
            ),
          ),
        ),
      );

      // Change width back to 100.
      controller.applyBatch(
        batch(3, [
          {
            'op': 'props',
            'id': 1,
            'set': {'width': 100, 'opacity': 1.0},
            'unset': const <String>[],
          },
        ]),
      );
      await tester.pump();

      // In reduced motion, AnimatedOpacity duration is 0, so it completes immediately.
      final size = blockRect(tester, 1);
      expect(size.width, 100);
    });

    // Fails if enter and exit animations do not run or dropped exiting nodes are freed before animation ends.
    testWidgets('enter_and_exit_run_once', (tester) async {
      final controller = _show([create(1, 'column', children: [])], 1);
      final inputs = Inputs(controller);
      addTearDown(inputs.cancel);

      await pumpView(tester, controller, width: 300, height: 300);
      await tester.pumpAndSettle();

      // Add child with enter: 'fade' and exit: 'fade'.
      controller.applyBatch(
        batch(2, [
          create(
            2,
            'text',
            key: 'k1',
            props: {
              'text': 'Hello',
              'enter': 'fade',
              'exit': 'fade',
              'animate': {'duration': 200, 'curve': 'linear'},
            },
            events: ['animation_end'],
          ),
          {
            'op': 'children',
            'id': 1,
            'children': [2],
          },
        ]),
      );

      // Start animation
      await tester.pump();
      // Pump 100 ms: enter animation in progress.
      await tester.pump(const Duration(milliseconds: 100));
      expect(find.text('Hello'), findsOneWidget);
      expect(inputs.of('event'), isEmpty);

      // Pump to completion: animation_end should fire once.
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 50));
      final enterEnds = inputs.of('event');
      expect(enterEnds.length, 1);
      expect(enterEnds.first['event'], 'animation_end');
      expect(enterEnds.first['node'], 2);

      // Now remove child 2 from column AND drop it (real core sends drop with removal).
      controller.applyBatch(
        batch(3, [
          {'op': 'children', 'id': 1, 'children': const <int>[]},
          {
            'op': 'drop',
            'ids': [2],
          },
        ]),
      );

      // Start exit animation
      await tester.pump();
      // Exit animation runs: child 2 is retained and still visible during exit animation!
      await tester.pump(const Duration(milliseconds: 100));
      expect(find.text('Hello'), findsOneWidget);

      // Finish exit animation.
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 50));
      // Child is now gone.
      expect(find.text('Hello'), findsNothing);

      // animation_end fired for exit.
      final totalEnds = inputs.of('event');
      expect(totalEnds.length, 2);
    });

    // Fails if keyed children are not reordered in row or column when children order changes.
    testWidgets('keyed_moves_reorder_column_children', (tester) async {
      final controller = _show([
        create(2, 'text', key: 'a', props: {'text': 'A'}),
        create(3, 'text', key: 'b', props: {'text': 'B'}),
        create(4, 'text', key: 'c', props: {'text': 'C'}),
        create(1, 'column', children: [2, 3, 4]),
      ], 1);
      await pumpView(tester, controller, width: 200, height: 300);
      await tester.pumpAndSettle();

      final aRectBefore = tester.getRect(find.text('A'));
      final cRectBefore = tester.getRect(find.text('C'));
      expect(cRectBefore.top, greaterThan(aRectBefore.top));

      // Replay move_keyed ops: [4, 2, 3] ('C', 'A', 'B, moved')
      controller.applyBatch(
        batch(2, [
          {
            'op': 'props',
            'id': 3,
            'set': {'text': 'B, moved'},
            'unset': <String>[],
          },
          {
            'op': 'children',
            'id': 1,
            'children': [4, 2, 3],
          },
        ]),
      );
      await tester.pumpAndSettle();

      final aRectAfter = tester.getRect(find.text('A'));
      final cRectAfter = tester.getRect(find.text('C'));
      // C is now above A!
      expect(cRectAfter.top, lessThan(aRectAfter.top));
      expect(find.text('B, moved'), findsOneWidget);
    });
  });
}
