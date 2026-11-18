import {
  DEFAULT_REACTIONS,
  emojiData,
  emojiForChar,
  emojiForShortcode,
  frequentEmoji,
  searchEmoji,
} from '../emoji';

describe('data', () => {
  // Fails if emojiData has fewer than 1500 entries or an entry lacks shortcodes.
  it('has the whole set, each with a shortcode', () => {
    expect(emojiData.length).toBeGreaterThan(1500);
    expect(emojiData.every((emoji) => emoji.shortcodes.length > 0)).toBe(true);
  });

  // Fails if bare skin tone modifier codepoints are included in emojiData.
  it('leaves out bare skin tone modifiers', () => {
    const tones = /^[\u{1F3FB}-\u{1F3FF}]$/u;
    expect(emojiData.filter((emoji) => tones.test(emoji.char))).toHaveLength(0);
  });

  // Fails if shortcodes are duplicated across different emoji.
  it('shortcodes name one emoji each', () => {
    const codes = emojiData.flatMap((emoji) => emoji.shortcodes);
    expect(new Set(codes).size).toBe(codes.length);
  });
});

describe('search', () => {
  // Fails if exact shortcode matches do not rank first.
  it('an exact shortcode comes first', () => {
    expect(searchEmoji('thumbsup')[0]?.char).toBe('👍');
    expect(searchEmoji(':heart:')[0]?.char).toBe('❤️');
  });

  // Fails if prefix matches or name word matches are not found.
  it('shortcode prefixes and name words match', () => {
    expect(searchEmoji('tad').map((e) => e.char)).toContain('🎉');
    expect(searchEmoji('rocket')[0]?.char).toBe('🚀');
  });

  // Fails if empty or colon-only queries return results.
  it('nothing for an empty query', () => {
    expect(searchEmoji('  ')).toEqual([]);
    expect(searchEmoji(':')).toEqual([]);
  });

  // Fails if primary shortcode is not alphanumeric words.
  it('shows a shortcode made of words', () => {
    expect(emojiForChar('👍')?.shortcode).toBe('thumbsup');
    expect(emojiForChar('💯')?.shortcode).toBe('100');
  });

  // Fails if lookup by shortcode or character fails.
  it('looks up by shortcode and by character', () => {
    expect(emojiForShortcode('tada')?.char).toBe('🎉');
    expect(emojiForChar('🔥')?.shortcode).toBe('fire');
  });
});

describe('frequently used', () => {
  // Fails if empty usage does not return default reactions.
  it('starts from the default reactions', () => {
    expect(frequentEmoji({})).toEqual(DEFAULT_REACTIONS);
  });

  // Fails if usage counts and tie-breaking do not order correctly.
  it('puts the most used first and fills up with defaults', () => {
    expect(frequentEmoji({ '🚀': 5, '👀': 2, '👍': 2 }, 4)).toEqual([
      '🚀',
      '👍',
      '👀',
      '❤️',
    ]);
  });
});
