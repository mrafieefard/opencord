import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';

import 'markdown.dart';

/// A plugin's `richText` on the phone (plugins M4, Task 2): the parser's
/// blocks drawn with plain Flutter text. Paragraphs, bold, italic,
/// strikethrough, inline code, fenced code (monospace on [codeBackground],
/// no highlighting), quotes, links (tappable through [onLink]) and mentions
/// by name. The desktop keeps its own `MarkdownView`.
class MarkdownText extends StatefulWidget {
  const MarkdownText({
    super.key,
    required this.markdown,
    required this.style,
    required this.linkColor,
    required this.codeBackground,
    this.userName,
    this.channelName,
    this.onLink,
  });

  final String markdown;
  final TextStyle style;
  final Color linkColor;
  final Color codeBackground;

  /// A user's name; null shows `@unknown`, as the desktop does.
  final String? Function(int id)? userName;

  /// A channel's name; null shows `#unknown`.
  final String? Function(int id)? channelName;

  final void Function(String url)? onLink;

  static const String monospace = 'JetBrainsMono';

  @override
  State<MarkdownText> createState() => _MarkdownTextState();
}

class _MarkdownTextState extends State<MarkdownText> {
  final _recognizers = <TapGestureRecognizer>[];

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

  List<InlineSpan> _spans(List<MdInline> inlines, TextStyle base) {
    final mention = base.copyWith(
      color: widget.linkColor,
      fontWeight: FontWeight.w600,
    );
    return [
      for (final inline in inlines)
        switch (inline) {
          MdText(:final text, :final bold, :final italic, :final strike) =>
            TextSpan(
              text: text,
              style: TextStyle(
                fontWeight: bold ? FontWeight.w700 : null,
                fontStyle: italic ? FontStyle.italic : null,
                decoration: strike ? TextDecoration.lineThrough : null,
              ),
            ),
          MdInlineCode(:final code) => TextSpan(
            text: code,
            style: TextStyle(
              fontFamily: MarkdownText.monospace,
              fontSize: (base.fontSize ?? 14) * 0.92,
              backgroundColor: widget.codeBackground,
            ),
          ),
          MdLink(:final url) => TextSpan(
            text: url,
            style: TextStyle(color: widget.linkColor),
            recognizer: widget.onLink == null
                ? null
                : (TapGestureRecognizer()..onTap = () => widget.onLink!(url))
                      .also(_recognizers.add),
          ),
          MdUserMention(:final userId) => TextSpan(
            text: '@${widget.userName?.call(userId) ?? 'unknown'}',
            style: mention,
          ),
          MdChannelMention(:final channelId) => TextSpan(
            text: '#${widget.channelName?.call(channelId) ?? 'unknown'}',
            style: mention,
          ),
        },
    ];
  }

  @override
  Widget build(BuildContext context) {
    _clearRecognizers();
    final base = widget.style;
    final muted = base.color?.withValues(alpha: 0.72);
    final blocks = parseMarkdown(widget.markdown);
    final children = <Widget>[];
    for (final block in blocks) {
      if (children.isNotEmpty) children.add(const SizedBox(height: 4));
      children.add(switch (block) {
        MdParagraph(:final inlines) => Text.rich(
          TextSpan(style: base, children: _spans(inlines, base)),
        ),
        MdQuote(:final inlines) => DecoratedBox(
          decoration: BoxDecoration(
            border: Border(
              left: BorderSide(
                color: muted ?? widget.linkColor,
                width: 2,
              ),
            ),
          ),
          child: Padding(
            padding: const EdgeInsets.only(left: 8),
            child: Text.rich(
              TextSpan(
                style: base.copyWith(color: muted),
                children: _spans(inlines, base),
              ),
            ),
          ),
        ),
        MdCode(:final code) => DecoratedBox(
          decoration: BoxDecoration(
            color: widget.codeBackground,
            borderRadius: BorderRadius.circular(6),
          ),
          child: SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
            child: Text.rich(
              TextSpan(
                text: code,
                style: base.copyWith(
                  fontFamily: MarkdownText.monospace,
                  fontSize: (base.fontSize ?? 14) * 0.92,
                ),
              ),
            ),
          ),
        ),
      });
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: children,
    );
  }
}

extension<T> on T {
  T also(void Function(T value) use) {
    use(this);
    return this;
  }
}
