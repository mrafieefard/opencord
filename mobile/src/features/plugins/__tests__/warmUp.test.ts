import { act, cleanup, renderHook } from '@testing-library/react-native';
import { MobileEvent } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, NIGHT_OWLS as WITH_PLUGINS, SELF_ID, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { COOL_DOWN_MS, useEngineWarmUp, warmth, watchCallWarmth } from '../warmUp';

const mockEngine = { warmUp: jest.fn(), coolDown: jest.fn(), available: true };
jest.mock('@opencord/plugin-area', () => ({
  get available() {
    return mockEngine.available;
  },
  warmUp: () => mockEngine.warmUp(),
  coolDown: () => mockEngine.coolDown(),
}));

/** A server with no plugin channel type and no activity. */
const PLAIN = 'plain.example:7710';

async function ready() {
  freshStore();
  const plain = { ...snapshot(NIGHT_OWLS), serverKey: PLAIN, pluginChannelTypes: [], pluginActivities: [] };
  onEvents([new MobileEvent.Ready({ snapshot: snapshot(WITH_PLUGINS) }), new MobileEvent.Ready({ snapshot: plain })]);
  await settle();
}

function inCall(on: boolean) {
  onEvents([
    new MobileEvent.VoiceStateChanged({
      serverKey: NIGHT_OWLS,
      state: {
        userId: SELF_ID,
        channelId: on ? CHANNELS.lounge : undefined,
        thisDevice: true,
        selfMute: false,
        selfDeaf: false,
        serverMute: false,
        serverDeaf: false,
      },
    }),
  ]);
}

beforeEach(async () => {
  jest.useRealTimers();
  await ready();
  jest.useFakeTimers();
  mockEngine.warmUp.mockClear();
  mockEngine.coolDown.mockClear();
  mockEngine.available = true;
  warmth.reset();
});

afterEach(async () => {
  // Each test's hooks go before the next test's store.
  await cleanup();
  warmth.reset();
  jest.useRealTimers();
});

describe('warming the plugin mockEngine', () => {
  // Fails if a server with plugins doesn't warm the mockEngine, or warms it
  // again on a re-render.
  it('opening a server with plugins warms up once', async () => {
    const hook = await renderHook(({ key }: { key: string }) => useEngineWarmUp(key), { initialProps: { key: WITH_PLUGINS } });
    await hook.rerender({ key: WITH_PLUGINS });
    expect(mockEngine.warmUp).toHaveBeenCalledTimes(1);
  });

  it("a server without plugins doesn't", async () => {
    await renderHook(() => useEngineWarmUp(PLAIN));
    expect(mockEngine.warmUp).not.toHaveBeenCalled();
  });

  // Fails if being in a call doesn't warm the mockEngine (an activity is one
  // tap away).
  it('joining a call warms up', async () => {
    const stop = watchCallWarmth();
    await act(async () => inCall(true));
    expect(useAppStore.getState().myVoice).not.toBeNull();
    expect(mockEngine.warmUp).toHaveBeenCalledTimes(1);
    stop();
  });

  // Fails if the spare mockEngine is dropped at once (leaving and coming back
  // would start cold) or never.
  it('it cools down 60 s after the last reason goes, and not before', async () => {
    const stop = watchCallWarmth();
    const hook = await renderHook(() => useEngineWarmUp(WITH_PLUGINS));
    await act(async () => inCall(true));
    await hook.unmount();
    await act(async () => jest.advanceTimersByTime(COOL_DOWN_MS));
    expect(mockEngine.coolDown).not.toHaveBeenCalled();
    await act(async () => inCall(false));
    await act(async () => jest.advanceTimersByTime(COOL_DOWN_MS - 1));
    expect(mockEngine.coolDown).not.toHaveBeenCalled();
    await act(async () => jest.advanceTimersByTime(1));
    expect(mockEngine.coolDown).toHaveBeenCalledTimes(1);
    expect(COOL_DOWN_MS).toBe(60_000);
    stop();
  });

  it('a reason coming back cancels the cool-down', async () => {
    const first = await renderHook(() => useEngineWarmUp(WITH_PLUGINS));
    const warmed = mockEngine.warmUp.mock.calls.length;
    expect(warmed).toBe(1);
    await first.unmount();
    await act(async () => jest.advanceTimersByTime(COOL_DOWN_MS / 2));
    await renderHook(() => useEngineWarmUp(WITH_PLUGINS));
    await act(async () => jest.advanceTimersByTime(COOL_DOWN_MS));
    expect(mockEngine.coolDown).not.toHaveBeenCalled();
    // Still warm: not made again.
    expect(mockEngine.warmUp).toHaveBeenCalledTimes(warmed);
  });

  // Fails if one of two holders of the same reason (two screens of one
  // server during a transition) letting go cools the engine for the other.
  it('a reason held twice stays until both let go', async () => {
    warmth.hold('server:a');
    warmth.hold('server:a');
    warmth.release('server:a');
    await act(async () => jest.advanceTimersByTime(COOL_DOWN_MS));
    expect(mockEngine.coolDown).not.toHaveBeenCalled();
    warmth.release('server:a');
    await act(async () => jest.advanceTimersByTime(COOL_DOWN_MS));
    expect(mockEngine.coolDown).toHaveBeenCalledTimes(1);
  });

  // Fails if a build without the renderer is asked to warm up.
  it('nothing happens without the renderer', async () => {
    mockEngine.available = false;
    await renderHook(() => useEngineWarmUp(WITH_PLUGINS));
    expect(mockEngine.warmUp).not.toHaveBeenCalled();
  });
});
