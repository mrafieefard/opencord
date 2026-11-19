import { Stack } from 'expo-router';

import { listStackOptions } from '@/features/servers/listStack';
import { useTheme } from '@/ui';

export default function MessagesLayout() {
  const { colors, scheme } = useTheme();
  return (
    <Stack screenOptions={listStackOptions(colors, scheme)}>
      <Stack.Screen name="index" options={{ headerShown: false }} />
      <Stack.Screen name="[peer]" options={{ headerShown: false }} />
      <Stack.Screen name="friends" options={{ headerShown: false }} />
      <Stack.Screen name="requests" options={{ headerShown: false }} />
    </Stack>
  );
}
