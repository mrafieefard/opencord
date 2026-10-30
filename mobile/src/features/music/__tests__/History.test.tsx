import { cleanup, fireEvent, render } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import {
  ChannelAccess,
  ConnectionRecord,
  MusicPlacement,
  MusicPlayState,
  MusicRepeat,
  type MusicQueueItemRecord,
  type MusicSessionRecord,
  type MusicSettingsRecord,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import * as musicActions from '@/state/actions/music';
import { useAppStore } from '@/state/store';
import { channelKey } from '@/state/types';
import { freshStore, settle } from '@/test/store';

import { History } from '../History';

async function renderScreen(ui: React.ReactElement) {
  return await render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

function makeHistoryItem(id: string, title: string): MusicQueueItemRecord {
  return {
    itemId: id,
    track: {
      title,
      artist: 'History Artist',
      durationMs: 150_000,
      isStream: false,
      isSeekable: true,
      source: 'youtube',
      sourceDisplayName: 'YouTube',
      uri: `https://example.com/h/${id}`,
      artworkId: `art-${id}`,
      removable: true,
    },
    requesterId: '1001',
    addedAtMs: 1_700_000_000_000,
  };
}

function makeSession(history: MusicQueueItemRecord[] = []): MusicSessionRecord {
  return {
    channelId: CHANNELS.lounge,
    version: 1,
    state: MusicPlayState.Playing,
    nowPlaying: makeHistoryItem('now', 'Current Track'),
    queue: [],
    history,
    positionMs: 10_000,
    anchorMs: 1_700_000_000_000,
    repeat: MusicRepeat.Off,
    shuffle: false,
    waitingForNode: false,
    pausedEmpty: false,
    skipVotes: [],
    skipVotesNeeded: 0,
  };
}

function initHistoryServer({
  accessOverrides = {},
  history = [],
}: {
  accessOverrides?: Partial<ChannelAccess>;
  history?: MusicQueueItemRecord[];
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
    musicManageQueue: false,
    musicManage: false,
    useSoundboard: true,
    useExternalSounds: true,
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
  };

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
        access: { [CHANNELS.lounge]: baseAccess },
        held: [],
        activity: {},
        typing: {},
        voice: {
          [SELF_ID]: {
            userId: SELF_ID,
            channelId: CHANNELS.lounge,
            thisDevice: true,
            selfMute: false,
            selfDeaf: false,
            serverMute: false,
            serverDeaf: false,
          },
        },
        nowPlaying: {},
        musicSettings: defaultSettings,
      },
    },
    music: {
      [channelKey(NIGHT_OWLS, CHANNELS.lounge)]: makeSession(history),
    },
    order: [NIGHT_OWLS],
  }));
}

describe('History', () => {
  beforeEach(() => {
    freshStore();
    jest.clearAllMocks();
  });

  afterEach(async () => {
    cleanup();
    await settle();
  });

  test('an empty history shows nothing at all', async () => {
    initHistoryServer({ history: [] });
    const view = await renderScreen(<History serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    expect(view.queryByText('Recently played')).toBeNull();
  });

  test('five songs show until Show all', async () => {
    initHistoryServer({ history: Array.from({ length: 7 }, (_, n) => makeHistoryItem(`h-${n}`, `Past Song ${n}`)) });
    const view = await renderScreen(<History serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    expect(view.getByText('Past Song 4')).toBeTruthy();
    expect(view.queryByText('Past Song 5')).toBeNull();

    await fireEvent.press(view.getByLabelText('Show all recently played songs'));
    expect(view.getByText('Past Song 6')).toBeTruthy();
  });

  test('history adds the track again', async () => {
    const item1 = makeHistoryItem('h-1', 'Past Song 1');
    initHistoryServer({
      accessOverrides: { musicAdd: true },
      history: [item1],
    });

    const enqueueSpy = jest.spyOn(musicActions, 'enqueue').mockResolvedValue(undefined);

    const view = await renderScreen(<History serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    expect(view.getByText('Past Song 1')).toBeTruthy();

    const addAgainBtn = view.getByLabelText('Add Past Song 1 again');
    expect(addAgainBtn).toBeTruthy();

    await fireEvent.press(addAgainBtn);
    expect(enqueueSpy).toHaveBeenCalledWith(
      NIGHT_OWLS,
      CHANNELS.lounge,
      [],
      ['h-1'],
      undefined,
      MusicPlacement.End,
    );
    await settle();
    expect(view.getByLabelText('Past Song 1 is queued')).toBeTruthy();
  });
});
