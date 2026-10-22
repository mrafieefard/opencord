import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/messages_state.dart';
import 'package:opencord/core/repository/events.dart';

import '../../support/fixtures.dart';

List<int> ids(ChannelMessages state) => [for (final m in state.all) m.id];

void main() {
  test('the first page loads in order and tells whether there is more', () {
    final full = withPage(ChannelMessages.initial, [
      message(1),
      message(2),
      message(3),
    ], limit: 3);
    final short = withPage(ChannelMessages.initial, [message(1)], limit: 3);

    expect(ids(full), [1, 2, 3]);
    expect(full.loaded, isTrue);
    expect(full.hasOlder, isTrue);
    expect(short.hasOlder, isFalse);
  });

  test('older pages go in front without duplicates', () {
    var state = withPage(ChannelMessages.initial, [
      message(4),
      message(5),
    ], limit: 2);

    state = withPage(state, [message(2), message(3), message(4)], limit: 3);

    expect(ids(state), [2, 3, 4, 5]);
  });

  test('new messages are added in id order, once', () {
    var state = withPage(ChannelMessages.initial, [
      message(1),
      message(3),
    ], limit: 50);

    state = reduceMessages(state, MessageCreated(serverKey, message(4)));
    state = reduceMessages(state, MessageCreated(serverKey, message(2)));
    state = reduceMessages(state, MessageCreated(serverKey, message(4)));

    expect(ids(state), [1, 2, 3, 4]);
  });

  test('a pending message is replaced by the server copy with its nonce', () {
    final pending = message(
      -1,
      authorId: selfId,
      nonce: 'n1',
    ).copyWith(sendState: SendState.pending);
    var viaEvent = withPending(
      withPage(ChannelMessages.initial, [message(1)], limit: 50),
      pending,
    );
    var viaResponse = viaEvent;

    viaEvent = reduceMessages(
      viaEvent,
      MessageCreated(serverKey, message(2, authorId: selfId, nonce: 'n1')),
    );
    viaResponse = resolvePending(
      viaResponse,
      message(2, authorId: selfId, nonce: 'n1'),
    );
    viaResponse = reduceMessages(
      viaResponse,
      MessageCreated(serverKey, message(2, authorId: selfId, nonce: 'n1')),
    );

    expect(ids(viaEvent), [1, 2]);
    expect(viaEvent.pending, isEmpty);
    expect(ids(viaResponse), [1, 2]);
  });

  test('a failed send stays with its text, marked failed', () {
    final pending = message(
      -1,
      authorId: selfId,
      nonce: 'n1',
      content: 'draft',
    ).copyWith(sendState: SendState.pending);

    final state = failPending(
      withPending(ChannelMessages.initial, pending),
      'n1',
    );

    expect(state.pending.single.sendState, SendState.failed);
    expect(state.pending.single.content, 'draft');
  });

  test('edits replace in place and deletions remove', () {
    var state = withPage(ChannelMessages.initial, [
      message(1),
      message(2),
    ], limit: 50);

    state = reduceMessages(
      state,
      MessageUpdated(serverKey, message(1, content: 'changed')),
    );
    state = reduceMessages(
      state,
      const MessageDeleted(serverKey, generalId, 2),
    );

    expect(state.all.single.content, 'changed');
  });

  test('pending messages always come after confirmed ones', () {
    final pending = message(
      -1,
      authorId: selfId,
      nonce: 'n1',
    ).copyWith(sendState: SendState.pending);
    var state = withPending(ChannelMessages.initial, pending);

    state = reduceMessages(state, MessageCreated(serverKey, message(9)));

    expect(ids(state), [9, -1]);
  });
}
