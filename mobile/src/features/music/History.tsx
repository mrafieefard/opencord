// History section for music (spec §8.7): the last 50 tracks, each with an Add again button.
import { MusicPlacement, type MusicQueueItemRecord } from '@opencord/core-native';
import { memo, useCallback, useEffect, useState } from 'react';
import { Image, StyleSheet, View } from 'react-native';

import { getCore, report } from '@/core';
import { enqueue } from '@/state/actions/music';
import { toast } from '@/state/actions/ui';
import { useChannelInfo, useMusicSession } from '@/state/selectors';
import { Button, EmptyState, Icon, Row, Text, radius, space, useTheme } from '@/ui';

import { formatDuration } from './position';

export interface HistoryProps {
  serverKey: string;
  channelId: string;
}

const ARTWORK_SIZE = 40;
const ROW_HEIGHT = 56;

const HistoryRow = memo(function HistoryRow({
  serverKey,
  item,
  canAdd,
  onAddAgain,
}: {
  serverKey: string;
  item: MusicQueueItemRecord;
  canAdd: boolean;
  onAddAgain: () => void;
}) {
  const { colors } = useTheme();
  const [artworkUri, setArtworkUri] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (item.track.artworkId) {
      getCore()
        .musicArtwork(serverKey, item.track.artworkId, ARTWORK_SIZE)
        .then((uri) => {
          if (!cancelled && uri) setArtworkUri(uri);
        })
        .catch(() => {
          if (!cancelled) setArtworkUri(null);
        });
    }
    return () => {
      cancelled = true;
    };
  }, [serverKey, item.track.artworkId]);

  const durationStr = formatDuration(item.track.durationMs);
  const subtitle = `${item.track.artist} · ${durationStr}`;

  return (
    <Row
      height={ROW_HEIGHT}
      accessibilityLabel={`${item.track.title}, ${subtitle}`}
      style={[styles.row, { backgroundColor: colors.elevated }]}
    >
      <View style={styles.artworkContainer}>
        {artworkUri ? (
          <Image
            source={{ uri: artworkUri }}
            style={[styles.artwork, { borderRadius: radius.pill }]}
            resizeMode="cover"
          />
        ) : (
          <View
            style={[
              styles.artworkPlaceholder,
              { backgroundColor: colors.surface, borderRadius: radius.pill },
            ]}
          >
            <Icon name="music" size={20} color="textMuted" />
          </View>
        )}
      </View>

      <View style={styles.infoCol}>
        <Text variant="subhead" numberOfLines={1} style={styles.title}>
          {item.track.title}
        </Text>
        <Text variant="callout" color="textSecondary" numberOfLines={1}>
          {subtitle}
        </Text>
      </View>

      {canAdd ? (
        <View style={styles.actionCol}>
          <Button
            title="Add again"
            variant="secondary"
            onPress={onAddAgain}
          />
        </View>
      ) : null}
    </Row>
  );
});

export function History({ serverKey, channelId }: HistoryProps) {
  const session = useMusicSession(serverKey, channelId);
  const info = useChannelInfo(serverKey, channelId);
  const canAdd = info?.access.musicAdd ?? false;

  const history = session?.history ?? [];

  const handleAddAgain = useCallback(
    (itemId: string) => {
      report(
        enqueue(
          serverKey,
          channelId,
          [],
          [itemId],
          undefined,
          MusicPlacement.End,
        ),
        'add track again',
      );
      toast('Added to the queue');
    },
    [serverKey, channelId],
  );

  if (history.length === 0) {
    return (
      <View style={styles.emptyContainer}>
        <EmptyState
          title="No history"
          body="Played tracks will show up here."
        />
      </View>
    );
  }

  return (
    <View style={styles.container}>
      {history.slice(0, 50).map((item) => (
        <HistoryRow
          key={item.itemId}
          serverKey={serverKey}
          item={item}
          canAdd={canAdd}
          onAddAgain={() => handleAddAgain(item.itemId)}
        />
      ))}
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
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: space.md,
    gap: space.md,
  },
  artworkContainer: {
    width: ARTWORK_SIZE,
    height: ARTWORK_SIZE,
  },
  artwork: {
    width: ARTWORK_SIZE,
    height: ARTWORK_SIZE,
  },
  artworkPlaceholder: {
    width: ARTWORK_SIZE,
    height: ARTWORK_SIZE,
    alignItems: 'center',
    justifyContent: 'center',
  },
  infoCol: {
    flex: 1,
    justifyContent: 'center',
  },
  title: {
    marginBottom: 2,
  },
  actionCol: {
    minWidth: 90,
  },
});
