import { FlashList, type FlashListRef, type ListRenderItem } from '@shopify/flash-list';
import { forwardRef, memo, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  Platform,
  StyleSheet,
  View,
  useWindowDimensions,
  type LayoutChangeEvent,
  type NativeScrollEvent,
  type NativeSyntheticEvent,
  type ScrollViewProps,
} from 'react-native';
import {
  KeyboardChatScrollView,
  type KeyboardChatScrollViewProps,
  type KeyboardChatScrollViewRef,
} from 'react-native-keyboard-controller';

import { markReadIfAtBottom } from '@/state/actions/chat';
import { useChannelActivity, useServer, useTypingNames } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { channelKey, compareIds } from '@/state/types';
import { useTheme } from '@/ui';

import { DaySeparator } from './DaySeparator';
import { useChatDock, useDockLift } from './dock';
import { buildChatItems, type ChatItem } from './items';
import { JumpToLatest } from './JumpToLatest';
import { MessageBubble } from './MessageBubble';
import { NewMessagesDivider } from './NewMessagesDivider';
import { TypingLine } from './TypingLine';
import { useChannelLoader } from './useChannelLoader';

/** A bubble is at most this share of the pane (spec §8.5)… */
const BUBBLE_SHARE = 0.78;
/** …and no wider than this on a tablet, where 78 % makes lines too long to read. */
const BUBBLE_MAX = 480;

/**
 * The list's scroll view: the keyboard lifts the messages with the dock
 * (keyboard-controller's chat view) by the same amount, and the dock's
 * height is left free under the last message, since the dock floats over it.
 */
const ChatScrollView = forwardRef<KeyboardChatScrollViewRef, ScrollViewProps & KeyboardChatScrollViewProps>(
  function ChatScrollView(props, ref) {
    const lift = useDockLift();
    const { padding } = useChatDock();
    return (
      <KeyboardChatScrollView
        ref={ref}
        automaticallyAdjustContentInsets={false}
        contentInsetAdjustmentBehavior="never"
        offset={lift}
        extraContentPadding={padding}
        // Scrolling down drags the keyboard closed with the finger, as in
        // WhatsApp and Telegram (Android has no interactive mode: a drag closes it).
        keyboardDismissMode={Platform.OS === 'ios' ? 'interactive' : 'on-drag'}
        {...props}
      />
    );
  },
);

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
  const dock = useChatDock();
  const listRef = useRef<FlashListRef<ChatItem>>(null);
  const [isScrolledUp, setIsScrolledUp] = useState(false);
  // The window until the pane is measured (a phone's pane is the window).
  const window = useWindowDimensions();
  const [paneWidth, setPaneWidth] = useState(window.width);
  const onLayout = useCallback((event: LayoutChangeEvent) => {
    setPaneWidth(Math.round(event.nativeEvent.layout.width));
  }, []);
  const maxBubbleWidth = Math.min(BUBBLE_MAX, Math.round(paneWidth * BUBBLE_SHARE));

  // A channel opens on its newest message. Messages arrive after the list
  // mounts and settle their heights (pictures, wrapping) afterwards, so the
  // list keeps to its end as its content grows until the user first drags.
  const pinnedToEnd = useRef(true);
  useEffect(() => {
    pinnedToEnd.current = true;
  }, [serverKey, channelId]);
  const onContentSizeChange = useCallback(() => {
    if (pinnedToEnd.current) listRef.current?.scrollToEnd({ animated: false });
  }, []);
  const onScrollBeginDrag = useCallback(() => {
    pinnedToEnd.current = false;
  }, []);

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
              maxWidth={maxBubbleWidth}
              onSelectChannel={onSelectChannel}
            />
          );
      }
    },
    [serverKey, onSelectChannel, maxBubbleWidth],
  );

  return (
    <View style={[styles.container, { backgroundColor: colors.chat }]} onLayout={onLayout}>
      <FlashList
        ref={listRef}
        renderScrollComponent={ChatScrollView}
        extraData={maxBubbleWidth}
        data={items}
        renderItem={renderItem}
        getItemType={(item) => item.type}
        keyExtractor={(item) => item.id}
        onScroll={handleScroll}
        onContentSizeChange={onContentSizeChange}
        onScrollBeginDrag={onScrollBeginDrag}
        onStartReached={loadOlder}
        onStartReachedThreshold={0.5}
        maintainVisibleContentPosition={{
          startRenderingFromBottom: true,
          autoscrollToBottomThreshold: 0.2,
        }}
        contentContainerStyle={styles.contentContainer}
      />

      {showJump && (
        <JumpToLatest count={unreadCount} onPress={handleJumpToLatest} bottom={dock.height} />
      )}

      {/* Just above the dock, over the messages. */}
      <View style={[styles.typing, { bottom: dock.height }]} pointerEvents="none">
        <TypingLine names={typingNames} />
      </View>
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
  typing: {
    position: 'absolute',
    left: 0,
    right: 0,
  },
});
