import { FakeCore } from '@/core/fake/FakeCore';

import { startViewPump } from '../viewPump';
import { FakeArea } from '@/test/fakeArea';

const flush = () => new Promise<void>((resolve) => setImmediate(resolve));
const batch = (seq: number) => JSON.stringify({ view: '1', seq, ops: [] });

describe('the view pump', () => {
  // Fails if batches reach their listener out of order, or one is lost.
  it('batches reach their view’s listener in order', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    const pump = startViewPump(core, new FakeArea());
    const seen: string[] = [];
    pump.listen('view-a', (json) => seen.push(json));
    core.emitViewOutputs({ viewId: 'view-a', json: batch(1) }, { viewId: 'view-a', json: batch(2) });
    await flush();
    core.emitViewOutputs({ viewId: 'view-a', json: batch(3) });
    await flush();
    expect(seen).toEqual([batch(1), batch(2), batch(3)]);
    pump.stop();
  });

  // Fails if a held batch for a view nobody opened reaches the opener (a
  // stale view's batches drawn into a new area).
  it('batches for another view are dropped on release', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    const pump = startViewPump(core, new FakeArea());
    const hold = pump.hold();
    core.emitViewOutputs({ viewId: 'view-a', json: batch(1) }, { viewId: 'view-b', json: batch(1) }, { viewId: 'view-a', json: batch(2) });
    await flush();
    const seen: string[] = [];
    pump.listen('view-a', (json) => seen.push(json));
    hold.release('view-a');
    expect(seen).toEqual([batch(1), batch(2)]);
    const other: string[] = [];
    pump.listen('view-b', (json) => other.push(json));
    expect(other).toEqual([]);
    pump.stop();
  });

  // Fails if two pumps both read the stream after a reload (each batch to
  // one of them at random), or the older keeps waiting.
  it('a newer pump ends the older', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    const older = startViewPump(core, new FakeArea());
    const olderSeen: string[] = [];
    older.listen('view-a', (json) => olderSeen.push(json));
    await flush();
    const newer = startViewPump(core, new FakeArea());
    const newerSeen: string[] = [];
    newer.listen('view-a', (json) => newerSeen.push(json));
    await flush();
    core.emitViewOutputs({ viewId: 'view-a', json: batch(1) });
    await flush();
    expect(olderSeen).toEqual([]);
    expect(newerSeen).toEqual([batch(1)]);
    newer.stop();
  });

  // Fails if the pump attaches before destroying the areas a JavaScript
  // reload left behind (their engines would live on, drawing nothing).
  it('starting the pump destroys leftover areas before attaching', () => {
    const core = new FakeCore({ scenario: 'empty' });
    const area = new FakeArea();
    const order: string[] = [];
    const attach = core.viewOutputsAttach.bind(core);
    core.viewOutputsAttach = () => {
      order.push('viewOutputsAttach');
      return attach();
    };
    area.destroyAllAreas = () => {
      order.push('destroyAllAreas');
    };
    startViewPump(core, area).stop();
    expect(order).toEqual(['destroyAllAreas', 'viewOutputsAttach']);
  });
});
