import { AttachmentVariant, MobileError } from '@opencord/core-native';

import { FakeCore } from '@/core/fake/FakeCore';
import { NIGHT_OWLS } from '@/core/fake/world';
import { FakeArea } from '@/test/fakeArea';

import { answerHostCalls, type HostCallDeps } from '../hostCalls';

const AREA = 'area-test-1';

function world(overrides: Partial<HostCallDeps> = {}) {
  const core = new FakeCore();
  const area = new FakeArea();
  const session = { areaId: AREA, viewId: 'fake-view-1' as string | null };
  const used: { method: string; args: unknown[] }[] = [];
  const deps: HostCallDeps = {
    core,
    serverKey: NIGHT_OWLS,
    pluginId: 'tickets',
    userName: (id) => (id === '1001' ? 'Ava' : undefined),
    userAvatarHash: (id) => (id === '1001' ? 'abc' : null),
    channelName: (id) => (id === '10' ? 'general' : undefined),
    pickImage: async () => {
      used.push({ method: 'pickImage', args: [] });
      return '/files/photo.jpg';
    },
    pickDocument: async (accept) => {
      used.push({ method: 'pickDocument', args: [accept] });
      return '/files/notes.txt';
    },
    confirmLink: async (url) => {
      used.push({ method: 'confirmLink', args: [url] });
      return true;
    },
    copy: async (text) => {
      used.push({ method: 'copy', args: [text] });
    },
    openChannel: (id) => used.push({ method: 'openChannel', args: [id] }),
    playSound: async (path, volume) => {
      used.push({ method: 'playSound', args: [path, volume] });
    },
    ...overrides,
  };
  const stop = answerHostCalls(session, area, deps);
  let next = 0;
  /** Dart asks; resolves with the parsed reply. */
  async function call(method: string, args: object): Promise<Record<string, unknown>> {
    const id = ++next;
    area.hostCall({ areaId: AREA, id, method, json: JSON.stringify(args) });
    for (let i = 0; i < 20; i++) {
      const reply = area.calls.find((c) => c.method === 'hostReply' && c.args[1] === id);
      if (reply) return JSON.parse(reply.args[2] as string) as Record<string, unknown>;
      await new Promise((resolve) => setTimeout(resolve, 0));
    }
    throw new Error(`no reply to ${method}`);
  }
  return { core, area, session, used, call, stop };
}

describe('host calls', () => {
  // Fails if an image source other than an attachment isn't fetched as the
  // plugin's image, or the file isn't the reply.
  it('image asks the core for the plugin image and replies its path', async () => {
    const { core, call } = world();
    const source = { url: 'https://example.com/a.png', version: 1 };
    const reply = await call('image', { source });
    const asked = core.calls.find((c) => c.method === 'pluginImageFile');
    expect(asked?.args).toEqual([NIGHT_OWLS, 'tickets', JSON.stringify(source)]);
    expect(reply).toEqual({ ok: { path: expect.any(String) } });
  });

  // Fails if an attachment source goes to the plugin image call (the core
  // refuses it) instead of the attachment's file.
  it('an attachment image is read as the attachment', async () => {
    const { core, call } = world();
    await call('image', { source: { attachment: '555' } });
    const asked = core.calls.find((c) => c.method === 'attachmentFile');
    expect(asked?.args).toEqual([NIGHT_OWLS, '555', AttachmentVariant.Original]);
    expect(core.calls.some((c) => c.method === 'pluginImageFile')).toBe(false);
  });

  it('asset asks for the plugin asset file', async () => {
    const { core, call } = world();
    const reply = await call('asset', { path: 'assets/fonts/Display.ttf' });
    expect(core.calls.find((c) => c.method === 'pluginAssetFile')?.args).toEqual([NIGHT_OWLS, 'tickets', 'assets/fonts/Display.ttf']);
    expect(reply).toEqual({ ok: { path: expect.any(String) } });
  });

  // Fails if the user's name or avatar file isn't the reply, or an unknown
  // user is an error (the block shows "User <id>" for an empty name).
  it('user replies the member name and avatar file', async () => {
    const { core, call } = world();
    expect(await call('user', { id: '1001' })).toEqual({ ok: { name: 'Ava', avatar_path: expect.any(String) } });
    expect(core.calls.find((c) => c.method === 'avatarFile')?.args).toEqual([NIGHT_OWLS, 'abc']);
    expect(await call('user', { id: '9' })).toEqual({ ok: { name: '', avatar_path: null } });
  });

  it('names replies only the names it knows', async () => {
    const { call } = world();
    expect(await call('names', { users: ['1001', '9'], channels: ['10', '99'] })).toEqual({
      ok: { users: { '1001': 'Ava' }, channels: { '10': 'general' } },
    });
  });

  // Fails if the image picker isn't used for an image pick, the document
  // picker with `accept` otherwise, or a cancel isn't `{}`.
  it('pick uses the right picker and a cancel replies {}', async () => {
    const { used, call } = world();
    expect(await call('pick', { image: true, accept: [] })).toEqual({ ok: { path: '/files/photo.jpg' } });
    expect(await call('pick', { image: false, accept: ['text/plain'] })).toEqual({ ok: { path: '/files/notes.txt' } });
    expect(used.map((u) => u.method)).toEqual(['pickImage', 'pickDocument']);
    expect(used[1]?.args).toEqual([['text/plain']]);
    const cancelled = world({ pickImage: async () => null });
    expect(await cancelled.call('pick', { image: true, accept: [] })).toEqual({ ok: {} });
  });

  it('upload hands the file to the view', async () => {
    const { core, call } = world();
    expect(await call('upload', { node_id: 12, path: '/files/photo.jpg' })).toEqual({ ok: null });
    expect(core.calls.find((c) => c.method === 'viewUpload')?.args).toEqual(['fake-view-1', 12n, '/files/photo.jpg']);
  });

  // Fails if a refused upload replies ok (the picker would never hear).
  it('a refused upload replies an error', async () => {
    const { core, call } = world();
    core.failNext('viewUpload', new MobileError.InvalidInput({ message: 'no view' }));
    expect(await call('upload', { node_id: 12, path: '/files/photo.jpg' })).toEqual({ error: 'InvalidInput' });
  });

  it('link asks the viewer and replies whether it opened', async () => {
    const { used, call } = world();
    expect(await call('link', { url: 'https://opencord.dev' })).toEqual({ ok: { opened: true } });
    expect(used).toEqual([{ method: 'confirmLink', args: ['https://opencord.dev'] }]);
  });

  it('copy, open_channel and sound use the phone and reply ok', async () => {
    const { core, used, call } = world();
    expect(await call('copy', { text: 'TICKET-7' })).toEqual({ ok: null });
    expect(await call('open_channel', { id: '10' })).toEqual({ ok: null });
    expect(await call('sound', { asset: 'assets/ding.ogg', volume: 0.5 })).toEqual({ ok: null });
    expect(core.calls.find((c) => c.method === 'pluginAssetFile')?.args).toEqual([NIGHT_OWLS, 'tickets', 'assets/ding.ogg']);
    expect(used.map((u) => u.method)).toEqual(['copy', 'openChannel', 'playSound']);
    expect(used[2]?.args).toEqual([expect.any(String), 0.5]);
  });

  // Fails if a call for another area is answered here, or one after stop.
  it('calls for another area, or after stop, are dropped', async () => {
    const { area, stop } = world();
    area.hostCall({ areaId: 'area-other', id: 1, method: 'copy', json: '{"text":"x"}' });
    stop();
    area.hostCall({ areaId: AREA, id: 2, method: 'copy', json: '{"text":"x"}' });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(area.calls.filter((c) => c.method === 'hostReply')).toEqual([]);
  });

  it('an unknown method replies an error', async () => {
    const { call } = world();
    expect(await call('teleport', {})).toEqual({ error: 'unknown_method' });
  });
});
