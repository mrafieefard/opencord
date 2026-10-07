import 'package:flutter/foundation.dart';

/// The markdown subset of §4.5, parsed by our own small parser: paragraphs,
/// `> quotes` and fenced code blocks, with **bold**, *italic* / _italic_,
/// ~~strike~~, `inline code`, links, `<@user>` mentions and `<#channel>`
/// links inside them.
@immutable
sealed class MdBlock {
  const MdBlock();
}

final class MdParagraph extends MdBlock {
  const MdParagraph(this.inlines);

  final List<MdInline> inlines;

  @override
  bool operator ==(Object other) =>
      other is MdParagraph && listEquals(other.inlines, inlines);

  @override
  int get hashCode => Object.hashAll(inlines);

  @override
  String toString() => 'MdParagraph($inlines)';
}

final class MdQuote extends MdBlock {
  const MdQuote(this.inlines);

  final List<MdInline> inlines;

  @override
  bool operator ==(Object other) =>
      other is MdQuote && listEquals(other.inlines, inlines);

  @override
  int get hashCode => Object.hashAll(inlines);

  @override
  String toString() => 'MdQuote($inlines)';
}

final class MdCode extends MdBlock {
  const MdCode(this.code, {this.language});

  final String code;
  final String? language;

  @override
  bool operator ==(Object other) =>
      other is MdCode && other.code == code && other.language == language;

  @override
  int get hashCode => Object.hash(code, language);

  @override
  String toString() => 'MdCode($language, $code)';
}

@immutable
sealed class MdInline {
  const MdInline();
}

final class MdText extends MdInline {
  const MdText(
    this.text, {
    this.bold = false,
    this.italic = false,
    this.strike = false,
  });

  final String text;
  final bool bold;
  final bool italic;
  final bool strike;

  @override
  bool operator ==(Object other) =>
      other is MdText &&
      other.text == text &&
      other.bold == bold &&
      other.italic == italic &&
      other.strike == strike;

  @override
  int get hashCode => Object.hash(text, bold, italic, strike);

  @override
  String toString() =>
      'MdText(${[if (bold) 'b', if (italic) 'i', if (strike) 's'].join()}"$text")';
}

final class MdInlineCode extends MdInline {
  const MdInlineCode(this.code);

  final String code;

  @override
  bool operator ==(Object other) => other is MdInlineCode && other.code == code;

  @override
  int get hashCode => code.hashCode;

  @override
  String toString() => 'MdInlineCode($code)';
}

final class MdLink extends MdInline {
  const MdLink(this.url);

  final String url;

  @override
  bool operator ==(Object other) => other is MdLink && other.url == url;

  @override
  int get hashCode => url.hashCode;

  @override
  String toString() => 'MdLink($url)';
}

final class MdUserMention extends MdInline {
  const MdUserMention(this.userId);

  final int userId;

  @override
  bool operator ==(Object other) =>
      other is MdUserMention && other.userId == userId;

  @override
  int get hashCode => userId.hashCode;
}

final class MdChannelMention extends MdInline {
  const MdChannelMention(this.channelId);

  final int channelId;

  @override
  bool operator ==(Object other) =>
      other is MdChannelMention && other.channelId == channelId;

  @override
  int get hashCode => channelId.hashCode;
}

/// Whether the message ends in a code block, where the time and status go
/// on a line of their own (§4.5).
bool endsWithCode(List<MdBlock> blocks) =>
    blocks.isNotEmpty && blocks.last is MdCode;

List<MdBlock> parseMarkdown(String source) {
  final lines = source.split('\n');
  final blocks = <MdBlock>[];
  final paragraph = <String>[];
  final quote = <String>[];
  void flushParagraph() {
    if (paragraph.isEmpty) return;
    blocks.add(MdParagraph(parseInline(paragraph.join('\n'))));
    paragraph.clear();
  }

  void flushQuote() {
    if (quote.isEmpty) return;
    blocks.add(MdQuote(parseInline(quote.join('\n'))));
    quote.clear();
  }

  var i = 0;
  while (i < lines.length) {
    final line = lines[i];
    if (line.trimLeft().startsWith('```')) {
      final end = _closingFence(lines, i + 1);
      if (end != -1) {
        flushParagraph();
        flushQuote();
        final language = line.trim().substring(3).trim();
        blocks.add(
          MdCode(
            lines.sublist(i + 1, end).join('\n'),
            language: language.isEmpty ? null : language,
          ),
        );
        i = end + 1;
        continue;
      }
    }
    if (line.startsWith('>')) {
      flushParagraph();
      final rest = line.substring(1);
      quote.add(rest.startsWith(' ') ? rest.substring(1) : rest);
    } else {
      flushQuote();
      paragraph.add(line);
    }
    i++;
  }
  flushParagraph();
  flushQuote();
  return blocks;
}

int _closingFence(List<String> lines, int from) {
  for (var j = from; j < lines.length; j++) {
    if (lines[j].trim() == '```') return j;
  }
  return -1;
}

final _userMention = RegExp(r'<@(\d+)>');
final _channelMention = RegExp(r'<#(\d+)>');
final _url = RegExp(r'''https?://[^\s<>"]+''');
const _escapable = r'\*_~`<>#@';
const _trailing = ".,!?;:'\")";

bool _isWord(String char) =>
    RegExp(r'[\p{L}\p{N}]', unicode: true).hasMatch(char);

/// Inline content of one paragraph or quote.
List<MdInline> parseInline(
  String text, {
  bool bold = false,
  bool italic = false,
  bool strike = false,
}) {
  final result = <MdInline>[];
  final buffer = StringBuffer();
  void flush() {
    if (buffer.isEmpty) return;
    result.add(
      MdText(buffer.toString(), bold: bold, italic: italic, strike: strike),
    );
    buffer.clear();
  }

  var i = 0;
  while (i < text.length) {
    final char = text[i];
    if (char == r'\' &&
        i + 1 < text.length &&
        _escapable.contains(text[i + 1])) {
      buffer.write(text[i + 1]);
      i += 2;
      continue;
    }
    if (char == '`') {
      final end = text.indexOf('`', i + 1);
      if (end > i + 1) {
        flush();
        result.add(MdInlineCode(text.substring(i + 1, end)));
        i = end + 1;
        continue;
      }
    }
    if (char == '<') {
      final user = _userMention.matchAsPrefix(text, i);
      final channel = _channelMention.matchAsPrefix(text, i);
      if (user != null || channel != null) {
        flush();
        result.add(
          user != null
              ? MdUserMention(int.parse(user[1]!))
              : MdChannelMention(int.parse(channel![1]!)),
        );
        i = (user ?? channel)!.end;
        continue;
      }
    }
    if (char == 'h' && (i == 0 || !_isWord(text[i - 1]))) {
      final link = _url.matchAsPrefix(text, i);
      if (link != null) {
        var url = link[0]!;
        while (url.isNotEmpty && _trailing.contains(url[url.length - 1])) {
          if (url.endsWith(')') &&
              '('.allMatches(url).length >= ')'.allMatches(url).length) {
            break;
          }
          url = url.substring(0, url.length - 1);
        }
        flush();
        result.add(MdLink(url));
        i += url.length;
        continue;
      }
    }
    final token = text.startsWith('**', i)
        ? '**'
        : text.startsWith('~~', i)
        ? '~~'
        : (char == '*' || char == '_')
        ? char
        : null;
    if (token != null) {
      final close = _closing(text, i, token);
      if (close != -1) {
        flush();
        result.addAll(
          parseInline(
            text.substring(i + token.length, close),
            bold: bold || token == '**',
            italic: italic || token == '*' || token == '_',
            strike: strike || token == '~~',
          ),
        );
        i = close + token.length;
        continue;
      }
      buffer.write(token);
      i += token.length;
      continue;
    }
    buffer.write(char);
    i++;
  }
  flush();
  return result;
}

/// Where [token] opened at [open] closes, or -1.
int _closing(String text, int open, String token) {
  final start = open + token.length;
  if (start >= text.length || text[start].trim().isEmpty) return -1;
  if (token == '_' && open > 0 && _isWord(text[open - 1])) return -1;
  for (var j = start + 1; j <= text.length - token.length; j++) {
    if (!text.startsWith(token, j)) continue;
    if (text[j - 1].trim().isEmpty) continue;
    if (token == '*' && (text.startsWith('**', j) || text[j - 1] == '*')) {
      continue;
    }
    final after = j + token.length;
    if (token == '_' && after < text.length && _isWord(text[after])) continue;
    return j;
  }
  return -1;
}
