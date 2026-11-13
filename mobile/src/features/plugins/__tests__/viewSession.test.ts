import { MobileError } from '@opencord/core-native';

import { FakeCore } from '@/core/fake/FakeCore';
import opened from '@/core/fake/fixtures/tickets-1.json';
import newTicket from '@/core/fake/fixtures/tickets-2.json';

import { startViewPump, type ViewPump } from '../viewPump';
import { ViewSession, type ViewPlace } from '../viewSession';
import { FakeArea } from '@/test/fakeArea';

const flush = () => new Promise<void>((resolve) => setImmediate(resolve));

// The renderer's first input once it has a size (BlocksView._reportContext).
const context = {
  kind: 'context',
  width: 394.7,
  height: 697.5,
  theme: 'light',
  input: 'touch',
  safe_area: { top: 0, right: 0, bottom: 0, left: 0 },
  text_scale: 1,
};

const place: ViewPlace = { kind: 'channel', serverKey: '10.0.2.2:7710', channelId: '114795140719050752' };
const setupJson = JSON.stringify({ theme: 'light' });

function world() {
  const core = new FakeCore({ scenario: 'empty' });
  const area = new FakeArea();
  const pump: ViewPump = startViewPump(core, area);
  const session = new ViewSession({ core, pump, area, place, setupJson });
  return { core, area, pump, session };
}

function areaIdOf(area: FakeArea): string {
  const created = area.calls.find((call) => call.method === 'createArea');
  if (!created) throw new Error('no area was created');
  return created.args[0] as string;
}

const openingJson = opened.batches.map((b) => JSON.stringify(b));

describe('a view session', () => {
  // Fails if the session opens the view before the renderer's context (the
  // core would lay out for no size), or forwards the context with its kind.
  it('opens the view with the renderer’s first context', async () => {
    const { core, area, session } = world();
    await session.start();
    expect(core.calls.some((call) => call.method === 'viewOpen')).toBe(false);
    area.input(areaIdOf(area), context);
    await flush();
    const open = core.calls.find((call) => call.method === 'viewOpen');
    const { kind: _kind, ...withoutKind } = context;
    expect(open?.args).toEqual([place.serverKey, place.channelId, JSON.stringify(withoutKind)]);
    session.close();
  });

  // Fails if batches that arrive before viewOpen returns its id are lost,
  // doubled or reordered (the core sends a view's first batches at once).
  it('early batches wait for the view’s id', async () => {
    const { core, area, session } = world();
    const release = core.pauseViewOpen();
    await session.start();
    const id = areaIdOf(area);
    area.input(id, context);
    await flush();
    await flush();
    expect(area.pushed.get(id)).toBeUndefined();
    release();
    await flush();
    expect(openingJson.length).toBeGreaterThanOrEqual(1);
    expect(area.pushed.get(id)).toEqual(openingJson);
    session.close();
  });

  // Fails if a recorded press doesn't reach the core, or its answer
  // doesn't reach the area.
  it('an input reaches the core and its batches the area', async () => {
    const { area, session } = world();
    await session.start();
    const id = areaIdOf(area);
    area.input(id, context);
    await flush();
    await flush();
    area.input(id, newTicket.input as object);
    await flush();
    await flush();
    expect(area.pushed.get(id)).toEqual([...openingJson, ...newTicket.batches.map((b) => JSON.stringify(b))]);
    session.close();
  });

  // Fails if a failed open leaves the area's engine alive, or the status
  // still says loading.
  it('a failed open closes the area and reports error', async () => {
    const { core, area, session } = world();
    core.failNext('viewOpen', new MobileError.NotConnected());
    await session.start();
    const id = areaIdOf(area);
    area.input(id, context);
    await flush();
    await flush();
    expect(area.methods()).toContain('destroyArea');
    expect(session.status.state).toBe('error');
  });

  // Fails if inputs Dart sends while the view is opening are dropped, or
  // sent out of order.
  it('inputs before the view is open are sent after it, in order', async () => {
    const { core, area, session } = world();
    const release = core.pauseViewOpen();
    await session.start();
    const id = areaIdOf(area);
    area.input(id, context);
    const first = { kind: 'event', node: 3, event: 'press', data: {} };
    const second = { kind: 'lifecycle', event: 'pause' };
    area.input(id, first);
    session.send(second);
    await flush();
    expect(core.calls.some((call) => call.method === 'viewInput')).toBe(false);
    release();
    await flush();
    await flush();
    const sent = core.calls.filter((call) => call.method === 'viewInput').map((call) => call.args[1]);
    expect(sent).toEqual([JSON.stringify(first), JSON.stringify(second)]);
    session.close();
  });

  // Fails if a session closed while opening leaves the core's view open
  // (it would run in the core with nobody drawing it).
  it('close while opening closes the view when it arrives', async () => {
    const { core, area, session } = world();
    const release = core.pauseViewOpen();
    await session.start();
    const id = areaIdOf(area);
    area.input(id, context);
    await flush();
    session.close();
    release();
    await flush();
    await flush();
    expect(core.calls.filter((call) => call.method === 'viewClose')).toHaveLength(1);
    expect(area.pushed.get(id)).toBeUndefined();
    expect(area.methods().filter((method) => method === 'destroyArea')).toHaveLength(1);
  });

  // Fails if a second close closes the view or the area again, or the
  // listeners stay registered.
  it('close is safe twice', async () => {
    const { core, area, session } = world();
    await session.start();
    area.input(areaIdOf(area), context);
    await flush();
    await flush();
    session.close();
    session.close();
    expect(core.calls.filter((call) => call.method === 'viewClose')).toHaveLength(1);
    expect(area.methods().filter((method) => method === 'destroyArea')).toHaveLength(1);
    expect(area.listenerCount()).toBe(0);
  });

  // Fails if two sessions share an area id (an old session's cleanup would
  // destroy a new area: seen on the emulator after a reload).
  it('each session has its own area id', async () => {
    const { core, area, pump } = world();
    const a = new ViewSession({ core, pump, area, place, setupJson });
    const b = new ViewSession({ core, pump, area, place, setupJson });
    await a.start();
    await b.start();
    const ids = area.calls.filter((call) => call.method === 'createArea').map((call) => call.args[0]);
    expect(new Set(ids).size).toBe(2);
    a.close();
    b.close();
  });

  // Fails if the status the area reports isn't the session's.
  it('the status follows the area’s', async () => {
    const { area, session } = world();
    await session.start();
    expect(session.status.state).toBe('loading');
    area.status(areaIdOf(area), { state: 'running', reason: '', detail: '', overlays: 1 });
    expect(session.status).toEqual({ state: 'running', reason: '', detail: '', overlays: 1 });
    session.close();
  });
});
