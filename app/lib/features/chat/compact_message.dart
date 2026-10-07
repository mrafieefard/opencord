import 'package:flutter/material.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// A message in compact density (§8.1): no bubble, a left-aligned line with
/// the time, the author on the first message of a group, then the text.
/// Everyone's messages sit on the left, your own too.
class CompactMessage extends StatelessWidget {
  const CompactMessage({
    super.key,
    required this.message,
    required this.first,
    required this.author,
    required this.links,
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
  });

  final Message message;
  final bool first;
  final String author;
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

  /// Room for "23:59" in the time column.
  static const double timeWidth = 44;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final failed = message.sendState == SendState.failed;
    final pending = message.sendState == SendState.pending;
    final muted = OcText.meta.copyWith(color: colors.textMuted);
    final text = MarkdownView(
      blocks: parsedContent(message.content),
      links: links,
      color: colors.text,
      lead: [
        if (first) ...[
          if (onAuthorTap case final onTap?)
            WidgetSpan(
              alignment: PlaceholderAlignment.baseline,
              baseline: TextBaseline.alphabetic,
              child: Hoverable(
                onTap: onTap,
                semanticLabel: author,
                focusRadius: BorderRadius.circular(4),
                builder: (context, state) => Text(
                  author,
                  style: OcText.bodyStrong.copyWith(
                    color: colors.text,
                    decoration: state.hovered ? TextDecoration.underline : null,
                    decorationColor: colors.text,
                  ),
                ),
              ),
            )
          else
            TextSpan(
              text: author,
              style: OcText.bodyStrong.copyWith(color: colors.text),
            ),
          if (role != null)
            TextSpan(
              text: ' · $role',
              style: OcText.body.copyWith(color: colors.textMuted),
            ),
          const TextSpan(text: '  '),
        ],
      ],
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
      duration: OcMotion.of(context).morph,
      margin: EdgeInsets.only(top: first ? OcSpace.s6 : 0),
      padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8, vertical: 1),
      decoration: BoxDecoration(
        color: highlighted
            ? colors.selected
            : mentionsMe
            ? colors.mentionBg
            : null,
        borderRadius: BorderRadius.circular(OcRadius.quote),
      ),
      // The mention bar is drawn over the edge, so the time column stays
      // in line with the other messages.
      foregroundDecoration: mentionsMe
          ? BoxDecoration(
              border: Border(left: BorderSide(color: colors.text, width: 2)),
            )
          : null,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: timeWidth,
            child: Padding(
              padding: const EdgeInsets.only(top: 3),
              child: Tooltip(
                message: fullTimestamp(message.createdAt),
                waitDuration: const Duration(milliseconds: 450),
                child: Text(clockTime(message.createdAt), style: muted),
              ),
            ),
          ),
          const SizedBox(width: OcSpace.s8),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                if (reply case final reply?)
                  _CompactReply(reply: reply, onTap: onReplyTap),
                switch (onSelectionChanged) {
                  final onChanged? => SelectableMessageText(
                    onSelectionChanged: onChanged,
                    child: text,
                  ),
                  null => text,
                },
                if (failed)
                  Padding(
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
                              decoration: state.active
                                  ? TextDecoration.underline
                                  : null,
                              decorationColor: colors.text,
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                if (message.reactions.isNotEmpty)
                  Padding(
                    padding: const EdgeInsets.only(top: OcSpace.s4, bottom: 2),
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
