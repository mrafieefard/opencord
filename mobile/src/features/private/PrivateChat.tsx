// A private chat (design §6.3): the server chat's list, bubbles and
// composer over a private conversation, through privateAdapter. Under the
// list sits the composer, or what stands in for it (a request, a block, no
// server to carry the chat).
import {
  ConnectionRecord_Tags,
  ConversationStatus,
  type MessageRecord,
  type SharedServerRecord,
} from '@opencord/core-native';
import * as Clipboard from 'expo-clipboard';
import { useCallback, useEffect, useMemo, useRef, useState, type ReactElement } from 'react';
import { StyleSheet, View } from 'react-native';

import { errorKind, getCore, report } from '@/core';
import { ChatBody } from '@/features/chat/ChatPane';
import { TextComposer } from '@/features/chat/Composer';
import { DaySeparator } from '@/features/chat/DaySeparator';
import { useDockBottom } from '@/features/chat/dock';
import { MessageBubble, type BubbleChat } from '@/features/chat/MessageBubble';
import { ChatList } from '@/features/chat/MessageList';
import { useThrottledTyping } from '@/features/chat/useTypingNotifier';
import {
  deletePrivateMessage,
  editPrivateMessage,
  loadMessages,
  loadOlder as loadOlderPrivate,
  loadPrivate,
  markPrivateRead,
  retryPrivateMessage,
  sendPrivateMessage,
  sendPrivateTyping,
  unblockPeer,
} from '@/state/actions/private';
import { toast } from '@/state/actions/ui';
import { usePrivateMessages, usePrivateTyping } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import { Button, GlassSurface, Icon, Menu, Text, confirm, space, useTheme, type IconName, type MenuAction } from '@/ui';

import { MoveCard } from './MoveCard';
import { buildPrivateItems, privateName, SELF_AUTHOR, type PrivateChatItem } from './privateAdapter';
import { PrivateHeader } from './PrivateHeader';
import { RelayDivider } from './RelayDivider';
import { RequestBar } from './RequestBar';

export interface PrivateChatProps {
  peer: string;
  /** The back button; none beside the list on a tablet. */
  onBack?: () => void;
  /** The chat went away (a request ignored, someone blocked from a request): leave it. */
  onLeave?: () => void;
  /** Opens the encryption sheet (Task 8). */
  onOpenEncryption?: () => void;
}

const NO_TYPING: string[] = [];
const NO_SERVERS: SharedServerRecord[] = [];
const ignoreAtBottom = () => {};

/** The servers this device shares with them, fetched while the chat shows; null until known. */
function useSharedServers(peer: string, relayServerKey: string | undefined): SharedServerRecord[] | null {
  const [servers, setServers] = useState<SharedServerRecord[] | null>(null);
  useEffect(() => {
    let live = true;
    report(
      getCore()
        .privateSharedServers(peer)
        .then((list) => {
          if (live) setServers(list);
        }),
      'load shared servers',
    );
    return () => {
      live = false;
    };
  }, [peer, relayServerKey]);
  return servers;
}

export function PrivateChat({ peer, onBack, onLeave, onOpenEncryption }: PrivateChatProps) {
  const { colors } = useTheme();
  const contact = useAppStore((state) => state.private.contacts[peer]);
  const contactsLoaded = useAppStore((state) => state.private.loaded);
  const messages = usePrivateMessages(peer);
  const typing = usePrivateTyping(peer);
  const name = privateName(contact, peer);
  const relayKey = contact?.relayServerKey;
  const shared = useSharedServers(peer, relayKey);
  const sharedServers = shared ?? NO_SERVERS;
  const relayConnected = useAppStore(
    (state) => relayKey !== undefined && state.servers[relayKey]?.connection.tag === ConnectionRecord_Tags.Connected,
  );

  // Contacts once (nothing may have loaded them yet, opened from a link),
  // and this chat's newest page each time it opens.
  useEffect(() => {
    if (!contactsLoaded) report(loadPrivate(), 'load private chats');
  }, [contactsLoaded]);
  useEffect(() => {
    report(loadMessages(peer), 'load messages');
  }, [peer]);

  // Opening the chat reads it, and so does what arrives while it's open.
  const unread = contact?.unread ?? 0;
  const readFor = useRef<string | null>(null);
  useEffect(() => {
    if (readFor.current === peer && unread === 0) return;
    readFor.current = peer;
    report(markPrivateRead(peer), 'mark read');
  }, [peer, unread]);

  const sendTyping = useCallback(() => {
    sendPrivateTyping(peer).catch((error: unknown) => {
      // Best effort, as a channel's typing is.
      console.warn(`privateTyping failed: ${errorKind(error)}`);
    });
  }, [peer]);
  const onTyping = useThrottledTyping(sendTyping);

  const [editing, setEditing] = useState<{ id: string; text: string } | null>(null);
  const cancelEditing = useCallback(() => setEditing(null), []);

  const bubbleChat = useMemo<BubbleChat>(
    () => ({
      retry: (message) => report(retryPrivateMessage(message.id), 'retry'),
      discard: (message) => report(deletePrivateMessage(peer, message.id), 'delete'),
      menu: (message, bubble) => (
        <PrivateMessageMenu peer={peer} message={message} onEdit={setEditing}>
          {bubble}
        </PrivateMessageMenu>
      ),
    }),
    [peer],
  );

  const items = useMemo(() => buildPrivateItems(messages, name), [messages, name]);

  const renderItem = useCallback(
    (item: PrivateChatItem, maxBubbleWidth: number): ReactElement => {
      switch (item.type) {
        case 'day':
          return <DaySeparator date={item.date} />;
        case 'message':
          // No server: mentions in private text aren't looked up, as on the desktop.
          return <MessageBubble item={item} serverKey="" maxWidth={maxBubbleWidth} chat={bubbleChat} />;
        case 'relay':
          return <RelayDivider serverName={item.serverName} serverKey={item.serverKey} />;
        case 'moved':
          return (
            <MoveCard
              peer={peer}
              name={name}
              messageId={item.messageId}
              serverName={item.serverName}
              serverKey={item.serverKey}
              relayServerKey={relayKey}
              sharedServers={sharedServers}
            />
          );
        case 'note':
          return <KeyNote icon={item.icon} text={item.text} />;
      }
    },
    [bubbleChat, peer, name, relayKey, sharedServers],
  );

  const loadOlder = useCallback(() => report(loadOlderPrivate(peer), 'load older messages'), [peer]);
  const send = useCallback(async (text: string) => {
    await sendPrivateMessage(peer, text);
  }, [peer]);
  const saveEdit = useCallback((id: string, text: string) => editPrivateMessage(peer, id, text), [peer]);

  const hasSharedServer = sharedServers.length > 0 || relayKey !== undefined;
  const anyConnected = relayConnected || sharedServers.some((server) => server.connected);

  let dock: ReactElement | null;
  if (contact === undefined || shared === null) {
    // Not known yet: no notice that might be wrong a moment later.
    dock = null;
  } else if (contact.conversation === ConversationStatus.Request) {
    dock = <RequestBar peer={peer} name={name} onLeave={onLeave} />;
  } else if (contact.blocked) {
    dock = (
      <DockNotice text={`You blocked ${name}.`}>
        <Button title="Unblock" variant="secondary" onPress={() => report(unblockPeer(peer), 'unblock')} />
      </DockNotice>
    );
  } else if (!hasSharedServer) {
    dock = <DockNotice text={`You no longer share a server with ${name}.`} />;
  } else if (!contact.reachable) {
    dock = (
      <DockNotice
        text={`Nothing you write would reach ${name} until you compare the words or use the new keys.`}
      />
    );
  } else if (!anyConnected) {
    dock = <DockNotice text={`You don't share a connected server with ${name} right now.`} />;
  } else {
    dock = (
      <TextComposer
        placeholder={`Message ${name}`}
        send={send}
        onTyping={onTyping}
        editing={editing}
        saveEdit={saveEdit}
        onCancelEditing={cancelEditing}
      />
    );
  }

  return (
    <View style={[styles.pane, { backgroundColor: colors.chat }]}>
      <PrivateHeader peer={peer} contact={contact} name={name} onBack={onBack} onOpenEncryption={onOpenEncryption} />
      <ChatBody
        list={
          <ChatList
            conversationKey={peer}
            items={items}
            renderItem={renderItem}
            loadOlder={loadOlder}
            typingNames={typing ? [name] : NO_TYPING}
            unreadCount={unread}
            onAtBottom={ignoreAtBottom}
          />
        }
        dock={dock}
      />
    </View>
  );
}

interface PrivateMessageMenuProps {
  peer: string;
  message: MessageRecord;
  onEdit: (editing: { id: string; text: string }) => void;
  children: ReactElement;
}

/** A private bubble's long press, the desktop's: Copy text, and on your own Edit and Delete. */
function PrivateMessageMenu({ peer, message, onEdit, children }: PrivateMessageMenuProps) {
  const own = message.authorId === SELF_AUTHOR;
  const actions: MenuAction[] = [{ id: 'copy', title: 'Copy text', icon: 'copy' }];
  if (own) {
    actions.push({ id: 'edit', title: 'Edit', icon: 'edit' });
    actions.push({ id: 'delete', title: 'Delete', icon: 'delete', destructive: true });
  }
  const onAction = (id: string) => {
    switch (id) {
      case 'copy':
        report(
          Clipboard.setStringAsync(message.content).then(() => toast('Copied')),
          'copy',
        );
        break;
      case 'edit':
        onEdit({ id: message.id, text: message.content });
        break;
      case 'delete':
        confirm({
          title: 'Delete message?',
          message: 'Are you sure you want to delete this message?',
          confirmTitle: 'Delete',
          destructive: true,
          onConfirm: () => report(deletePrivateMessage(peer, message.id), 'delete'),
        });
        break;
    }
  };
  return (
    <Menu actions={actions} onAction={onAction} trigger="longPress" fit="content" testID="private-message-menu">
      {children}
    </Menu>
  );
}

/** A quiet line about the chat's keys (design 2026-11-01 §8.2). */
function KeyNote({ icon, text }: { icon: IconName; text: string }) {
  return (
    <View style={styles.note} testID="key-note">
      <Icon name={icon} size={14} color="textMuted" />
      <Text variant="footnote" color="textMuted" style={styles.noteText}>
        {text}
      </Text>
    </View>
  );
}

/** In the composer's place: why there's nothing to write in, and what to do. */
function DockNotice({ text, children }: { text: string; children?: ReactElement }) {
  const bottom = useDockBottom();
  return (
    <View style={[styles.dock, { paddingBottom: bottom }]} pointerEvents="box-none">
      <GlassSurface style={styles.notice} testID="private-notice">
        <Text variant="callout" color="textSecondary" style={styles.noticeText}>
          {text}
        </Text>
        {children}
      </GlassSurface>
    </View>
  );
}

const styles = StyleSheet.create({
  pane: { flex: 1 },
  dock: { paddingHorizontal: space.sm },
  notice: {
    borderRadius: 22,
    minHeight: 44,
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
    gap: space.sm,
    alignItems: 'center',
    justifyContent: 'center',
  },
  noticeText: { textAlign: 'center' },
  note: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: space.xs + 2,
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
  },
  noteText: { flexShrink: 1 },
});
