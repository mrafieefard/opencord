import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent, ActivityEndReason, type ActivitySessionRecord } from '@opencord/core-native';

import type { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import { VoicePane } from '@/features/voice/VoicePane';
import { openActivity } from '@/state/actions/plugins';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';
import type { FakeArea } from '@/test/fakeArea';
import * as ui from '@/ui';

import { activityActions } from '../activityActions';
import { ENDED_SHOWN_MS, stageInPane, watchOpenActivity } from '../activityHolder';
import { viewersLine } from '../ActivityTile';

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
jest.mock('@/ui', () => ({ ...jest.requireActual<object>('@/ui'), confirm: jest.fn() }));
jest.mock('expo-audio', () => ({
  getRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestNotificationPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  setAudioModeAsync: jest.fn(async () => {}),
  createAudioPlayer: jest.fn(),
}));

const area = (jest.requireMock('@opencord/plugin-area') as { __fake: FakeArea }).__fake;

const context = {
  kind: 'context',
  width: 390,
  height: 500,
  theme: 'dark',
  input: 'touch',
  safe_area: { top: 0, right: 0, bottom: 0, left: 0 },
  text_scale: 1,
};

function session(id: string, activityKey: string, starterId = SELF_ID, viewerIds: string[] = []): ActivitySessionRecord {
  return {
    id,
    channelId: CHANNELS.lounge,
    pluginId: activityKey,
    activityKey,
    name: activityKey === 'paint' ? 'Paint together' : 'Chess',
    icon: '',
    pluginName: activityKey,
    starterId,
    startedAtMs: BigInt(id),
    viewerIds,
  };
}

function voice(userId: string, inLounge: boolean) {
  return new MobileEvent.VoiceStateChanged({
    serverKey: NIGHT_OWLS,
    state: {
      userId,
      channelId: inLounge ? CHANNELS.lounge : undefined,
      thisDevice: userId === SELF_ID,
      selfMute: false,
      selfDeaf: false,
      serverMute: false,
      serverDeaf: false,
    },
  });
}

let stopWatching: () => void = () => {};

/** Night Owls, this device in Lounge, with the sessions given. */
async function inCall(...sessions: ActivitySessionRecord[]): Promise<FakeCore> {
  const core = freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    voice(SELF_ID, true),
    ...sessions.map((s) => new MobileEvent.ActivitySessionCreated({ serverKey: NIGHT_OWLS, session: s })),
  ]);
  await settle();
  stopWatching = watchOpenActivity();
  return core;
}

async function pane(props: { asCall?: boolean; shown?: boolean } = {}) {
  return render(
    <GestureHandlerRootView style={{ flex: 1 }}>
      <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} {...props} />
    </GestureHandlerRootView>,
  );
}

function lastAreaId(): string {
  const created = area.calls.filter((call) => call.method === 'createArea').at(-1);
  if (!created) throw new Error('no area was created');
  return created.args[0] as string;
}

function inputs(core: FakeCore, kind: string): Record<string, unknown>[] {
  return core.calls
    .filter((call) => call.method === 'viewInput')
    .map((call) => JSON.parse(call.args[1] as string) as Record<string, unknown>)
    .filter((input) => input.kind === kind);
}

/** Opens the session as a tap on its tile does, and lets the renderer start it. */
async function openIt(id: string) {
  await act(async () => openActivity(NIGHT_OWLS, CHANNELS.lounge, id));
  await settle();
  await act(async () => area.input(lastAreaId(), context));
  await settle();
}

beforeEach(() => {
  area.calls.length = 0;
});

afterEach(() => {
  stopWatching();
  stopWatching = () => {};
  jest.useRealTimers();
});

describe('activities in a call', () => {
  it("its tile says who started it and how many have it open (0, 1, 3)", () => {
    expect(viewersLine(0)).toBe('No one has it open');
    expect(viewersLine(1)).toBe('1 person has it open');
    expect(viewersLine(3)).toBe('3 people have it open');
  });

  // Fails if an activity's tile comes before a person's, or is missing.
  it('a session adds a tile after the people', async () => {
    await inCall(session('900', 'paint', '1001', ['1001']));
    await pane();
    const tiles = screen.getAllByRole('button').map((node) => node.props.accessibilityLabel as string);
    const paint = tiles.indexOf('Paint together');
    expect(paint).toBeGreaterThan(-1);
    expect(tiles.indexOf('Ava')).toBeLessThan(paint);
    expect(screen.getByText('Started by Kai')).toBeTruthy();
  });

  // Fails if the tap opens another session, channel or key, or the people
  // don't stay above it.
  it('tapping the tile opens the activity over the stage, the people in a strip above', async () => {
    const core = await inCall(session('900', 'paint'));
    await pane();
    await fireEvent.press(screen.getByLabelText('Paint together'));
    await settle();
    await act(async () => area.input(lastAreaId(), context));
    await settle();
    const opened = core.calls.filter((call) => call.method === 'viewOpenActivity').map((call) => call.args.slice(0, 4));
    expect(opened).toEqual([[NIGHT_OWLS, '900', CHANNELS.lounge, 'paint']]);
    expect(screen.getByTestId('activity-stage')).toBeTruthy();
    expect(screen.getByLabelText('Ava')).toBeTruthy();
  });

  // Fails if Close leaves the view open in the core, ends the session, or
  // doesn't bring the tile back.
  it('Close closes my view and the tile returns', async () => {
    const core = await inCall(session('900', 'paint'));
    await pane();
    await openIt('900');
    await fireEvent.press(screen.getByLabelText('Close'));
    await settle();
    expect(core.calls.filter((call) => call.method === 'viewClose')).toHaveLength(1);
    expect(useAppStore.getState().servers[NIGHT_OWLS]?.snapshot?.activitySessions.map((s) => s.id)).toEqual(['900']);
    expect(screen.queryByTestId('activity-stage')).toBeNull();
    expect(screen.getByLabelText('Paint together')).toBeTruthy();
  });

  it('opening a second activity closes the first', async () => {
    await inCall(session('900', 'paint'), session('901', 'chess'));
    await pane();
    await openIt('900');
    const first = lastAreaId();
    await openIt('901');
    expect(area.calls.filter((call) => call.method === 'destroyArea').map((call) => call.args[0])).toEqual([first]);
  });

  // Fails if End activity shows to someone who may not end it, or ending
  // doesn't ask first.
  it('End activity asks, then ends it for everyone', async () => {
    const core = await inCall(session('900', 'paint'), session('901', 'chess', '1001'));
    const server = useAppStore.getState().servers[NIGHT_OWLS];
    const mineIds = activityActions(server, session('900', 'paint'), false).map((a) => a.id);
    expect(mineIds).toEqual(['open', 'end']);
    expect(activityActions(server, session('901', 'chess', '1001'), false).map((a) => a.id)).toEqual(['open']);
    const confirm = ui.confirm as jest.Mock;
    const { runActivityAction } = jest.requireActual<typeof import('../activityActions')>('../activityActions');
    confirm.mockImplementationOnce((options: ui.ConfirmOptions) => options.onCancel?.());
    runActivityAction(NIGHT_OWLS, session('900', 'paint'), 'end');
    await settle();
    expect(core.calls.some((call) => call.method === 'activityEnd')).toBe(false);
    confirm.mockImplementationOnce((options: ui.ConfirmOptions) => options.onConfirm());
    runActivityAction(NIGHT_OWLS, session('900', 'paint'), 'end');
    await settle();
    expect(core.calls.filter((call) => call.method === 'activityEnd').map((call) => call.args)).toEqual([[NIGHT_OWLS, '900']]);
    expect(confirm.mock.calls.at(-1)?.[0]).toMatchObject({
      title: 'End Paint together for everyone?',
      message: 'It closes for everyone in the call.',
      confirmTitle: 'End activity',
    });
  });

  // Fails if leaving the pane destroys the activity or tells it more than
  // one pause, or coming back makes a new view or skips the resume.
  it("an open activity survives leaving the call's pane", async () => {
    const core = await inCall(session('900', 'paint'));
    const shown = await pane();
    await openIt('900');
    const areaId = lastAreaId();
    const before = core.calls.filter((call) => call.method === 'viewOpenActivity').length;
    await shown.unmount();
    await settle();
    await pane();
    await settle();
    expect(area.methods()).not.toContain('destroyArea');
    expect(lastAreaId()).toBe(areaId);
    expect(core.calls.filter((call) => call.method === 'viewOpenActivity')).toHaveLength(before);
    expect(inputs(core, 'lifecycle').map((input) => input.event)).toEqual(['pause', 'resume']);
  });

  // Fails if the plugin isn't told who is in the call, or a joining member
  // isn't sent.
  it('the call input follows who is in the call', async () => {
    const core = await inCall(session('900', 'paint'));
    await pane();
    await openIt('900');
    // Lounge has Ava (this device), Kai and Mira in the fake world.
    const inLounge = ['1000', '1001', '1002'];
    expect(inputs(core, 'call')).toEqual([{ kind: 'call', members: inLounge, speaking: [] }]);
    await act(async () => onEvents([voice('1003', true)]));
    await act(async () => new Promise((resolve) => setTimeout(resolve, 150)));
    expect(inputs(core, 'call')).toHaveLength(2);
    expect(inputs(core, 'call').at(-1)?.members).toEqual([...inLounge, '1003']);
  });

  it('leaving the call closes the opened activity', async () => {
    const core = await inCall(session('900', 'paint'));
    await pane();
    await openIt('900');
    await act(async () => onEvents([voice(SELF_ID, false)]));
    await settle();
    expect(useAppStore.getState().openActivity).toBeNull();
    expect(core.calls.filter((call) => call.method === 'viewClose')).toHaveLength(1);
  });

  // Fails if an ended activity's screen vanishes without saying so, or
  // stays after the moment it should.
  it('an ended activity closes its screen and its tile goes', async () => {
    await inCall(session('900', 'paint'));
    await pane();
    await openIt('900');
    jest.useFakeTimers();
    await act(async () => {
      onEvents([new MobileEvent.ActivitySessionDeleted({ serverKey: NIGHT_OWLS, sessionId: '900', channelId: CHANNELS.lounge, reason: ActivityEndReason.Ended })]);
      area.status(lastAreaId(), { state: 'closed', reason: 'activity_ended', detail: '', overlays: 0 });
    });
    expect(screen.getByText('This activity ended')).toBeTruthy();
    await act(async () => jest.advanceTimersByTime(ENDED_SHOWN_MS));
    expect(screen.queryByTestId('activity-stage')).toBeNull();
    expect(screen.queryByLabelText('Paint together')).toBeNull();
  });

  it('the voice pane and the call sheet never both draw it', async () => {
    await inCall(session('900', 'paint'));
    await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />
        <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} asCall shown />
      </GestureHandlerRootView>,
    );
    await openIt('900');
    expect(screen.getAllByTestId('activity-stage')).toHaveLength(1);
  });

  // Fails if the phone can't tell that the pane, not the call sheet, draws
  // the activity: the sidebar's swipe must step aside only for the pane's.
  it('says whether the pane draws it', async () => {
    await inCall(session('900', 'paint'));
    const shown = await pane();
    expect(stageInPane()).toBe(false);
    await openIt('900');
    expect(stageInPane()).toBe(true);
    await shown.rerender(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />
        <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} asCall shown />
      </GestureHandlerRootView>,
    );
    await settle();
    expect(stageInPane()).toBe(false);
  });
});
