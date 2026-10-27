import { FlashList, type FlashListRef, type ListRenderItem } from '@shopify/flash-list';
import { memo, useCallback, useMemo, useRef, useState } from 'react';
import { StyleSheet, View, type NativeScrollEvent, type NativeSyntheticEvent } from 'react-native';

import { markReadIfAtBottom } from '@/state/actions/chat';
import { useChannelActivity, useServer, useTypingNames } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { channelKey, compareIds } from '@/state/types';
import { useTheme } from '@/ui';

import { DaySeparator } from './DaySeparator';
import { buildChatItems, type ChatItem } from './items';
import { JumpToLatest } from './JumpToLatest';
import { MessageBubble } from './MessageBubble';
import { NewMessagesDivider } from './NewMessagesDivider';
import { TypingLine } from './TypingLine';
import { useChannelLoader } from './useChannelLoader';

export interface MessageListProps {
  serverKey: string;
  channelId: string;
  onSelectChannel?: (channelId: string) => void;
}

export const MessageList = memo(function MessageList({
  serverKey,
  channelId,
  onSelectChannel,
}: MessageListProps) {
  const { colors } = useTheme();
  const listRef = useRef<FlashListRef<ChatItem>>(null);
  const [isScrolledUp, setIsScrolledUp] = useState(false);

  const { loadOlder } = useChannelLoader(serverKey, channelId);

  const key = channelKey(serverKey, channelId);
  const channelMsgs = useAppStore((state) => state.messages[key]);
  const openChannelState = useAppStore((state) => state.openChannel);
  const server = useServer(serverKey);
  const activity = useChannelActivity(serverKey, channelId);

  const selfId = server?.snapshot?.selfUser.id ?? '';
  const firstUnreadId = useMemo(() => {
    if (!activity || activity.unread === 0 || !activity.lastReadId) return undefined;
    const msgs = channelMsgs?.items ?? [];
    const firstUnread = msgs.find(
      (m) => m.authorId !== selfId && compareIds(m.id, activity.lastReadId) > 0,
    );
    return firstUnread?.id;
  }, [activity, channelMsgs?.items, selfId]);

  const typingNames = useTypingNames(serverKey, channelId, Date.now());

  const items = useMemo(() => {
    return buildChatItems(channelMsgs?.items ?? [], channelMsgs?.pending ?? [], {
      selfId,
      firstUnreadId,
    });
  }, [channelMsgs?.items, channelMsgs?.pending, selfId, firstUnreadId]);

  const atBottom = openChannelState?.atBottom ?? true;
  const showJump = !atBottom || isScrolledUp;
  const unreadCount = activity?.unread ?? 0;

  const handleScroll = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      const { layoutMeasurement, contentOffset, contentSize } = event.nativeEvent;
      const paddingToBottom = 40;
      const isAtBottom =
        layoutMeasurement.height + contentOffset.y >= contentSize.height - paddingToBottom;
      const scrolledUp =
        contentSize.height - (layoutMeasurement.height + contentOffset.y) > layoutMeasurement.height;

      setIsScrolledUp(scrolledUp);
      markReadIfAtBottom(serverKey, channelId, isAtBottom);
    },
    [serverKey, channelId],
  );

  const handleJumpToLatest = useCallback(() => {
    listRef.current?.scrollToEnd({ animated: true });
    setIsScrolledUp(false);
    markReadIfAtBottom(serverKey, channelId, true);
  }, [serverKey, channelId]);

  const renderItem: ListRenderItem<ChatItem> = useCallback(
    ({ item }) => {
      switch (item.type) {
        case 'day':
          return <DaySeparator date={item.date} />;
        case 'divider':
          return <NewMessagesDivider />;
        case 'message':
          return (
            <MessageBubble
              item={item}
              serverKey={serverKey}
              onSelectChannel={onSelectChannel}
            />
          );
      }
    },
    [serverKey, onSelectChannel],
  );

  return (
    <View style={[styles.container, { backgroundColor: colors.chat }]}>
      <FlashList
        ref={listRef}
        data={items}
        renderItem={renderItem}
        getItemType={(item) => item.type}
        keyExtractor={(item) => item.id}
        onScroll={handleScroll}
        onStartReached={loadOlder}
        onStartReachedThreshold={0.5}
        maintainVisibleContentPosition={{
          startRenderingFromBottom: true,
          autoscrollToBottomThreshold: 0.2,
        }}
        contentContainerStyle={styles.contentContainer}
      />

      {showJump && (
        <JumpToLatest count={unreadCount} onPress={handleJumpToLatest} />
      )}

      <TypingLine names={typingNames} />
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  contentContainer: {
    paddingVertical: 8,
  },
});
