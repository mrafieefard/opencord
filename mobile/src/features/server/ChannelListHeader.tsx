// The channel list's pane header (spec §8.4): back, the server's name, and
// the connection state under it while not connected. The name opens the
// server sheet on both systems: it is the only way in while a chat is in
// front and the tab bar is hidden. Android also has the server button at
// the right (design §6.1); on iOS it is the tab bar's search slot.
import { router } from 'expo-router';
import { StyleSheet, View } from 'react-native';

import { canInvite, openServerSheet } from '@/features/serverMenu/serverRows';
import { chrome } from '@/features/servers/chrome';
import { connectionText } from '@/features/servers/connectionText';
import { useServerHeader } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { IconButton, PaneHeader } from '@/ui';

export interface ChannelListHeaderProps {
  serverKey: string;
  /** Where back goes: "Servers" leaves the server, "All channels" widens the sidebar into the full list. */
  backLabel: 'Servers' | 'All channels';
  onBack(): void;
}

export function ChannelListHeader({ serverKey, backLabel, onBack }: ChannelListHeaderProps) {
  const header = useServerHeader(serverKey);
  const snapshot = useAppStore((state) => state.servers[serverKey]?.snapshot);
  const state = header ? connectionText(header.connection) : null;
  const open = () => openServerSheet(serverKey);
  const openInvite = () => router.push(`/server/${serverKey}/invite`);
  const mayInvite = canInvite(snapshot);

  return (
    <PaneHeader
      title={header?.name ?? ''}
      subtitle={state?.text}
      left={{ icon: 'back', label: backLabel, onPress: onBack }}
      onTitlePress={open}
      rightElement={
        <View style={styles.rightGroup}>
          {mayInvite && (
            <IconButton
              icon="personAdd"
              label="Invite people"
              onPress={openInvite}
              testID="invite-button"
            />
          )}
          {!chrome.tabBarSearch && (
            <IconButton
              icon="serverMenu"
              label="Server"
              onPress={open}
              testID="server-button"
            />
          )}
        </View>
      }
    />
  );
}

const styles = StyleSheet.create({
  rightGroup: {
    flexDirection: 'row',
    alignItems: 'center',
  },
});

