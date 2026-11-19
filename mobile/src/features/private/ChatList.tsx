import { memo, useCallback, useEffect, useState } from 'react';
import { FlatList, StyleSheet, View, useWindowDimensions } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { router } from 'expo-router';
import type { PrivateContactRecord } from '@opencord/core-native';

import { report } from '@/core';
import { deviceLocale } from '@/features/servers/formatTime';
import { useMinuteClock } from '@/features/servers/useMinuteClock';
import { loadPrivate } from '@/state/actions/private';
import { usePrivateContacts } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { Badge, EmptyState, Icon, PaneHeader, Text, space, useTheme } from '@/ui';

import { ChatRow } from './ChatRow';
import { PrivateChat } from './PrivateChat';

export const TABLET_MIN_WIDTH = 768;
export const TABLET_LIST_WIDTH = 320;

export interface ChatListProps {
  /** Given, called on selecting a chat instead of navigating. */
  onSelectChat?: (peer: string) => void;
  /** Active chat on tablet. */
  activePeer?: string;
}

export const ChatList = memo(function ChatList({ onSelectChat, activePeer }: ChatListProps) {
  const { colors } = useTheme();
  const { width } = useWindowDimensions();
  const isTablet = width >= TABLET_MIN_WIDTH;

  const contactsLoaded = useAppStore((state) => state.private.loaded);
  useEffect(() => {
    if (!contactsLoaded) report(loadPrivate(), 'load private chats');
  }, [contactsLoaded]);

  const chats = usePrivateContacts('chats');
  const requests = usePrivateContacts('requests');
  const nowMs = useMinuteClock();
  const locale = deviceLocale();

  const [selectedPeer, setSelectedPeer] = useState<string | null>(activePeer ?? (chats[0]?.peer ?? null));

  const handleOpenChat = useCallback(
    (peer: string) => {
      if (onSelectChat) {
        onSelectChat(peer);
      } else if (isTablet) {
        setSelectedPeer(peer);
      } else {
        router.push(`/(tabs)/messages/${peer}` as never);
      }
    },
    [onSelectChat, isTablet],
  );

  const handleOpenRequests = useCallback(() => {
    router.push('/(tabs)/messages/requests');
  }, []);

  const handleOpenFriends = useCallback(() => {
    router.push('/(tabs)/messages/friends');
  }, []);

  const renderHeader = () => (
    <View>
      {/* Slot for own device notice (Task 8) */}
      <View testID="own-device-notice-slot" />

      {requests.length > 0 && (
        <Touchable
          testID="message-requests-row"
          onPress={handleOpenRequests}
          style={[styles.requestsRow, { backgroundColor: colors.sidebar, borderBottomColor: colors.border }]}
        >
          <View style={styles.requestsContent}>
            <View style={[styles.requestsIcon, { backgroundColor: colors.surface }]}>
              <Icon name="chat" size={20} color="accent" />
            </View>
            <View style={styles.requestsDetails}>
              <Text variant="headline">
                Message requests
              </Text>
              <Text variant="footnote" color="textSecondary">
                {requests.length} {requests.length === 1 ? 'request' : 'requests'} waiting
              </Text>
            </View>
            <Badge count={requests.length} />
          </View>
        </Touchable>
      )}
    </View>
  );

  const renderItem = ({ item }: { item: PrivateContactRecord }) => (
    <ChatRow contact={item} nowMs={nowMs} locale={locale} onPress={handleOpenChat} />
  );

  const listContent = (
    <View style={styles.listContainer}>
      <PaneHeader
        title="Messages"
        right={[
          {
            icon: 'people',
            label: 'Friends',
            onPress: handleOpenFriends,
          },
        ]}
      />
      {chats.length === 0 && requests.length === 0 ? (
        <EmptyState title="No conversations yet" body="Start a private chat with a friend." />
      ) : (
        <FlatList
          data={chats}
          keyExtractor={(item) => item.peer}
          renderItem={renderItem}
          ListHeaderComponent={renderHeader}
          contentContainerStyle={styles.listContent}
        />
      )}
    </View>
  );

  if (!isTablet) {
    return listContent;
  }

  const currentPeer = onSelectChat ? activePeer : selectedPeer;

  return (
    <View style={styles.tabletContainer}>
      <View style={[styles.tabletSidebar, { borderRightColor: colors.border, backgroundColor: colors.sidebar }]}>
        {listContent}
      </View>
      <View style={[styles.tabletContent, { backgroundColor: colors.chat }]}>
        {currentPeer ? (
          <PrivateChat key={currentPeer} peer={currentPeer} onLeave={() => setSelectedPeer(null)} />
        ) : (
          <EmptyState title="No conversation selected" body="Choose a conversation from the list." />
        )}
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  listContainer: {
    flex: 1,
  },
  listContent: {
    flexGrow: 1,
  },
  requestsRow: {
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  requestsContent: {
    flexDirection: 'row',
    alignItems: 'center',
  },
  requestsIcon: {
    width: 40,
    height: 40,
    borderRadius: 20,
    alignItems: 'center',
    justifyContent: 'center',
  },
  requestsDetails: {
    flex: 1,
    marginLeft: space.md,
  },
  tabletContainer: {
    flex: 1,
    flexDirection: 'row',
  },
  tabletSidebar: {
    width: TABLET_LIST_WIDTH,
    borderRightWidth: StyleSheet.hairlineWidth,
  },
  tabletContent: {
    flex: 1,
  },
});
