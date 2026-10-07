import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/pins_state.dart';
import 'package:opencord/core/repository/events.dart';

import '../../support/fixtures.dart';

Message pinned(int id, {bool pinned = true, int channelId = generalId}) =>
    message(id, channelId: channelId).copyWith(pinned: pinned);

void main() {
  test('pins are kept newest first', () {
    expect(sortPins([pinned(2), pinned(9), pinned(5)]).map((m) => m.id), [
      9,
      5,
      2,
    ]);
  });

  test('a message pinned in this channel joins the pins', () {
    final pins = reducePins(
      [pinned(2)],
      MessageUpdated(serverKey, pinned(7)),
      channelId: generalId,
    );

    expect(pins.map((m) => m.id), [7, 2]);
  });

  test('an edit of a pinned message replaces it in place', () {
    final edited = pinned(2).copyWith(content: 'edited');
    final pins = reducePins(
      [pinned(7), pinned(2)],
      MessageUpdated(serverKey, edited),
      channelId: generalId,
    );

    expect(pins.map((m) => m.content), ['hello', 'edited']);
  });

  test('unpinning or deleting removes it', () {
    final unpinned = reducePins(
      [pinned(7), pinned(2)],
      MessageUpdated(serverKey, pinned(7, pinned: false)),
      channelId: generalId,
    );
    final deleted = reducePins(
      [pinned(7), pinned(2)],
      const MessageDeleted(serverKey, generalId, 2),
      channelId: generalId,
    );

    expect(unpinned.map((m) => m.id), [2]);
    expect(deleted.map((m) => m.id), [7]);
  });

  test('other channels are ignored', () {
    final pins = [pinned(2)];

    expect(
      reducePins(
        pins,
        MessageUpdated(serverKey, pinned(9, channelId: randomId)),
        channelId: generalId,
      ),
      same(pins),
    );
  });
}
