import { memo, useRef } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import ReanimatedSwipeable, { type SwipeableMethods } from 'react-native-gesture-handler/ReanimatedSwipeable';
import type { PrivateContactRecord } from '@opencord/core-native';

import { report } from '@/core';
import { formatTime } from '@/features/servers/formatTime';
import { blockPeer, hidePrivate, markPrivateRead, setPrivateMuted } from '@/state/actions/private';
import { Avatar, Badge, Icon, Menu, type MenuAction, type MenuHandle, Row, Text, confirm, space, useTheme } from '@/ui';

import { privateName } from './privateAdapter';

export const ROW_HEIGHT = 72;
export const ICON_SIZE = 52;

let openRow: SwipeableMethods | null = null;

export interface ChatRowProps {
  contact: PrivateContactRecord;
  nowMs: number;
  locale: string;
  onPress: (peer: string) => void;
}

export const ChatRow = memo(function ChatRow({ contact, nowMs, locale, onPress }: ChatRowProps) {
  const { colors } = useTheme();
  const swipeableRef = useRef<SwipeableMethods>(null);
  const menuRef = useRef<MenuHandle>(null);
  const name = privateName(contact, contact.peer);

  const markRead = () => report(markPrivateRead(contact.peer), 'mark chat read');
  const hideChat = () => report(hidePrivate(contact.peer), 'close chat');
  const toggleMute = () => report(setPrivateMuted(contact.peer, !contact.muted), 'toggle mute');

  const confirmBlock = () => {
    confirm({
      title: `Block ${name}?`,
      message: "They won't be able to message you or see your presence.",
      confirmTitle: 'Block',
      destructive: true,
      onConfirm: () => report(blockPeer(contact.peer), 'block peer'),
    });
  };

  const menuActions: MenuAction[] = [
    {
      id: 'mute',
      title: contact.muted ? 'Unmute notifications' : 'Mute notifications',
      icon: contact.muted ? 'bell' : 'bellSlash',
    },
    { id: 'close', title: 'Close conversation', icon: 'close' },
    { id: 'block', title: 'Block', icon: 'block', destructive: true },
  ];

  const handleAction = (id: string) => {
    switch (id) {
      case 'mute':
        toggleMute();
        break;
      case 'close':
        hideChat();
        break;
      case 'block':
        confirmBlock();
        break;
    }
  };

  const renderLeftActions = () => (
    <Touchable
      testID={`read-chat-${contact.peer}`}
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
      testID={`close-chat-${contact.peer}`}
      style={[styles.action, { backgroundColor: colors.recording }]}
      onPress={() => {
        swipeableRef.current?.close();
        hideChat();
      }}
    >
      <View accessible accessibilityRole="button" accessibilityLabel="Close" style={styles.actionContent}>
        <Icon name="close" color="onAccent" size={24} />
        <Text variant="caption" color="onAccent" style={styles.actionText}>
          Close
        </Text>
      </View>
    </Touchable>
  );

  const timeText = contact.lastActivityMs ? formatTime(contact.lastActivityMs, nowMs, locale) : null;
  const preview = contact.preview && contact.preview.length > 0 ? contact.preview : 'No messages yet';

  return (
    <View testID={`chat-row-${contact.peer}`}>
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
          testID={`chat-menu-${contact.peer}`}
          trigger="longPress"
          background="sidebar"
          actions={menuActions}
          onAction={handleAction}
        >
          <Row
            height={ROW_HEIGHT}
            accessibilityLabel={`${name}, ${preview}${contact.unread > 0 ? `, ${contact.unread} unread` : ''}${contact.muted ? ', muted' : ''}`}
            onPress={() => onPress(contact.peer)}
            onLongPress={() => menuRef.current?.open()}
            style={[styles.rowContainer, { backgroundColor: colors.sidebar }]}
          >
            <View style={styles.content}>
              <Avatar name={name} size={ICON_SIZE} />
              <View style={styles.details}>
                <View style={styles.topLine}>
                  <Text variant="headline" numberOfLines={1} style={styles.name}>
                    {name}
                  </Text>
                  {timeText && (
                    <Text variant="footnote" color="textMuted" tabular>
                      {timeText}
                    </Text>
                  )}
                </View>
                <View style={styles.bottomLine}>
                  <Text variant="subhead" color="textSecondary" numberOfLines={1} style={styles.preview}>
                    {preview}
                  </Text>
                  <View style={styles.badges}>
                    {contact.muted && (
                      <View style={styles.bellIcon}>
                        <Icon name="bellSlash" size={14} color="textMuted" />
                      </View>
                    )}
                    {contact.unread > 0 && <Badge count={contact.unread} />}
                  </View>
                </View>
              </View>
            </View>
          </Row>
        </Menu>
      </ReanimatedSwipeable>
    </View>
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
    alignItems: 'center',
    justifyContent: 'center',
  },
  actionText: {
    marginTop: space.xs,
    fontWeight: '600',
  },
  content: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
  },
  details: {
    flex: 1,
    marginLeft: space.md,
    justifyContent: 'center',
  },
  topLine: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  name: {
    flex: 1,
    marginRight: space.sm,
  },
  bottomLine: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginTop: space.xs,
  },
  preview: {
    flex: 1,
    marginRight: space.sm,
  },
  badges: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
  },
  bellIcon: {
    marginRight: space.xs,
  },
});
