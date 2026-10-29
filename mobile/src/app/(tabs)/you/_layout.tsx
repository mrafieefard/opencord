import { Stack } from 'expo-router';

import { fonts, nativeHeaderScheme, useTheme } from '@/ui';

export default function YouLayout() {
  const { colors, scheme } = useTheme();
  return (
    <Stack
      screenOptions={{
        headerLargeTitle: true,
        headerShadowVisible: false,
        ...nativeHeaderScheme(scheme),
        headerStyle: { backgroundColor: colors.chat },
        headerLargeStyle: { backgroundColor: colors.chat },
        headerTintColor: colors.text,
        headerTitleStyle: { fontFamily: fonts.semiBold, color: colors.text },
        headerLargeTitleStyle: { fontFamily: fonts.bold, color: colors.text },
        // An inset grouped list: surface cards on the chat background.
        contentStyle: { backgroundColor: colors.chat },
      }}
    >
      <Stack.Screen name="index" options={{ title: 'You' }} />
      <Stack.Screen name="name" options={{ title: 'Display name', headerLargeTitle: false }} />
    </Stack>
  );
}
