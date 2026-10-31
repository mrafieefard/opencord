import type { MessageRecord } from '@opencord/core-native';

import type { PendingSend } from '@/state/types';

import { buildChatItems, type MessageBubbleItem } from '../items';

function msg(
  id: string,
  authorId: string,
  content: string,
  createdAtMs: number,
  channelId = '10',
): MessageRecord {
  return {
    id,
    channelId,
    authorId,
    content,
    createdAtMs,
    attachments: [],
    editedAtMs: undefined,
    nonce: undefined,
  };
}

describe('items', () => {
  const SELF = '1000';
  const OTHER = '2000';

  it('messages group by author within 5 minutes', () => {
    const baseTime = 1700000000000;
    const messages = [
      msg('1', OTHER, 'first', baseTime),
      msg('2', OTHER, 'second', baseTime + 2 * 60 * 1000), // 2 min later
      msg('3', OTHER, 'third', baseTime + 4 * 60 * 1000), // 2 min later
      msg('4', OTHER, 'much later', baseTime + 10 * 60 * 1000), // 6 min later -> new group
    ];

    const items = buildChatItems(messages, [], { selfId: SELF });
    const msgItems = items.filter((i): i is MessageBubbleItem => i.type === 'message');

    expect(msgItems).toHaveLength(4);
    expect(msgItems[0]?.position).toBe('first');
    expect(msgItems[1]?.position).toBe('middle');
    expect(msgItems[2]?.position).toBe('last');
    expect(msgItems[3]?.position).toBe('single');
  });

  it('a new day starts a separator', () => {
    // 2026-10-26 10:00:00 UTC and 2026-10-27 10:00:00 UTC
    const day1 = new Date('2026-10-26T10:00:00Z').getTime();
    const day2 = new Date('2026-10-27T10:00:00Z').getTime();

    const messages = [
      msg('1', OTHER, 'day 1 message', day1),
      msg('2', OTHER, 'day 2 message', day2),
    ];

    const items = buildChatItems(messages, [], { selfId: SELF });
    const daySeparators = items.filter((i) => i.type === 'day');

    expect(daySeparators).toHaveLength(2);
    expect(items[0]?.type).toBe('day');
    expect(items[1]?.type).toBe('message');
    expect(items[2]?.type).toBe('day');
    expect(items[3]?.type).toBe('message');
  });

  it('the divider sits above the first unread message', () => {
    const baseTime = 1700000000000;
    const messages = [
      msg('101', OTHER, 'old read message', baseTime),
      msg('102', OTHER, 'first unread message', baseTime + 60000),
      msg('103', OTHER, 'second unread message', baseTime + 120000),
    ];

    const items = buildChatItems(messages, [], {
      selfId: SELF,
      firstUnreadId: '102',
    });

    const dividerIndex = items.findIndex((i) => i.type === 'divider');
    expect(dividerIndex).toBeGreaterThan(-1);

    const message102Index = items.findIndex(
      (i) => i.type === 'message' && i.message.id === '102',
    );
    expect(dividerIndex).toBe(message102Index - 1);
  });

  it('pending sends come after the loaded messages', () => {
    const baseTime = 1700000000000;
    const messages = [msg('1', OTHER, 'loaded message', baseTime)];
    const pending: PendingSend[] = [
      {
        nonce: 'nonce-1',
        serverKey: 'server-1',
        channelId: '10',
        content: 'sending message',
        files: [],
        state: 'sending',
        createdAtMs: baseTime + 1000,
      },
    ];

    const items = buildChatItems(messages, pending, { selfId: SELF });
    const msgItems = items.filter((i): i is MessageBubbleItem => i.type === 'message');

    expect(msgItems).toHaveLength(2);
    expect(msgItems[0]?.message.id).toBe('1');
    expect(msgItems[1]?.pending?.nonce).toBe('nonce-1');
    expect(msgItems[1]?.own).toBe(true);
  });

  it('bubbles in the middle of a group get the small corner on the author\'s side', () => {
    const baseTime = 1700000000000;
    const messages = [
      msg('1', OTHER, 'in 1', baseTime),
      msg('2', OTHER, 'in 2', baseTime + 60000),
      msg('3', OTHER, 'in 3', baseTime + 120000),
      msg('4', SELF, 'out 1', baseTime + 180000),
      msg('5', SELF, 'out 2', baseTime + 240000),
      msg('6', SELF, 'out 3', baseTime + 300000),
    ];

    const items = buildChatItems(messages, [], { selfId: SELF });
    const msgItems = items.filter((i): i is MessageBubbleItem => i.type === 'message');

    // Middle incoming bubble
    expect(msgItems[1]?.position).toBe('middle');
    expect(msgItems[1]?.own).toBe(false);

    // Middle own bubble
    expect(msgItems[4]?.position).toBe('middle');
    expect(msgItems[4]?.own).toBe(true);
  });
});
