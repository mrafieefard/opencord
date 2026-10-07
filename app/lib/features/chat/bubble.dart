import 'package:flutter/material.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/features/chat/markdown.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

final _parsed = <String, List<MdBlock>>{};

/// Parsed markdown, cached: scrolling rebuilds rows often.
List<MdBlock> parsedContent(String content) {
  final cached = _parsed.remove(content);
  final blocks = cached ?? parseMarkdown(content);
  _parsed[content] = blocks;
  if (_parsed.length > 500) _parsed.remove(_parsed.keys.first);
  return blocks;
}

/// The message a reply points at, as the quote shows it.
@immutable
class ReplyPreview {
  const ReplyPreview({required this.author, required this.text});

  final String author;
  final String text;
}

/// A Telegram-style bubble (§4.5): rounded corners that tighten where
/// bubbles of a group meet, the author and their highest role on the first
/// incoming one, a reply quote, the time inline at the end of the text,
/// and reactions underneath.
class MessageBubble extends StatelessWidget {
  const MessageBubble({
    super.key,
    required this.message,
    required this.own,
    required this.first,
    required this.last,
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
  final bool own;
  final bool first;
  final bool last;
  final String author;

  /// The author's highest role, shown muted after the name (never colored).
  final String? role;
  final ReplyPreview? reply;
  final bool mentionsMe;

  /// Briefly set after jumping to this message.
  final bool highlighted;
  final MarkdownContext links;
  final String Function(List<int> userIds)? reactionNames;
  final ValueChanged<String>? onReaction;
  final VoidCallback? onReplyTap;
  final VoidCallback? onRetry;

  /// Makes the text selectable (§4.5) and reports what is selected.
  final ValueChanged<String?>? onSelectionChanged;

  /// Opens the author's profile from their name (§16).
  final VoidCallback? onAuthorTap;

  static const double _big = OcRadius.bubble;
  static const double _small = OcRadius.bubbleGrouped;

  BorderRadius get _radius {
    final top = Radius.circular(first ? _big : _small);
    final bottom = Radius.circular(last ? _big : _small);
    const round = Radius.circular(_big);
    return own
        ? BorderRadius.only(
            topLeft: round,
            bottomLeft: round,
            topRight: top,
            bottomRight: bottom,
          )
        : BorderRadius.only(
            topLeft: top,
            bottomLeft: bottom,
            topRight: round,
            bottomRight: round,
          );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final blocks = parsedContent(message.content);
    final failed = message.sendState == SendState.failed;
    final separateMeta = endsWithCode(blocks) || failed;
    final meta = _Meta(message: message, own: own, onRetry: onRetry);
    final background = highlighted
        ? colors.selected
        : own
        ? colors.bubbleOut
        : colors.bubbleIn;
    final radius = _radius;
    final content = Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 7),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          if (first && !own)
            Padding(
              padding: const EdgeInsets.only(bottom: 2),
              child: _AuthorLine(
                author: author,
                role: role,
                onTap: onAuthorTap,
              ),
            ),
          if (reply != null) _ReplyQuote(reply: reply!, onTap: onReplyTap),
          Stack(
            children: [
              _selectable(
                MarkdownView(
                  blocks: blocks,
                  links: links,
                  color: colors.text,
                  trailing: separateMeta
                      ? Size.zero
                      : _Meta.size(context, message, own: own),
                ),
              ),
              if (!separateMeta) Positioned(right: 0, bottom: -1, child: meta),
            ],
          ),
          if (separateMeta)
            Padding(
              padding: const EdgeInsets.only(top: 4),
              child: Align(alignment: Alignment.centerRight, child: meta),
            ),
          if (message.reactions.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(top: 6, bottom: 1),
              child: Wrap(
                spacing: 4,
                runSpacing: 4,
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
    );
    return AnimatedContainer(
      duration: OcMotion.of(context).morph,
      constraints: const BoxConstraints(maxWidth: OcSize.bubbleMaxWidth),
      clipBehavior: mentionsMe ? Clip.antiAlias : Clip.none,
      decoration: BoxDecoration(
        color: background,
        borderRadius: radius,
        border: own ? null : Border.all(color: colors.border, width: 0.8),
      ),
      // Shrink-wraps the content, so short messages get short bubbles.
      child: IntrinsicWidth(
        child: mentionsMe
            ? Stack(
                children: [
                  content,
                  Positioned(
                    left: 0,
                    top: 0,
                    bottom: 0,
                    child: ColoredBox(
                      color: colors.text,
                      child: const SizedBox(width: 2),
                    ),
                  ),
                ],
              )
            : content,
      ),
    );
  }
}

extension on MessageBubble {
  Widget _selectable(Widget text) => switch (onSelectionChanged) {
    final onChanged? => SelectableMessageText(
      onSelectionChanged: onChanged,
      child: text,
    ),
    null => text,
  };
}

/// Lets the text of one message be selected with the mouse (§4.5) without
/// adding a Tab stop or a menu of its own: the message's context menu
/// offers to copy the selection.
class SelectableMessageText extends StatefulWidget {
  const SelectableMessageText({
    super.key,
    required this.onSelectionChanged,
    required this.child,
  });

  final ValueChanged<String?> onSelectionChanged;
  final Widget child;

  @override
  State<SelectableMessageText> createState() => _SelectableMessageTextState();
}

class _SelectableMessageTextState extends State<SelectableMessageText> {
  final _focus = FocusNode(skipTraversal: true, debugLabel: 'message text');

  @override
  void dispose() {
    _focus.dispose();
    super.dispose();
  }

  /// The row's own context menu shows instead (see `MessageItem`). A null
  /// builder is not an option: SelectableRegion still calls it.
  static Widget _noMenu(BuildContext context, SelectableRegionState state) =>
      const SizedBox.shrink();

  @override
  Widget build(BuildContext context) => SelectionArea(
    focusNode: _focus,
    contextMenuBuilder: _noMenu,
    onSelectionChanged: (content) {
      // The space kept for the time is a placeholder character.
      final text = content?.plainText.replaceAll('\uFFFC', '').trim();
      widget.onSelectionChanged(text == null || text.isEmpty ? null : text);
    },
    child: widget.child,
  );
}

class _AuthorLine extends StatelessWidget {
  const _AuthorLine({required this.author, this.role, this.onTap});

  final String author;
  final String? role;

  /// Opens the author's profile (§16).
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final style = OcText.bodyStrong.copyWith(fontSize: 13, color: colors.text);
    Text line({required bool underline}) => Text.rich(
      TextSpan(
        style: style,
        children: [
          TextSpan(
            text: author,
            style: underline
                ? const TextStyle(decoration: TextDecoration.underline)
                : null,
          ),
          if (role != null)
            TextSpan(
              text: ' · $role',
              style: OcText.body.copyWith(
                fontSize: 13,
                color: colors.textMuted,
              ),
            ),
        ],
      ),
      maxLines: 1,
      overflow: TextOverflow.ellipsis,
    );
    if (onTap == null) return line(underline: false);
    return Align(
      alignment: Alignment.centerLeft,
      widthFactor: 1,
      child: Hoverable(
        onTap: onTap,
        semanticLabel: author,
        focusRadius: BorderRadius.circular(4),
        builder: (context, state) => line(underline: state.hovered),
      ),
    );
  }
}

/// Time, "edited", and for your own messages a status mark: a clock while
/// sending, a double check once delivered, an outlined error with Retry
/// when it failed (§4.5, §6).
class _Meta extends StatelessWidget {
  const _Meta({required this.message, required this.own, this.onRetry});

  final Message message;
  final bool own;
  final VoidCallback? onRetry;

  static String _text(Message message) =>
      '${message.edited ? 'edited ' : ''}${clockTime(message.createdAt)}';

  /// The space the meta needs at the end of the last line.
  static Size size(BuildContext context, Message message, {required bool own}) {
    final painter = TextPainter(
      text: TextSpan(text: _text(message), style: OcText.meta),
      textDirection: TextDirection.ltr,
      textScaler: MediaQuery.textScalerOf(context),
      maxLines: 1,
    )..layout();
    const gap = 8.0;
    final icon = own ? 4.0 + _iconSize : 0.0;
    final size = Size(
      gap + painter.width + icon,
      painter.height > _iconSize ? painter.height : _iconSize,
    );
    painter.dispose();
    return size;
  }

  static const double _iconSize = 14;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final failed = message.sendState == SendState.failed;
    return Tooltip(
      message: fullTimestamp(message.createdAt),
      waitDuration: const Duration(milliseconds: 450),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (failed) ...[
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
            const SizedBox(width: 8),
          ],
          Text(
            _text(message),
            style: OcText.meta.copyWith(color: colors.textMuted),
          ),
          if (failed) ...[
            const SizedBox(width: 4),
            Icon(
              OcIcons.error,
              size: _iconSize,
              fill: 0,
              color: colors.text,
              semanticLabel: 'Not sent',
            ),
          ],
          if (own && !failed) ...[
            const SizedBox(width: 4),
            Icon(
              message.sendState == SendState.pending
                  ? OcIcons.schedule
                  : OcIcons.doneAll,
              size: _iconSize,
              color: colors.textMuted,
              semanticLabel: message.sendState == SendState.pending
                  ? 'Sending'
                  : 'Delivered',
            ),
          ],
        ],
      ),
    );
  }
}

class _ReplyQuote extends StatelessWidget {
  const _ReplyQuote({required this.reply, this.onTap});

  final ReplyPreview reply;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.only(top: 2, bottom: 6),
      child: Hoverable(
        onTap: onTap,
        semanticLabel: 'Reply to ${reply.author}: ${reply.text}',
        focusRadius: BorderRadius.circular(OcRadius.quote),
        builder: (context, state) => ClipRRect(
          borderRadius: BorderRadius.circular(OcRadius.quote),
          child: Container(
            color: colors.text.withValues(alpha: state.active ? 0.1 : 0.06),
            child: IntrinsicHeight(
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Container(width: 2, color: colors.text),
                  Flexible(
                    child: Padding(
                      padding: const EdgeInsets.fromLTRB(8, 4, 10, 4),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Text(
                            reply.author,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: OcText.small.copyWith(
                              fontWeight: FontWeight.w600,
                              color: colors.text,
                            ),
                          ),
                          Text(
                            reply.text,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: OcText.small.copyWith(
                              color: colors.textSecondary,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// "👍 3": inverted when you reacted; a click toggles yours (§4.5).
class ReactionChip extends StatelessWidget {
  const ReactionChip({
    super.key,
    required this.reaction,
    this.names,
    this.onTap,
  });

  final Reaction reaction;
  final String? names;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final mine = reaction.me;
    final chip = Hoverable(
      onTap: onTap,
      semanticLabel:
          '${reaction.emoji} ${reaction.count}${mine ? ', including you' : ''}',
      selected: mine,
      focusRadius: BorderRadius.circular(OcRadius.reaction),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: 24,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        decoration: BoxDecoration(
          color: mine
              ? colors.accent
              : state.active
              ? colors.selected
              : null,
          borderRadius: BorderRadius.circular(OcRadius.reaction),
          border: mine ? null : Border.all(color: colors.border),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              reaction.emoji,
              style: const TextStyle(fontSize: 13, height: 1),
            ),
            const SizedBox(width: 4),
            Text(
              '${reaction.count}',
              style: OcText.small.copyWith(
                fontWeight: FontWeight.w600,
                height: 1,
                color: mine ? colors.onAccent : colors.text,
              ),
            ),
          ],
        ),
      ),
    );
    return names == null ? chip : Tooltip(message: names!, child: chip);
  }
}
