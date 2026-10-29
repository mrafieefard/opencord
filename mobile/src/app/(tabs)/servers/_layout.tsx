import { Stack, router } from 'expo-router';

import { IconButton, fonts, nativeHeaderScheme, useTheme } from '@/ui';

export default function ServersLayout() {
  const { colors, scheme } = useTheme();
  return (
    <Stack
      screenOptions={{
        headerLargeTitle: true,
        headerShadowVisible: false,
        ...nativeHeaderScheme(scheme),
        headerStyle: { backgroundColor: colors.sidebar },
        headerLargeStyle: { backgroundColor: colors.sidebar },
        headerTintColor: colors.text,
        headerTitleStyle: { fontFamily: fonts.semiBold, color: colors.text },
        headerLargeTitleStyle: { fontFamily: fonts.bold, color: colors.text },
        contentStyle: { backgroundColor: colors.sidebar },
      }}
    >
      <Stack.Screen
        name="index"
        options={{
          title: 'Servers',
          headerRight: () => (
            <IconButton
              icon="add"
              label="Add a server"
              onPress={() => router.push('/add-server')}
            />
          ),
        }}
      />
    </Stack>
  );
}
