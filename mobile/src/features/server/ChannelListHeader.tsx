// The channel list's pane header (spec §8.4): back, the server's name, and
// the connection state under it while not connected.
import { Alert } from 'react-native';
import * as Clipboard from 'expo-clipboard';

import { describeError } from '@/core';
import { connectionText } from '@/features/servers/connectionText';
import { leaveServer, markServerRead } from '@/state/actions/servers';
import { toast } from '@/state/actions/ui';
import { useServerHeader } from '@/state/selectors';
import { IconButton, Menu, PaneHeader, type MenuAction } from '@/ui';

export interface ChannelListHeaderProps {
  serverKey: string;
  /** Where back goes: "Servers" leaves the server, "All channels" widens the sidebar into the full list. */
  backLabel: 'Servers' | 'All channels';
  onBack(): void;
}

export function ChannelListHeader({ serverKey, backLabel, onBack }: ChannelListHeaderProps) {
  const header = useServerHeader(serverKey);
  const state = header ? connectionText(header.connection) : null;

  const confirmLeave = () => {
    Alert.alert(`Leave ${header?.name ?? 'server'}?`, 'You can come back with an invite.', [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Leave',
        style: 'destructive',
        onPress: () => {
          leaveServer(serverKey).catch((error: unknown) => {
            toast(describeError(error), 'error');
          });
        },
      },
    ]);
  };

  const menuActions: MenuAction[] = [
    { id: 'mark-read', title: 'Mark all as read', icon: 'check' },
    { id: 'copy-address', title: 'Copy address', icon: 'copy' },
    { id: 'leave', title: 'Leave server', icon: 'leave', destructive: true },
  ];

  const handleAction = (id: string) => {
    switch (id) {
      case 'mark-read':
        markServerRead(serverKey);
        break;
      case 'copy-address':
        if (header?.address) {
          Clipboard.setStringAsync(header.address)
            .then(() => toast('Address copied'))
            .catch(() => toast('Could not copy address', 'error'));
        }
        break;
      case 'leave':
        confirmLeave();
        break;
    }
  };

  return (
    <PaneHeader
      title={header?.name ?? ''}
      subtitle={state?.text}
      left={{ icon: 'back', label: backLabel, onPress: onBack }}
      rightElement={
        <Menu actions={menuActions} onAction={handleAction} trigger="press" testID="channel-list-menu">
          <IconButton icon="more" label="Server options" onPress={() => {}} />
        </Menu>
      }
    />
  );
}
