import { Stack } from 'expo-router';

import { fonts, useTheme } from '@/ui';

export default function YouLayout() {
  const { colors } = useTheme();
  return (
    <Stack
      screenOptions={{
        headerLargeTitle: true,
        headerShadowVisible: false,
        headerStyle: { backgroundColor: colors.sidebar },
        headerLargeStyle: { backgroundColor: colors.sidebar },
        headerTintColor: colors.text,
        headerTitleStyle: { fontFamily: fonts.semiBold, color: colors.text },
        headerLargeTitleStyle: { fontFamily: fonts.bold, color: colors.text },
        contentStyle: { backgroundColor: colors.sidebar },
      }}
    >
      <Stack.Screen name="index" options={{ title: 'You' }} />
      <Stack.Screen name="name" options={{ title: 'Display name', headerLargeTitle: false }} />
    </Stack>
  );
}
