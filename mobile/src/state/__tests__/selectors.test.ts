import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, RUST_CLUB, SELF_ID, WEEKEND_RAID, fullAccess, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';

import { buildChannelSections, buildServerRows, buildVoiceMembers } from '../selectors';
import { onEvents, useAppStore } from '../store';

const now = 1_761_500_000_000;

async function connected() {
  freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
  ]);
  await settle();
}

describe('selectors', () => {
  it('server rows sort by latest activity, then by name', async () => {
    await connected();
    const rows = buildServerRows(useAppStore.getState(), '');
    expect(rows.map((row) => row.key)).toEqual([NIGHT_OWLS, RUST_CLUB, WEEKEND_RAID]);
    expect(rows[0]?.lastAtMs).not.toBeNull();
    expect(rows[1]?.lastAtMs).toBeNull();
  });

  it("a row's preview names the author, You for own messages", async () => {
    await connected();
    const send = (authorId: string, content: string, id: string) =>
      onEvents([
        new MobileEvent.MessageCreated({
          serverKey: NIGHT_OWLS,
          message: { id, channelId: CHANNELS.general, authorId, content, createdAtMs: now, attachments: [] },
        }),
      ]);
    send('1002', 'hello <@1001>', '400001');
    expect(buildServerRows(useAppStore.getState(), '')[0]?.preview).toEqual({
      channelName: 'general',
      authorName: 'Mira',
      text: 'hello @Kai',
      own: false,
    });
    send(SELF_ID, 'mine', '400002');
    expect(buildServerRows(useAppStore.getState(), '')[0]?.preview).toMatchObject({ authorName: 'You', own: true });
  });

  it("channel sections put channels without a category first, then categories by position, without channels the user can't view", async () => {
    await connected();
    const server = useAppStore.getState().servers[NIGHT_OWLS];
    if (!server?.snapshot) throw new Error('no snapshot');
    const hidden = { ...server, snapshot: { ...server.snapshot, access: server.snapshot.access.map((access) => (access.channelId === CHANNELS.quiet ? { ...fullAccess(CHANNELS.quiet), view: false } : access)) } };
    const sections = buildChannelSections(hidden, {});
    expect(sections.map((section) => section.category?.name ?? null)).toEqual([null, 'Text', 'Voice']);
    expect(sections.map((section) => section.items.map((item) => item.channel.name))).toEqual([
      ['general', 'music-requests'],
      ['off-topic'],
      ['Lounge'],
    ]);
  });

  it('voice members carry speaking and mute flags', async () => {
    await connected();
    const server = useAppStore.getState().servers[NIGHT_OWLS];
    if (!server) throw new Error('no server');
    expect(buildVoiceMembers(server, CHANNELS.lounge, ['1001'])).toEqual([
      expect.objectContaining({ userId: '1001', name: 'Kai', speaking: true, muted: false }),
      expect.objectContaining({ userId: '1002', name: 'Mira', speaking: false, muted: true }),
    ]);
  });

  it('filtering rows ignores case', async () => {
    await connected();
    expect(buildServerRows(useAppStore.getState(), 'rUsT').map((row) => row.key)).toEqual([RUST_CLUB]);
  });
});
