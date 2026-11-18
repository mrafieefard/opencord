import rawData from './emojiData.json';

export type EmojiCategory =
  | 'smileys'
  | 'people'
  | 'nature'
  | 'food'
  | 'activities'
  | 'travel'
  | 'objects'
  | 'symbols'
  | 'flags';

export interface Emoji {
  char: string;
  name: string;
  shortcodes: string[];
  category: EmojiCategory;
  shortcode: string;
}

const WORDS = /^[a-z][a-z0-9_]*$/;

function primaryShortcode(shortcodes: string[]): string {
  const match = shortcodes.find((code) => WORDS.test(code));
  return match ?? shortcodes[0] ?? '';
}

export const EMOJI_CATEGORIES: EmojiCategory[] = [
  'smileys',
  'people',
  'nature',
  'food',
  'activities',
  'travel',
  'objects',
  'symbols',
  'flags',
];

export const CATEGORY_LABELS: Record<EmojiCategory, string> = {
  smileys: 'Smileys & emotion',
  people: 'People & body',
  nature: 'Animals & nature',
  food: 'Food & drink',
  activities: 'Activities',
  travel: 'Travel & places',
  objects: 'Objects',
  symbols: 'Symbols',
  flags: 'Flags',
};

export const DEFAULT_REACTIONS = ['👍', '❤️', '😂', '😮', '😢', '🙏', '🎉', '🔥'];

export const emojiData: Emoji[] = (rawData as { e: string; n: string; s: string[]; c: string }[]).map((entry) => ({
  char: entry.e,
  name: entry.n,
  shortcodes: entry.s,
  category: entry.c as EmojiCategory,
  shortcode: primaryShortcode(entry.s),
}));

const byShortcode = new Map<string, Emoji>();
const byChar = new Map<string, Emoji>();

for (const emoji of emojiData) {
  for (const code of emoji.shortcodes) {
    if (!byShortcode.has(code)) {
      byShortcode.set(code, emoji);
    }
  }
  if (!byChar.has(emoji.char)) {
    byChar.set(emoji.char, emoji);
  }
}

export function emojiForShortcode(code: string): Emoji | undefined {
  return byShortcode.get(code);
}

export function emojiForChar(char: string): Emoji | undefined {
  return byChar.get(char);
}

export function searchEmoji(query: string, limit: number = 50): Emoji[] {
  const needle = query.trim().toLowerCase().replace(/^:|:$/g, '');
  if (needle.length === 0) return [];

  function rank(emoji: Emoji): number | null {
    let best: number | null = null;
    function consider(value: number): void {
      if (best === null || value < best) best = value;
    }

    for (const code of emoji.shortcodes) {
      if (code === needle) consider(0);
      if (code.startsWith(needle)) consider(1);
      if (code.includes(`_${needle}`)) consider(2);
    }
    if (emoji.name.split(' ').some((word) => word.startsWith(needle))) {
      consider(3);
    }
    return best;
  }

  const ranked: { rank: number; index: number; emoji: Emoji }[] = [];
  for (let i = 0; i < emojiData.length; i++) {
    const emoji = emojiData[i];
    if (!emoji) continue;
    const r = rank(emoji);
    if (r !== null) {
      ranked.push({ rank: r, index: i, emoji });
    }
  }

  ranked.sort((a, b) => (a.rank !== b.rank ? a.rank - b.rank : a.index - b.index));
  return ranked.slice(0, limit).map((r) => r.emoji);
}

export function frequentEmoji(usage: Record<string, number>, count: number = 8): string[] {
  function place(emoji: string): number {
    const index = DEFAULT_REACTIONS.indexOf(emoji);
    return index === -1 ? DEFAULT_REACTIONS.length : index;
  }

  const used = Object.keys(usage)
    .filter((k) => (usage[k] ?? 0) > 0)
    .sort((a, b) => {
      const countA = usage[a] ?? 0;
      const countB = usage[b] ?? 0;
      if (countB !== countA) return countB - countA;
      const placeA = place(a);
      const placeB = place(b);
      if (placeA !== placeB) return placeA - placeB;
      return a.localeCompare(b);
    });

  const combined = [...used, ...DEFAULT_REACTIONS.filter((emoji) => !(emoji in usage))];
  return combined.slice(0, count);
}
