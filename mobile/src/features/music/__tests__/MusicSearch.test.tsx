import { cleanup, fireEvent, render } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import {
  ChannelAccess,
  ConnectionRecord,
  MusicPlacement,
  type MusicSearchResultRecord,
  type MusicSettingsRecord,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import * as musicActions from '@/state/actions/music';
import { useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { MusicSearch } from '../MusicSearch';

async function renderScreen(ui: React.ReactElement) {
  return await render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

function initSearchServer({
  accessOverrides = {},
  settingsOverrides = {},
}: {
  accessOverrides?: Partial<ChannelAccess>;
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
    order: [NIGHT_OWLS],
  }));
}

const sampleResults: MusicSearchResultRecord[] = [
  {
    resultId: 'res-1',
    track: {
      title: 'Local Jam',
      artist: 'Band',
      durationMs: 180_000,
      isStream: false,
      isSeekable: true,
      source: 'local',
      sourceDisplayName: 'Local',
      uri: '',
      artworkId: 'art-local',
      localSongId: 'local-1',
      removable: true,
    },
  },
  {
    resultId: 'res-2',
    track: {
      title: 'YouTube Hit',
      artist: 'Singer',
      durationMs: 210_000,
      isStream: false,
      isSeekable: true,
      source: 'youtube',
      sourceDisplayName: 'YouTube',
      uri: 'https://youtube.com/watch?v=123',
      artworkId: 'art-yt',
      localSongId: undefined,
      removable: true,
    },
  },
];

describe('MusicSearch', () => {
  const onCancel = jest.fn();

  beforeEach(() => {
    freshStore();
    jest.clearAllMocks();
  });

  afterEach(async () => {
    cleanup();
    await settle();
  });

  test('search runs on submit, not while typing', async () => {
    initSearchServer();
    const searchSpy = jest.spyOn(musicActions, 'search').mockResolvedValue(sampleResults);

    const view = await renderScreen(<MusicSearch serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onCancel={onCancel} />);
    const input = view.getByPlaceholderText('Search songs or paste a link');
    expect(input).toBeTruthy();

    // Typing does not trigger search
    await fireEvent.changeText(input, 'local');
    await settle();
    expect(searchSpy).not.toHaveBeenCalled();

    // Submitting triggers search
    await fireEvent(input, 'submitEditing');
    await settle();
    expect(searchSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.lounge, 'local', 'youtube');

    view.unmount();
    await settle();
  });

  test('lists results with the Local pill', async () => {
    initSearchServer();
    jest.spyOn(musicActions, 'search').mockResolvedValue(sampleResults);

    const view = await renderScreen(<MusicSearch serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onCancel={onCancel} />);
    const input = view.getByPlaceholderText('Search songs or paste a link');
    await fireEvent.changeText(input, 'test');
    await settle();
    await fireEvent(input, 'submitEditing');
    await settle();

    expect(view.getByText('Local Jam')).toBeTruthy();
    expect(view.getByText('YouTube Hit')).toBeTruthy();
    expect(view.getByText('Local')).toBeTruthy(); // Local pill

    view.unmount();
    await settle();
  });

  test('tapping adds to the end', async () => {
    initSearchServer({ accessOverrides: { musicAdd: true } });
    jest.spyOn(musicActions, 'search').mockResolvedValue(sampleResults);
    const enqueueSpy = jest.spyOn(musicActions, 'enqueue').mockResolvedValue(undefined);

    const view = await renderScreen(<MusicSearch serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onCancel={onCancel} />);
    const input = view.getByPlaceholderText('Search songs or paste a link');
    await fireEvent.changeText(input, 'test');
    await settle();
    await fireEvent(input, 'submitEditing');
    await settle();

    const row = view.getByText('Local Jam');
    await fireEvent.press(row);
    await settle();

    expect(enqueueSpy).toHaveBeenCalledWith(
      NIGHT_OWLS,
      CHANNELS.lounge,
      ['res-1'],
      [],
      undefined,
      MusicPlacement.End,
    );
    // The row's button turns into a tick once the server has the song.
    expect(view.getByLabelText('Local Jam is queued')).toBeTruthy();

    view.unmount();
    await settle();
  });

  test('a long press offers Play next and Play now (with music_manage_queue)', async () => {
    initSearchServer({ accessOverrides: { musicAdd: true, musicManageQueue: true } });
    jest.spyOn(musicActions, 'search').mockResolvedValue(sampleResults);
    const enqueueSpy = jest.spyOn(musicActions, 'enqueue').mockResolvedValue(undefined);

    const view = await renderScreen(<MusicSearch serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onCancel={onCancel} />);
    const input = view.getByPlaceholderText('Search songs or paste a link');
    await fireEvent.changeText(input, 'test');
    await settle();
    await fireEvent(input, 'submitEditing');
    await settle();

    const menu = view.getByTestId('search-result-menu-res-1');
    expect(menu).toBeTruthy();

    await fireEvent(menu, 'action', 'play-next');
    await settle();
    expect(enqueueSpy).toHaveBeenCalledWith(
      NIGHT_OWLS,
      CHANNELS.lounge,
      ['res-1'],
      [],
      undefined,
      MusicPlacement.Next,
    );

    await fireEvent(menu, 'action', 'play-now');
    await settle();
    expect(enqueueSpy).toHaveBeenCalledWith(
      NIGHT_OWLS,
      CHANNELS.lounge,
      ['res-1'],
      [],
      undefined,
      MusicPlacement.Now,
    );

    view.unmount();
    await settle();
  });

  test('a server error shows its message', async () => {
    initSearchServer();
    jest.spyOn(musicActions, 'search').mockRejectedValue(new Error('a search is 1 to 200 characters'));

    const view = await renderScreen(<MusicSearch serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onCancel={onCancel} />);
    const input = view.getByPlaceholderText('Search songs or paste a link');
    await fireEvent.changeText(input, 'x'.repeat(201));
    await settle();
    await fireEvent(input, 'submitEditing');
    await settle();

    expect(view.getByText('a search is 1 to 200 characters')).toBeTruthy();

    view.unmount();
    await settle();
  });

  test('a blank search asks nothing of the server', async () => {
    initSearchServer();
    const searchSpy = jest.spyOn(musicActions, 'search').mockResolvedValue([]);

    const view = await renderScreen(<MusicSearch serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onCancel={onCancel} />);
    const input = view.getByPlaceholderText('Search songs or paste a link');
    await fireEvent.changeText(input, '   ');
    await fireEvent(input, 'submitEditing');
    await settle();

    expect(searchSpy).not.toHaveBeenCalled();
    expect(view.getByText('Search YouTube')).toBeTruthy();
    view.unmount();
    await settle();
  });

  test('another source searches the same words there', async () => {
    initSearchServer();
    const searchSpy = jest.spyOn(musicActions, 'search').mockResolvedValue([]);

    const view = await renderScreen(<MusicSearch serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onCancel={onCancel} />);
    const input = view.getByPlaceholderText('Search songs or paste a link');
    await fireEvent.changeText(input, 'night owl');
    await fireEvent(input, 'submitEditing');
    await settle();
    expect(view.getByText('No results for “night owl”')).toBeTruthy();

    await fireEvent.press(view.getByLabelText('SoundCloud'));
    await settle();

    expect(searchSpy).toHaveBeenLastCalledWith(NIGHT_OWLS, CHANNELS.lounge, 'night owl', 'soundcloud');
    view.unmount();
    await settle();
  });

  test('Cancel leaves search', async () => {
    initSearchServer();

    const view = await renderScreen(<MusicSearch serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onCancel={onCancel} />);
    await fireEvent.press(view.getByLabelText('Cancel'));

    expect(onCancel).toHaveBeenCalled();
    view.unmount();
    await settle();
  });
});
