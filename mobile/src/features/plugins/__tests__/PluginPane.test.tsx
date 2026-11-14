import { act, render, screen } from '@testing-library/react-native';
import { ConnectionRecord, MobileEvent, PluginChannelState, type ChannelRecord } from '@opencord/core-native';

import type { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { ContentPane } from '@/features/server/ContentPane';
import { onEvents } from '@/state/store';
import { freshStore, settle } from '@/test/store';
import type { FakeArea } from '@/test/fakeArea';

// The native module, faked: one FakeArea for the whole file, its view
// visible to the test.
jest.mock('@opencord/plugin-area', () => {
  const { View } = jest.requireActual<typeof import('react-native')>('react-native');
  const { FakeArea: Fake } = jest.requireActual<typeof import('@/test/fakeArea')>('@/test/fakeArea');
  const fake = new Fake();
  const bound = <K extends keyof InstanceType<typeof Fake>>(name: K) =>
    (...args: unknown[]) => (fake[name] as (...a: unknown[]) => unknown)(...args);
  return {
    __fake: fake,
    available: true,
    warmUp: jest.fn(),
    coolDown: jest.fn(),
    createArea: bound('createArea'),
    destroyArea: bound('destroyArea'),
    destroyAllAreas: bound('destroyAllAreas'),
    setSetup: bound('setSetup'),
    pushBatch: bound('pushBatch'),
    hostReply: bound('hostReply'),
    back: bound('back'),
    addInputListener: bound('addInputListener'),
    addStatusListener: bound('addStatusListener'),
    addHostCallListener: bound('addHostCallListener'),
    PluginAreaView: ({ areaId }: { areaId: string }) => <View testID={`area-view-${areaId}`} />,
  };
});
jest.mock('expo-router', () => ({ router: { back: jest.fn(), push: jest.fn() } }));

const area = (jest.requireMock('@opencord/plugin-area') as { __fake: FakeArea }).__fake;

// The renderer's first input once it has a size (BlocksView._reportContext).
const context = {
  kind: 'context',
  width: 390,
  height: 700,
  theme: 'dark',
  input: 'touch',
  safe_area: { top: 0, right: 0, bottom: 0, left: 0 },
  text_scale: 1,
};

/** Night Owls with both plugin channels in the states given. */
async function ready(oldBoard = PluginChannelState.Available): Promise<FakeCore> {
  const core = freshStore();
  const base = snapshot(NIGHT_OWLS);
  const channels = base.channels.map((channel) =>
    channel.id === CHANNELS.oldBoard && channel.plugin ? { ...channel, plugin: { ...channel.plugin, state: oldBoard } } : channel,
  );
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: { ...base, channels } }),
  ]);
  await settle();
  return core;
}

function lastAreaId(): string {
  const created = area.calls.filter((call) => call.method === 'createArea').at(-1);
  if (!created) throw new Error('no area was created');
  return created.args[0] as string;
}

async function opened(areaId: string) {
  await act(async () => area.input(areaId, context));
  await settle();
}

beforeEach(() => {
  area.calls.length = 0;
});

describe('a plugin channel', () => {
  // Fails if the pane opens another channel's view (two plugin channels),
  // or the plugin's screen isn't the session's area once it runs.
  it("opening one draws the plugin's screen", async () => {
    const core = await ready();
    await render(<ContentPane serverKey={NIGHT_OWLS} open={{ id: CHANNELS.oldBoard, kind: 'plugin' }} />);
    await settle();
    const areaId = lastAreaId();
    await opened(areaId);
    expect(core.calls.filter((call) => call.method === 'viewOpen').map((call) => call.args[1])).toEqual([CHANNELS.oldBoard]);
    await act(async () => area.status(areaId, { state: 'running', reason: '', detail: '', overlays: 0 }));
    expect(screen.getByTestId(`area-view-${areaId}`)).toBeTruthy();
    expect(screen.getByText('old-board')).toBeTruthy();
  });

  // Fails if leaving the channel leaves its view open in the core (§14.3).
  it('leaving closes the view', async () => {
    const core = await ready();
    const shown = await render(<ContentPane serverKey={NIGHT_OWLS} open={{ id: CHANNELS.helpDesk, kind: 'plugin' }} />);
    await settle();
    const areaId = lastAreaId();
    await opened(areaId);
    expect(core.calls.some((call) => call.method === 'viewOpen')).toBe(true);
    // The fake core's first view of this start.
    const viewId = 'fake-view-1';
    await shown.unmount();
    expect(core.calls.filter((call) => call.method === 'viewClose').map((call) => call.args[0])).toEqual([viewId]);
    expect(area.calls.filter((call) => call.method === 'destroyArea').map((call) => call.args[0])).toEqual([areaId]);
  });

  // Fails if the sidebar over the pane doesn't pause the plugin.
  it('the sidebar over the pane pauses it', async () => {
    const core = await ready();
    const shown = await render(<ContentPane serverKey={NIGHT_OWLS} open={{ id: CHANNELS.helpDesk, kind: 'plugin' }} />);
    await settle();
    await opened(lastAreaId());
    await shown.rerender(<ContentPane serverKey={NIGHT_OWLS} open={{ id: CHANNELS.helpDesk, kind: 'plugin' }} covered />);
    await settle();
    const lifecycle = core.calls
      .filter((call) => call.method === 'viewInput')
      .map((call) => JSON.parse(call.args[1] as string) as { kind: string; event?: string })
      .filter((input) => input.kind === 'lifecycle');
    expect(lifecycle).toEqual([{ kind: 'lifecycle', event: 'pause' }]);
  });

  // Fails if an unavailable channel starts an area, or one that becomes
  // available while its pane is open doesn't open a fresh session.
  it('a channel that comes back opens again', async () => {
    await ready(PluginChannelState.PluginStopped);
    await render(<ContentPane serverKey={NIGHT_OWLS} open={{ id: CHANNELS.oldBoard, kind: 'plugin' }} />);
    await settle();
    expect(screen.getByText('This channel is unavailable')).toBeTruthy();
    expect(area.calls.filter((call) => call.method === 'createArea')).toHaveLength(0);
    const stopped = snapshot(NIGHT_OWLS).channels.find((channel) => channel.id === CHANNELS.oldBoard) as ChannelRecord;
    if (!stopped.plugin) throw new Error('old-board is a plugin channel');
    const back: ChannelRecord = { ...stopped, plugin: { ...stopped.plugin, state: PluginChannelState.Available } };
    await act(async () => {
      onEvents([new MobileEvent.ChannelUpserted({ serverKey: NIGHT_OWLS, channel: back })]);
    });
    await settle();
    expect(area.calls.filter((call) => call.method === 'createArea')).toHaveLength(1);
  });
});
