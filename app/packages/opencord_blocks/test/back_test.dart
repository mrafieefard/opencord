import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

/// A controller showing one text, its inputs collected.
(BlocksController, List<Map<String, Object?>>) _view({bool onBack = true}) {
  final controller = BlocksController();
  addTearDown(controller.dispose);
  final inputs = <Map<String, Object?>>[];
  controller.inputs.listen(
    (json) => inputs.add(jsonDecode(json) as Map<String, Object?>),
  );
  controller.applyBatch(
    batch(1, [
      create(1, 'text', props: {'text': 'detail'}),
      {'op': 'roots', 'main': 1, 'modals': const <Object?>[]},
      if (onBack) {'op': 'back_handler', 'on': true},
    ]),
  );
  return (controller, inputs);
}

void main() {
  // Fails if `back_handler` isn't read (an unknown op stops the view), or
  // its `on` isn't kept.
  test('back_handler says whether the plugin takes back', () {
    final (controller, _) = _view();
    expect(controller.error, isNull);
    expect(controller.backHandler, isTrue);
    controller.applyBatch(
      batch(2, [
        {'op': 'back_handler', 'on': false},
      ]),
    );
    expect(controller.backHandler, isFalse);
  });

  // Fails if askBack doesn't send a `back` input with an id, or its future
  // doesn't take the matching `back` op's answer.
  testWidgets('askBack sends back and takes the answer', (tester) async {
    final (controller, inputs) = _view();
    bool? handled;
    controller.askBack().then((value) => handled = value);
    await tester.pump();
    final asked = inputs.where((input) => input['kind'] == 'back').toList();
    expect(asked, hasLength(1));
    controller.applyBatch(
      batch(2, [
        {'op': 'back', 'id': asked.single['id'], 'handled': true},
      ]),
    );
    await tester.pump();
    expect(handled, isTrue);
  });

  // Fails if a back with no handler waits for a round trip, or one asked
  // while another waits sends a second input.
  testWidgets('no handler answers at once; a second ask shares the first', (
    tester,
  ) async {
    final (none, noneInputs) = _view(onBack: false);
    bool? handled;
    none.askBack().then((value) => handled = value);
    await tester.pump();
    expect(handled, isFalse);
    expect(noneInputs.where((input) => input['kind'] == 'back'), isEmpty);

    final (controller, inputs) = _view();
    final first = controller.askBack();
    final second = controller.askBack();
    expect(identical(first, second), isTrue);
    await tester.pump();
    expect(inputs.where((input) => input['kind'] == 'back'), hasLength(1));
    await tester.pump(BlocksController.backTimeout);
  });

  // Fails if a core that never answers traps the viewer (back must leave),
  // or a late answer to a timed-out ask is taken for the next one.
  testWidgets('an unanswered back is not handled after the timeout', (
    tester,
  ) async {
    final (controller, inputs) = _view();
    bool? handled;
    controller.askBack().then((value) => handled = value);
    await tester.pump(
      BlocksController.backTimeout - const Duration(milliseconds: 1),
    );
    expect(handled, isNull);
    await tester.pump(const Duration(milliseconds: 1));
    expect(handled, isFalse);

    bool? next;
    controller.askBack().then((value) => next = value);
    await tester.pump();
    final ids = [
      for (final input in inputs)
        if (input['kind'] == 'back') input['id'],
    ];
    expect(ids, hasLength(2));
    controller.applyBatch(
      batch(2, [
        {'op': 'back', 'id': ids.first, 'handled': true},
      ]),
    );
    await tester.pump();
    expect(next, isNull, reason: 'the late answer was for the first ask');
    controller.applyBatch(
      batch(3, [
        {'op': 'back', 'id': ids.last, 'handled': true},
      ]),
    );
    await tester.pump();
    expect(next, isTrue);
  });

  // Fails if a view that stops while back waits leaves the ask hanging.
  testWidgets('a view that stops answers a waiting back not handled', (
    tester,
  ) async {
    final (controller, _) = _view();
    bool? handled;
    controller.askBack().then((value) => handled = value);
    controller.applyBatch(
      batch(2, [
        {'op': 'error', 'message': 'This plugin hit an error'},
      ]),
    );
    await tester.pump();
    expect(handled, isFalse);
    expect(controller.backHandler, isFalse);
  });
}
