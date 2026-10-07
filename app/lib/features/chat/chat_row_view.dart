import 'package:flutter/material.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/chat_rows.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/message_line.dart';
import 'package:opencord/features/chat/message_rows.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/avatar.dart';

/// What rows look up while they draw: names, roles and replied-to
/// messages, from the server's state and the loaded history.
class ChatLookups {
  ChatLookups({required this.data, required List<Message> messages})
    : _byId = {for (final message in messages) message.id: message};

  final ServerData data;
  final Map<int, Message> _byId;

  int get selfId => data.self.id;

  Message? message(int id) => _byId[id];

  String name(int userId) =>
      data.members[userId]?.displayName ??
      (userId == data.self.id ? data.self.displayName : 'Unknown user');

  /// The author's highest role, never @everyone.
  String? role(int userId) {
    final member = data.members[userId];
    if (member == null) return null;
    final role = data.highestRole(member);
    return role == null || role.id == data.info.everyoneRoleId
        ? null
        : role.name;
  }

  String? channelName(int id) => data.channels[id]?.name;

  ReplyPreview? reply(Message message) {
    final id = message.replyToId;
    if (id == null) return null;
    final original = _byId[id];
    if (original == null) {
      return const ReplyPreview(
        author: 'Reply',
        text: 'Original message not loaded',
      );
    }
    return ReplyPreview(
      author: name(original.authorId),
      text: previewText(
        original.content,
        user: (id) => data.members[id]?.displayName,
        channel: channelName,
      ),
    );
  }

  /// "Kai, Mira and 3 others", for the reaction tooltip.
  String reactors(List<int> userIds) {
    final names = [
      for (final id in userIds.take(3)) id == selfId ? 'You' : name(id),
    ];
    final others = userIds.length - names.length;
    if (others > 0) {
      return '${names.join(', ')} and $others other${others == 1 ? '' : 's'}';
    }
    if (names.length < 2) return names.join();
    return '${names.sublist(0, names.length - 1).join(', ')} and ${names.last}';
  }
}

/// What a message row can do, wired by the list.
class ChatRowActions {
  const ChatRowActions({
    required this.links,
    required this.onRetry,
    required this.onReaction,
    required this.onJumpTo,
  });

  final MarkdownContext links;
  final void Function(Message message) onRetry;
  final void Function(Message message, String emoji) onReaction;
  final void Function(int messageId) onJumpTo;
}

/// Draws one row of the message list.
class ChatRowView extends StatelessWidget {
  const ChatRowView({
    super.key,
    required this.row,
    required this.lookups,
    required this.actions,
    required this.channel,
    required this.now,
    this.highlighted = false,
    this.reactions = true,
  });

  final ChatRow row;
  final ChatLookups lookups;
  final ChatRowActions actions;
  final Channel channel;
  final DateTime now;
  final bool highlighted;

  /// Whether reactions can be toggled (the repository may not support
  /// them).
  final bool reactions;

  @override
  Widget build(BuildContext context) {
    final row = this.row;
    return switch (row) {
      StartRow() => StartOfChannel(kind: channel.kind, name: channel.name),
      DayRow(:final day) => Padding(
        padding: const EdgeInsets.only(top: OcSpace.s16, bottom: OcSpace.s4),
        child: CenterPill(text: dayLabel(day, now)),
      ),
      UnreadRow() => const Padding(
        padding: EdgeInsets.only(top: OcSpace.s16, bottom: OcSpace.s4),
        child: UnreadLine(),
      ),
      SystemRow(:final message) => Padding(
        padding: const EdgeInsets.symmetric(vertical: OcSpace.s8),
        child: CenterPill(
          text: systemText(
            message.systemEvent!,
            lookups.name(message.authorId),
          ),
          icon: systemIcon(message.systemEvent!),
        ),
      ),
      MessageRow() => _message(row),
    };
  }

  Widget _message(MessageRow row) {
    final message = row.message;
    final author = lookups.name(message.authorId);
    return MessageLine(
      own: row.own,
      first: row.first,
      avatar: row.showAvatar
          ? OcAvatar(id: '${message.authorId}', name: author)
          : null,
      bubble: MessageBubble(
        message: message,
        own: row.own,
        first: row.first,
        last: row.last,
        author: author,
        role: row.showAuthor ? lookups.role(message.authorId) : null,
        reply: lookups.reply(message),
        mentionsMe: row.mentionsMe,
        highlighted: highlighted,
        links: actions.links,
        reactionNames: lookups.reactors,
        onReaction: reactions
            ? (emoji) => actions.onReaction(message, emoji)
            : null,
        onReplyTap: message.replyToId == null
            ? null
            : () => actions.onJumpTo(message.replyToId!),
        onRetry: () => actions.onRetry(message),
      ),
    );
  }
}

/// Placeholder rows while history loads (§4.13): blocks in `hover`, no
/// spinner.
class ChatSkeleton extends StatelessWidget {
  const ChatSkeleton({super.key, this.rows = 6});

  final int rows;

  static const _widths = [220.0, 320.0, 180.0, 260.0, 140.0, 300.0];

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return ExcludeSemantics(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (var i = 0; i < rows; i++)
            MessageLine(
              own: i % 3 == 2,
              first: true,
              avatar: i % 3 == 2
                  ? null
                  : DecoratedBox(
                      decoration: BoxDecoration(
                        color: colors.hover,
                        shape: BoxShape.circle,
                      ),
                      child: const SizedBox.square(
                        dimension: OcSize.messageAvatar,
                      ),
                    ),
              bubble: Container(
                width: _widths[i % _widths.length],
                height: i.isEven ? 40 : 58,
                decoration: BoxDecoration(
                  color: colors.hover,
                  borderRadius: BorderRadius.circular(OcRadius.bubble),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
