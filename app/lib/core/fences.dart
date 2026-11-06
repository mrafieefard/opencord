import 'package:flutter/foundation.dart';

/// A fenced code block in markdown (§7.1).
@immutable
class Fence {
  const Fence({
    required this.start,
    required this.end,
    required this.code,
    this.language,
  });

  final int start;
  final int end;
  final String? language;
  final String code;

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is Fence &&
          runtimeType == other.runtimeType &&
          start == other.start &&
          end == other.end &&
          language == other.language &&
          code == other.code;

  @override
  int get hashCode => Object.hash(start, end, language, code);

  @override
  String toString() =>
      'Fence(start: $start, end: $end, language: $language, code: $code)';
}

final _langPattern = RegExp(r'^[A-Za-z0-9_+#.-]{1,32}$');

/// Scans [text] for fenced code blocks (§7.1).
///
/// - Opening: ``` at the start of a line, after optional indentation.
/// - Language: the rest of the opening line if it matches `[A-Za-z0-9_+#.-]{1,32}`
///   and a newline follows. Otherwise there is no language, and that text is
///   the code's first line.
/// - Closing: the next ``` anywhere, including at the end of a code line.
///   Text after it on the same line starts a new paragraph.
/// - The code: blank lines at its start and end are dropped; indentation and
///   blank lines inside are kept.
/// - Not a block: no closing fence, nothing between the fences, or an opening
///   fence in the middle of a line.
List<Fence> findFences(String text) {
  final fences = <Fence>[];
  var cursor = 0;

  while (cursor < text.length) {
    final lineStart = cursor;
    var i = lineStart;
    while (i < text.length && (text[i] == ' ' || text[i] == '\t')) {
      i++;
    }

    if (i + 3 <= text.length && text.startsWith('```', i)) {
      final openFenceEnd = i + 3;
      final closeStart = text.indexOf('```', openFenceEnd);
      if (closeStart != -1) {
        final raw = text.substring(openFenceEnd, closeStart);
        final newlineIndex = raw.indexOf('\n');
        String? language;
        String codeRaw;

        if (newlineIndex != -1) {
          var langCandidate = raw.substring(0, newlineIndex);
          if (langCandidate.endsWith('\r')) {
            langCandidate = langCandidate.substring(
              0,
              langCandidate.length - 1,
            );
          }
          if (_langPattern.hasMatch(langCandidate)) {
            language = langCandidate;
            codeRaw = raw.substring(newlineIndex + 1);
          } else {
            language = null;
            codeRaw = raw;
          }
        } else {
          language = null;
          codeRaw = raw;
        }

        final codeLines = codeRaw
            .replaceAll('\r\n', '\n')
            .replaceAll('\r', '\n')
            .split('\n');
        var startLine = 0;
        while (startLine < codeLines.length &&
            codeLines[startLine].trim().isEmpty) {
          startLine++;
        }
        var endLine = codeLines.length - 1;
        while (endLine >= startLine && codeLines[endLine].trim().isEmpty) {
          endLine--;
        }

        if (startLine <= endLine) {
          final code = codeLines.sublist(startLine, endLine + 1).join('\n');
          if (code.isNotEmpty) {
            final closeEnd = closeStart + 3;
            fences.add(
              Fence(
                start: lineStart,
                end: closeEnd,
                language: language,
                code: code,
              ),
            );
            final nextNewline = text.indexOf('\n', closeEnd);
            if (nextNewline != -1) {
              cursor = nextNewline + 1;
            } else {
              cursor = text.length;
            }
            continue;
          }
        }
      }
    }

    final nextNewline = text.indexOf('\n', cursor);
    if (nextNewline != -1) {
      cursor = nextNewline + 1;
    } else {
      break;
    }
  }

  return fences;
}

/// Whether the text before the cursor is inside an unclosed fence (§7.2):
/// [findFences] would make a block of it once a closing fence is typed.
bool endsInsideFence(String textBeforeCursor) {
  // Blocks closed before the cursor end inside it, so only an open one can
  // reach the closing fence added here, and a line of code keeps it from
  // being empty.
  final probe = findFences('$textBeforeCursor\nx\n```');
  return probe.isNotEmpty && probe.last.end > textBeforeCursor.length;
}
