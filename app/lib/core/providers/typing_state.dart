import 'package:flutter/foundation.dart';

import 'package:opencord/core/repository/events.dart';

/// How long someone shows as typing after their last signal (§6).
const typingTimeout = Duration(seconds: 10);

/// Who is typing where on one server, with when each signal runs out.
@immutable
class TypingState {
  const TypingState([this.byChannel = const {}]);

  static const empty = TypingState();

  final Map<int, Map<int, DateTime>> byChannel;

  /// Users typing in [channelId] at [now], in the order they started.
  List<int> typingIn(int channelId, DateTime now) => [
    for (final MapEntry(:key, :value) in (byChannel[channelId] ?? {}).entries)
      if (value.isAfter(now)) key,
  ];

  /// Users typing in [channelId], assuming expired signals were pruned.
  List<int> users(int channelId) => [...?byChannel[channelId]?.keys];

  /// When the next signal runs out, to know when to look again.
  DateTime? get nextExpiry {
    DateTime? earliest;
    for (final users in byChannel.values) {
      for (final expiry in users.values) {
        if (earliest == null || expiry.isBefore(earliest)) earliest = expiry;
      }
    }
    return earliest;
  }
}

TypingState reduceTyping(
  TypingState state,
  RepoEvent event, {
  required DateTime now,
  required int selfId,
}) {
  switch (event) {
    case TypingStarted(:final channelId, :final userId) when userId != selfId:
      final users = {...?state.byChannel[channelId]}
        ..remove(userId)
        ..[userId] = now.add(typingTimeout);
      return TypingState({...state.byChannel, channelId: users});
    case MessageCreated(:final message):
      final users = state.byChannel[message.channelId];
      if (users == null || !users.containsKey(message.authorId)) return state;
      return TypingState({
        ...state.byChannel,
        message.channelId: {...users}..remove(message.authorId),
      });
    case Ready():
      return TypingState.empty;
    default:
      return state;
  }
}

/// Drops signals that ran out before [now].
TypingState pruneTyping(TypingState state, DateTime now) {
  var changed = false;
  final pruned = <int, Map<int, DateTime>>{};
  for (final MapEntry(key: channel, value: users) in state.byChannel.entries) {
    final live = {
      for (final MapEntry(:key, :value) in users.entries)
        if (value.isAfter(now)) key: value,
    };
    changed |= live.length != users.length;
    if (live.isNotEmpty) pruned[channel] = live;
  }
  return changed ? TypingState(pruned) : state;
}
