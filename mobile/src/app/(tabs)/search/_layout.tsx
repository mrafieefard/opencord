import { TabInsetProbe } from '@/features/voice/TabInsetProbe';
import { Stack } from 'expo-router';

import { listStackOptions } from '@/features/servers/listStack';
import { useTheme } from '@/ui';

export default function SearchLayout() {
  const { colors, scheme } = useTheme();
  return (
    <>
      <Stack screenOptions={listStackOptions(colors, scheme)}>
        <Stack.Screen name="index" options={{ title: 'Search' }} />
      </Stack>
      <TabInsetProbe />
    </>
  );
}
