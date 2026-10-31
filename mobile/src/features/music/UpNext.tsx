// Up next (spec §8.7): how many songs and how long, Clear (asks first) with
// Manage queue, then the songs.
import type { MusicQueueItemRecord } from '@opencord/core-native';
import { useCallback, useMemo } from 'react';
import { View } from 'react-native';

import { report } from '@/core';
import { clearQueue, moveItem, removeItems } from '@/state/actions/music';
import { useChannelInfo, useMusicSession, useServer } from '@/state/selectors';
import { confirm } from '@/ui';

import { QueueRow } from './QueueRow';
import { SectionHeader, SectionNote } from './SectionHeader';

export interface UpNextProps {
  serverKey: string;
  channelId: string;
}

/** "5 songs · 23 min", or "1 song · 1 h 5 min"; live streams add no time. */
export function queueSummary(queue: MusicQueueItemRecord[]): string {
  const songs = `${queue.length} ${queue.length === 1 ? 'song' : 'songs'}`;
  const totalMs = queue.reduce((sum, item) => sum + (item.track.isStream ? 0 : item.track.durationMs), 0);
  const minutes = Math.round(totalMs / 60_000);
  if (minutes === 0) return songs;
  const time = minutes >= 60 ? `${Math.floor(minutes / 60)} h ${minutes % 60} min` : `${minutes} min`;
  return `${songs} · ${time}`;
}

export function UpNext({ serverKey, channelId }: UpNextProps) {
  const session = useMusicSession(serverKey, channelId);
  const info = useChannelInfo(serverKey, channelId);
  const server = useServer(serverKey);

  const selfUserId = server?.snapshot?.selfUser.id ?? server?.entry.userId;
  const canManageQueue = info?.access.musicManageQueue ?? false;
  const canAdd = info?.access.musicAdd ?? false;
  const queue = useMemo(() => session?.queue ?? [], [session?.queue]);

  const clear = useCallback(() => {
    confirm({
      title: 'Clear the queue?',
      message: `This removes ${queue.length === 1 ? 'the song' : `all ${queue.length} songs`} waiting to play.`,
      confirmTitle: 'Clear',
      destructive: true,
      onConfirm: () => report(clearQueue(serverKey, channelId), 'clear queue'),
    });
  }, [serverKey, channelId, queue.length]);

  const move = useCallback(
    (itemId: string, beforeId: string | undefined, what: string) =>
      report(moveItem(serverKey, channelId, itemId, beforeId), what),
    [serverKey, channelId],
  );

  return (
    <View>
      <SectionHeader
        title="Up next"
        detail={queue.length > 0 ? queueSummary(queue) : undefined}
        action={canManageQueue && queue.length > 0 ? { title: 'Clear', label: 'Clear queue', onPress: clear } : undefined}
      />
      {queue.length === 0 ? (
        <SectionNote title="Nothing queued" body="Search for a song or upload one." />
      ) : (
        queue.map((item, index) => (
          <QueueRow
            key={item.itemId}
            serverKey={serverKey}
            item={item}
            isFirst={index === 0}
            isLast={index === queue.length - 1}
            canRemove={canManageQueue || (canAdd && item.requesterId === selfUserId)}
            canManageQueue={canManageQueue}
            onPlayNext={() => move(item.itemId, queue[0]?.itemId, 'play next')}
            onMoveUp={() => move(item.itemId, queue[index - 1]?.itemId, 'move up')}
            onMoveDown={() => move(item.itemId, queue[index + 2]?.itemId, 'move down')}
            onRemove={() => report(removeItems(serverKey, channelId, [item.itemId]), 'remove item')}
          />
        ))
      )}
    </View>
  );
}
