// The native tab bar: Servers and You (design §7.1).
import { NativeTabs } from 'expo-router/native-tabs';

import { useTheme } from '@/ui';
import { icons } from '@/ui/icons';

export default function TabsLayout() {
  const { colors } = useTheme();
  return (
    <NativeTabs
      tintColor={colors.text}
      iconColor={{ default: colors.textMuted, selected: colors.text }}
      backgroundColor={colors.rail}
      labelStyle={{ default: { color: colors.textMuted }, selected: { color: colors.text } }}
    >
      <NativeTabs.Trigger name="servers">
        <NativeTabs.Trigger.Icon sf={icons.servers.ios} md={icons.servers.android} />
        <NativeTabs.Trigger.Label>Servers</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="you">
        <NativeTabs.Trigger.Icon sf={icons.you.ios} md={icons.you.android} />
        <NativeTabs.Trigger.Label>You</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
