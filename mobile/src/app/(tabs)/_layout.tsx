// The native tab bar: Servers and You (design §7.1). On iOS, as in Apple's
// own apps, also the search tab beside the bar, and while in voice the
// connection rides above the bar as its accessory (features/servers/chrome).
import { NativeTabs } from 'expo-router/native-tabs';

import { chrome } from '@/features/servers/chrome';
import { VoiceAccessory } from '@/features/voice/VoiceAccessory';
import { useMyVoice } from '@/state/selectors';
import { useTheme } from '@/ui';
import { icons } from '@/ui/icons';

export default function TabsLayout() {
  const { colors } = useTheme();
  const inVoice = useMyVoice() !== null;
  return (
    <NativeTabs
      tintColor={colors.text}
      iconColor={{ default: colors.textMuted, selected: colors.text }}
      backgroundColor={colors.rail}
      // Material's default indicator and ripple are tinted; the app is monochrome.
      indicatorColor={colors.selected}
      rippleColor={colors.hover}
      labelStyle={{ default: { color: colors.textMuted }, selected: { color: colors.text } }}
    >
      {chrome.tabBarSearch && inVoice ? (
        <NativeTabs.BottomAccessory>
          <VoiceAccessory />
        </NativeTabs.BottomAccessory>
      ) : null}
      <NativeTabs.Trigger name="servers">
        <NativeTabs.Trigger.Icon sf={icons.servers.ios} md={icons.servers.android} />
        <NativeTabs.Trigger.Label>Servers</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="you">
        <NativeTabs.Trigger.Icon sf={icons.you.ios} md={icons.you.android} />
        <NativeTabs.Trigger.Label>You</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="search" role="search" hidden={!chrome.tabBarSearch}>
        <NativeTabs.Trigger.Icon sf={icons.search.ios} md={icons.search.android} />
        <NativeTabs.Trigger.Label>Search</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
