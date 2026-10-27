// The channel list's pane header (spec §8.4): back, the server's name, and
// the connection state under it while not connected. Task 12 adds the menu.
import { connectionText } from '@/features/servers/connectionText';
import { useServerHeader } from '@/state/selectors';
import { PaneHeader } from '@/ui';

export interface ChannelListHeaderProps {
  serverKey: string;
  /** Where back goes: "Servers" leaves the server, "All channels" widens the sidebar into the full list. */
  backLabel: 'Servers' | 'All channels';
  onBack(): void;
}

export function ChannelListHeader({ serverKey, backLabel, onBack }: ChannelListHeaderProps) {
  const header = useServerHeader(serverKey);
  const state = header ? connectionText(header.connection) : null;
  return (
    <PaneHeader
      title={header?.name ?? ''}
      subtitle={state?.text}
      left={{ icon: 'back', label: backLabel, onPress: onBack }}
    />
  );
}
