/// Mentions as people type them and as they are sent (§4.6): the composer
/// shows "@Kai Nakamura" and "#general"; messages carry `<@1001>` and
/// `<#10>`, which every client renders by name.
library;

final _code = RegExp(r'```[\s\S]*?```|`[^`\n]*`');

/// Applies [convert] to the parts of [text] outside code, which stays as
/// written.
String _outsideCode(String text, String Function(String part) convert) {
  final buffer = StringBuffer();
  var last = 0;
  for (final match in _code.allMatches(text)) {
    buffer
      ..write(convert(text.substring(last, match.start)))
      ..write(match[0]);
    last = match.end;
  }
  buffer.write(convert(text.substring(last)));
  return buffer.toString();
}

/// Matches [prefix] followed by one of [names] (longest first), as a whole
/// word; [extraWordChars] also count as part of a word after the name.
RegExp? _namePattern(
  String prefix,
  Iterable<String> names,
  String extraWordChars,
) {
  final sorted = names.where((name) => name.isNotEmpty).toList()
    ..sort((a, b) => b.length.compareTo(a.length));
  if (sorted.isEmpty) return null;
  final alternatives = sorted.map(RegExp.escape).join('|');
  return RegExp(
    '(?<![\\p{L}\\p{N}_])$prefix($alternatives)'
    '(?![\\p{L}\\p{N}_$extraWordChars])',
    unicode: true,
  );
}

Map<String, int> _ids(Map<int, String> names) {
  final ids = <String, int>{};
  for (final MapEntry(key: id, value: name) in names.entries) {
    ids.putIfAbsent(name, () => id);
  }
  return ids;
}

/// "@Kai Nakamura" → `<@1001>`, "#general" → `<#10>`, for sending.
String encodeMentions(
  String text, {
  required Map<int, String> members,
  required Map<int, String> channels,
}) {
  final memberIds = _ids(members);
  final channelIds = _ids(channels);
  final memberPattern = _namePattern('@', memberIds.keys, '');
  final channelPattern = _namePattern('#', channelIds.keys, r'\-');
  return _outsideCode(text, (part) {
    var result = part;
    if (memberPattern != null) {
      result = result.replaceAllMapped(
        memberPattern,
        (match) => '<@${memberIds[match[1]]}>',
      );
    }
    if (channelPattern != null) {
      result = result.replaceAllMapped(
        channelPattern,
        (match) => '<#${channelIds[match[1]]}>',
      );
    }
    return result;
  });
}

/// The other way round, for editing a sent message. Unknown ids stay.
String decodeMentions(
  String content, {
  required Map<int, String> members,
  required Map<int, String> channels,
}) => _outsideCode(
  content,
  (part) => part
      .replaceAllMapped(RegExp(r'<@(\d+)>'), (match) {
        final name = members[int.parse(match[1]!)];
        return name == null ? match[0]! : '@$name';
      })
      .replaceAllMapped(RegExp(r'<#(\d+)>'), (match) {
        final name = channels[int.parse(match[1]!)];
        return name == null ? match[0]! : '#$name';
      }),
);

typedef ActiveToken = ({String trigger, String query, int start});

bool _startsWord(String text, int index) =>
    index == 0 || RegExp(r'''[\s(\["']''').hasMatch(text[index - 1]);

/// What is being typed at [caret] for the autocomplete (§4.6): `@` and a
/// member name (which may hold spaces), `#` and a channel, `:` and at least
/// two letters of an emoji. Null when the caret is not in such a token.
ActiveToken? activeToken(String text, int caret) {
  if (caret < 0 || caret > text.length) return null;
  final before = text.substring(0, caret);
  final lineStart = before.lastIndexOf('\n') + 1;
  final line = before.substring(lineStart);
  for (var i = line.length - 1; i >= 0; i--) {
    final trigger = line[i];
    if (trigger != '@' && trigger != '#' && trigger != ':') continue;
    if (!_startsWord(line, i)) continue;
    final query = line.substring(i + 1);
    if (query.startsWith(' ')) return null;
    final spaced = query.contains(RegExp(r'\s'));
    return switch (trigger) {
      '@' when query.length <= 32 => (
        trigger: trigger,
        query: query,
        start: lineStart + i,
      ),
      '#' when !spaced => (
        trigger: trigger,
        query: query,
        start: lineStart + i,
      ),
      ':' when !spaced && query.length >= 2 => (
        trigger: trigger,
        query: query,
        start: lineStart + i,
      ),
      _ => null,
    };
  }
  return null;
}
