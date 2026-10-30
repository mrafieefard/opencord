import { Stack, router } from 'expo-router';

import { listStackOptions } from '@/features/servers/listStack';
import { IconButton, useTheme } from '@/ui';

export default function ServersLayout() {
  const { colors, scheme } = useTheme();
  return (
    <Stack screenOptions={listStackOptions(colors, scheme)}>
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
      {/* Inside a server, under the tab bar: it draws its own headers. */}
      <Stack.Screen name="[serverKey]" options={{ headerShown: false }} />
    </Stack>
  );
}
