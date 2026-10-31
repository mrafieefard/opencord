import { MobileError, MobileEvent, type MessageRecord } from '@opencord/core-native';

import { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import { setCore } from '@/core';
import { freshStore, settle } from '@/test/store';

import { markChannelRead, markServerRead } from '../actions/servers';
import { setOpenChannel } from '../actions/ui';
import { onEvents, readPositions, setToaster, useAppStore } from '../store';

const now = 1_761_500_000_000;
const ready = (name = 'Night Owls') => new MobileEvent.Ready({ snapshot: { ...snapshot(NIGHT_OWLS), name } });
const created = (id: string, channelId: string, authorId = '1001', content = 'hi'): MobileEvent =>
  new MobileEvent.MessageCreated({
    serverKey: NIGHT_OWLS,
    message: { id, channelId, authorId, content, createdAtMs: now, attachments: [] },
  });
const activity = (channelId: string) => useAppStore.getState().servers[NIGHT_OWLS]?.activity[channelId];

async function others(core: FakeCore, after: string): Promise<MessageRecord[]> {
  const recent = await core.messagesFetch(NIGHT_OWLS, CHANNELS.general, undefined, 50);
  return recent.filter((message) => BigInt(message.id) > BigInt(after) && message.authorId !== SELF_ID);
}

describe('unread counts, as the desktop counts them', () => {
  it("seeding counts others' messages since the saved position", async () => {
    const core = freshStore();
    core.settingsSet(`mobile.read.${NIGHT_OWLS}`, JSON.stringify({ [CHANNELS.general]: '100110' }));
    onEvents([ready()]);
    await settle();
    const expected = await others(core, '100110');
    expect(expected.length).toBeGreaterThan(0);
    expect(activity(CHANNELS.general)).toMatchObject({ lastReadId: '100110', unread: expected.length, last: { id: '100119' } });
  });

  it('a channel never read starts read at its newest message', async () => {
    freshStore();
    onEvents([ready()]);
    await settle();
    expect(activity(CHANNELS.general)).toMatchObject({ lastReadId: '100119', unread: 0 });
    expect(readPositions.of(NIGHT_OWLS, CHANNELS.general)).toBe('100119');
  });

  it('an empty channel starts at 0 so its next message counts', async () => {
    freshStore();
    onEvents([ready()]);
    await settle();
    expect(readPositions.of(NIGHT_OWLS, CHANNELS.offTopic)).toBe('0');
    onEvents([created('500', CHANNELS.offTopic)]);
    expect(activity(CHANNELS.offTopic)).toMatchObject({ unread: 1, last: { id: '500' } });
  });

  it('own messages, and messages in the open chat at the bottom, never count', async () => {
    freshStore();
    onEvents([ready()]);
    await settle();
    onEvents([created('300600', CHANNELS.general, SELF_ID)]);
    expect(activity(CHANNELS.general)?.unread).toBe(0);
    setOpenChannel(NIGHT_OWLS, CHANNELS.general, true);
    onEvents([created('300601', CHANNELS.general)]);
    expect(activity(CHANNELS.general)?.unread).toBe(0);
    setOpenChannel(NIGHT_OWLS, CHANNELS.general, false);
    onEvents([created('300602', CHANNELS.general)]);
    expect(activity(CHANNELS.general)?.unread).toBe(1);
  });

  it('mentions count messages with <@self>', async () => {
    freshStore();
    onEvents([ready()]);
    await settle();
    onEvents([created('300700', CHANNELS.general, '1002', `hey <@${SELF_ID}>`), created('300701', CHANNELS.general, '1002', 'hey <@1001>')]);
    expect(activity(CHANNELS.general)).toMatchObject({ unread: 2, mentions: 1 });
  });

  it('events during seeding are applied after it, in order, once', async () => {
    freshStore();
    // 100119 is in the seeding fetch already; 300800 is new.
    onEvents([ready(), created('100119', CHANNELS.general), created('300800', CHANNELS.general)]);
    expect(useAppStore.getState().servers[NIGHT_OWLS]?.held).toHaveLength(2);
    expect(activity(CHANNELS.general)).toBeUndefined();
    await settle();
    expect(useAppStore.getState().servers[NIGHT_OWLS]?.held).toHaveLength(0);
    expect(activity(CHANNELS.general)).toMatchObject({ unread: 1, last: { id: '300800' } });
  });

  it('a newer Ready during seeding discards the older result', async () => {
    freshStore();
    onEvents([ready('First')]);
    onEvents([ready('Second')]);
    await settle();
    const server = useAppStore.getState().servers[NIGHT_OWLS];
    expect(server?.snapshot?.name).toBe('Second');
    expect(server?.readyCount).toBe(2);
    expect(server?.seeding).toBe(false);
  });

  it('a failed fetch leaves the channel without activity', async () => {
    class GeneralFails extends FakeCore {
      override async messagesFetch(serverKey: string, channelId: string, before: string | undefined, limit: number) {
        if (channelId === CHANNELS.general) throw new MobileError.Timeout();
        return super.messagesFetch(serverKey, channelId, before, limit);
      }
    }
    freshStore();
    setCore(new GeneralFails({ now: () => now }));
    onEvents([ready()]);
    await settle();
    expect(activity(CHANNELS.general)).toBeUndefined();
    expect(activity(CHANNELS.musicRequests)).toBeDefined();
  });

  it('read positions only move forward and are written at most every 2 s', async () => {
    jest.useFakeTimers();
    try {
      const core = freshStore();
      const writes = () => core.calls.filter((call) => call.method === 'settingsSet' && String(call.args[0]).startsWith('mobile.read.'));
      readPositions.save(NIGHT_OWLS, CHANNELS.general, '100');
      readPositions.save(NIGHT_OWLS, CHANNELS.general, '200');
      readPositions.save(NIGHT_OWLS, CHANNELS.general, '150');
      expect(readPositions.of(NIGHT_OWLS, CHANNELS.general)).toBe('200');
      await jest.advanceTimersByTimeAsync(1999);
      expect(writes()).toHaveLength(0);
      await jest.advanceTimersByTimeAsync(1);
      expect(writes()).toHaveLength(1);
      expect(JSON.parse(String(writes()[0]?.args[1]))).toEqual({ [CHANNELS.general]: '200' });
    } finally {
      jest.useRealTimers();
    }
  });

  it('marking a server read clears all its channels', async () => {
    freshStore();
    onEvents([ready()]);
    await settle();
    onEvents([created('300900', CHANNELS.general), created('300901', CHANNELS.musicRequests)]);
    markServerRead(NIGHT_OWLS);
    expect(activity(CHANNELS.general)).toMatchObject({ unread: 0, lastReadId: '300900' });
    expect(activity(CHANNELS.musicRequests)).toMatchObject({ unread: 0, lastReadId: '300901' });
    markChannelRead(NIGHT_OWLS, CHANNELS.general, '10');
    expect(activity(CHANNELS.general)?.lastReadId).toBe('300900');
  });

  it('a music notice becomes a toast', () => {
    freshStore();
    const toasts: string[] = [];
    setToaster((text) => toasts.push(text));
    onEvents([new MobileEvent.MusicNotice({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge, text: 'Skipped by vote' })]);
    expect(toasts).toEqual(['Skipped by vote']);
  });
});
