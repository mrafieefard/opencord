import 'package:flutter/widgets.dart';

import 'package:opencord/ui/emoji/emoji_data.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

/// The picker's sections, with the tab icon for each.
enum EmojiCategory {
  smileys('Smileys & emotion', OcIcons.mood),
  people('People & body', OcIcons.emojiPeople),
  nature('Animals & nature', OcIcons.emojiNature),
  food('Food & drink', OcIcons.emojiFoodBeverage),
  activities('Activities', OcIcons.emojiEvents),
  travel('Travel & places', OcIcons.emojiTransportation),
  objects('Objects', OcIcons.emojiObjects),
  symbols('Symbols', OcIcons.emojiSymbols),
  flags('Flags', OcIcons.emojiFlags);

  const EmojiCategory(this.label, this.icon);

  final String label;
  final IconData icon;
}

/// One emoji, with the names people type for it (`:thumbsup:`).
@immutable
class Emoji {
  const Emoji(this.char, this.name, this.shortcodes, this.category);

  final String char;

  /// "thumbs up sign".
  final String name;

  /// "+1", "thumbsup".
  final List<String> shortcodes;
  final EmojiCategory category;

  /// The one shown: the first made of words, so 👍 reads `:thumbsup:`
  /// rather than `:+1:`.
  String get shortcode =>
      shortcodes.where((code) => _words.hasMatch(code)).firstOrNull ??
      shortcodes.first;
}

final _words = RegExp(r'^[a-z][a-z0-9_]*$');

final Map<String, Emoji> _byShortcode = {
  for (final emoji in emojiData)
    for (final code in emoji.shortcodes) code: emoji,
};

final Map<String, Emoji> _byChar = {
  for (final emoji in emojiData) emoji.char: emoji,
};

Emoji? emojiForShortcode(String code) => _byShortcode[code];

Emoji? emojiForChar(String char) => _byChar[char];

/// Emoji matching [query], for the picker's search and the `:`
/// autocomplete (§4.6): an exact shortcode first, then shortcodes that start
/// with it, then shortcodes and names with a word that does.
List<Emoji> searchEmoji(String query, {int limit = 50}) {
  final needle = query.trim().toLowerCase().replaceAll(RegExp(r'^:|:$'), '');
  if (needle.isEmpty) return const [];
  int? rank(Emoji emoji) {
    int? best;
    void consider(int value) {
      if (best == null || value < best!) best = value;
    }

    for (final code in emoji.shortcodes) {
      if (code == needle) consider(0);
      if (code.startsWith(needle)) consider(1);
      if (code.contains('_$needle')) consider(2);
    }
    if (emoji.name.split(' ').any((word) => word.startsWith(needle))) {
      consider(3);
    }
    return best;
  }

  final ranked = <(int, int, Emoji)>[];
  for (final (index, emoji) in emojiData.indexed) {
    final value = rank(emoji);
    if (value != null) ranked.add((value, index, emoji));
  }
  ranked.sort((a, b) => a.$1 != b.$1 ? a.$1 - b.$1 : a.$2 - b.$2);
  return [for (final (_, _, emoji) in ranked.take(limit)) emoji];
}
