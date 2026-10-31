// Each row (72 pt tall), like a WhatsApp chat (spec §8.2):
// Left: server icon (52 pt)
// Title line: Name, last activity time
// Second line: preview / connection status
// Right of preview: unread / mentions badge, voice glyph if in voice
// Swipes: leading Read, trailing Leave
// Long press: Menu (Mark as read, Retry now, Copy address, Leave server)
// (iOS draws its rows natively: ServerList.ios.)

import { memo, useRef } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import ReanimatedSwipeable, { type SwipeableMethods } from 'react-native-gesture-handler/ReanimatedSwipeable';

import type { ServerRow as ServerRowType } from '@/state/types';
import { Icon, Menu, type MenuHandle, Row, Text, space, useTheme } from '@/ui';

import { ROW_HEIGHT, ServerRowContent, rowText, useServerRowActions } from './serverRowParts';

export { ROW_HEIGHT, ROW_INSET, rowLabel } from './serverRowParts';

/** The row whose actions are showing: opening another closes it, as in Mail. */
let openRow: SwipeableMethods | null = null;

export interface ServerRowProps {
  row: ServerRowType;
  /** The list's clock, so times move on with it. */
  nowMs: number;
  locale: string;
  onOpen: (serverKey: string) => void;
}

export const ServerRow = memo(function ServerRow({ row, nowMs, locale, onOpen }: ServerRowProps) {
  const { colors } = useTheme();
  const swipeableRef = useRef<SwipeableMethods>(null);
  const menuRef = useRef<MenuHandle>(null);
  const { menuActions, handleAction, confirmLeave, markRead } = useServerRowActions(row);

  const renderLeftActions = () => (
    <Touchable
      testID={`read-server-${row.key}`}
      style={[styles.action, { backgroundColor: colors.accent }]}
      onPress={() => {
        swipeableRef.current?.close();
        markRead();
      }}
    >
      <View accessible accessibilityRole="button" accessibilityLabel="Read" style={styles.actionContent}>
        <Icon name="check" color="onAccent" size={24} />
        <Text variant="caption" color="onAccent" style={styles.actionText}>
          Read
        </Text>
      </View>
    </Touchable>
  );

  const renderRightActions = () => (
    <Touchable
      testID={`leave-server-${row.key}`}
      style={[styles.action, { backgroundColor: colors.recording }]}
      onPress={() => {
        swipeableRef.current?.close();
        confirmLeave();
      }}
    >
      <View accessible accessibilityRole="button" accessibilityLabel="Leave" style={styles.actionContent}>
        <Icon name="leave" color="onAccent" size={24} />
        <Text variant="caption" color="onAccent" style={styles.actionText}>
          Leave
        </Text>
      </View>
    </Touchable>
  );

  return (
    <ReanimatedSwipeable
      ref={swipeableRef}
      overshootFriction={8}
      onSwipeableWillOpen={() => {
        if (openRow !== swipeableRef.current) openRow?.close();
        openRow = swipeableRef.current;
      }}
      onSwipeableClose={() => {
        if (openRow === swipeableRef.current) openRow = null;
      }}
      renderLeftActions={renderLeftActions}
      renderRightActions={renderRightActions}
    >
      <Menu
        ref={menuRef}
        testID={`server-menu-${row.key}`}
        trigger="longPress"
        background="sidebar"
        actions={menuActions}
        onAction={handleAction}
      >
        <Row
          height={ROW_HEIGHT}
          accessibilityLabel={rowText(row, nowMs, locale).label}
          onPress={() => onOpen(row.key)}
          onLongPress={() => menuRef.current?.open()}
          // Opaque, so the swipe actions stay hidden until the row moves.
          style={[styles.rowContainer, { backgroundColor: colors.sidebar }]}
        >
          <ServerRowContent row={row} nowMs={nowMs} locale={locale} />
        </Row>
      </Menu>
    </ReanimatedSwipeable>
  );
});

const styles = StyleSheet.create({
  rowContainer: {
    paddingHorizontal: space.lg,
  },
  action: {
    width: ROW_HEIGHT,
  },
  actionContent: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  actionText: {
    marginTop: space.xs,
  },
});
