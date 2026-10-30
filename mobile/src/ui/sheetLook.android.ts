// How a sheet and what sits in it are drawn. Android's sheets are the
// app's own surfaces (sheetLook.ios.ts follows Apple's glass sheets).
import type { ViewStyle } from 'react-native';

import { withAlpha, type ColorTokens } from './theme/tokens';

export interface FieldState {
  focused: boolean;
  invalid: boolean;
}

export const sheetLook = {
  /** What the sheet itself is drawn in. */
  background: (colors: ColorTokens): string => colors.elevated,
  /** A field or a well (a fingerprint): sunk into the sheet. */
  well: (colors: ColorTokens): string => colors.chat,
  /** A well's edge. */
  wellBorder: (colors: ColorTokens): string => colors.border,
  /** A field's outline, which says it has focus or is wrong. */
  fieldOutline: (colors: ColorTokens, { focused, invalid }: FieldState): string =>
    invalid ? colors.recording : focused ? colors.textMuted : colors.border,
  /** A panel of choices in the sheet (I own this server). */
  panel: (colors: ColorTokens): string => withAlpha(colors.chat, 0.5),
  /** Controls set in one group (a member's volume and mute): none here. */
  group: (_colors: ColorTokens): ViewStyle => ({}),
};
