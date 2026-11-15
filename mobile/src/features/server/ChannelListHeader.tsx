// The channel list's pane header (spec §8.4): back, the server's name, and
// the connection state under it while not connected. The name opens the
// server sheet on both systems: it is the only way in while a chat is in
// front and the tab bar is hidden. Android also has the server button at
// the right (design §6.1); on iOS it is the tab bar's search slot.
import { openServerSheet } from '@/features/serverMenu/serverRows';
import { chrome } from '@/features/servers/chrome';
import { connectionText } from '@/features/servers/connectionText';
import { useServerHeader } from '@/state/selectors';
import { IconButton, PaneHeader } from '@/ui';

export interface ChannelListHeaderProps {
  serverKey: string;
  /** Where back goes: "Servers" leaves the server, "All channels" widens the sidebar into the full list. */
  backLabel: 'Servers' | 'All channels';
  onBack(): void;
}

export function ChannelListHeader({ serverKey, backLabel, onBack }: ChannelListHeaderProps) {
  const header = useServerHeader(serverKey);
  const state = header ? connectionText(header.connection) : null;
  const open = () => openServerSheet(serverKey);
  return (
    <PaneHeader
      title={header?.name ?? ''}
      subtitle={state?.text}
      left={{ icon: 'back', label: backLabel, onPress: onBack }}
      onTitlePress={open}
      rightElement={chrome.tabBarSearch ? undefined : <IconButton icon="serverMenu" label="Server" onPress={open} testID="server-button" />}
    />
  );
}
