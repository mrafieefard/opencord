// The colors (design §9.1): the desktop's OcColors, value for value
// (app/lib/ui/theme/oc_colors.dart; tokens.test.ts holds them equal).
// Monochrome, with `voice` and `recording` the only hues. No other file
// writes a color literal (noColorLiterals.test.ts).

export interface ColorTokens {
  rail: string;
  sidebar: string;
  chat: string;
  surface: string;
  elevated: string;
  hover: string;
  selected: string;
  border: string;
  text: string;
  textSecondary: string;
  textMuted: string;
  accent: string;
  onAccent: string;
  bubbleIn: string;
  bubbleOut: string;
  mentionBg: string;
  scrim: string;
  avatarShades: readonly string[];
  voice: string;
  recording: string;
}

export type ColorToken = Exclude<keyof ColorTokens, 'avatarShades'>;
export type Scheme = 'light' | 'dark';

const dark: ColorTokens = {
  rail: '#0A0A0B',
  sidebar: '#121214',
  chat: '#0E0E10',
  surface: '#18181B',
  elevated: '#1F1F23',
  hover: '#1C1C20',
  selected: '#27272C',
  border: '#222226',
  text: '#EDEDEF',
  textSecondary: '#A1A1A8',
  textMuted: '#6E6E76',
  accent: '#EDEDEF',
  onAccent: '#0E0E10',
  bubbleIn: '#1A1A1E',
  bubbleOut: '#2A2A30',
  mentionBg: '#26262B',
  scrim: '#000000B3',
  avatarShades: ['#2B2B30', '#34343A', '#3E3E45', '#494951', '#55555D'],
  voice: '#4C8DFF',
  recording: '#F2555A',
};

const light: ColorTokens = {
  rail: '#E9E9EC',
  sidebar: '#FFFFFF',
  chat: '#F3F3F5',
  surface: '#FFFFFF',
  elevated: '#FFFFFF',
  hover: '#F2F2F4',
  selected: '#E7E7EA',
  border: '#E3E3E7',
  text: '#111114',
  textSecondary: '#55555C',
  textMuted: '#8A8A92',
  accent: '#111114',
  onAccent: '#FFFFFF',
  bubbleIn: '#FFFFFF',
  bubbleOut: '#E4E4E8',
  mentionBg: '#E2E2E6',
  scrim: '#00000066',
  avatarShades: ['#D9D9DE', '#CDCDD3', '#C1C1C8', '#E3E3E7', '#B6B6BE'],
  voice: '#2F6FEB',
  recording: '#DC3E42',
};

export const tokens: Record<Scheme, ColorTokens> = { dark, light };

/** Anything drawn on a photo or video: the dark set, whatever the theme. */
export const onMedia = dark;

/** `color` at `alpha` (0–1), for the few see-through layers. */
export function withAlpha(color: string, alpha: number): string {
  const hex = color.slice(0, 7);
  return `${hex}${Math.round(Math.min(Math.max(alpha, 0), 1) * 255)
    .toString(16)
    .padStart(2, '0')}`;
}
