// How a sheet and what sits in it are drawn on iOS: as Apple's own sheets
// (Maps, Find My). The sheet is Liquid Glass, so it has no colour of its
// own; fields and panels are soft fills on the glass, with no outlines,
// and controls that belong together share one rounded group.
import type { ViewStyle } from 'react-native';

import { radius, space } from './theme/space';
import { withAlpha, type ColorTokens } from './theme/tokens';

export interface FieldState {
  focused: boolean;
  invalid: boolean;
}

/** A fill on glass: the text colour, faint, so it works in both themes. */
const fill = (colors: ColorTokens, alpha: number) => withAlpha(colors.text, alpha);

export const sheetLook = {
  background: (_colors: ColorTokens): string => 'transparent',
  well: (colors: ColorTokens): string => fill(colors, 0.07),
  wellBorder: (_colors: ColorTokens): string => 'transparent',
  /** Only a wrong value is outlined; focus shows as the caret. */
  fieldOutline: (colors: ColorTokens, { invalid }: FieldState): string => (invalid ? colors.recording : 'transparent'),
  panel: (colors: ColorTokens): string => fill(colors, 0.05),
  group: (colors: ColorTokens): ViewStyle => ({
    backgroundColor: fill(colors, 0.05),
    borderRadius: radius.tile,
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
    marginTop: space.lg,
  }),
};
