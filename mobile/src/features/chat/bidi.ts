// The direction of a text by its first letter, as Telegram sets a message's
// (and the desktop's app/lib/core/bidi.dart): right-to-left from the first
// Persian, Arabic or Hebrew letter, left-to-right from the first other one,
// null when there is no letter (only digits, spaces and symbols).

/** Hebrew, Arabic, Syriac, Thaana, N'Ko and the other right-to-left scripts, with their presentation forms. */
function isRtl(codePoint: number): boolean {
  return (
    (codePoint >= 0x0590 && codePoint <= 0x08ff) ||
    (codePoint >= 0xfb1d && codePoint <= 0xfdff) ||
    (codePoint >= 0xfe70 && codePoint <= 0xfeff) ||
    (codePoint >= 0x10800 && codePoint <= 0x10fff) ||
    (codePoint >= 0x1e800 && codePoint <= 0x1efff)
  );
}

const LETTER = /\p{L}/u;

export function textDirectionOf(text: string): 'rtl' | 'ltr' | null {
  // `for…of` walks code points, as Dart's `runes` does.
  for (const char of text) {
    const codePoint = char.codePointAt(0);
    if (codePoint !== undefined && isRtl(codePoint)) return 'rtl';
    if (LETTER.test(char)) return 'ltr';
  }
  return null;
}
