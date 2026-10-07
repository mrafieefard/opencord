import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/message_rows.dart';

const me = 1;
const kai = 2;
const mira = 3;
final day = DateTime(2026, 10, 7, 9);

Message msg(
  int id,
  int author, {
  int minute = 0,
  int dayOffset = 0,
  int? replyTo,
  SystemEvent? system,
  String content = 'text',
}) => Message(
  id: id,
  channelId: 10,
  authorId: author,
  content: content,
  createdAt: day.add(Duration(days: dayOffset, minutes: minute)),
  replyToId: replyTo,
  systemEvent: system,
);

String describe(ChatRow row) => switch (row) {
  StartRow() => 'start',
  DayRow(:final day) => 'day ${day.day}',
  UnreadRow() => 'unread',
  SystemRow(:final message) => 'system ${message.id}',
  MessageRow(:final message, :final first, :final last, :final showAuthor) =>
    '${message.id}${first ? 'F' : ''}${last ? 'L' : ''}${showAuthor ? 'A' : ''}',
};

List<String> rows(
  List<Message> messages, {
  int? readUpTo,
  bool reachedStart = false,
}) => [
  for (final row in buildRows(
    messages,
    selfId: me,
    lastReadId: readUpTo,
    reachedStart: reachedStart,
  ))
    describe(row),
];

void main() {
  test('messages group by author within five minutes', () {
    expect(
      rows([
        msg(1, kai),
        msg(2, kai, minute: 4),
        msg(3, kai, minute: 10),
        msg(4, mira, minute: 11),
      ]),
      ['day 7', '1FA', '2L', '3FLA', '4FLA'],
    );
  });

  test('replies, system notices and new days start a new group', () {
    expect(
      rows([
        msg(1, kai),
        msg(2, kai, minute: 1, replyTo: 1),
        msg(3, kai, minute: 2, system: SystemEvent.messagePinned),
        msg(4, kai, minute: 3),
        msg(5, kai, minute: 4, dayOffset: 1),
      ]),
      ['day 7', '1FLA', '2FLA', 'system 3', '4FLA', 'day 8', '5FLA'],
    );
  });

  test('own messages never show the author', () {
    expect(rows([msg(1, me), msg(2, me, minute: 1)]), ['day 7', '1F', '2L']);
  });

  test(
    'the unread line sits before the first message from someone else after the read point',
    () {
      expect(
        rows([
          msg(1, kai),
          msg(2, me, minute: 10),
          msg(3, mira, minute: 20),
          msg(4, mira, minute: 21),
        ], readUpTo: 1),
        ['day 7', '1FLA', '2FL', 'unread', '3FA', '4L'],
      );
      expect(rows([msg(1, kai), msg(2, me, minute: 1)], readUpTo: 1), [
        'day 7',
        '1FLA',
        '2FL',
      ]);
    },
  );

  test('the start of the channel shows once history is complete', () {
    expect(rows([msg(1, kai)], reachedStart: true).first, 'start');
    expect(rows([msg(1, kai)]).first, 'day 7');
  });

  test('rows know whether a message mentions the reader', () {
    final built = buildRows(
      [msg(1, kai, content: 'hi <@$me>'), msg(2, kai, minute: 1)],
      selfId: me,
      lastReadId: null,
      reachedStart: false,
    );
    final messages = built.whereType<MessageRow>().toList();

    expect(messages.first.mentionsMe, isTrue);
    expect(messages.last.mentionsMe, isFalse);
  });

  test('row keys are stable and unique', () {
    final built = buildRows(
      [
        msg(1, kai),
        msg(2, kai, minute: 1),
        msg(3, mira, minute: 2, dayOffset: 1),
      ],
      selfId: me,
      lastReadId: 1,
      reachedStart: true,
    );

    expect(built.map((row) => row.key).toSet(), hasLength(built.length));
  });
}
