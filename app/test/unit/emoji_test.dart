import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/emoji/emoji.dart';
import 'package:opencord/ui/emoji/emoji_data.dart';

void main() {
  group('data', () {
    test('has the whole set, each with a shortcode', () {
      expect(emojiData.length, greaterThan(1500));
      expect(emojiData.every((emoji) => emoji.shortcodes.isNotEmpty), isTrue);
    });

    test('leaves out bare skin tone modifiers', () {
      final tones = RegExp('^[\u{1F3FB}-\u{1F3FF}]\$', unicode: true);
      expect(emojiData.where((emoji) => tones.hasMatch(emoji.char)), isEmpty);
    });

    test('shortcodes name one emoji each', () {
      final codes = [for (final emoji in emojiData) ...emoji.shortcodes];
      expect(codes.toSet(), hasLength(codes.length));
    });
  });

  group('search', () {
    test('an exact shortcode comes first', () {
      expect(searchEmoji('thumbsup').first.char, '👍');
      expect(searchEmoji(':heart:').first.char, '❤️');
    });

    test('shortcode prefixes and name words match', () {
      expect(searchEmoji('tad').map((e) => e.char), contains('🎉'));
      expect(searchEmoji('rocket').first.char, '🚀');
    });

    test('nothing for an empty query', () {
      expect(searchEmoji('  '), isEmpty);
      expect(searchEmoji(':'), isEmpty);
    });

    test('shows a shortcode made of words', () {
      expect(emojiForChar('👍')?.shortcode, 'thumbsup');
      expect(emojiForChar('💯')?.shortcode, '100');
    });

    test('looks up by shortcode and by character', () {
      expect(emojiForShortcode('tada')?.char, '🎉');
      expect(emojiForChar('🔥')?.shortcode, 'fire');
    });
  });

  group('frequently used', () {
    test('starts from the default reactions', () {
      expect(frequentEmoji(const {}), defaultReactions);
    });

    test('puts the most used first and fills up with defaults', () {
      expect(frequentEmoji(const {'🚀': 5, '👀': 2, '👍': 2}, count: 4), [
        '🚀',
        '👍',
        '👀',
        '❤️',
      ]);
    });
  });
}
