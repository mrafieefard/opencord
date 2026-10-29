import { cleanup, fireEvent, render } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import {
  ChannelAccess,
  ConnectionRecord,
  MusicAction,
  MusicPlayState,
  MusicRepeat,
  type MusicSessionRecord,
  type MusicSettingsRecord,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import * as musicActions from '@/state/actions/music';
import * as prefsActions from '@/state/actions/prefs';
import * as voiceActions from '@/state/actions/voice';
import { useAppStore } from '@/state/store';
import { channelKey } from '@/state/types';
import { freshStore, settle } from '@/test/store';

import { MusicPlayer } from '../MusicPlayer';

jest.mock('expo-router', () => ({
  router: { back: jest.fn(), push: jest.fn() },
}));

async function renderScreen(ui: React.ReactElement) {
  return await render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

function makeSession(overrides: Partial<MusicSessionRecord> = {}): MusicSessionRecord {
  return {
    channelId: CHANNELS.lounge,
    version: 1,
    state: MusicPlayState.Playing,
    nowPlaying: {
      itemId: 'item-1',
      track: {
        title: 'Sunflower',
        artist: 'Post Malone',
        durationMs: 158_000,
        isStream: false,
        isSeekable: true,
        source: 'youtube',
        sourceDisplayName: 'YouTube',
        uri: 'https://example.com/sunflower',
        artworkId: 'art-sunflower',
        removable: true,
      },
      requesterId: '1001',
      addedAtMs: 1_700_000_000_000,
    },
    queue: [],
    history: [],
    positionMs: 45_000,
    anchorMs: 1_700_000_000_000,
    repeat: MusicRepeat.Off,
    shuffle: false,
    waitingForNode: false,
    pausedEmpty: false,
    skipVotes: [],
    skipVotesNeeded: 0,
    ...overrides,
  };
}

function initMusicServer({
  inVoice = true,
  accessOverrides = {},
  session = makeSession(),
  settingsOverrides = {},
}: {
  inVoice?: boolean;
  accessOverrides?: Partial<ChannelAccess>;
  session?: MusicSessionRecord | null;
  settingsOverrides?: Partial<MusicSettingsRecord>;
} = {}) {
  const snap = snapshot(NIGHT_OWLS);
  const baseAccess: ChannelAccess = {
    channelId: CHANNELS.lounge,
    view: true,
    readHistory: true,
    send: true,
    attach: true,
    connect: true,
    speak: true,
    manageMessages: false,
    musicAdd: true,
    musicControl: true,
    musicManageQueue: true,
    musicManage: false,
    ...accessOverrides,
  };

  const defaultSettings: MusicSettingsRecord = {
    supported: true,
    enabled: true,
    configured: true,
    hasNodes: true,
    localSongs: true,
    availableSources: ['youtube', 'soundcloud'],
    defaultSource: 'youtube',
    allowLinks: true,
    voteSkip: false,
    ...settingsOverrides,
  };

  const voiceStates = inVoice
    ? [{ userId: SELF_ID, channelId: CHANNELS.lounge, thisDevice: true, selfMute: false, selfDeaf: false, serverMute: false, serverDeaf: false }]
    : [];

  const voiceMap = Object.fromEntries(voiceStates.map((v) => [v.userId, v]));

  useAppStore.setState((s) => ({
    servers: {
      ...s.servers,
      [NIGHT_OWLS]: {
        key: NIGHT_OWLS,
        entry: { key: NIGHT_OWLS, name: 'Night Owls', host: '127.0.0.1', port: 9000, userId: SELF_ID },
        connection: new ConnectionRecord.Connected(),
        snapshot: {
          ...snap,
          access: snap.access.map((a) => (a.channelId === CHANNELS.lounge ? baseAccess : a)),
        },
        seeding: false,
        readyCount: 1,
        held: [],
        activity: {},
        typing: {},
        voice: voiceMap,
        nowPlaying: {},
        musicSettings: defaultSettings,
      },
    },
    order: [NIGHT_OWLS],
    music: session ? { [channelKey(NIGHT_OWLS, CHANNELS.lounge)]: session } : {},
  }));
}

describe('MusicPlayer', () => {
  beforeEach(async () => {
    freshStore();
    await settle();
    jest.clearAllMocks();
    jest.spyOn(musicActions, 'subscribeMusic').mockResolvedValue();
    jest.spyOn(musicActions, 'unsubscribeMusic').mockResolvedValue();
  });

  afterEach(() => {
    cleanup();
  });

  it('it subscribes when opened and unsubscribes when closed', async () => {
    initMusicServer();
    const subSpy = jest.spyOn(musicActions, 'subscribeMusic').mockResolvedValue();
    const unsubSpy = jest.spyOn(musicActions, 'unsubscribeMusic').mockResolvedValue();

    const view = await renderScreen(
      <MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    expect(subSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge);
    view.unmount();
    await settle();
    expect(unsubSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge);
  });

  it('play pauses and resumes', async () => {
    initMusicServer({ session: makeSession({ state: MusicPlayState.Playing }) });
    const playbackSpy = jest.spyOn(musicActions, 'playback').mockResolvedValue();

    let view = await renderScreen(
      <MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const playPauseBtn = view.getByLabelText('Pause');
    await fireEvent.press(playPauseBtn);
    expect(playbackSpy).toHaveBeenCalledWith(
      NIGHT_OWLS,
      CHANNELS.lounge,
      MusicAction.Pause,
      undefined,
      'item-1',
    );
    view.unmount();
    await settle();

    // When paused, pressing it resumes
    initMusicServer({ session: makeSession({ state: MusicPlayState.Paused }) });
    view = await renderScreen(
      <MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const resumeBtn = view.getByLabelText('Play');
    await fireEvent.press(resumeBtn);
    expect(playbackSpy).toHaveBeenCalledWith(
      NIGHT_OWLS,
      CHANNELS.lounge,
      MusicAction.Resume,
      undefined,
      'item-1',
    );
  });

  it("controls the user can't use stay visible and say why", async () => {
    initMusicServer({ accessOverrides: { musicControl: false } });

    const view = await renderScreen(
      <MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const prevBtn = view.getByLabelText('Previous');
    await fireEvent.press(prevBtn);

    // Toast shown
    const toasts = useAppStore.getState().toasts;
    expect(toasts.some((t) => t.text === 'You need Control music for this.')).toBe(true);
    view.unmount();
    await settle();
  });

  it('next becomes Vote to skip without control when vote skip is on', async () => {
    initMusicServer({
      accessOverrides: { musicControl: false },
      settingsOverrides: { voteSkip: true },
      session: makeSession({ skipVotes: ['1002'], skipVotesNeeded: 3 }),
    });

    const voteSkipSpy = jest.spyOn(musicActions, 'voteSkip').mockResolvedValue();

    const view = await renderScreen(
      <MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const voteBtn = view.getByLabelText('Vote to skip (1/3)');
    expect(voteBtn).toBeTruthy();

    await fireEvent.press(voteBtn);
    expect(voteSkipSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, 'item-1');
    view.unmount();
    await settle();
  });

  it('repeat goes off, queue, track', async () => {
    const modeSpy = jest.spyOn(musicActions, 'setMode').mockResolvedValue();

    // From Off -> Queue
    initMusicServer({ session: makeSession({ repeat: MusicRepeat.Off }) });
    let view = await renderScreen(<MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    const repeatBtn = view.getByLabelText('Repeat');
    await fireEvent.press(repeatBtn);
    expect(modeSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, MusicRepeat.Queue, false);
    view.unmount();
    await settle();

    // From Queue -> Track
    initMusicServer({ session: makeSession({ repeat: MusicRepeat.Queue }) });
    view = await renderScreen(<MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    await fireEvent.press(view.getByLabelText('Repeat'));
    expect(modeSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, MusicRepeat.Track, false);
    view.unmount();
    await settle();

    // From Track -> Off
    initMusicServer({ session: makeSession({ repeat: MusicRepeat.Track }) });
    view = await renderScreen(<MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    await fireEvent.press(view.getByLabelText('Repeat'));
    expect(modeSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, MusicRepeat.Off, false);
    view.unmount();
    await settle();
  });

  it('seeking is offered only on a seekable track with control', async () => {
    initMusicServer({ accessOverrides: { musicControl: true } });
    const playbackSpy = jest.spyOn(musicActions, 'playback').mockResolvedValue();

    const view = await renderScreen(
      <MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const slider = view.getByLabelText('Progress');
    expect(slider).toBeTruthy();

    fireEvent(slider, 'onChange', 60_000);
    await settle();

    expect(playbackSpy).toHaveBeenCalledWith(
      NIGHT_OWLS,
      CHANNELS.lounge,
      MusicAction.Seek,
      60_000,
      'item-1',
    );
    view.unmount();
    await settle();
  });

  it('the volume applies through musicSetListening and is kept', async () => {
    initMusicServer();
    const setListeningSpy = jest.spyOn(prefsActions, 'setMusicListening').mockImplementation(() => {});

    const view = await renderScreen(
      <MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    const volumeSlider = view.getByLabelText('Music volume');
    await fireEvent(volumeSlider, 'onChange', 1.2);

    expect(setListeningSpy).toHaveBeenCalledWith(expect.objectContaining({ volume: 1.2 }));
    view.unmount();
    await settle();
  });

  it('outside the channel it offers Join', async () => {
    initMusicServer({ inVoice: false });
    const joinSpy = jest.spyOn(voiceActions, 'joinVoice').mockResolvedValue(true);

    const view = await renderScreen(
      <MusicPlayer serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />,
    );

    expect(view.getByText('Listen by joining Lounge')).toBeTruthy();
    const joinBtn = view.getByText('Join');
    await fireEvent.press(joinBtn);
    expect(joinSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge);
    view.unmount();
    await settle();
  });
});
