// The native tab bar: Servers and You (design §7.1). On iOS, as in Apple's
// own apps, also the search tab beside the bar, and while in voice the
// connection rides above the bar as its accessory (features/servers/chrome).
import { NativeTabs } from 'expo-router/native-tabs';

import { chrome } from '@/features/servers/chrome';
import { VoiceAccessory } from '@/features/voice/VoiceAccessory';
import { useMyVoice } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { useTheme } from '@/ui';
import { icons } from '@/ui/icons';

export default function TabsLayout() {
  const { colors } = useTheme();
  const myVoice = useMyVoice();
  // Not over the call's own page, which shows it whole.
  const onItsPage = useAppStore(
    (state) =>
      state.voicePageInFront !== null &&
      state.voicePageInFront.serverKey === myVoice?.serverKey &&
      state.voicePageInFront.channelId === myVoice.channelId,
  );
  // Under every screen, a server's too, but a text channel in front.
  const chatInFront = useAppStore((state) => state.chatInFront);
  // Hiding the bar keeps its accessory on iOS: it goes too, and the chat
  // shows the call above its composer instead. The call's glass grows out
  // of it (it shows nothing meanwhile, VoiceAccessory); once the call is
  // wholly open, and no finger is pulling it, it goes until the call shrinks.
  const callFull = useAppStore((state) => state.callFull);
  const inVoice = myVoice !== null && !onItsPage && !chatInFront && !callFull;
  return (
    <NativeTabs
      hidden={chatInFront}
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
