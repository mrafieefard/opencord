// Each row (72 pt tall), like a WhatsApp chat (spec §8.2):
// Left: server icon (52 pt)
// Title line: Name, last activity time
// Second line: preview / connection status
// Right of preview: unread / mentions badge, voice glyph if in voice
// Swipes: leading Read, trailing Leave
// Long press: Menu (Mark as read, Retry now, Copy address, Leave server)

import * as Clipboard from 'expo-clipboard';
import { memo, useRef } from 'react';
import { Alert, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import ReanimatedSwipeable, { type SwipeableMethods } from 'react-native-gesture-handler/ReanimatedSwipeable';
import { ConnectionRecord_Tags } from '@opencord/core-native';

import { report } from '@/core';
import { leaveServer, markServerRead, retryServer } from '@/state/actions/servers';
import type { ServerRow as ServerRowType } from '@/state/types';
import { Badge, Icon, Menu, type MenuAction, type MenuHandle, Row, ServerIcon, Text, space, useTheme } from '@/ui';
import { connectionText } from './connectionText';
import { formatTime } from './formatTime';
import { useImageFile } from './useImageFile';

export const ROW_HEIGHT = 72;
const ICON_SIZE = 52;
/** Where the hairline between rows starts: after the icon (design §9.3). */
export const ROW_INSET = space.lg + ICON_SIZE + space.md;

/** The row whose actions are showing: opening another closes it, as in Mail. */
let openRow: SwipeableMethods | null = null;

export interface ServerRowProps {
  row: ServerRowType;
  /** The list's clock, so times move on with it. */
  nowMs: number;
  locale: string;
  onOpen: (serverKey: string) => void;
}

/** What a screen reader says for the row: name, second line, counts, voice, time. */
export function rowLabel(row: ServerRowType, second: string, time: string | null): string {
  const counts = row.mentions > 0 ? `${row.mentions} mentions` : row.unread > 0 ? `${row.unread} unread` : null;
  return [row.name, second, counts, row.inVoice ? 'in voice' : null, time].filter(Boolean).join(', ');
}

export const ServerRow = memo(function ServerRow({ row, nowMs, locale, onOpen }: ServerRowProps) {
  const { colors } = useTheme();
  const swipeableRef = useRef<SwipeableMethods>(null);
  const menuRef = useRef<MenuHandle>(null);
  const iconFile = useImageFile(row.key, row.iconHash);
  const conn = connectionText(row.connection);
  const connected = row.connection.tag === ConnectionRecord_Tags.Connected;

  const confirmLeave = () => {
    Alert.alert(`Leave ${row.name}?`, 'You can come back with an invite.', [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Leave',
        style: 'destructive',
        onPress: () => report(leaveServer(row.key), 'leave server'),
      },
    ]);
  };

  const menuActions: MenuAction[] = [
    { id: 'read', title: 'Mark as read', icon: 'check' },
    ...(connected ? [] : [{ id: 'retry', title: 'Retry now', icon: 'retry' } satisfies MenuAction]),
    { id: 'copy-address', title: 'Copy address', icon: 'copy' },
    { id: 'leave', title: 'Leave server', icon: 'leave', destructive: true },
  ];

  const handleAction = (id: string) => {
    switch (id) {
      case 'read':
        markServerRead(row.key);
        break;
      case 'retry':
        report(retryServer(row.key), 'retry server');
        break;
      case 'copy-address':
        report(Clipboard.setStringAsync(row.address), 'copy address');
        break;
      case 'leave':
        confirmLeave();
        break;
    }
  };

  const renderLeftActions = () => (
    <Touchable
      testID={`read-server-${row.key}`}
      style={[styles.action, { backgroundColor: colors.accent }]}
      onPress={() => {
        swipeableRef.current?.close();
        markServerRead(row.key);
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

  const timeText = row.lastAtMs ? formatTime(row.lastAtMs, nowMs, locale) : null;
  const second = conn?.text ?? (row.preview ? previewLine(row.preview) : 'No messages yet');

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
        actions={menuActions}
        onAction={handleAction}
      >
        <Row
          height={ROW_HEIGHT}
          accessibilityLabel={rowLabel(row, second, timeText)}
          onPress={() => onOpen(row.key)}
          onLongPress={() => menuRef.current?.open()}
          // Opaque, so the swipe actions stay hidden until the row moves.
          style={[styles.rowContainer, { backgroundColor: colors.sidebar }]}
        >
          <View style={styles.content}>
            <ServerIcon name={row.name} file={iconFile} size={ICON_SIZE} />
            <View style={styles.details}>
              <View style={styles.topLine}>
                <Text variant="headline" numberOfLines={1} style={styles.name}>
                  {row.name}
                </Text>
                {timeText ? (
                  <Text variant="footnote" color="textMuted" tabular>
                    {timeText}
                  </Text>
                ) : null}
              </View>
              <View style={styles.bottomLine}>
                <View style={styles.previewContainer}>
                  <Text variant="callout" color={conn?.error ? 'recording' : 'textMuted'} numberOfLines={1}>
                    {second}
                  </Text>
                </View>
                <View style={styles.rightAccessories}>
                  {row.inVoice ? (
                    <View testID={`voice-glyph-${row.key}`} style={styles.accessory}>
                      <Icon name="voiceChannel" color="voice" size={16} />
                    </View>
                  ) : null}
                  {row.mentions > 0 ? (
                    <Badge count={row.mentions} mention />
                  ) : row.unread > 0 ? (
                    <Badge count={row.unread} />
                  ) : null}
                </View>
              </View>
            </View>
          </View>
        </Row>
      </Menu>
    </ReanimatedSwipeable>
  );
});

function previewLine(preview: NonNullable<ServerRowType['preview']>): string {
  return `#${preview.channelName} · ${preview.authorName}: ${preview.text}`;
}

const styles = StyleSheet.create({
  rowContainer: {
    paddingHorizontal: space.lg,
  },
  content: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
  },
  details: {
    flex: 1,
    justifyContent: 'center',
    gap: 2,
  },
  topLine: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'baseline',
  },
  name: {
    flex: 1,
    marginRight: space.sm,
  },
  bottomLine: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
  previewContainer: {
    flex: 1,
    marginRight: space.sm,
  },
  rightAccessories: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
  },
  accessory: {
    justifyContent: 'center',
    alignItems: 'center',
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
