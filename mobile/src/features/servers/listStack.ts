// The header of a server-list stack: Servers, and iOS's search tab.
import { fonts, nativeHeaderScheme, type ColorTokens } from '@/ui';
import type { Scheme } from '@/ui/theme/tokens';

export function listStackOptions(colors: ColorTokens, scheme: Scheme) {
  return {
    headerLargeTitle: true,
    headerShadowVisible: false,
    ...nativeHeaderScheme(scheme),
    headerStyle: { backgroundColor: colors.sidebar },
    headerLargeStyle: { backgroundColor: colors.sidebar },
    headerTintColor: colors.text,
    headerTitleStyle: { fontFamily: fonts.semiBold, color: colors.text },
    headerLargeTitleStyle: { fontFamily: fonts.bold, color: colors.text },
    contentStyle: { backgroundColor: colors.sidebar },
  };
}
