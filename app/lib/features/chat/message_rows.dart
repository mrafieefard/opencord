import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/activity_state.dart';

/// One row of the message list (§4.5).
@immutable
sealed class ChatRow {
  const ChatRow();

  /// Stable across rebuilds, so rows keep their state while history loads.
  Object get key;
}

/// "Welcome to #general", at the very top of a complete history.
final class StartRow extends ChatRow {
  const StartRow();

  @override
  Object get key => 'start';
}

final class DayRow extends ChatRow {
  const DayRow(this.day);

  final DateTime day;

  @override
  Object get key => 'day-${day.year}-${day.month}-${day.day}';
}

/// "Unread messages", before the first one that arrived since the last
/// visit.
final class UnreadRow extends ChatRow {
  const UnreadRow();

  @override
  Object get key => 'unread';
}

final class SystemRow extends ChatRow {
  const SystemRow(this.message);

  final Message message;

  @override
  Object get key => 's${message.id}';
}

final class MessageRow extends ChatRow {
  const MessageRow(
    this.message, {
    required this.first,
    required this.last,
    required this.own,
    required this.mentionsMe,
  });

  final Message message;

  /// First and last bubble of a group from one author (§4.5 grouping).
  final bool first;
  final bool last;
  final bool own;
  final bool mentionsMe;

  /// Incoming groups show the author's name on their first bubble.
  bool get showAuthor => first && !own;

  /// ...and the author's avatar next to their last.
  bool get showAvatar => last && !own;

  /// A pending message keeps its key when the server's copy replaces it.
  @override
  Object get key => 'm${message.nonce ?? message.id}';
}

const groupWindow = Duration(minutes: 5);

DateTime _day(DateTime time) => DateTime(time.year, time.month, time.day);

/// Turns a channel's messages (oldest first) into list rows: day separators,
/// the unread line after [lastReadId], system notices and grouped bubbles.
/// Messages group when the same author writes again within five minutes on
/// the same day without replying.
List<ChatRow> buildRows(
  List<Message> messages, {
  required int selfId,
  required int? lastReadId,
  required bool reachedStart,
}) {
  final rows = <ChatRow>[if (reachedStart) const StartRow()];
  DateTime? currentDay;
  Message? previous;
  var unreadPlaced = false;
  // Bubbles of the group being built, finished when the group ends.
  final group = <Message>[];

  void endGroup() {
    for (var i = 0; i < group.length; i++) {
      final message = group[i];
      rows.add(
        MessageRow(
          message,
          first: i == 0,
          last: i == group.length - 1,
          own: message.authorId == selfId,
          mentionsMe: mentionsUser(message.content, selfId),
        ),
      );
    }
    group.clear();
    previous = null;
  }

  for (final message in messages) {
    final day = _day(message.createdAt);
    if (day != currentDay) {
      endGroup();
      rows.add(DayRow(day));
      currentDay = day;
    }
    if (!unreadPlaced &&
        lastReadId != null &&
        message.id > lastReadId &&
        message.authorId != selfId &&
        message.sendState == SendState.sent) {
      endGroup();
      rows.add(const UnreadRow());
      unreadPlaced = true;
    }
    if (message.isSystem) {
      endGroup();
      rows.add(SystemRow(message));
      continue;
    }
    final last = previous;
    final continues =
        last != null &&
        last.authorId == message.authorId &&
        message.replyToId == null &&
        message.createdAt.difference(last.createdAt) < groupWindow;
    if (!continues) endGroup();
    group.add(message);
    previous = message;
  }
  endGroup();
  return rows;
}

/// Where the list's newer half starts (§4.5): at the first message whose
/// id is at least [splitId], or that is still being sent, together with
/// the day and unread rows right before it. Rows before that grow upward
/// from the anchor, rows after it downward, so neither loading history nor
/// receiving messages moves what is on screen.
int splitIndex(List<ChatRow> rows, int splitId) {
  var index = rows.indexWhere(
    (row) => switch (row) {
      MessageRow(:final message) || SystemRow(:final message) =>
        message.id >= splitId || message.sendState != SendState.sent,
      _ => false,
    },
  );
  if (index == -1) return rows.length;
  while (index > 0 &&
      (rows[index - 1] is DayRow || rows[index - 1] is UnreadRow)) {
    index--;
  }
  return index;
}

/// The message a row shows, if it shows one.
Message? messageOf(ChatRow row) => switch (row) {
  MessageRow(:final message) || SystemRow(:final message) => message,
  _ => null,
};

/// The day a row belongs to, for the floating date while scrolling.
DateTime? dayOf(ChatRow row) => switch (row) {
  DayRow(:final day) => day,
  MessageRow(:final message) ||
  SystemRow(:final message) => _day(message.createdAt),
  _ => null,
};
