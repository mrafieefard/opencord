// Private messages as the rows the chat list draws (design §6.3), as the
// desktop's private_message_adapter.dart does: text becomes a bubble of the
// server chat's kind; a relay change, a move and a key change become rows
// of their own.
import {
  PrivateMessageKind,
  PrivateSendState,
  type MessageRecord,
  type PrivateContactRecord,
  type PrivateMessageRecord,
} from '@opencord/core-native';

import {
  dayKey,
  GROUP_WINDOW_MS,
  groupPosition,
  type DaySeparatorItem,
  type MessageBubbleItem,
} from '@/features/chat/items';
import type { IconName } from '@/ui';

/** Where the chat started or moved: a line with the server's name. */
export interface RelayItem {
  type: 'relay';
  id: string;
  serverName: string;
  serverKey?: string;
}

/** They moved the chat to another server: Move too. */
export interface MovedItem {
  type: 'moved';
  id: string;
  messageId: string;
  serverName: string;
  serverKey?: string;
}

/** A quiet line about the chat's keys, or a message that couldn't be read. */
export interface NoteItem {
  type: 'note';
  id: string;
  icon: IconName;
  text: string;
}

export type PrivateChatItem = DaySeparatorItem | MessageBubbleItem | RelayItem | MovedItem | NoteItem;

/** The author id the bubbles take for this device's own messages. */
export const SELF_AUTHOR = 'self';

/** The name a contact goes by: the one last seen, else their key (the desktop's last resort). */
export function privateName(contact: PrivateContactRecord | undefined, peer: string): string {
  return contact?.name ? contact.name : peer;
}

/** A private text as the server chat's message record. */
export function asChatMessage(message: PrivateMessageRecord): MessageRecord {
  return {
    id: message.id,
    channelId: '',
    authorId: message.outgoing ? SELF_AUTHOR : message.peer,
    content: message.content,
    createdAtMs: message.sentAtMs,
    editedAtMs: message.editedAtMs,
    nonce: undefined,
    attachments: [],
  };
}

function sendStateOf(state: PrivateSendState): MessageBubbleItem['sendState'] {
  switch (state) {
    case PrivateSendState.Sending:
      return 'sending';
    case PrivateSendState.Failed:
      return 'failed';
    default:
      return undefined;
  }
}

/** The row a message other than text becomes; the desktop's words. */
function otherRow(message: PrivateMessageRecord, name: string): RelayItem | MovedItem | NoteItem {
  switch (message.kind) {
    case PrivateMessageKind.Divider:
      return { type: 'relay', id: message.id, serverName: message.content, serverKey: message.serverKey };
    case PrivateMessageKind.Moved:
      return { type: 'moved', id: message.id, messageId: message.id, serverName: message.content, serverKey: message.serverKey };
    case PrivateMessageKind.KeysChanged:
      return { type: 'note', id: message.id, icon: 'key', text: message.outgoing ? 'Your keys changed.' : `${name}'s keys changed.` };
    case PrivateMessageKind.NewDevice:
      return { type: 'note', id: message.id, icon: 'lock', text: `${name} signed in on a new device.` };
    case PrivateMessageKind.UnacceptedKeys:
      return { type: 'note', id: message.id, icon: 'warning', text: "A message from keys you haven't accepted." };
    default:
      return { type: 'note', id: message.id, icon: 'lock', text: "This message couldn't be decrypted." };
  }
}

/**
 * The rows for a chat's messages, oldest first: a day line where the day
 * changes, and bubbles grouped as the server chat groups them (one side,
 * within five minutes), a row other than text ending a group. No names or
 * avatars: the chat is with one person.
 */
export function buildPrivateItems(messages: readonly PrivateMessageRecord[], name: string): PrivateChatItem[] {
  const result: PrivateChatItem[] = [];
  let group: PrivateMessageRecord[] = [];
  let currentDay: string | null = null;

  const flush = () => {
    group.forEach((message, index) => {
      result.push({
        type: 'message',
        id: message.id,
        message: asChatMessage(message),
        sendState: sendStateOf(message.state),
        position: groupPosition(index, group.length),
        own: message.outgoing,
        showAuthor: false,
        showAvatar: false,
        mentionsMe: false,
      });
    });
    group = [];
  };

  for (const message of messages) {
    const date = new Date(message.sentAtMs);
    const day = dayKey(date);
    if (day !== currentDay) {
      flush();
      result.push({ type: 'day', id: `day-${day}`, date });
      currentDay = day;
    }
    if (message.kind !== PrivateMessageKind.Text) {
      flush();
      result.push(otherRow(message, name));
      continue;
    }
    const last = group[group.length - 1];
    const continues =
      last !== undefined && last.outgoing === message.outgoing && message.sentAtMs - last.sentAtMs < GROUP_WINDOW_MS;
    if (!continues) flush();
    group.push(message);
  }
  flush();
  return result;
}
