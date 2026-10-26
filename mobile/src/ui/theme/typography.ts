// The type scale (design §9.2): Inter and JetBrains Mono, from app/assets.
// Each weight is its own font file, so the family names the weight.
import type { TextStyle } from 'react-native';

export type TextVariant =
  | 'display'
  | 'title1'
  | 'title2'
  | 'headline'
  | 'body'
  | 'callout'
  | 'subhead'
  | 'footnote'
  | 'caption'
  | 'mono';

export const fonts = {
  regular: 'Inter-Regular',
  medium: 'Inter-Medium',
  semiBold: 'Inter-SemiBold',
  bold: 'Inter-Bold',
  italic: 'Inter-Italic',
  semiBoldItalic: 'Inter-SemiBoldItalic',
  mono: 'JetBrainsMono-Regular',
} as const;

export const typography: Record<TextVariant, TextStyle> = {
  display: { fontFamily: fonts.bold, fontSize: 34, lineHeight: 41 },
  title1: { fontFamily: fonts.bold, fontSize: 28, lineHeight: 34 },
  title2: { fontFamily: fonts.semiBold, fontSize: 22, lineHeight: 28 },
  headline: { fontFamily: fonts.semiBold, fontSize: 17, lineHeight: 22 },
  body: { fontFamily: fonts.regular, fontSize: 16, lineHeight: 22 },
  callout: { fontFamily: fonts.regular, fontSize: 15, lineHeight: 20 },
  subhead: { fontFamily: fonts.semiBold, fontSize: 14, lineHeight: 19 },
  footnote: { fontFamily: fonts.regular, fontSize: 13, lineHeight: 18 },
  caption: { fontFamily: fonts.semiBold, fontSize: 12, lineHeight: 16, letterSpacing: 0.4, textTransform: 'uppercase' },
  mono: { fontFamily: fonts.mono, fontSize: 14, lineHeight: 20 },
};

/** The family for a weight, keeping the variant's size. */
export function weightFamily(weight: 400 | 500 | 600 | 700): string {
  switch (weight) {
    case 400:
      return fonts.regular;
    case 500:
      return fonts.medium;
    case 600:
      return fonts.semiBold;
    case 700:
      return fonts.bold;
  }
}

/** Text follows the system size up to this many times. */
export const MAX_FONT_SCALE = 1.6;
