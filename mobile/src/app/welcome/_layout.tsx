import { Stack } from 'expo-router';

import { useTheme } from '@/ui';

export default function WelcomeLayout() {
  const { colors } = useTheme();
  return <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: colors.chat } }} />;
}
