// The Up next queue section (spec §8.7): list of queued tracks, swipe to remove,
// reordering, and queue clearing with confirmation.
import { useCallback, useMemo } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { report } from '@/core';
import { clearQueue, moveItem, removeItems } from '@/state/actions/music';
import { useChannelInfo, useMusicSession, useServer } from '@/state/selectors';
import { EmptyState, Text, confirm, space } from '@/ui';

import { QueueRow } from './QueueRow';

export interface UpNextProps {
  serverKey: string;
  channelId: string;
}

export function UpNext({ serverKey, channelId }: UpNextProps) {
  const session = useMusicSession(serverKey, channelId);
  const info = useChannelInfo(serverKey, channelId);
  const server = useServer(serverKey);

  const selfUserId = server?.snapshot?.selfUser.id ?? server?.entry.userId;
  const access = info?.access;
  const canManageQueue = access?.musicManageQueue ?? false;
  const canAdd = access?.musicAdd ?? false;

  const queue = useMemo(() => session?.queue ?? [], [session?.queue]);

  const handleClear = useCallback(() => {
    confirm({
      title: 'Clear queue?',
      message: 'This will remove all songs from the queue.',
      confirmTitle: 'Clear',
      destructive: true,
      onConfirm: () => {
        report(clearQueue(serverKey, channelId), 'clear queue');
      },
    });
  }, [serverKey, channelId]);

  const handlePlayNext = useCallback(
    (itemId: string) => {
      const firstId = queue[0]?.itemId;
      if (firstId === itemId) return;
      report(moveItem(serverKey, channelId, itemId, firstId), 'play next');
    },
    [serverKey, channelId, queue],
  );

  const handleMoveUp = useCallback(
    (index: number) => {
      if (index <= 0) return;
      const itemId = queue[index]?.itemId;
      const beforeId = queue[index - 1]?.itemId;
      if (!itemId || !beforeId) return;
      report(moveItem(serverKey, channelId, itemId, beforeId), 'move up');
    },
    [serverKey, channelId, queue],
  );

  const handleMoveDown = useCallback(
    (index: number) => {
      if (index >= queue.length - 1) return;
      const itemId = queue[index]?.itemId;
      const beforeId = queue[index + 2]?.itemId;
      if (!itemId) return;
      report(moveItem(serverKey, channelId, itemId, beforeId), 'move down');
    },
    [serverKey, channelId, queue],
  );

  const handleRemove = useCallback(
    (itemId: string) => {
      report(removeItems(serverKey, channelId, [itemId]), 'remove item');
    },
    [serverKey, channelId],
  );

  if (queue.length === 0) {
    return (
      <View style={styles.emptyContainer}>
        <EmptyState
          title="Nothing queued"
          body="Search for a song or upload one."
        />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <View style={styles.header}>
        <Text variant="headline">Up next</Text>
        {canManageQueue ? (
          <Touchable
            onPress={handleClear}
            accessible
            accessibilityRole="button"
            accessibilityLabel="Clear queue"
            hitSlop={space.xs}
          >
            <Text variant="callout" color="recording">
              Clear
            </Text>
          </Touchable>
        ) : null}
      </View>

      <View style={styles.list}>
        {queue.map((item, index) => {
          const isOwn = item.requesterId === selfUserId;
          const canRemove = (isOwn && canAdd) || canManageQueue;
          return (
            <QueueRow
              key={item.itemId}
              serverKey={serverKey}
              channelId={channelId}
              item={item}
              index={index}
              totalItems={queue.length}
              canRemove={canRemove}
              canManageQueue={canManageQueue}
              onPlayNext={() => handlePlayNext(item.itemId)}
              onMoveUp={() => handleMoveUp(index)}
              onMoveDown={() => handleMoveDown(index)}
              onRemove={() => handleRemove(item.itemId)}
            />
          );
        })}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    width: '100%',
  },
  emptyContainer: {
    paddingVertical: space.xl,
    alignItems: 'center',
    width: '100%',
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: space.md,
    paddingVertical: space.sm,
  },
  list: {
    width: '100%',
  },
});
