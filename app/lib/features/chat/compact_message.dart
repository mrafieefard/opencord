import 'package:flutter/material.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/attachments/attachment_view.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/message_item.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// A message in compact density (§8.1), laid out as Discord's: the first
/// message of a group has the author's avatar, then their name, role and
/// the time above the text; the ones after it are just text, with the time
/// in the avatar's column while hovered. Everyone's messages sit on the
/// left, your own too.
class CompactMessage extends StatelessWidget {
  const CompactMessage({
    super.key,
    required this.message,
    required this.first,
    required this.author,
    required this.links,
    this.avatar,
    this.role,
    this.reply,
    this.mentionsMe = false,
    this.highlighted = false,
    this.reactionNames,
    this.onReaction,
    this.onReplyTap,
    this.onRetry,
    this.onSelectionChanged,
    this.onAuthorTap,
    required this.serverKey,
  });

  final Message message;
  final String serverKey;
  final bool first;
  final String author;

  /// The author's avatar, shown on the first message of a group.
  final Widget? avatar;
  final String? role;
  final ReplyPreview? reply;
  final bool mentionsMe;
  final bool highlighted;
  final MarkdownContext links;
  final String Function(List<int> userIds)? reactionNames;
  final ValueChanged<String>? onReaction;
  final VoidCallback? onReplyTap;
  final VoidCallback? onRetry;
  final ValueChanged<String?>? onSelectionChanged;

  /// Opens the author's profile from their name (§16).
  final VoidCallback? onAuthorTap;

  /// The avatar's column, which holds the time on later messages.
  static const double gutter = OcSize.messageAvatar;
  static const double _gap = OcSpace.s12;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final hovered = MessageHover.of(context);
    final failed = message.sendState == SendState.failed;
    final pending = message.sendState == SendState.pending;
    final muted = OcText.meta.copyWith(color: colors.textMuted);
    final time = Tooltip(
      message: fullTimestamp(message.createdAt),
      waitDuration: const Duration(milliseconds: 450),
      child: Text(clockTime(message.createdAt), style: muted),
    );
    final text = MarkdownView(
      blocks: parsedContent(message.content),
      links: links,
      color: colors.text,
      tail: [
        if (message.edited) TextSpan(text: '  (edited)', style: muted),
        if (pending)
          WidgetSpan(
            alignment: PlaceholderAlignment.middle,
            child: Padding(
              padding: const EdgeInsets.only(left: OcSpace.s6),
              child: Icon(
                OcIcons.schedule,
                size: 13,
                color: colors.textMuted,
                semanticLabel: 'Sending',
              ),
            ),
          ),
      ],
    );
    return AnimatedContainer(
      duration: OcMotion.of(context).hover,
      margin: EdgeInsets.only(top: first ? OcSpace.s16 : 0),
      padding: EdgeInsets.fromLTRB(
        OcSpace.s16,
        first ? OcSpace.s4 : 2,
        OcSpace.s16,
        2,
      ),
      decoration: BoxDecoration(
        color: highlighted
            ? colors.selected
            : mentionsMe
            ? colors.mentionBg
            : hovered
            ? colors.hover
            : null,
      ),
      // The mention bar is drawn over the edge, so the avatar column stays
      // in line with the other messages.
      foregroundDecoration: mentionsMe
          ? BoxDecoration(
              border: Border(left: BorderSide(color: colors.text, width: 2)),
            )
          : null,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (reply case final reply?)
            Padding(
              padding: const EdgeInsets.only(left: gutter + _gap),
              child: _CompactReply(reply: reply, onTap: onReplyTap),
            ),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              SizedBox(
                width: gutter,
                child: first
                    ? Padding(
                        padding: const EdgeInsets.only(top: 2),
                        child: SizedBox.square(
                          dimension: gutter,
                          child: avatar,
                        ),
                      )
                    : hovered
                    ? Padding(
                        padding: const EdgeInsets.only(top: 3),
                        child: Center(child: time),
                      )
                    : null,
              ),
              const SizedBox(width: _gap),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (first)
                      _Header(
                        author: author,
                        role: role,
                        time: time,
                        onAuthorTap: onAuthorTap,
                      ),
                    switch (onSelectionChanged) {
                      final onChanged? => SelectableMessageText(
                        onSelectionChanged: onChanged,
                        child: text,
                      ),
                      null => text,
                    },
                    if (message.attachments.isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.only(top: 4),
                        child: AttachmentsView(
                          serverKey: serverKey,
                          message: message,
                        ),
                      ),
                    if (failed) _Retry(onRetry: onRetry),
                    if (message.reactions.isNotEmpty)
                      Padding(
                        padding: const EdgeInsets.only(
                          top: OcSpace.s4,
                          bottom: 2,
                        ),
                        child: Wrap(
                          spacing: OcSpace.s4,
                          runSpacing: OcSpace.s4,
                          children: [
                            for (final reaction in message.reactions)
                              ReactionChip(
                                reaction: reaction,
                                names: reactionNames?.call(reaction.userIds),
                                onTap: onReaction == null
                                    ? null
                                    : () => onReaction!(reaction.emoji),
                              ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// "Kai Nakamura  Maintainer  10:20" over a group's first message.
class _Header extends StatelessWidget {
  const _Header({
    required this.author,
    required this.role,
    required this.time,
    this.onAuthorTap,
  });

  final String author;
  final String? role;
  final Widget time;
  final VoidCallback? onAuthorTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    Widget name(bool hovered) => Text(
      author,
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
      style: OcText.bodyStrong.copyWith(
        color: colors.text,
        decoration: hovered ? TextDecoration.underline : null,
        decorationColor: colors.text,
      ),
    );
    return Padding(
      padding: const EdgeInsets.only(bottom: 1),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.center,
        children: [
          Flexible(
            child: switch (onAuthorTap) {
              final onTap? => Hoverable(
                onTap: onTap,
                semanticLabel: author,
                focusRadius: BorderRadius.circular(4),
                builder: (context, state) => name(state.hovered),
              ),
              null => name(false),
            },
          ),
          if (role case final role?) ...[
            const SizedBox(width: OcSpace.s6),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
              decoration: BoxDecoration(
                color: colors.hover,
                borderRadius: BorderRadius.circular(4),
                border: Border.all(color: colors.border),
              ),
              child: Text(
                role,
                maxLines: 1,
                style: OcText.meta.copyWith(
                  color: colors.textSecondary,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ),
          ],
          const SizedBox(width: OcSpace.s8),
          time,
        ],
      ),
    );
  }
}

/// "Not sent · Retry" under a failed message.
class _Retry extends StatelessWidget {
  const _Retry({this.onRetry});

  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.only(top: 2),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(
            OcIcons.error,
            size: 14,
            fill: 0,
            color: colors.text,
            semanticLabel: 'Not sent',
          ),
          const SizedBox(width: OcSpace.s6),
          Hoverable(
            onTap: onRetry,
            semanticLabel: 'Retry sending',
            focusRadius: BorderRadius.circular(4),
            builder: (context, state) => Text(
              'Retry',
              style: OcText.meta.copyWith(
                color: colors.text,
                fontWeight: FontWeight.w700,
                decoration: state.active ? TextDecoration.underline : null,
                decorationColor: colors.text,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// "↳ Kai: the original…" above a reply in compact density.
class _CompactReply extends StatelessWidget {
  const _CompactReply({required this.reply, this.onTap});

  final ReplyPreview reply;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: 'Reply to ${reply.author}: ${reply.text}',
      focusRadius: BorderRadius.circular(4),
      builder: (context, state) => Padding(
        padding: const EdgeInsets.only(bottom: 1),
        child: Row(
          children: [
            Icon(OcIcons.reply, size: 13, color: colors.textMuted),
            const SizedBox(width: OcSpace.s4),
            Flexible(
              child: Text.rich(
                TextSpan(
                  children: [
                    TextSpan(
                      text: reply.author,
                      style: TextStyle(
                        fontWeight: FontWeight.w600,
                        color: state.active
                            ? colors.text
                            : colors.textSecondary,
                      ),
                    ),
                    TextSpan(text: '  ${reply.text}'),
                  ],
                ),
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.small.copyWith(color: colors.textMuted),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
