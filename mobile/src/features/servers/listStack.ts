// The header of a server-list stack: Servers, and iOS's search tab.
import { fonts, nativeHeaderLook, type ColorTokens } from '@/ui';
import type { Scheme } from '@/ui/theme/tokens';

export function listStackOptions(colors: ColorTokens, scheme: Scheme) {
  return {
    headerLargeTitle: true,
    headerShadowVisible: false,
    ...nativeHeaderLook(scheme, colors.sidebar),
    headerTintColor: colors.text,
    headerTitleStyle: { fontFamily: fonts.semiBold, color: colors.text },
    headerLargeTitleStyle: { fontFamily: fonts.bold, color: colors.text },
    contentStyle: { backgroundColor: colors.sidebar },
  };
}
