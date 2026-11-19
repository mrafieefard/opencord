// The native tab bar: Servers and You (design §7.1). On iOS, as in Apple's
// own apps, also the search tab beside the bar; while in voice the call's
// glass pill floats above it (features/voice/CallSheet).
import { NativeTabs } from 'expo-router/native-tabs';

import { openServerSheet } from '@/features/serverMenu/serverRows';
import { chrome } from '@/features/servers/chrome';
import { usePrivateUnreadTotal } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { useTheme } from '@/ui';
import { icons } from '@/ui/icons';

export default function TabsLayout() {
  const { colors } = useTheme();
  const unreadTotal = usePrivateUnreadTotal();
  // Under every screen, a server's too, but a text channel in front.
  const chatInFront = useAppStore((state) => state.chatInFront);
  // On iOS the search slot is this server's button while its screen is in front.
  const serverInFront = useAppStore((state) => (chrome.tabBarSearch ? state.serverInFront : null));
  const slotIcon = serverInFront !== null ? icons.serverMenu : icons.search;
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
      <NativeTabs.Trigger name="servers">
        <NativeTabs.Trigger.Icon sf={icons.servers.ios} md={icons.servers.android} />
        <NativeTabs.Trigger.Label>Servers</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="messages">
        <NativeTabs.Trigger.Icon sf="bubble.left.and.bubble.right" md="chat" />
        <NativeTabs.Trigger.Label>Messages</NativeTabs.Trigger.Label>
        {unreadTotal > 0 && <NativeTabs.Trigger.Badge>{String(unreadTotal)}</NativeTabs.Trigger.Badge>}
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="you">
        <NativeTabs.Trigger.Icon sf={icons.you.ios} md={icons.you.android} />
        <NativeTabs.Trigger.Label>You</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
      {/*
        The search slot. While a server's screen is in front it is that
        server's button (design §6.1): disabled, so a press stays on the
        server and comes as `tabPress` alone, which opens the server sheet.
        Only the props change: `name` and `hidden` stay, so the tabs aren't
        rebuilt (their key is the triggers' names).
      */}
      <NativeTabs.Trigger
        name="search"
        role="search"
        hidden={!chrome.tabBarSearch}
        disabled={serverInFront !== null}
        accessibilityLabel={serverInFront !== null ? 'Server' : 'Search'}
        listeners={{
          tabPress: () => {
            if (serverInFront !== null) openServerSheet(serverInFront);
          },
        }}
      >
        <NativeTabs.Trigger.Icon sf={slotIcon.ios} md={slotIcon.android} />
        <NativeTabs.Trigger.Label>{serverInFront !== null ? 'Server' : 'Search'}</NativeTabs.Trigger.Label>
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
