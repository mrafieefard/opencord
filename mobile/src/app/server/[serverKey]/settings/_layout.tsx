// The server sheet's own stack (design §6.1): the sheet's rows push their
// pages here, inside the form sheet the root stack presents.
// - The sheet opens at medium; once a page is pushed it allows only large,
//   so it grows to full height for the page.
// - A pushed page has the native header (its title, back, and the Save
//   that useSaveButton puts there); the sheet's own list draws SheetHeader.
// - A page whose permission goes closes back to the list (useOpenPageGuard),
//   so no page has to remember to.
import { Stack, useLocalSearchParams, useNavigation, usePathname, useSegments } from 'expo-router';
import { useLayoutEffect } from 'react';

import { SHEET_DETENTS } from '@/features/serverMenu/ServerSheet';
import { useOpenPageGuard } from '@/features/serverMenu/serverRows';
import { fonts, nativeHeaderLook, useTheme } from '@/ui';

export default function ServerSheetLayout() {
  const { colors, scheme } = useTheme();
  const { serverKey } = useLocalSearchParams<{ serverKey: string }>();
  const navigation = useNavigation();
  const segments: readonly string[] = useSegments();
  const pathname = usePathname();
  useOpenPageGuard(serverKey ?? '', pathname);

  // A page is open when something follows `settings`; without `settings`
  // at all, another sheet is over this one, and the detents stay.
  const at = segments.indexOf('settings');
  const pageOpen = at === -1 ? null : at < segments.length - 1;
  useLayoutEffect(() => {
    if (pageOpen === null) return;
    navigation.setOptions({ sheetAllowedDetents: pageOpen ? [1] : SHEET_DETENTS });
  }, [navigation, pageOpen]);

  return (
    <Stack
      screenOptions={{
        headerShadowVisible: false,
        ...nativeHeaderLook(scheme, colors.chat),
        headerTintColor: colors.text,
        headerTitleStyle: { fontFamily: fonts.semiBold, color: colors.text },
        headerBackButtonDisplayMode: 'minimal',
        contentStyle: { backgroundColor: colors.chat },
      }}
    >
      <Stack.Screen name="index" options={{ headerShown: false }} />
    </Stack>
  );
}
