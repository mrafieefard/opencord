// A row in the Up next queue (spec §8.7): 56 pt, with 40 pt artwork,
// title, artist · duration, requester avatar, swipe to remove, and long-press menu.
import type { MusicQueueItemRecord } from '@opencord/core-native';
import * as Clipboard from 'expo-clipboard';
import { memo, useEffect, useRef, useState } from 'react';
import { Image, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import ReanimatedSwipeable, {
  type SwipeableMethods,
} from 'react-native-gesture-handler/ReanimatedSwipeable';

import { getCore, report } from '@/core';
import { toast } from '@/state/actions/ui';
import { useMember } from '@/state/selectors';
import {
  Avatar,
  Icon,
  Menu,
  type MenuAction,
  type MenuHandle,
  Row,
  Text,
  onHue,
  radius,
  space,
  useTheme,
} from '@/ui';

import { formatDuration } from './position';

export interface QueueRowProps {
  serverKey: string;
  channelId: string;
  item: MusicQueueItemRecord;
  index: number;
  totalItems: number;
  canRemove: boolean;
  canManageQueue: boolean;
  onPlayNext: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
}

const ARTWORK_SIZE = 40;
const ROW_HEIGHT = 56;

export const QueueRow = memo(function QueueRow({
  serverKey,
  item,
  index,
  totalItems,
  canRemove,
  canManageQueue,
  onPlayNext,
  onMoveUp,
  onMoveDown,
  onRemove,
}: QueueRowProps) {
  const { colors } = useTheme();
  const swipeableRef = useRef<SwipeableMethods>(null);
  const menuRef = useRef<MenuHandle>(null);

  const requester = useMember(serverKey, item.requesterId);
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

  const menuActions: MenuAction[] = [
    ...(canManageQueue
      ? [
          { id: 'play-next', title: 'Play next', icon: 'play' as const },
          { id: 'move-up', title: 'Move up', icon: 'arrowUp' as const, disabled: index === 0 },
          { id: 'move-down', title: 'Move down', icon: 'arrowDown' as const, disabled: index === totalItems - 1 },
        ]
      : []),
    ...(item.track.uri
      ? [{ id: 'copy-link', title: 'Copy link', icon: 'copy' as const }]
      : []),
    ...(canRemove
      ? [{ id: 'remove', title: 'Remove', icon: 'delete' as const, destructive: true }]
      : []),
  ];

  const handleAction = (actionId: string) => {
    if (actionId === 'play-next') onPlayNext();
    else if (actionId === 'move-up') onMoveUp();
    else if (actionId === 'move-down') onMoveDown();
    else if (actionId === 'copy-link') {
      report(
        Clipboard.setStringAsync(item.track.uri).then(() => {
          toast('Copied link');
        }),
        'copy link',
      );
    } else if (actionId === 'remove') onRemove();
  };

  const durationStr = formatDuration(item.track.durationMs);
  const subtitle = `${item.track.artist} · ${durationStr}`;

  const renderRightActions = () => {
    if (!canRemove) return null;
    return (
      <Touchable
        testID={`swipe-remove-${item.itemId}`}
        onPress={() => {
          swipeableRef.current?.close();
          onRemove();
        }}
        accessible
        accessibilityRole="button"
        accessibilityLabel={`Remove ${item.track.title}`}
        style={[styles.deleteAction, { backgroundColor: colors.recording }]}
      >
        <Icon name="delete" size={24} tint={onHue} />
      </Touchable>
    );
  };

  const rowContent = (
    <Row
      height={ROW_HEIGHT}
      accessibilityLabel={`${item.track.title}, ${subtitle}`}
      onLongPress={() => menuRef.current?.open()}
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

      {requester ? (
        <View style={styles.avatarBox}>
          <Avatar name={requester.name} size={24} />
        </View>
      ) : null}
    </Row>
  );

  return (
    <View style={styles.container}>
      <Menu
        ref={menuRef}
        testID={`queue-row-menu-${item.itemId}`}
        trigger="longPress"
        actions={menuActions}
        onAction={handleAction}
      >
        {canRemove ? (
          <ReanimatedSwipeable
            ref={swipeableRef}
            renderRightActions={renderRightActions}
            overshootFriction={8}
          >
            {rowContent}
          </ReanimatedSwipeable>
        ) : (
          rowContent
        )}
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
  title: {
    marginBottom: 2,
  },
  avatarBox: {
    marginLeft: space.xs,
  },
  deleteAction: {
    width: 72,
    height: ROW_HEIGHT,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
