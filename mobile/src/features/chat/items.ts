import type { MessageRecord } from '@opencord/core-native';

import { compareIds } from '@/state/types';
import type { EphemeralNote, PendingSend } from '@/state/types';

export type GroupPosition = 'single' | 'first' | 'middle' | 'last';

export interface DaySeparatorItem {
  type: 'day';
  id: string;
  date: Date;
}

export interface NewMessagesDividerItem {
  type: 'divider';
  id: string;
}

export interface MessageBubbleItem {
  type: 'message';
  id: string;
  message: MessageRecord;
  pending?: PendingSend;
  /** A private message's own state; a server channel's comes from `pending`. */
  sendState?: 'sending' | 'failed';
  position: GroupPosition;
  own: boolean;
  showAuthor: boolean;
  showAvatar: boolean;
  mentionsMe: boolean;
}

export interface EphemeralItem {
  type: 'ephemeral';
  id: string;
  note: EphemeralNote;
}

export type ChatItem = DaySeparatorItem | NewMessagesDividerItem | MessageBubbleItem | EphemeralItem;

/** Messages by one author closer than this join one group. */
export const GROUP_WINDOW_MS = 5 * 60 * 1000;

export function dayKey(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth() + 1}-${date.getDate()}`;
}

/** Where the i-th of a group of `count` sits. */
export function groupPosition(index: number, count: number): GroupPosition {
  return count === 1 ? 'single' : index === 0 ? 'first' : index === count - 1 ? 'last' : 'middle';
}

export interface BuildChatItemsOpts {
  selfId: string;
  firstUnreadId?: string;
  /** Replies only this device sees, after everything else. */
  ephemeral?: readonly EphemeralNote[];
  nowMs?: number;
  locale?: string;
}

interface RawEntry {
  message: MessageRecord;
  pending?: PendingSend;
}

export function buildChatItems(
  messages: MessageRecord[],
  pending: PendingSend[] = [],
  opts: BuildChatItemsOpts,
): ChatItem[] {
  const result: ChatItem[] = [];

  // Convert pending sends into entries following loaded messages
  const rawList: RawEntry[] = [
    ...messages.map((message) => ({ message })),
    ...pending.map((send) => ({
      message: {
        id: `pending-${send.nonce}`,
        channelId: send.channelId,
        authorId: opts.selfId,
        content: send.content,
        createdAtMs: send.createdAtMs,
        attachments: [],
        editedAtMs: undefined,
        nonce: send.nonce,
      },
      pending: send,
    })),
  ];

  let currentDay: string | null = null;
  let dividerPlaced = false;
  let currentGroup: RawEntry[] = [];
  let previousEntry: RawEntry | null = null;

  const flushGroup = () => {
    if (currentGroup.length === 0) return;
    const count = currentGroup.length;
    for (let i = 0; i < count; i++) {
      const entry = currentGroup[i];
      if (!entry) continue;
      const own = entry.message.authorId === opts.selfId;
      const position = groupPosition(i, count);
      const showAuthor = (position === 'first' || position === 'single') && !own;
      const showAvatar = (position === 'last' || position === 'single') && !own;
      const mentionsMe = entry.message.content.includes(`<@${opts.selfId}>`);

      result.push({
        type: 'message',
        id: entry.message.id,
        message: entry.message,
        pending: entry.pending,
        position,
        own,
        showAuthor,
        showAvatar,
        mentionsMe,
      });
    }
    currentGroup = [];
    previousEntry = null;
  };

  for (const entry of rawList) {
    const date = new Date(entry.message.createdAtMs);
    const day = dayKey(date);

    if (day !== currentDay) {
      flushGroup();
      result.push({
        type: 'day',
        id: `day-${day}`,
        date,
      });
      currentDay = day;
    }

    if (
      !dividerPlaced &&
      opts.firstUnreadId !== undefined &&
      !entry.pending &&
      compareIds(entry.message.id, opts.firstUnreadId) >= 0
    ) {
      flushGroup();
      result.push({
        type: 'divider',
        id: `divider-${opts.firstUnreadId}`,
      });
      dividerPlaced = true;
    }

    const last = previousEntry;
    const continues =
      last !== null &&
      last.message.authorId === entry.message.authorId &&
      entry.message.createdAtMs - last.message.createdAtMs < GROUP_WINDOW_MS;

    if (!continues) {
      flushGroup();
    }

    currentGroup.push(entry);
    previousEntry = entry;
  }

  flushGroup();
  (opts.ephemeral ?? []).forEach((note, index) => {
    // By nonce: a reply takes its thinking line's place in the list.
    result.push({ type: 'ephemeral', id: `ephemeral-${note.nonce || `note-${index}`}`, note });
  });
  return result;
}
