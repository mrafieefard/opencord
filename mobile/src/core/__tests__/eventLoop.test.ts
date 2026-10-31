import { MobileError, MobileEvent } from '@opencord/core-native';

import { startEventLoop } from '../eventLoop';
import { FakeCore } from '../fake/FakeCore';

const typing = (userId: string) =>
  new MobileEvent.Typing({ serverKey: 's', channelId: '10', userId });

const flush = () => new Promise<void>((resolve) => setImmediate(resolve));

describe('the event loop', () => {
  it('delivers batches in order', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    const seen: string[] = [];
    const loop = startEventLoop(core, (events) => {
      for (const event of events) if (MobileEvent.Typing.instanceOf(event)) seen.push(event.inner.userId);
    });
    core.emit(typing('1'), typing('2'));
    await flush();
    core.emit(typing('3'));
    await flush();
    expect(seen).toEqual(['1', '2', '3']);
    loop.stop();
  });

  it('a failing call backs off and keeps going', async () => {
    jest.useFakeTimers();
    try {
      const core = new FakeCore({ scenario: 'empty' });
      const seen: MobileEvent[] = [];
      core.failNext('nextEvents', new MobileError.Timeout());
      startEventLoop(core, (events) => seen.push(...events));
      await Promise.resolve();
      // The first retry waits 250 ms; it fails again and the next waits 500 ms.
      core.failNext('nextEvents', new MobileError.Timeout());
      await jest.advanceTimersByTimeAsync(249);
      expect(core.calls.filter((call) => call.method === 'nextEvents')).toHaveLength(1);
      await jest.advanceTimersByTimeAsync(1);
      expect(core.calls.filter((call) => call.method === 'nextEvents')).toHaveLength(2);
      await jest.advanceTimersByTimeAsync(499);
      expect(core.calls.filter((call) => call.method === 'nextEvents')).toHaveLength(2);
      await jest.advanceTimersByTimeAsync(1);
      core.emit(typing('9'));
      await jest.advanceTimersByTimeAsync(0);
      expect(seen).toHaveLength(1);
    } finally {
      jest.useRealTimers();
    }
  });

  it('stopping ends the loop without another call', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    const loop = startEventLoop(core, () => {});
    await flush();
    loop.stop();
    core.emit(typing('1'));
    await flush();
    expect(core.calls.filter((call) => call.method === 'nextEvents')).toHaveLength(1);
  });

  it('restarting the loop takes over the stream', async () => {
    const core = new FakeCore({ scenario: 'empty' });
    const first: MobileEvent[] = [];
    const second: MobileEvent[] = [];
    startEventLoop(core, (events) => first.push(...events));
    await flush();
    startEventLoop(core, (events) => second.push(...events));
    await flush();
    core.emit(typing('1'));
    await flush();
    expect(first).toHaveLength(0);
    expect(second).toHaveLength(1);
    // The first loop ended: only the second keeps asking.
    const calls = core.calls.filter((call) => call.method === 'nextEvents').length;
    core.emit(typing('2'));
    await flush();
    expect(core.calls.filter((call) => call.method === 'nextEvents').length).toBe(calls + 1);
  });
});
