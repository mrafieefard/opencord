import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord_blocks/opencord_blocks.dart';

import 'support.dart';

BlocksController _controller() {
  final controller = BlocksController();
  addTearDown(controller.dispose);
  return controller;
}

void main() {
  // Fails if a batch is applied when its seq is equal to or below the last
  // one applied (a stream redelivering, or batches out of order).
  test('a_stale_batch_is_ignored', () {
    final controller = _controller();
    controller.applyBatch(
      batch(2, [
        create(1, 'box', props: {'padding': 4}),
        roots(1),
      ]),
    );
    controller.applyBatch(
      batch(2, [
        {
          'op': 'props',
          'id': 1,
          'set': {'padding': 9},
          'unset': const <String>[],
        },
      ]),
    );
    controller.applyBatch(
      batch(1, [
        {
          'op': 'props',
          'id': 1,
          'set': {'padding': 7},
          'unset': const <String>[],
        },
      ]),
    );
    expect(controller.node(1)!.props['padding'], 4);
    expect(controller.seq, 2);

    controller.applyBatch(
      batch(3, [
        {
          'op': 'props',
          'id': 1,
          'set': {'padding': 5},
          'unset': const <String>[],
        },
      ]),
    );
    expect(controller.node(1)!.props['padding'], 5);
    expect(controller.seq, 3);
  });

  // Fails if one op of a broken batch is applied before the broken one is
  // read, if the failure is swallowed rather than shown as the view's
  // error, or if the error text carries the batch's own content.
  test('a_broken_batch_changes_nothing_and_shows_an_error', () {
    final controller = _controller();
    final reported = <FlutterErrorDetails>[];
    final before = FlutterError.onError;
    FlutterError.onError = reported.add;
    addTearDown(() => FlutterError.onError = before);

    controller.applyBatch(batch(1, [create(1, 'box'), roots(1)]));
    controller.applyBatch(
      batch(2, [
        {
          'op': 'props',
          'id': 1,
          'set': {'padding': 3},
          'unset': const <String>[],
        },
        {
          'op': 'props',
          'id': 'secret text',
          'set': const {},
          'unset': const [],
        },
      ]),
    );
    expect(controller.node(1)!.props, isEmpty);
    expect(controller.error?.message, 'This plugin hit an error');
    expect(reported, hasLength(1));
    expect(reported.single.exceptionAsString(), isNot(contains('secret')));
  });

  // Fails if `roots`, `frames`, `dialog`, `error`, `too_old` or `closed`
  // don't reach the controller's state, or if listeners hear of a batch
  // more than once.
  test('every_state_op_reaches_the_controller', () {
    final controller = _controller();
    var heard = 0;
    controller.addListener(() => heard++);
    controller.applyBatch(
      batch(1, [
        create(1, 'column'),
        create(2, 'box'),
        {
          'op': 'roots',
          'main': 1,
          'modals': [
            {
              'id': 2,
              'name': 'edit',
              'size': 'large',
              'sheet': true,
              'dismissible': false,
            },
          ],
        },
        {'op': 'frames', 'on': true},
        {
          'op': 'dialog',
          'id': 7,
          'kind': 'prompt',
          'text': 'Name?',
          'default': 'Ann',
        },
      ]),
    );
    expect(heard, 1);
    expect(controller.roots!.main, 1);
    final modal = controller.roots!.modals.single;
    expect(
      (modal.id, modal.name, modal.size, modal.sheet, modal.dismissible),
      (2, 'edit', 'large', true, false),
    );
    expect(controller.framesOn, isTrue);
    final dialog = controller.dialogs.single;
    expect(
      (dialog.id, dialog.kind, dialog.text, dialog.defaultText),
      (7, 'prompt', 'Name?', 'Ann'),
    );

    controller.applyBatch(
      batch(2, [
        {
          'op': 'error',
          'message': 'This plugin hit an error',
          'details': 'main.luau:3',
        },
      ]),
    );
    expect(controller.error?.message, 'This plugin hit an error');
    expect(controller.error?.details, 'main.luau:3');
    // The view stopped: its dialogs and frames went with it.
    expect(controller.dialogs, isEmpty);
    expect(controller.framesOn, isFalse);

    controller.applyBatch(
      batch(3, [
        {'op': 'too_old'},
      ]),
    );
    expect(controller.tooOld, isTrue);
    controller.applyBatch(
      batch(4, [
        {'op': 'closed', 'reason': 'lost_access'},
      ]),
    );
    expect(controller.closed, 'lost_access');
    expect(heard, 4);
  });

  // Fails if a dropped node stays in the store, or if a dropped node's
  // watcher isn't told it's gone.
  test('a_dropped_node_leaves_the_store', () {
    final controller = _controller();
    controller.applyBatch(
      batch(1, [
        create(2, 'box'),
        create(1, 'column', children: [2]),
        roots(1),
      ]),
    );
    final watched = controller.watch(2)!;
    BlockNode? last = watched.value;
    watched.addListener(() => last = watched.value);
    controller.applyBatch(
      batch(2, [
        {'op': 'children', 'id': 1, 'children': const <int>[]},
        {
          'op': 'drop',
          'ids': [2],
        },
      ]),
    );
    expect(controller.node(2), isNull);
    expect(controller.watch(2), isNull);
    expect(last, isNull);
    expect(controller.nodes.map((node) => node.id), [1]);
  });

  // Fails if an event is sent for a node that has no handler for it (the
  // core would only ignore it, but every input costs the view's rate), or
  // if the input's shape differs from the wire format.
  test('an_event_is_sent_only_when_the_node_handles_it', () async {
    final controller = _controller();
    final inputs = Inputs(controller);
    controller.applyBatch(
      batch(1, [
        create(1, 'box', events: ['press']),
        roots(1),
      ]),
    );
    controller.event(1, 'press');
    controller.event(1, 'long_press');
    controller.event(9, 'press');
    await pumpEventQueue();
    expect(inputs.all, [
      {
        'kind': 'event',
        'node': 1,
        'event': 'press',
        'data': <String, Object?>{},
      },
    ]);
    await inputs.cancel();
  });

  // Fails if answering leaves the dialog waiting, sends another shape than
  // the wire format's `dialog` input, or answers a dialog that isn't there.
  test('answering_a_dialog_sends_it_once', () async {
    final controller = _controller();
    final inputs = Inputs(controller);
    controller.applyBatch(
      batch(1, [
        {'op': 'dialog', 'id': 3, 'kind': 'confirm', 'text': 'Sure?'},
      ]),
    );
    controller.answerDialog(3, true);
    controller.answerDialog(3, false);
    await pumpEventQueue();
    expect(controller.dialogs, isEmpty);
    expect(inputs.of('dialog'), [
      {'kind': 'dialog', 'id': 3, 'result': true},
    ]);
    await inputs.cancel();
  });

  // Fails if a side effect is lost before a view takes it, taken twice, or
  // taken out of order.
  test('side_effects_wait_in_order_until_taken', () {
    final controller = _controller();
    controller.applyBatch(
      batch(1, [
        {'op': 'copy', 'text': 'hi'},
        {'op': 'open_link', 'url': 'https://example.com'},
        {'op': 'open_channel', 'channel_id': '42'},
        {'op': 'sound', 'id': 5, 'asset': 'assets/ding.ogg', 'volume': 0.5},
      ]),
    );
    final effects = controller.takeEffects();
    expect(effects, [
      const CopyEffect('hi'),
      const OpenLinkEffect('https://example.com'),
      const OpenChannelEffect('42'),
      const SoundEffect(5, 'assets/ding.ogg', 0.5),
    ]);
    expect(controller.takeEffects(), isEmpty);
  });

  // Fails if `send` after the view is gone throws (a hover exit fires as
  // its widget is removed) or still reaches the stream.
  test('inputs_after_dispose_are_dropped', () async {
    final controller = BlocksController();
    final inputs = Inputs(controller);
    controller.dispose();
    controller.send({'kind': 'lifecycle', 'event': 'blur'});
    await pumpEventQueue();
    expect(inputs.all, isEmpty);
  });
}
