import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { AppState, BackHandler, type AppStateStatus, type NativeEventSubscription } from 'react-native';

import { FakeCore } from '@/core/fake/FakeCore';
import { FakeArea } from '@/test/fakeArea';

import { NavigationContext } from 'expo-router/react-navigation';

import { PluginArea } from '../PluginArea';
import { swipeBackHeld } from '../swipeBack';
import { startViewPump } from '../viewPump';
import { ViewSession, type AreaStatus, type ViewPlace } from '../viewSession';

// The native view, visible to the test: the real one draws nothing in jest.
jest.mock('@opencord/plugin-area', () => {
  const { View } = jest.requireActual<typeof import('react-native')>('react-native');
  return {
    ...jest.requireActual<object>('@opencord/plugin-area'),
    PluginAreaView: ({ areaId }: { areaId: string }) => <View testID={`area-view-${areaId}`} />,
  };
});

const flush = () => act(() => new Promise<void>((resolve) => setImmediate(resolve)));
const place: ViewPlace = { kind: 'channel', serverKey: 'night-owls', channelId: '13' };

async function world() {
  const core = new FakeCore({ scenario: 'empty' });
  const area = new FakeArea();
  const pump = startViewPump(core, area);
  const session = new ViewSession({ core, pump, area, place, setupJson: '{}' });
  await session.start();
  const onLeave = jest.fn();
  const onReload = jest.fn();
  const view = (paused = false) => (
    <PluginArea session={session} pluginId="tickets" paused={paused} onLeave={onLeave} onReload={onReload} />
  );
  const rendered = await render(view());
  const status = async (next: Partial<AreaStatus>) => {
    area.status(session.areaId, { state: 'running', reason: '', detail: '', overlays: 0, ...next });
    await flush();
  };
  return { core, area, session, onLeave, onReload, view, rendered, status };
}

/** The input kinds the area's session sent the core, as lifecycle events. */
function lifecycle(core: FakeCore): string[] {
  return core.calls
    .filter((call) => call.method === 'viewInput')
    .map((call) => JSON.parse(call.args[1] as string) as { kind: string; event?: string })
    .filter((input) => input.kind === 'lifecycle')
    .map((input) => input.event ?? '');
}

describe('the plugin area', () => {
  // One test per row of the states table: the words React Native draws in
  // the area's place. Fails if a state shows the wrong words or the view.
  it.each([
    [{ state: 'error', detail: '' }, 'This plugin hit an error'],
    [{ state: 'too_old' }, 'Update Opencord to open this'],
    [{ state: 'closed', reason: 'plugin_stopped' }, 'This channel is unavailable'],
    [{ state: 'closed', reason: 'lost_access' }, 'This channel is unavailable'],
    [{ state: 'closed', reason: 'channel_deleted' }, 'This channel is unavailable'],
    [{ state: 'closed', reason: 'unavailable' }, 'This channel is unavailable'],
    [{ state: 'closed', reason: 'activity_ended' }, 'This activity ended'],
    [{ state: 'closed', reason: 'something_new' }, 'This plugin closed this screen'],
  ] as const)('%j shows %s', async (status, words) => {
    const w = await world();
    await w.status(status as Partial<AreaStatus>);
    expect(screen.getByText(words)).toBeTruthy();
    expect(screen.getByTestId('plugin-area-cover')).toBeTruthy();
  });

  it('loading shows a spinner', async () => {
    await world();
    expect(screen.getByTestId('plugin-area-loading')).toBeTruthy();
  });

  // Fails if an error's detail isn't shown, or Reload doesn't ask for a
  // new session.
  it('an error shows its detail and Reload', async () => {
    const w = await world();
    await w.status({ state: 'error', detail: 'boom at line 3' });
    expect(screen.getByText('boom at line 3')).toBeTruthy();
    await fireEvent.press(screen.getByText('Reload'));
    expect(w.onReload).toHaveBeenCalledTimes(1);
  });

  it('a closed screen with no known reason offers Reload; left_call shows nothing', async () => {
    const w = await world();
    await w.status({ state: 'closed', reason: 'something_new' });
    expect(screen.getByText('Reload')).toBeTruthy();
    await w.status({ state: 'closed', reason: 'left_call' });
    expect(screen.queryByText('Reload')).toBeNull();
    expect(screen.queryByText('This plugin closed this screen')).toBeNull();
  });

  // Fails if the native view waits for `running` to mount: with no view the
  // engine is paused and lays nothing out, so the renderer never sends the
  // context that opens the view (found live: a spinner for ever). It's
  // mounted from the start, covered by React Native's states until it runs.
  it('the native view is mounted from the start and covered until it runs', async () => {
    const w = await world();
    const id = `area-view-${w.session.areaId}`;
    expect(screen.getByTestId(id)).toBeTruthy();
    expect(screen.getByTestId('plugin-area-cover')).toBeTruthy();
    await w.status({ state: 'running' });
    expect(screen.getByTestId(id)).toBeTruthy();
    expect(screen.queryByTestId('plugin-area-cover')).toBeNull();
    await w.status({ state: 'error' });
    expect(screen.getByTestId('plugin-area-cover')).toBeTruthy();
  });

  // Fails if unmounting the component destroys the area (an activity's
  // state would be lost when its pane is left), or closing the session
  // doesn't, once.
  it('an area outlives its native view and is destroyed with its session', async () => {
    const w = await world();
    await w.status({ state: 'running' });
    await w.rendered.unmount();
    expect(w.area.methods()).not.toContain('destroyArea');
    w.session.close();
    w.session.close();
    expect(w.area.methods().filter((m) => m === 'destroyArea')).toHaveLength(1);
  });

  // Fails if pause or resume is sent twice for one change, or the
  // background while already paused sends another.
  it('pause and resume are sent once per change', async () => {
    const listeners: ((state: AppStateStatus) => void)[] = [];
    jest.spyOn(AppState, 'addEventListener').mockImplementation((_type, listener) => {
      listeners.push(listener as (state: AppStateStatus) => void);
      return { remove: () => {} } as NativeEventSubscription;
    });
    const w = await world();
    // The renderer's first input opens the view (BlocksView._reportContext).
    w.area.input(w.session.areaId, {
      kind: 'context',
      width: 390,
      height: 700,
      theme: 'dark',
      input: 'touch',
      safe_area: { top: 0, right: 0, bottom: 0, left: 0 },
      text_scale: 1,
    });
    await flush();
    await flush();
    await w.status({ state: 'running' });
    w.core.calls.length = 0;
    await w.rendered.rerender(w.view(true));
    await flush();
    // Rendered again while paused: nothing new to tell.
    await w.rendered.rerender(w.view(true));
    await flush();
    await act(() => listeners.forEach((listener) => listener('background')));
    await flush();
    await act(() => listeners.forEach((listener) => listener('active')));
    await flush();
    await w.rendered.rerender(w.view(false));
    await flush();
    expect(lifecycle(w.core)).toEqual(['pause', 'resume']);
  });

  // Fails if the Android back button leaves before the area is asked, or
  // leaves when the area closed something with it.
  it('back asks the area first', async () => {
    const handlers: ((event: never) => boolean | null | undefined)[] = [];
    jest.spyOn(BackHandler, 'addEventListener').mockImplementation((_type, handler) => {
      handlers.push(handler);
      return { remove: () => {} };
    });
    const w = await world();
    await w.status({ state: 'running' });
    const back = jest.spyOn(w.session, 'back');
    back.mockResolvedValueOnce(true);
    let took: boolean | null | undefined;
    await act(() => {
      took = handlers.at(-1)?.(undefined as never);
    });
    expect(took).toBe(true);
    await flush();
    expect(back).toHaveBeenCalledTimes(1);
    expect(w.onLeave).not.toHaveBeenCalled();
    back.mockResolvedValueOnce(false);
    await act(() => {
      handlers.at(-1)?.(undefined as never);
    });
    await flush();
    expect(w.onLeave).toHaveBeenCalledTimes(1);
  });

  // Fails if the keyboard covers the plugin's sheet and its buttons: the
  // area must shrink above it (padded by keyboardOverlap), not only be told
  // an inset the renderer's sheet ignores; nor may it be lifted twice.
  it('the area shrinks above the keyboard, and its setup inset stays 0', async () => {
    const w = await world();
    await w.status({ state: 'running' });
    // The native view sits in the frame that's padded by the keyboard.
    const view = screen.getByTestId(`area-view-${w.session.areaId}`);
    expect(view.parent?.props.testID).toBe('plugin-area-frame');
    const keyboard = jest.requireMock<{ useKeyboardState: jest.Mock }>('react-native-keyboard-controller').useKeyboardState;
    const up = { isVisible: true, height: 300 };
    keyboard.mockImplementation((selector?: (state: typeof up) => unknown) => (selector ? selector(up) : up));
    await w.rendered.rerender(w.view());
    await flush();
    const setups = w.area.calls.filter((c) => c.method === 'setSetup').map((c) => JSON.parse(c.args[1] as string) as { safe_area: { bottom: number }; plugin_id: string });
    expect(setups.at(-1)?.plugin_id).toBe('tickets');
    expect(setups.every((setup) => setup.safe_area.bottom === 0)).toBe(true);
    keyboard.mockReset();
  });

  // Fails if the iOS edge swipe can leave while the plugin could take back:
  // an open overlay or `app.on_back` holds it; nothing, or a paused or gone
  // area, lets it go.
  it('holds the swipe-back while the plugin could take back', async () => {
    const w = await world();
    await w.status({ state: 'running' });
    expect(swipeBackHeld()).toBe(false);
    await w.status({ state: 'running', overlays: 1 });
    expect(swipeBackHeld()).toBe(true);
    await w.rendered.rerender(w.view(true));
    expect(swipeBackHeld()).toBe(false);
    await w.rendered.rerender(w.view(false));
    expect(swipeBackHeld()).toBe(true);
    await w.status({ state: 'running', overlays: 0, back: true });
    expect(swipeBackHeld()).toBe(true);
    await w.status({ state: 'running', overlays: 0, back: false });
    expect(swipeBackHeld()).toBe(false);
    await w.status({ state: 'running', overlays: 2 });
    await w.rendered.unmount();
    expect(swipeBackHeld()).toBe(false);
  });

  // Fails if Android's back reaches a plugin whose screen has a sheet pushed
  // over it: the sheet's own back must win.
  it('back is not the area’s while its screen is not in front', async () => {
    const handlers: ((event: never) => boolean | null | undefined)[] = [];
    jest.spyOn(BackHandler, 'addEventListener').mockImplementation((_type, handler) => {
      handlers.push(handler);
      return { remove: () => void handlers.splice(handlers.indexOf(handler), 1) };
    });
    const focus = new Set<() => void>();
    let focused = false;
    const navigation = {
      isFocused: () => focused,
      addListener: (event: string, listener: () => void) => {
        const wrapped = () => {
          focused = event === 'focus';
          listener();
        };
        focus.add(wrapped);
        return () => focus.delete(wrapped);
      },
    };
    const w = await world();
    await w.rendered.rerender(
      <NavigationContext.Provider value={navigation as never}>{w.view()}</NavigationContext.Provider>,
    );
    await w.status({ state: 'running' });
    expect(handlers).toHaveLength(0);
  });
});
