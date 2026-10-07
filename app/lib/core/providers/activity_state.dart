import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/repository/events.dart';

/// Mentions are written `<@userId>` in message content.
bool mentionsUser(String content, int userId) => content.contains('<@$userId>');

/// The newest message of a channel and how much of it was read: what a
/// channel row shows (§4.2).
@immutable
class ChannelActivity {
  const ChannelActivity({this.last, this.read = ReadState.empty});

  static const none = ChannelActivity();

  final Message? last;
  final ReadState read;

  bool get unread => read.unread > 0;
}

/// Per-channel activity of one server, plus which channel is being read.
@immutable
class ActivityState {
  const ActivityState({
    this.selfId = 0,
    this.channels = const {},
    this.focused,
  });

  static const empty = ActivityState();

  final int selfId;
  final Map<int, ChannelActivity> channels;

  /// The channel the user is reading right now (open, scrolled to the
  /// bottom, window focused); its messages never count as unread.
  final int? focused;

  ChannelActivity of(int channelId) =>
      channels[channelId] ?? ChannelActivity.none;

  bool get hasUnread => channels.values.any((channel) => channel.unread);

  int get mentions =>
      channels.values.fold(0, (sum, channel) => sum + channel.read.mentions);

  ActivityState _with(int channelId, ChannelActivity activity) => ActivityState(
    selfId: selfId,
    channels: {...channels, channelId: activity},
    focused: focused,
  );
}

ActivityState reduceActivity(ActivityState state, RepoEvent event) {
  switch (event) {
    case Ready(:final snapshot):
      final ids = {...snapshot.lastMessages.keys, ...snapshot.readStates.keys};
      return ActivityState(
        selfId: snapshot.self.id,
        focused: state.focused,
        channels: {
          for (final id in ids)
            id: ChannelActivity(
              last: snapshot.lastMessages[id],
              read: snapshot.readStates[id] ?? ReadState.empty,
            ),
        },
      );
    case MessageCreated(:final message):
      final current = state.of(message.channelId);
      final last = current.last;
      final newest = last == null || message.id > last.id ? message : last;
      final read = current.read;
      final caughtUp =
          message.authorId == state.selfId ||
          message.channelId == state.focused;
      return state._with(
        message.channelId,
        ChannelActivity(
          last: newest,
          read: caughtUp
              ? ReadState(lastReadId: newest.id)
              : message.id <= read.lastReadId
              ? read
              : ReadState(
                  lastReadId: read.lastReadId,
                  unread: read.unread + 1,
                  mentions:
                      read.mentions +
                      (mentionsUser(message.content, state.selfId) ? 1 : 0),
                ),
        ),
      );
    case MessageUpdated(:final message):
      final current = state.of(message.channelId);
      if (current.last?.id != message.id) return state;
      return state._with(
        message.channelId,
        ChannelActivity(last: message, read: current.read),
      );
    case MessageDeleted(:final channelId, :final messageId):
      final current = state.of(channelId);
      if (current.last?.id != messageId) return state;
      return state._with(channelId, ChannelActivity(read: current.read));
    case ChannelDeleted(:final channelId):
      if (!state.channels.containsKey(channelId)) return state;
      return ActivityState(
        selfId: state.selfId,
        channels: {...state.channels}..remove(channelId),
        focused: state.focused == channelId ? null : state.focused,
      );
    default:
      return state;
  }
}

ActivityState markChannelRead(ActivityState state, int channelId) {
  final current = state.of(channelId);
  final newest = current.last?.id ?? current.read.lastReadId;
  if (current.read == ReadState(lastReadId: newest)) return state;
  return state._with(
    channelId,
    ChannelActivity(
      last: current.last,
      read: ReadState(lastReadId: newest),
    ),
  );
}

ActivityState focusChannel(ActivityState state, int? channelId) {
  if (state.focused == channelId) return state;
  return ActivityState(
    selfId: state.selfId,
    channels: state.channels,
    focused: channelId,
  );
}
