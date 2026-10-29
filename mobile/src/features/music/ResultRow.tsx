// A search result row (spec §8.7): 56 pt, with 40 pt artwork,
// title, Local pill on uploaded songs, artist · duration, tap to add, and long-press menu.
import type { MusicSearchResultRecord } from '@opencord/core-native';
import { memo, useEffect, useRef, useState } from 'react';
import { Image, StyleSheet, View } from 'react-native';

import { getCore } from '@/core';
import {
  Icon,
  Menu,
  type MenuAction,
  type MenuHandle,
  Row,
  Text,
  radius,
  space,
  useTheme,
} from '@/ui';

import { formatDuration } from './position';

export interface ResultRowProps {
  serverKey: string;
  result: MusicSearchResultRecord;
  canAdd: boolean;
  canManageQueue: boolean;
  onAddEnd: () => void;
  onPlayNext: () => void;
  onPlayNow: () => void;
}

const ARTWORK_SIZE = 40;
const ROW_HEIGHT = 56;

export const ResultRow = memo(function ResultRow({
  serverKey,
  result,
  canAdd,
  canManageQueue,
  onAddEnd,
  onPlayNext,
  onPlayNow,
}: ResultRowProps) {
  const { colors } = useTheme();
  const menuRef = useRef<MenuHandle>(null);
  const [artworkUri, setArtworkUri] = useState<string | null>(null);

  const track = result.track;

  useEffect(() => {
    let cancelled = false;
    if (track.artworkId) {
      getCore()
        .musicArtwork(serverKey, track.artworkId, ARTWORK_SIZE)
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
  }, [serverKey, track.artworkId]);

  const menuActions: MenuAction[] = canManageQueue
    ? [
        { id: 'play-next', title: 'Play next', icon: 'play' as const },
        { id: 'play-now', title: 'Play now', icon: 'play' as const },
      ]
    : [];

  const handleAction = (actionId: string) => {
    if (actionId === 'play-next') onPlayNext();
    else if (actionId === 'play-now') onPlayNow();
  };

  const isLocal = track.source === 'local' || track.sourceDisplayName === 'Local';
  const durationStr = formatDuration(track.durationMs);
  const subtitle = `${track.artist} · ${durationStr}`;

  return (
    <View style={styles.container}>
      <Menu
        ref={menuRef}
        testID={`search-result-menu-${result.resultId}`}
        trigger="longPress"
        actions={menuActions}
        onAction={handleAction}
      >
        <Row
          height={ROW_HEIGHT}
          accessibilityLabel={`${track.title}, ${subtitle}`}
          onPress={canAdd ? onAddEnd : undefined}
          onLongPress={canManageQueue ? () => menuRef.current?.open() : undefined}
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
            <View style={styles.titleRow}>
              <Text variant="subhead" numberOfLines={1} style={styles.title}>
                {track.title}
              </Text>
              {isLocal ? (
                <View
                  style={[
                    styles.localPill,
                    { backgroundColor: colors.surface, borderColor: colors.border },
                  ]}
                >
                  <Text variant="caption" color="textSecondary" style={styles.localText}>
                    Local
                  </Text>
                </View>
              ) : null}
            </View>
            <Text variant="callout" color="textSecondary" numberOfLines={1}>
              {subtitle}
            </Text>
          </View>
        </Row>
      </Menu>
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
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
  titleRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
    marginBottom: 2,
  },
  title: {
    flexShrink: 1,
  },
  localPill: {
    paddingHorizontal: space.xs,
    paddingVertical: 1,
    borderRadius: radius.pill,
    borderWidth: StyleSheet.hairlineWidth,
  },
  localText: {
    fontSize: 10,
    lineHeight: 12,
  },
});
