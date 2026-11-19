import { FlashList, type FlashListRef, type ListRenderItem } from '@shopify/flash-list';
import { forwardRef, memo, useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from 'react';
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
import { useNow } from '@/features/music/useNow';
import { useChannelActivity, useServer, useTypingNames } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { channelKey, compareIds } from '@/state/types';
import { useTheme } from '@/ui';

import { DaySeparator } from './DaySeparator';
import { useChatDock, useMessagesLift } from './dock';
import { buildChatItems, type ChatItem } from './items';
import { JumpToLatest } from './JumpToLatest';
import { EphemeralNote } from './EphemeralNote';
import { MessageBubble } from './MessageBubble';
import { NewMessagesDivider } from './NewMessagesDivider';
import { TypingLine } from './TypingLine';
import { useChannelLoader } from './useChannelLoader';

/** How long a keyboard takes to open, after which the list keeps to its end again. */
const KEYBOARD_SETTLE_MS = 350;
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
    const lift = useMessagesLift();
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

/** A server channel's messages: the chat list over the channel's store. */
export const MessageList = memo(function MessageList({
  serverKey,
  channelId,
  onSelectChannel,
}: MessageListProps) {
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

  const now = useNow(true);
  const typingNames = useTypingNames(serverKey, channelId, now);

  const ephemeral = useAppStore((state) => state.servers[serverKey]?.ephemeral?.[channelId]);

  const items = useMemo(() => {
    return buildChatItems(channelMsgs?.items ?? [], channelMsgs?.pending ?? [], {
      selfId,
      firstUnreadId,
      ephemeral,
    });
  }, [channelMsgs?.items, channelMsgs?.pending, selfId, firstUnreadId, ephemeral]);

  const onAtBottom = useCallback(
    (isAtBottom: boolean) => markReadIfAtBottom(serverKey, channelId, isAtBottom),
    [serverKey, channelId],
  );

  const renderItem = useCallback(
    (item: ChatItem, maxBubbleWidth: number) => {
      switch (item.type) {
        case 'day':
          return <DaySeparator date={item.date} />;
        case 'divider':
          return <NewMessagesDivider />;
        case 'ephemeral':
          return <EphemeralNote serverKey={serverKey} channelId={channelId} note={item.note} />;
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
    [serverKey, channelId, onSelectChannel],
  );

  return (
    <ChatList
      conversationKey={key}
      items={items}
      renderItem={renderItem}
      loadOlder={loadOlder}
      typingNames={typingNames}
      unreadCount={activity?.unread ?? 0}
      awayFromEnd={!(openChannelState?.atBottom ?? true)}
      onAtBottom={onAtBottom}
    />
  );
});

/** A row of a chat list: what FlashList recycles by. */
export interface ChatListItem {
  id: string;
  type: string;
}

export interface ChatListProps<T extends ChatListItem> {
  /** Another conversation: the list keeps to its end again. */
  conversationKey: string;
  items: T[];
  /** Draws a row; bubbles are at most `maxBubbleWidth` wide. */
  renderItem: (item: T, maxBubbleWidth: number) => ReactElement | null;
  /** The list neared its top. */
  loadOlder: () => void;
  typingNames: string[];
  /** On Jump to latest. */
  unreadCount: number;
  /** The store says the reader is away from the end, besides the scroll itself. */
  awayFromEnd?: boolean;
  /** Each scroll, and Jump to latest: whether the reader is at the end. */
  onAtBottom: (atBottom: boolean) => void;
}

/**
 * The chat's list, whatever the messages are: a server channel's
 * (MessageList) or a private chat's. It keeps to the end as messages
 * arrive, lifts with the keyboard and the dock, and loads older ones near
 * its top without jumping (FlashList keeps the visible content in place).
 */
export function ChatList<T extends ChatListItem>({
  conversationKey,
  items,
  renderItem: drawItem,
  loadOlder,
  typingNames,
  unreadCount,
  awayFromEnd = false,
  onAtBottom,
}: ChatListProps<T>) {
  const { colors } = useTheme();
  const dock = useChatDock();
  const listRef = useRef<FlashListRef<T>>(null);
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
  }, [conversationKey]);
  const onContentSizeChange = useCallback(() => {
    if (pinnedToEnd.current) listRef.current?.scrollToEnd({ animated: false });
  }, []);
  const onScrollBeginDrag = useCallback(() => {
    pinnedToEnd.current = false;
  }, []);
  // The dock growing (the editing card, files to send) would cover the
  // newest messages: at the end, the list follows it up.
  const atEnd = useRef(true);
  useEffect(() => {
    if (!atEnd.current) return undefined;
    listRef.current?.scrollToEnd({ animated: true });
    // Again once a keyboard opening with it (picked files bring it up) has
    // settled: its own scrolling would leave the newest message covered.
    const settle = setTimeout(() => listRef.current?.scrollToEnd({ animated: true }), KEYBOARD_SETTLE_MS);
    return () => clearTimeout(settle);
  }, [dock.height]);

  // While recording, the mic's lock hint takes that corner.
  const recording = useAppStore((state) => state.voiceRecording !== null);
  const showJump = (awayFromEnd || isScrolledUp) && !recording;

  const handleScroll = useCallback(
    (event: NativeSyntheticEvent<NativeScrollEvent>) => {
      const { layoutMeasurement, contentOffset, contentSize } = event.nativeEvent;
      const paddingToBottom = 40;
      const isAtBottom =
        layoutMeasurement.height + contentOffset.y >= contentSize.height - paddingToBottom;
      const scrolledUp =
        contentSize.height - (layoutMeasurement.height + contentOffset.y) > layoutMeasurement.height;

      atEnd.current = isAtBottom;
      setIsScrolledUp(scrolledUp);
      onAtBottom(isAtBottom);
    },
    [onAtBottom],
  );

  const handleJumpToLatest = useCallback(() => {
    listRef.current?.scrollToEnd({ animated: true });
    setIsScrolledUp(false);
    onAtBottom(true);
  }, [onAtBottom]);

  const renderItem: ListRenderItem<T> = useCallback(
    ({ item }) => drawItem(item, maxBubbleWidth),
    [drawItem, maxBubbleWidth],
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
}

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
