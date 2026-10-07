import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/activity_state.dart';
import 'package:opencord/core/providers/presence_state.dart';
import 'package:opencord/core/providers/typing_state.dart';
import 'package:opencord/core/repository/events.dart';

import '../../support/fixtures.dart';

ActivityState readyActivity() => reduceActivity(
  ActivityState.empty,
  Ready(
    serverKey,
    snapshot(
      lastMessages: {generalId: message(5)},
      readStates: const {
        generalId: ReadState(lastReadId: 3, unread: 2, mentions: 1),
      },
    ),
  ),
);

void main() {
  group('activity', () {
    test('Ready seeds previews and read states', () {
      final state = readyActivity();

      expect(state.of(generalId).last?.id, 5);
      expect(state.of(generalId).read.unread, 2);
      expect(state.of(randomId).read.unread, 0);
      expect(state.mentions, 1);
      expect(state.hasUnread, isTrue);
    });

    test('messages from others count as unread; mentions count twice', () {
      var state = readyActivity();

      state = reduceActivity(
        state,
        MessageCreated(serverKey, message(6, channelId: randomId)),
      );
      state = reduceActivity(
        state,
        MessageCreated(
          serverKey,
          message(7, channelId: randomId, content: 'hi <@$selfId>'),
        ),
      );

      expect(state.of(randomId).last?.id, 7);
      expect(state.of(randomId).read.unread, 2);
      expect(state.of(randomId).read.mentions, 1);
    });

    test('own messages mark the channel read up to them', () {
      final state = reduceActivity(
        readyActivity(),
        MessageCreated(serverKey, message(8, authorId: selfId)),
      );

      expect(state.of(generalId).read.unread, 0);
      expect(state.of(generalId).read.lastReadId, 8);
    });

    test('messages in the channel being read do not count', () {
      var state = focusChannel(readyActivity(), generalId);

      state = reduceActivity(state, MessageCreated(serverKey, message(9)));

      expect(state.of(generalId).read.unread, 0);
      expect(state.of(generalId).read.lastReadId, 9);
    });

    test('marking read clears the counts', () {
      final state = markChannelRead(readyActivity(), generalId);

      expect(state.of(generalId).read, const ReadState(lastReadId: 5));
      expect(state.hasUnread, isFalse);
    });

    test('edits update the preview; deleting the newest clears it', () {
      var state = readyActivity();

      state = reduceActivity(
        state,
        MessageUpdated(serverKey, message(5, content: 'edited')),
      );
      final edited = state.of(generalId).last?.content;
      state = reduceActivity(
        state,
        const MessageDeleted(serverKey, generalId, 5),
      );

      expect(edited, 'edited');
      expect(state.of(generalId).last, isNull);
    });

    test('mentions are written as <@id>', () {
      expect(mentionsUser('ping <@1000>', 1000), isTrue);
      expect(mentionsUser('ping <@10000>', 1000), isFalse);
      expect(mentionsUser('ping @1000', 1000), isFalse);
    });
  });

  group('presence', () {
    test('Ready seeds presences and activities; changes replace them', () {
      var state = reducePresence(
        PresenceState.empty,
        Ready(serverKey, snapshot()),
      );
      state = reducePresence(
        state,
        const PresenceChanged(
          serverKey,
          miraId,
          Presence.doNotDisturb,
          activity: 'Focusing',
        ),
      );
      state = reducePresence(state, const MemberLeft(serverKey, kaiId));

      expect(state.of(selfId), Presence.online);
      expect(state.of(miraId), Presence.doNotDisturb);
      expect(state.activities[miraId], 'Focusing');
      expect(state.of(kaiId), Presence.offline);
    });
  });

  group('typing', () {
    test('lasts 10 s, ends with a message, and ignores yourself', () {
      var state = reduceTyping(
        TypingState.empty,
        const TypingStarted(serverKey, generalId, kaiId),
        now: t0,
        selfId: selfId,
      );
      state = reduceTyping(
        state,
        const TypingStarted(serverKey, generalId, miraId),
        now: t0,
        selfId: selfId,
      );
      state = reduceTyping(
        state,
        const TypingStarted(serverKey, generalId, selfId),
        now: t0,
        selfId: selfId,
      );
      final both = state.typingIn(generalId, t0);
      state = reduceTyping(
        state,
        MessageCreated(serverKey, message(1, authorId: miraId)),
        now: t0,
        selfId: selfId,
      );

      expect(both, [kaiId, miraId]);
      expect(state.typingIn(generalId, t0), [kaiId]);
      expect(
        state.typingIn(generalId, t0.add(const Duration(seconds: 11))),
        isEmpty,
      );
    });
  });
}
