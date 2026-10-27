// Each row (72 pt tall), like a WhatsApp chat (spec §8.2):
// Left: server icon (52 pt)
// Title line: Name, last activity time
// Second line: preview / connection status
// Right of preview: unread / mentions badge, voice glyph if in voice
// Swipes: leading Read, trailing Leave
// Long press: Menu (Mark as read, Retry now, Copy address, Leave server)

import * as Clipboard from 'expo-clipboard';
import { useRef, useState } from 'react';
import { Alert, Pressable, StyleSheet, View } from 'react-native';
import ReanimatedSwipeable, { type SwipeableMethods } from 'react-native-gesture-handler/ReanimatedSwipeable';
import { ConnectionRecord_Tags } from '@opencord/core-native';

import { leaveServer, markServerRead, retryServer } from '@/state/actions/servers';
import type { ServerRow as ServerRowType } from '@/state/types';
import { Badge, Icon, type IconName, Menu, type MenuAction, Row, ServerIcon, Text, space, useTheme } from '@/ui';
import { connectionText } from './connectionText';
import { formatTime } from './formatTime';
import { useImageFile } from './useImageFile';

export function ServerRow({
  row,
  onPress,
}: {
  row: ServerRowType;
  onPress: () => void;
}) {
  const { colors } = useTheme();
  const swipeableRef = useRef<SwipeableMethods>(null);
  const iconFile = useImageFile(row.key, row.iconHash);
  const conn = connectionText(row.connection);
  const connected = row.connection.tag === ConnectionRecord_Tags.Connected;

  const confirmLeave = () => {
    Alert.alert(`Leave ${row.name}?`, 'You can come back with an invite.', [
      { text: 'Cancel', style: 'cancel' },
      {
        text: 'Leave',
        style: 'destructive',
        onPress: () => leaveServer(row.key),
      },
    ]);
  };

  const menuActions: MenuAction[] = [
    { id: 'read', title: 'Mark as read', icon: 'check' },
    ...(connected ? [] : [{ id: 'retry', title: 'Retry now', icon: 'retry' as IconName }]),
    { id: 'copy-address', title: 'Copy address', icon: 'copy' as IconName },
    { id: 'leave', title: 'Leave server', icon: 'leave' as IconName, destructive: true },
  ];

  const handleAction = (id: string) => {
    switch (id) {
      case 'read':
        markServerRead(row.key);
        break;
      case 'retry':
        void retryServer(row.key);
        break;
      case 'copy-address':
        void Clipboard.setStringAsync(row.address);
        break;
      case 'leave':
        confirmLeave();
        break;
    }
  };

  const renderLeftActions = () => (
    <Pressable
      testID={`read-server-${row.key}`}
      style={[styles.action, { backgroundColor: colors.accent }]}
      onPress={() => {
        swipeableRef.current?.close();
        markServerRead(row.key);
      }}
    >
      <Icon name="check" color="onAccent" size={24} />
      <Text variant="caption" color="onAccent" style={styles.actionText}>
        Read
      </Text>
    </Pressable>
  );

  const renderRightActions = () => (
    <Pressable
      testID={`leave-server-${row.key}`}
      style={[styles.action, { backgroundColor: colors.recording }]}
      onPress={() => {
        confirmLeave();
      }}
    >
      <Icon name="leave" color="text" size={24} />
      <Text variant="caption" color="text" style={styles.actionText}>
        Leave
      </Text>
    </Pressable>
  );

  const [now] = useState(() => Date.now());
  const locale = Intl.DateTimeFormat().resolvedOptions().locale || 'en-US';
  const timeText = row.lastAtMs ? formatTime(row.lastAtMs, now, locale) : null;

  return (
    <ReanimatedSwipeable
      ref={swipeableRef}
      overshootFriction={8}
      renderLeftActions={renderLeftActions}
      renderRightActions={renderRightActions}
    >
      <Menu
        testID={`server-menu-${row.key}`}
        trigger="longPress"
        actions={menuActions}
        onAction={handleAction}
      >
        <Row
          height={72}
          accessibilityLabel={row.name}
          onPress={onPress}
          style={styles.rowContainer}
        >
          <View style={styles.content}>
            <ServerIcon name={row.name} file={iconFile} size={52} />
            <View style={styles.details}>
              <View style={styles.topLine}>
                <Text variant="headline" numberOfLines={1} style={styles.name}>
                  {row.name}
                </Text>
                {timeText ? (
                  <Text variant="footnote" color="textMuted">
                    {timeText}
                  </Text>
                ) : null}
              </View>
              <View style={styles.bottomLine}>
                <View style={styles.previewContainer}>
                  {conn ? (
                    <Text
                      variant="callout"
                      color={conn.error ? 'recording' : 'textMuted'}
                      numberOfLines={1}
                    >
                      {conn.text}
                    </Text>
                  ) : row.preview ? (
                    <Text variant="callout" color="textMuted" numberOfLines={1}>
                      #{row.preview.channelName} · {row.preview.authorName}: {row.preview.text}
                    </Text>
                  ) : (
                    <Text variant="callout" color="textMuted" numberOfLines={1}>
                      No messages yet
                    </Text>
                  )}
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
    width: 72,
    justifyContent: 'center',
    alignItems: 'center',
  },
  actionText: {
    marginTop: space.xs,
  },
});
