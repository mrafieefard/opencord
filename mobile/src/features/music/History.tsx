// Recently played (spec §8.7): the last songs, newest first, five until
// "Show all" (up to 50). Each adds again from its button, which turns
// into a tick once the server has it. Nothing at all while empty.
import { MusicPlacement, type MusicQueueItemRecord } from '@opencord/core-native';
import * as Haptics from 'expo-haptics';
import { useCallback, useState } from 'react';
import { View } from 'react-native';

import { report } from '@/core';
import { enqueue } from '@/state/actions/music';
import { useChannelInfo, useMusicSession } from '@/state/selectors';
import { IconButton } from '@/ui';

import { SectionHeader } from './SectionHeader';
import { TrackRow, trackLength } from './TrackRow';

export interface HistoryProps {
  serverKey: string;
  channelId: string;
}

const FOLDED = 5;
const MOST = 50;

export function History({ serverKey, channelId }: HistoryProps) {
  const session = useMusicSession(serverKey, channelId);
  const info = useChannelInfo(serverKey, channelId);
  const canAdd = info?.access.musicAdd ?? false;
  const [expanded, setExpanded] = useState(false);
  const [added, setAdded] = useState<ReadonlySet<string>>(new Set());

  const history = (session?.history ?? []).slice(0, MOST);

  const addAgain = useCallback(
    (item: MusicQueueItemRecord) => {
      void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
      report(
        enqueue(serverKey, channelId, [], [item.itemId], undefined, MusicPlacement.End).then(() =>
          setAdded((current) => new Set(current).add(item.itemId)),
        ),
        'add track again',
      );
    },
    [serverKey, channelId],
  );

  if (history.length === 0) return null;
  const shown = expanded ? history : history.slice(0, FOLDED);

  return (
    <View>
      <SectionHeader
        title="Recently played"
        action={
          history.length > FOLDED
            ? {
                title: expanded ? 'Show less' : 'Show all',
                label: expanded ? 'Show fewer recently played songs' : 'Show all recently played songs',
                onPress: () => setExpanded(!expanded),
              }
            : undefined
        }
      />
      {shown.map((item) => {
        const done = added.has(item.itemId);
        return (
          <TrackRow
            key={item.itemId}
            serverKey={serverKey}
            track={item.track}
            requesterId={item.requesterId}
            accessibilityLabel={`${item.track.title}, ${item.track.artist}, ${trackLength(item.track)}`}
            trailing={
              canAdd ? (
                <IconButton
                  icon={done ? 'check' : 'add'}
                  label={done ? `${item.track.title} is queued` : `Add ${item.track.title} again`}
                  size={36}
                  onPress={() => {
                    if (!done) addAgain(item);
                  }}
                />
              ) : undefined
            }
          />
        );
      })}
    </View>
  );
}
