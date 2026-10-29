import { fireEvent, render } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import {
  ConnectionRecord,
  VoiceLinkState,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import * as voiceActions from '@/state/actions/voice';
import { useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { tileLayout, VoicePane } from '../VoicePane';
import { LinkBanner } from '../LinkBanner';
import { ParticipantSheet } from '../ParticipantSheet';

jest.mock('expo-router', () => ({
  router: { back: jest.fn(), push: jest.fn() },
}));

jest.mock('expo-audio', () => ({
  getRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestRecordingPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  requestNotificationPermissionsAsync: jest.fn(async () => ({ granted: true, canAskAgain: true, status: 'granted' })),
  setAudioModeAsync: jest.fn(async () => {}),
}));

function renderWithGesture(ui: React.ReactElement) {
  return render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

function initServer(snap = snapshot(NIGHT_OWLS)) {
  const voiceMap = snap.voiceStates
    ? Object.fromEntries(snap.voiceStates.map((v) => [v.userId, v]))
    : {};
  useAppStore.setState((s) => ({
    servers: {
      ...s.servers,
      [NIGHT_OWLS]: {
        key: NIGHT_OWLS,
        entry: { key: NIGHT_OWLS, name: 'Night Owls', host: '127.0.0.1', port: 9000, userId: SELF_ID },
        connection: new ConnectionRecord.Connected(),
        snapshot: snap,
        seeding: false,
        readyCount: 1,
        held: [],
        activity: {},
        typing: {},
        voice: voiceMap,
        nowPlaying: {},
        musicSettings: null,
      },
    },
    order: [NIGHT_OWLS],
  }));
}

describe('VoicePane', () => {
  beforeEach(async () => {
    freshStore();
    await settle();
    jest.clearAllMocks();
  });

  it('not connected, it shows who is there and Join voice', async () => {
    const snap = snapshot(NIGHT_OWLS);
    initServer(snap);

    const joinSpy = jest.spyOn(voiceActions, 'joinVoice').mockResolvedValue(true);

    const { getByText } = await renderWithGesture(
      <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    expect(getByText('Kai')).toBeTruthy();
    const joinBtn = getByText('Join voice');
    expect(joinBtn).toBeTruthy();

    await fireEvent.press(joinBtn);
    expect(joinSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge);
  });

  it('join is off without connect, saying why', async () => {
    const snap = snapshot(NIGHT_OWLS);
    snap.access = snap.access.map((a) =>
      a.channelId === CHANNELS.lounge ? { ...a, connect: false } : a,
    );
    initServer(snap);

    const { getByText } = await renderWithGesture(
      <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    expect(getByText("You can't join this channel")).toBeTruthy();
  });

  it('a full channel says so', async () => {
    const snap = snapshot(NIGHT_OWLS);
    const channel = snap.channels.find((c) => c.id === CHANNELS.lounge);
    if (!channel) throw new Error('missing lounge');
    channel.userLimit = 2; // Kai and Mira already in lounge
    initServer(snap);

    const { getByText } = await renderWithGesture(
      <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    expect(getByText('This channel is full')).toBeTruthy();
  });

  it('connected, it shows the controls, and mute toggles', async () => {
    const snap = snapshot(NIGHT_OWLS);
    initServer(snap);
    useAppStore.setState({
      myVoice: {
        serverKey: NIGHT_OWLS,
        channelId: CHANNELS.lounge,
        link: new VoiceLinkState.Connected(),
        muted: false,
        deafened: false,
      },
    });

    const setMutedSpy = jest.spyOn(voiceActions, 'setMuted').mockImplementation(() => {});

    const { getByTestId } = await renderWithGesture(
      <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const muteBtn = getByTestId('control-mute');
    expect(muteBtn).toBeTruthy();
    expect(getByTestId('control-deafen')).toBeTruthy();
    expect(getByTestId('control-music')).toBeTruthy();
    expect(getByTestId('control-leave')).toBeTruthy();

    await fireEvent.press(muteBtn);
    expect(setMutedSpy).toHaveBeenCalledWith(true);
  });

  it('speaking rings follow the store', async () => {
    const snap = snapshot(NIGHT_OWLS);
    initServer(snap);
    useAppStore.setState((s) => ({
      speaking: {
        ...s.speaking,
        [`${NIGHT_OWLS}/${CHANNELS.lounge}`]: ['1001'],
      },
    }));

    const { getAllByTestId } = await renderWithGesture(
      <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const rings = getAllByTestId('avatar-ring');
    expect(rings.length).toBeGreaterThan(0);
    const hasSpeaking = rings.some((ring) => {
      const flat = Array.isArray(ring.props.style) ? Object.assign({}, ...ring.props.style) : ring.props.style;
      return flat?.opacity === 1;
    });
    expect(hasSpeaking).toBe(true);
  });

  it("the link banner shows each state's words, and Disconnected offers Rejoin", async () => {
    const onRejoin = jest.fn();

    const screen = await renderWithGesture(<LinkBanner link={new VoiceLinkState.AwaitingEndpoint()} onRejoin={onRejoin} />);
    expect(screen.getByText('Connecting…')).toBeTruthy();

    await screen.rerender(<GestureHandlerRootView style={{ flex: 1 }}><LinkBanner link={new VoiceLinkState.Reconnecting()} onRejoin={onRejoin} /></GestureHandlerRootView>);
    expect(screen.getByText('Reconnecting…')).toBeTruthy();

    await screen.rerender(<GestureHandlerRootView style={{ flex: 1 }}><LinkBanner link={new VoiceLinkState.NoRoute()} onRejoin={onRejoin} /></GestureHandlerRootView>);
    expect(screen.getByText("Can't reach the voice server. Your network may block it.")).toBeTruthy();

    await screen.rerender(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <LinkBanner link={new VoiceLinkState.Disconnected({ reason: 'timeout' })} onRejoin={onRejoin} />
      </GestureHandlerRootView>,
    );
    expect(screen.getByText('Disconnected from voice.')).toBeTruthy();
    const rejoinBtn = screen.getByText('Rejoin');
    await fireEvent.press(rejoinBtn);
    expect(onRejoin).toHaveBeenCalled();

    await screen.rerender(<GestureHandlerRootView style={{ flex: 1 }}><LinkBanner link={new VoiceLinkState.Connected()} onRejoin={onRejoin} /></GestureHandlerRootView>);
    expect(screen.queryByText('Connecting…')).toBeNull();
  });

  it('the person sheet sets the volume and mute for me', async () => {
    const snap = snapshot(NIGHT_OWLS);
    initServer(snap);

    const setVolumeSpy = jest.spyOn(voiceActions, 'setUserVolume').mockImplementation(() => {});
    const setMuteSpy = jest.spyOn(voiceActions, 'setUserLocalMute').mockImplementation(() => {});

    const screen = await renderWithGesture(
      <ParticipantSheet serverKey={NIGHT_OWLS} userId="1001" />,
    );

    expect(screen.getAllByText('Kai').length).toBeGreaterThan(0);
    expect(screen.getByText('Volume')).toBeTruthy();
    expect(screen.getByText('Mute for me')).toBeTruthy();

    const slider = screen.getByLabelText('Volume');
    await fireEvent(slider, 'valueChange', 1.5);
    expect(setVolumeSpy).toHaveBeenCalledWith(NIGHT_OWLS, '1001', 1.5);

    const muteSwitch = screen.getByRole('switch');
    await fireEvent.press(muteSwitch);
    expect(setMuteSpy).toHaveBeenCalledWith(NIGHT_OWLS, '1001', true);
    await settle();
  });

  it('your own sheet says This is you', async () => {
    const snap = snapshot(NIGHT_OWLS);
    initServer(snap);

    const screen = await renderWithGesture(
      <ParticipantSheet serverKey={NIGHT_OWLS} userId={SELF_ID} />,
    );

    expect(screen.getByText('This is you.')).toBeTruthy();
    expect(screen.queryByText('Mute for me')).toBeNull();
    expect(screen.queryByText('Volume')).toBeNull();
  });

  it('the person sheet opens on what was set before', async () => {
    jest.restoreAllMocks();
    initServer();
    voiceActions.setUserVolume(NIGHT_OWLS, '1001', 0.5);
    voiceActions.setUserLocalMute(NIGHT_OWLS, '1001', true);

    const screen = await renderWithGesture(<ParticipantSheet serverKey={NIGHT_OWLS} userId="1001" />);

    expect(screen.getByText('50 %')).toBeTruthy();
    expect(screen.getByRole('switch').props.accessibilityState).toMatchObject({ checked: true });
    // Someone else keeps the defaults.
    await screen.rerender(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ParticipantSheet serverKey={NIGHT_OWLS} userId="1002" />
      </GestureHandlerRootView>,
    );
    expect(screen.queryByText('50 %')).toBeNull();
    expect(screen.getByRole('switch').props.accessibilityState).toMatchObject({ checked: false });
  });

  it('tiles are two columns on a phone and more on a tablet', () => {
    expect(tileLayout(412).columns).toBe(2);
    expect(tileLayout(320).columns).toBe(2);
    const tablet = tileLayout(960);
    expect(tablet.columns).toBeGreaterThan(2);
    expect(tablet.size).toBeLessThanOrEqual(200);
  });

  it('without a connected bar of its own, the pane shows none while in voice elsewhere', async () => {
    initServer();
    useAppStore.setState({
      myVoice: { serverKey: NIGHT_OWLS, channelId: CHANNELS.quiet, link: new VoiceLinkState.Connected(), muted: false, deafened: false },
    });

    const tablet = await renderWithGesture(<VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    expect(tablet.queryByTestId('connected-bar-pressable')).toBeNull();
    await tablet.rerender(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onOpenChannel={() => {}} />
      </GestureHandlerRootView>,
    );
    expect(tablet.getByTestId('connected-bar-pressable')).toBeTruthy();
  });

  it('fetches the music settings, and shows Music when the server has it on', async () => {
    initServer();

    const view = await renderWithGesture(<VoicePane serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    await settle();

    expect(useAppStore.getState().servers[NIGHT_OWLS]?.musicSettings?.enabled).toBe(true);
    expect(view.getByLabelText('Music')).toBeTruthy();
  });
});
