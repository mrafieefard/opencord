// A song in Up next (spec §8.7): swipe to remove, long-press for the menu
// (Play next, Move up, Move down, Copy link, Remove).
import type { MusicQueueItemRecord } from '@opencord/core-native';
import * as Clipboard from 'expo-clipboard';
import { memo, useRef } from 'react';
import { StyleSheet } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import ReanimatedSwipeable, { type SwipeableMethods } from 'react-native-gesture-handler/ReanimatedSwipeable';

import { report } from '@/core';
import { toast } from '@/state/actions/ui';
import { Icon, Menu, type MenuAction, type MenuHandle, onHue, useTheme } from '@/ui';

import { TRACK_ROW_HEIGHT, TrackRow, trackLength } from './TrackRow';

export interface QueueRowProps {
  serverKey: string;
  item: MusicQueueItemRecord;
  isFirst: boolean;
  isLast: boolean;
  canRemove: boolean;
  canManageQueue: boolean;
  onPlayNext: () => void;
  onMoveUp: () => void;
  onMoveDown: () => void;
  onRemove: () => void;
}

const SWIPE_ACTION_WIDTH = 80;

export const QueueRow = memo(function QueueRow({
  serverKey,
  item,
  isFirst,
  isLast,
  canRemove,
  canManageQueue,
  onPlayNext,
  onMoveUp,
  onMoveDown,
  onRemove,
}: QueueRowProps) {
  const { colors } = useTheme();
  const swipeable = useRef<SwipeableMethods>(null);
  const menu = useRef<MenuHandle>(null);
  const { track } = item;

  const actions: MenuAction[] = [
    ...(canManageQueue
      ? [
          { id: 'play-next', title: 'Play next', icon: 'play' as const, disabled: isFirst },
          { id: 'move-up', title: 'Move up', icon: 'arrowUp' as const, disabled: isFirst },
          { id: 'move-down', title: 'Move down', icon: 'arrowDown' as const, disabled: isLast },
        ]
      : []),
    ...(track.uri ? [{ id: 'copy-link', title: 'Copy link', icon: 'copy' as const }] : []),
    ...(canRemove ? [{ id: 'remove', title: 'Remove', icon: 'delete' as const, destructive: true }] : []),
  ];

  const onAction = (id: string) => {
    if (id === 'play-next') onPlayNext();
    else if (id === 'move-up') onMoveUp();
    else if (id === 'move-down') onMoveDown();
    else if (id === 'remove') onRemove();
    else if (id === 'copy-link') {
      report(
        Clipboard.setStringAsync(track.uri).then(() => toast('Copied link')),
        'copy link',
      );
    }
  };

  const row = (
    <TrackRow
      serverKey={serverKey}
      track={track}
      requesterId={item.requesterId}
      onLongPress={actions.length > 0 ? () => menu.current?.open() : undefined}
      accessibilityLabel={`${track.title}, ${track.artist}, ${trackLength(track)}`}
    />
  );

  const removeAction = () => (
    <Touchable
      testID={`swipe-remove-${item.itemId}`}
      onPress={() => {
        swipeable.current?.close();
        onRemove();
      }}
      accessible
      accessibilityRole="button"
      accessibilityLabel={`Remove ${track.title}`}
      style={[styles.remove, { backgroundColor: colors.recording }]}
    >
      <Icon name="delete" size={22} tint={onHue} />
    </Touchable>
  );

  return (
    <Menu ref={menu} testID={`queue-row-menu-${item.itemId}`} trigger="longPress" actions={actions} onAction={onAction}>
      {canRemove ? (
        <ReanimatedSwipeable ref={swipeable} renderRightActions={removeAction} rightThreshold={SWIPE_ACTION_WIDTH / 2} overshootFriction={8}>
          {row}
        </ReanimatedSwipeable>
      ) : (
        row
      )}
    </Menu>
  );
});

const styles = StyleSheet.create({
  remove: { width: SWIPE_ACTION_WIDTH, height: TRACK_ROW_HEIGHT, alignItems: 'center', justifyContent: 'center' },
});
