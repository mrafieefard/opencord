import { fireEvent, render, screen } from '@testing-library/react-native';
import { ConnectionRecord, MobileError, MobileEvent, type ActivitySessionRecord, type PluginActivityRecord } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { ActivityPicker, showActivitiesButton } from '../ActivityPicker';

jest.mock('expo-router', () => ({ router: { back: jest.fn(), push: jest.fn() } }));
const mockOpenCall = jest.fn();
jest.mock('@/features/voice/callPosition', () => ({
  ...jest.requireActual<object>('@/features/voice/callPosition'),
  openCall: (...args: unknown[]) => mockOpenCall(...args),
}));

const CHESS: PluginActivityRecord = {
  pluginId: 'chess',
  activityKey: 'chess',
  name: 'Chess',
  description: '',
  icon: '',
  pluginName: 'Board games',
};

/** Night Owls with Paint together and Chess, the device in Lounge. */
async function ready(activities: PluginActivityRecord[] = [...snapshot(NIGHT_OWLS).pluginActivities, CHESS]) {
  const core = freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: { ...snapshot(NIGHT_OWLS), pluginActivities: activities } }),
  ]);
  await settle();
  return core;
}

function session(id: string, pluginId: string, activityKey: string): ActivitySessionRecord {
  return {
    id,
    channelId: CHANNELS.lounge,
    pluginId,
    activityKey,
    name: activityKey,
    icon: '',
    pluginName: pluginId,
    starterId: SELF_ID,
    startedAtMs: 1n,
    viewerIds: [],
  };
}

async function picker() {
  const onClose = jest.fn();
  await render(<ActivityPicker serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onClose={onClose} />);
  return onClose;
}

describe('the Activities button', () => {
  // Fails if the button shows without an activity, the permission or the
  // renderer, or hides when all three hold.
  it('the button shows only with an activity, the permission and the renderer', async () => {
    await ready();
    const server = () => useAppStore.getState().servers[NIGHT_OWLS];
    expect(showActivitiesButton(server(), CHANNELS.lounge, true)).toBe(true);
    expect(showActivitiesButton(server(), CHANNELS.lounge, false)).toBe(false);
    await ready([]);
    expect(showActivitiesButton(server(), CHANNELS.lounge, true)).toBe(false);
    await ready();
    const current = server();
    const base = current?.snapshot;
    if (!current || !base) throw new Error('ready');
    useAppStore.setState((state) => ({
      servers: {
        ...state.servers,
        [NIGHT_OWLS]: {
          ...current,
          snapshot: {
            ...base,
            access: base.access.map((entry) => (entry.channelId === CHANNELS.lounge ? { ...entry, startActivities: false } : entry)),
          },
        },
      },
    }));
    expect(showActivitiesButton(server(), CHANNELS.lounge, true)).toBe(false);
  });
});

describe('the activity picker', () => {
  it('lists each activity by plugin, then name, with its description or plugin name', async () => {
    await ready();
    await picker();
    expect(screen.getByText('Activities')).toBeTruthy();
    const names = screen.getAllByTestId(/^activity-row-/).map((row) => row.props.testID as string);
    expect(names).toEqual(['activity-row-chess/chess', 'activity-row-paint/paint']);
    expect(screen.getByText('Draw with everyone in the call')).toBeTruthy();
    expect(screen.getByText('Board games')).toBeTruthy();
  });

  // Fails if the start goes to the wrong channel, plugin or key (two
  // activities), or the sheet stays open.
  it("pressing a row starts it in the call's channel", async () => {
    const core = await ready();
    const onClose = await picker();
    await fireEvent.press(screen.getByText('Paint together'));
    await settle();
    expect(core.calls.filter((call) => call.method === 'activityStart').map((call) => call.args)).toEqual([
      [NIGHT_OWLS, CHANNELS.lounge, 'paint', 'paint'],
    ]);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  // Fails if a second tap while the start is on its way closes the sheet
  // again: the second `router.back()` would leave the server screen too.
  it('a double tap starts and closes once', async () => {
    const core = await ready();
    const onClose = await picker();
    await fireEvent.press(screen.getByText('Paint together'));
    await fireEvent.press(screen.getByText('Paint together'));
    await settle();
    expect(core.calls.filter((call) => call.method === 'activityStart')).toHaveLength(1);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  // Fails if the started session isn't the one opened (Task 15).
  it('starting opens what came back', async () => {
    const core = await ready();
    const start = jest.spyOn(core, 'activityStart');
    await picker();
    await fireEvent.press(screen.getByText('Paint together'));
    await settle();
    const started = (await start.mock.results[0]?.value) as ActivitySessionRecord;
    expect(useAppStore.getState().openActivity).toEqual({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge, sessionId: started.id });
    // The call comes up to show it (Android put it away for the sheet).
    expect(mockOpenCall).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge);
  });

  it('a running activity says Running', async () => {
    await ready();
    onEvents([new MobileEvent.ActivitySessionCreated({ serverKey: NIGHT_OWLS, session: session('70', 'paint', 'paint') })]);
    await settle();
    await picker();
    expect(screen.getByText('Running')).toBeTruthy();
  });

  // Fails if a fifth activity can be asked for (the server refuses), or
  // the line doesn't say why.
  it('a full call disables the rest', async () => {
    await ready();
    onEvents(
      ['70', '71', '72', '73'].map(
        (id, n) => new MobileEvent.ActivitySessionCreated({ serverKey: NIGHT_OWLS, session: session(id, n === 0 ? 'paint' : `other${n}`, n === 0 ? 'paint' : `other${n}`) }),
      ),
    );
    await settle();
    const onClose = await picker();
    expect(screen.getByText('This call already has 4 activities.')).toBeTruthy();
    await fireEvent.press(screen.getByText('Chess'));
    expect(onClose).not.toHaveBeenCalled();
    await fireEvent.press(screen.getByText('Paint together'));
    await settle();
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it.each([
    ['plugin_unavailable', "This activity isn't available right now."],
    ['plugin_limit', 'This call already has 4 activities.'],
    ['forbidden', "You don't have permission to start activities here."],
  ])('a refused start says why (%s)', async (code, words) => {
    const core = await ready();
    core.failNext('activityStart', new MobileError.Server({ code, message: code, retryAfterMs: undefined }));
    await picker();
    await fireEvent.press(screen.getByText('Paint together'));
    await settle();
    expect(useAppStore.getState().toasts.at(-1)?.text).toBe(words);
    expect(useAppStore.getState().openActivity).toBeNull();
  });
});
