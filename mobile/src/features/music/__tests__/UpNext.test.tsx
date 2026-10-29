import { cleanup, fireEvent, render } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import {
  ChannelAccess,
  ConnectionRecord,
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
import * as confirmModule from '@/ui/confirm';

import { UpNext } from '../UpNext';

async function renderScreen(ui: React.ReactElement) {
  return await render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

function makeQueueItem(id: string, requesterId: string, title: string): MusicQueueItemRecord {
  return {
    itemId: id,
    track: {
      title,
      artist: 'Artist',
      durationMs: 200_000,
      isStream: false,
      isSeekable: true,
      source: 'youtube',
      sourceDisplayName: 'YouTube',
      uri: `https://example.com/${id}`,
      artworkId: `art-${id}`,
      removable: true,
    },
    requesterId,
    addedAtMs: 1_700_000_000_000,
  };
}

function makeSession(queue: MusicQueueItemRecord[] = []): MusicSessionRecord {
  return {
    channelId: CHANNELS.lounge,
    version: 1,
    state: MusicPlayState.Playing,
    nowPlaying: makeQueueItem('now', '1001', 'Playing Track'),
    queue,
    history: [],
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

function initUpNextServer({
  accessOverrides = {},
  queue = [],
}: {
  accessOverrides?: Partial<ChannelAccess>;
  queue?: MusicQueueItemRecord[];
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
    ...accessOverrides,
  };

  const updatedSnapshot = {
    ...snap,
    access: snap.access.map((a) => (a.channelId === CHANNELS.lounge ? baseAccess : a)),
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
        snapshot: updatedSnapshot,
        seeding: false,
        readyCount: 1,
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
      [channelKey(NIGHT_OWLS, CHANNELS.lounge)]: makeSession(queue),
    },
    order: [NIGHT_OWLS],
  }));
}

describe('UpNext', () => {
  beforeEach(() => {
    freshStore();
    jest.clearAllMocks();
  });

  afterEach(async () => {
    cleanup();
    await settle();
  });

  test('an empty queue says what to do', async () => {
    initUpNextServer({ queue: [] });
    const view = await renderScreen(<UpNext serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    expect(view.getByText('Nothing queued')).toBeTruthy();
    expect(view.getByText('Search for a song or upload one.')).toBeTruthy();
  });

  test('swipe to remove needs the right permission (own track with music_add, any track with music_manage_queue)', async () => {
    const ownItem = makeQueueItem('q-own', SELF_ID, 'My Track');
    const otherItem = makeQueueItem('q-other', '1001', "Kai's Track");

    // With musicAdd only: can remove own, not other's
    initUpNextServer({
      accessOverrides: { musicAdd: true, musicManageQueue: false },
      queue: [ownItem, otherItem],
    });

    const view1 = await renderScreen(<UpNext serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    expect(view1.queryByTestId('swipe-remove-q-own')).toBeTruthy();
    expect(view1.queryByTestId('swipe-remove-q-other')).toBeNull();
    view1.unmount();
    await settle();

    // With musicManageQueue: can remove both
    initUpNextServer({
      accessOverrides: { musicAdd: true, musicManageQueue: true },
      queue: [ownItem, otherItem],
    });

    const removeSpy = jest.spyOn(musicActions, 'removeItems').mockResolvedValue(undefined);
    const view2 = await renderScreen(<UpNext serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    expect(view2.queryByTestId('swipe-remove-q-own')).toBeTruthy();
    const otherRemoveBtn = view2.getByTestId('swipe-remove-q-other');
    expect(otherRemoveBtn).toBeTruthy();

    await fireEvent.press(otherRemoveBtn);
    expect(removeSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, ['q-other']);
  });

  test('the menu offers Play next, Move up, Move down, Remove', async () => {
    const item1 = makeQueueItem('q-1', '1001', 'Track 1');
    const item2 = makeQueueItem('q-2', '1002', 'Track 2');
    const item3 = makeQueueItem('q-3', '1003', 'Track 3');

    initUpNextServer({
      accessOverrides: { musicAdd: true, musicManageQueue: true },
      queue: [item1, item2, item3],
    });

    const moveSpy = jest.spyOn(musicActions, 'moveItem').mockResolvedValue(undefined);
    const removeSpy = jest.spyOn(musicActions, 'removeItems').mockResolvedValue(undefined);

    const view = await renderScreen(<UpNext serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);

    // Item 2 menu
    const menu = view.getByTestId('queue-row-menu-q-2');
    expect(menu).toBeTruthy();

    // Move up
    await fireEvent(menu, 'action', 'move-up');
    expect(moveSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, 'q-2', 'q-1');

    // Move down
    await fireEvent(menu, 'action', 'move-down');
    expect(moveSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, 'q-2', undefined);

    // Play next
    await fireEvent(menu, 'action', 'play-next');
    expect(moveSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, 'q-2', 'q-1');

    // Remove
    await fireEvent(menu, 'action', 'remove');
    expect(removeSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, ['q-2']);
  });

  test('clearing asks first', async () => {
    const item1 = makeQueueItem('q-1', '1001', 'Track 1');
    initUpNextServer({
      accessOverrides: { musicAdd: true, musicManageQueue: true },
      queue: [item1],
    });

    const confirmSpy = jest.spyOn(confirmModule, 'confirm').mockImplementation(({ onConfirm }) => {
      // Simulate confirming
      onConfirm();
    });
    const clearSpy = jest.spyOn(musicActions, 'clearQueue').mockResolvedValue(undefined);

    const view = await renderScreen(<UpNext serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);
    const clearBtn = view.getByLabelText('Clear queue');
    expect(clearBtn).toBeTruthy();

    await fireEvent.press(clearBtn);
    expect(confirmSpy).toHaveBeenCalledWith(
      expect.objectContaining({
        title: 'Clear the queue?',
        destructive: true,
      }),
    );
    expect(clearSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge);
  });
});
