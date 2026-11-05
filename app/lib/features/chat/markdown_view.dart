import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'package:opencord/features/chat/markdown.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Looks up names for mentions, and handles clicks on links.
@immutable
class MarkdownContext {
  const MarkdownContext({
    required this.userName,
    required this.channelName,
    this.onLink,
    this.onUser,
    this.onChannel,
  });

  final String? Function(int id) userName;
  final String? Function(int id) channelName;
  final ValueChanged<String>? onLink;
  final ValueChanged<int>? onUser;
  final ValueChanged<int>? onChannel;
}

/// Renders parsed markdown (§4.5). [trailing] is reserved at the end of the
/// last line of text, where the bubble puts its time (Telegram's trick): if
/// it does not fit there, it wraps onto a line of its own.
class MarkdownView extends StatefulWidget {
  const MarkdownView({
    super.key,
    required this.blocks,
    required this.links,
    required this.color,
    this.trailing = Size.zero,
    this.lead = const [],
    this.tail = const [],
  });

  final List<MdBlock> blocks;
  final MarkdownContext links;
  final Color color;
  final Size trailing;

  /// Put before the first paragraph, like the author in compact mode;
  /// on a line of its own when the message starts with a quote or code.
  final List<InlineSpan> lead;

  /// Put after the last paragraph, like "(edited)"; on a line of its own
  /// after code.
  final List<InlineSpan> tail;

  @override
  State<MarkdownView> createState() => _MarkdownViewState();
}

class _MarkdownViewState extends State<MarkdownView> {
  final _recognizers = <GestureRecognizer>[];

  void _clearRecognizers() {
    for (final recognizer in _recognizers) {
      recognizer.dispose();
    }
    _recognizers.clear();
  }

  @override
  void dispose() {
    _clearRecognizers();
    super.dispose();
  }

  TapGestureRecognizer _tap(VoidCallback onTap) {
    final recognizer = TapGestureRecognizer()..onTap = onTap;
    _recognizers.add(recognizer);
    return recognizer;
  }

  List<InlineSpan> _spans(List<MdInline> inlines, TextStyle base) {
    final colors = context.oc;
    final links = widget.links;
    return [
      for (final inline in inlines)
        switch (inline) {
          MdText(:final text, :final bold, :final italic, :final strike) =>
            TextSpan(
              text: text,
              style: base.copyWith(
                fontWeight: bold ? FontWeight.w600 : null,
                fontStyle: italic ? FontStyle.italic : null,
                decoration: strike ? TextDecoration.lineThrough : null,
                decorationColor: base.color,
              ),
            ),
          MdInlineCode(:final code) => WidgetSpan(
            alignment: PlaceholderAlignment.baseline,
            baseline: TextBaseline.alphabetic,
            // The pill's 1 above and below is painted and not laid out, so a
            // line with code in it is as tall as any other.
            child: CustomPaint(
              painter: _CodePill(colors.mentionBg),
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4),
                child: Text(
                  code,
                  style: OcText.mono.copyWith(
                    fontSize: 12.5,
                    color: base.color,
                  ),
                ),
              ),
            ),
          ),
          MdLink(:final url) => TextSpan(
            text: url,
            style: base.copyWith(
              decoration: TextDecoration.underline,
              decorationColor: base.color,
            ),
            mouseCursor: SystemMouseCursors.click,
            recognizer: links.onLink == null
                ? null
                : _tap(() => links.onLink!(url)),
          ),
          MdUserMention(:final userId) => WidgetSpan(
            alignment: PlaceholderAlignment.baseline,
            baseline: TextBaseline.alphabetic,
            child: MouseRegion(
              cursor: SystemMouseCursors.click,
              child: GestureDetector(
                onTap: links.onUser == null
                    ? null
                    : () => links.onUser!(userId),
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 3,
                    vertical: 0,
                  ),
                  decoration: BoxDecoration(
                    color: colors.mentionBg,
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text(
                    '@${links.userName(userId) ?? 'unknown'}',
                    style: base.copyWith(fontWeight: FontWeight.w600),
                  ),
                ),
              ),
            ),
          ),
          MdChannelMention(:final channelId) => WidgetSpan(
            alignment: PlaceholderAlignment.baseline,
            baseline: TextBaseline.alphabetic,
            child: MouseRegion(
              cursor: SystemMouseCursors.click,
              child: GestureDetector(
                onTap: links.onChannel == null
                    ? null
                    : () => links.onChannel!(channelId),
                child: Container(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 3,
                    vertical: 0,
                  ),
                  decoration: BoxDecoration(
                    color: colors.mentionBg,
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: Text(
                    '#${links.channelName(channelId) ?? 'unknown'}',
                    style: base.copyWith(fontWeight: FontWeight.w600),
                  ),
                ),
              ),
            ),
          ),
        },
    ];
  }

  @override
  Widget build(BuildContext context) {
    _clearRecognizers();
    final colors = context.oc;
    final base = OcText.body.copyWith(color: widget.color);
    final blocks = widget.blocks;
    final children = <Widget>[];
    final lead = widget.lead;
    final tail = widget.tail;
    final leadInline = blocks.firstOrNull is MdParagraph;
    final tailInline = blocks.lastOrNull is! MdCode;
    if (lead.isNotEmpty && !leadInline) {
      children.add(Text.rich(TextSpan(style: base, children: lead)));
    }
    for (var i = 0; i < blocks.length; i++) {
      final block = blocks[i];
      final last = i == blocks.length - 1;
      final before = i == 0 && leadInline ? lead : const <InlineSpan>[];
      final after = last && tailInline ? tail : const <InlineSpan>[];
      final reserve = last && widget.trailing != Size.zero
          ? [
              WidgetSpan(
                alignment: PlaceholderAlignment.bottom,
                child: SizedBox.fromSize(size: widget.trailing),
              ),
            ]
          : const <InlineSpan>[];
      // A quote's own margin is the gap on either side of it. A lead on its
      // own line, before the first block, keeps the usual gap.
      if (children.isNotEmpty &&
          (i == 0 || (block is! MdQuote && blocks[i - 1] is! MdQuote))) {
        children.add(const SizedBox(height: OcSpace.s4));
      }
      children.add(switch (block) {
        MdParagraph(:final inlines) => Text.rich(
          TextSpan(
            style: base,
            children: [
              ...before,
              ..._spans(inlines, base),
              ...after,
              ...reserve,
            ],
          ),
        ),
        MdQuote(:final inlines) => Container(
          margin: const EdgeInsets.fromLTRB(0, 1, 0, 2),
          padding: const EdgeInsets.only(left: OcSpace.s8),
          decoration: BoxDecoration(
            border: Border(left: BorderSide(color: colors.textMuted, width: 2)),
          ),
          child: Text.rich(
            TextSpan(
              style: base,
              children: [
                ..._spans(inlines, base.copyWith(color: colors.textSecondary)),
                ...after,
                ...reserve,
              ],
            ),
          ),
        ),
        MdCode(:final code, :final language) => CodeBlock(
          code: code,
          language: language,
        ),
      });
    }
    if (tail.isNotEmpty && !tailInline) {
      children.add(Text.rich(TextSpan(style: base, children: tail)));
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: children,
    );
  }
}

/// The pill behind inline code (§5.11): its box, and 1 above and below.
class _CodePill extends CustomPainter {
  const _CodePill(this.color);

  final Color color;

  @override
  void paint(Canvas canvas, Size size) => canvas.drawRRect(
    RRect.fromRectAndRadius(
      Rect.fromLTRB(0, -1, size.width, size.height + 1),
      const Radius.circular(4),
    ),
    Paint()..color = color,
  );

  @override
  bool shouldRepaint(_CodePill oldDelegate) => color != oldDelegate.color;
}

/// A fenced code block (§4.5): full bubble width, an optional language
/// label and a copy button, scrolling sideways instead of wrapping.
class CodeBlock extends StatelessWidget {
  const CodeBlock({super.key, required this.code, this.language});

  final String code;
  final String? language;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      decoration: BoxDecoration(
        color: colors.chat,
        borderRadius: BorderRadius.circular(8),
        border: Border.all(color: colors.border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        mainAxisSize: MainAxisSize.min,
        children: [
          Padding(
            padding: const EdgeInsets.fromLTRB(10, 4, 4, 0),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    language ?? 'code',
                    style: OcText.meta.copyWith(color: colors.textMuted),
                  ),
                ),
                Hoverable(
                  onTap: () {
                    Clipboard.setData(ClipboardData(text: code));
                    showOcToast(context, 'Copied');
                  },
                  semanticLabel: 'Copy code',
                  focusRadius: BorderRadius.circular(6),
                  builder: (context, state) => Container(
                    padding: const EdgeInsets.all(4),
                    decoration: BoxDecoration(
                      color: state.active ? colors.selected : null,
                      borderRadius: BorderRadius.circular(6),
                    ),
                    child: Icon(
                      OcIcons.contentCopy,
                      size: 14,
                      color: state.active ? colors.text : colors.textMuted,
                    ),
                  ),
                ),
              ],
            ),
          ),
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.fromLTRB(10, 2, 10, 8),
            child: Text(code, style: OcText.mono.copyWith(color: colors.text)),
          ),
        ],
      ),
    );
  }
}
