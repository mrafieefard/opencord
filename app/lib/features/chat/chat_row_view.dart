import 'package:flutter/material.dart';

import 'package:opencord/core/model/author.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/chat/attachments/summary.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/chat_rows.dart';
import 'package:opencord/features/chat/command_rows.dart';
import 'package:opencord/features/chat/compact_message.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/message_actions.dart';
import 'package:opencord/features/chat/message_item.dart';
import 'package:opencord/features/chat/message_rows.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// What rows look up while they draw: names, roles and replied-to
/// messages, from the server's state and the loaded history.
class ChatLookups {
  ChatLookups({required this.data, required List<Message> messages})
    : _byId = {for (final message in messages) message.id: message};

  final ServerData data;
  final Map<int, Message> _byId;

  int get selfId => data.self.id;

  Message? message(int id) => _byId[id];

  Author authorOf(int userId) => data.authorOf(userId);

  String name(int userId) => authorOf(userId).name;

  /// The author's highest role, never @everyone.
  String? role(int userId) => authorOf(userId).role;

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
        messageSummary(original),
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
    required this.message,
    required this.onRetry,
    this.onCancelSend,
    required this.onReaction,
    required this.onJumpTo,
    required this.onProfile,
    required this.onDismissNote,
  });

  final MarkdownContext links;

  /// Reply, react, edit, the context menu (§4.5).
  final MessageActions message;
  final void Function(Message message) onRetry;
  final void Function(Message message)? onCancelSend;
  final void Function(Message message, String emoji) onReaction;
  final void Function(int messageId) onJumpTo;

  /// Opens someone's profile from their avatar or name (§16).
  final void Function(int userId) onProfile;

  /// Takes an "only you" reply away (Phase 3 §17.3).
  final void Function(EphemeralNote note) onDismissNote;
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
        padding: EdgeInsets.only(top: 12, bottom: 6),
        child: UnreadLine(),
      ),
      SystemRow(:final message) => Padding(
        padding: const EdgeInsets.only(top: 6),
        child: CenterPill(
          text: systemText(
            message.systemEvent!,
            lookups.name(message.authorId),
          ),
          icon: systemIcon(message.systemEvent!),
        ),
      ),
      CommandRow(:final message) => CommandResultLine(
        message: message,
        author: lookups.name(message.authorId),
        links: actions.links,
      ),
      EphemeralRow(:final note) => EphemeralLine(
        note: note,
        links: actions.links,
        onDismiss: () => actions.onDismissNote(note),
      ),
      MessageRow() => _message(row),
    };
  }

  Widget _message(MessageRow row) {
    final message = row.message;
    final author = lookups.authorOf(message.authorId);
    final onReaction = reactions
        ? (String emoji) => actions.onReaction(message, emoji)
        : null;
    final onReplyTap = message.replyToId == null
        ? null
        : () => actions.onJumpTo(message.replyToId!);
    Widget avatar() => Hoverable(
      onTap: author.isPlugin ? null : () => actions.onProfile(message.authorId),
      semanticLabel: '${author.name}, profile',
      focusRadius: BorderRadius.circular(OcSize.messageAvatar / 2),
      builder: (context, state) => MemberAvatar(
        serverKey: actions.message.channel.server,
        userId: message.authorId,
        name: author.name,
      ),
    );
    return MessageItem(
      message: message,
      own: row.own,
      first: row.first,
      actions: actions.message,
      builder: (onSelectionChanged) => CompactMessage(
        message: message,
        serverKey: actions.message.channel.server,
        first: row.first,
        author: author.name,
        isPlugin: author.isPlugin,
        avatar: row.first ? avatar() : null,
        role: row.first ? author.role : null,
        reply: lookups.reply(message),
        mentionsMe: row.mentionsMe,
        highlighted: highlighted,
        links: actions.links,
        reactionNames: lookups.reactors,
        onReaction: onReaction,
        onReplyTap: onReplyTap,
        onRetry: () => actions.onRetry(message),
        onCancelSend: message.nonce != null
            ? () => actions.onCancelSend?.call(message)
            : null,
        onSelectionChanged: onSelectionChanged,
        onAuthorTap: author.isPlugin
            ? null
            : () => actions.onProfile(message.authorId),
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
      child: SingleChildScrollView(
        physics: const NeverScrollableScrollPhysics(),
        reverse: true,
        child: Padding(
          padding: const EdgeInsets.symmetric(
            horizontal: OcSpace.s16,
            vertical: OcSpace.s8,
          ),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (var i = 0; i < rows; i++) ...[
                if (i > 0 && i % 3 == 0) const SizedBox(height: OcSpace.s12),
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 3.0),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      SizedBox(
                        width: OcSize.messageAvatar,
                        child: i % 3 == 0
                            ? DecoratedBox(
                                decoration: BoxDecoration(
                                  color: colors.hover,
                                  shape: BoxShape.circle,
                                ),
                                child: const SizedBox.square(
                                  dimension: OcSize.messageAvatar,
                                ),
                              )
                            : null,
                      ),
                      const SizedBox(width: OcSpace.s12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            if (i % 3 == 0) ...[
                              Container(
                                width: 80,
                                height: 12,
                                margin: const EdgeInsets.only(bottom: 6),
                                decoration: BoxDecoration(
                                  color: colors.hover,
                                  borderRadius: BorderRadius.circular(4),
                                ),
                              ),
                            ],
                            Container(
                              width: _widths[i % _widths.length],
                              height: 14,
                              decoration: BoxDecoration(
                                color: colors.hover,
                                borderRadius: BorderRadius.circular(4),
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
