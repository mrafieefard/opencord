// The setup a plugin area draws with ("The plugin area channel", setup): the
// phone theme's colours under the twelve names the desktop's
// buildBlocksTheme uses, the insets, the text scale and reduced motion.
import type { ColorTokens, Scheme } from '@/ui/theme/tokens';

export interface AreaInsets {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export interface AreaSetupOptions {
  colors: ColorTokens;
  scheme: Scheme;
  /** What covers the area's edges; `bottom` follows the keyboard. */
  insets: AreaInsets;
  textScale: number;
  reduceMotion: boolean;
  serverKey: string;
  pluginId: string;
}

export function areaSetup({ colors, scheme, insets, textScale, reduceMotion, serverKey, pluginId }: AreaSetupOptions): string {
  return JSON.stringify({
    theme: scheme,
    colors: {
      text: colors.text,
      text_secondary: colors.textSecondary,
      text_muted: colors.textMuted,
      background: colors.chat,
      surface: colors.surface,
      elevated: colors.elevated,
      border: colors.border,
      accent: colors.accent,
      on_accent: colors.onAccent,
      selected: colors.selected,
      hover: colors.hover,
      danger: colors.recording,
    },
    text_scale: textScale,
    safe_area: insets,
    reduce_motion: reduceMotion,
    server_key: serverKey,
    plugin_id: pluginId,
  });
}
