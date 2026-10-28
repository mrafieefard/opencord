import { Alert } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import type { ReactNode } from 'react';

import { describeError } from '@/core';
import { deleteMessage, startEditingMessage } from '@/state/actions/chat';
import { toast } from '@/state/actions/ui';
import { useChannelInfo } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { Menu, type MenuAction } from '@/ui';
import type { MessageRecord } from '@opencord/core-native';

export interface MessageMenuProps {
  serverKey: string;
  channelId: string;
  message: MessageRecord;
  trigger?: 'press' | 'longPress';
  children: ReactNode;
}

export function MessageMenu({
  serverKey,
  channelId,
  message,
  trigger = 'longPress',
  children,
}: MessageMenuProps) {
  const selfId = useAppStore((state) => state.servers[serverKey]?.snapshot?.selfUser.id);
  const info = useChannelInfo(serverKey, channelId);

  const isOwn = message.authorId === selfId;
  const canDelete = isOwn || (info?.access.manageMessages ?? false);

  const actions: MenuAction[] = [
    { id: 'copy', title: 'Copy text', icon: 'copy' },
  ];

  if (isOwn) {
    actions.push({ id: 'edit', title: 'Edit', icon: 'edit' });
  }

  if (canDelete) {
    actions.push({ id: 'delete', title: 'Delete', icon: 'delete', destructive: true });
  }

  const handleAction = (id: string) => {
    switch (id) {
      case 'copy':
        Clipboard.setStringAsync(message.content);
        toast('Copied');
        break;
      case 'edit':
        startEditingMessage(serverKey, channelId, message.id, message.content);
        break;
      case 'delete':
        Alert.alert('Delete message?', 'This cannot be undone.', [
          { text: 'Cancel', style: 'cancel' },
          {
            text: 'Delete',
            style: 'destructive',
            onPress: () => {
              deleteMessage(serverKey, message.id).catch((err: unknown) => {
                toast(describeError(err), 'error');
              });
            },
          },
        ]);
        break;
    }
  };

  return (
    <Menu actions={actions} onAction={handleAction} trigger={trigger} testID="message-menu">
      {children}
    </Menu>
  );
}
