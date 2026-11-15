import { act, fireEvent, render, renderHook } from '@testing-library/react-native';
import { Alert } from 'react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { router } from 'expo-router';
import {
  ConnectionRecord,
  MobileEvent,
  type ChannelAccess,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import { BAR_HEIGHT, PILL_GAP, useListEndRoom } from '@/features/voice/callPosition';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { ChannelList } from '../ChannelList';
import { ChannelListHeader } from '../ChannelListHeader';

jest.mock('expo-router', () => ({
  router: { back: jest.fn(), push: jest.fn() },
}));

jest.spyOn(Alert, 'alert');

describe('ChannelList', () => {
  beforeEach(() => {
    freshStore();
    jest.clearAllMocks();
  });

  // Fails if a list's last rows can't scroll out from under the tab bar,
  // or (in voice, iOS) the floating call bar above it: iOS doesn't inset a
  // list that isn't its screen's own for the bar, so the list leaves both.
  it('a list under the tab bar leaves room for it, and the call bar in voice', async () => {
    // A tab's safe area takes the tab bar in: 83 at its foot, as an iPhone's.
    const metrics = { frame: { x: 0, y: 0, width: 402, height: 874 }, insets: { top: 62, left: 0, right: 0, bottom: 83 } };
    const room = await renderHook(() => useListEndRoom(), {
      wrapper: ({ children }) => <SafeAreaProvider initialMetrics={metrics}>{children}</SafeAreaProvider>,
    });
    expect(room.result.current).toBe(83);
    await act(async () => useAppStore.setState({ myVoice: { serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge, muted: false, deafened: false } as never }));
    expect(room.result.current).toBe(83 + BAR_HEIGHT + PILL_GAP);
  });

  it('channels without a category come first, then each category by position', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();

    const onSelect = jest.fn();
    const { getByText } = await render(<ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={onSelect} />);

    // Uncategorized channels: general, music-requests
    // Categories: TEXT (with off-topic), VOICE (with Lounge, Quiet room)
    expect(getByText('general')).toBeTruthy();
    expect(getByText('music-requests')).toBeTruthy();
    expect(getByText('TEXT')).toBeTruthy();
    expect(getByText('off-topic')).toBeTruthy();
    expect(getByText('VOICE')).toBeTruthy();
    expect(getByText('Lounge')).toBeTruthy();
    expect(getByText('Quiet room')).toBeTruthy();
  });

  it('unread channels are bold with the pill, and mentions show a count', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
      new MobileEvent.MessageCreated({
        serverKey: NIGHT_OWLS,
        message: {
          id: '999999',
          channelId: CHANNELS.general,
          authorId: '1001',
          content: `<@${SELF_ID}> check this out`,
          createdAtMs: Date.now(),
          attachments: [],
        },
      }),
    ]);
    await settle();

    const { getByTestId, getByText } = await render(<ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={jest.fn()} />);

    expect(getByTestId('channel-unread-pill')).toBeTruthy();
    expect(getByTestId('channel-mentions-badge')).toBeTruthy();
    expect(getByText('1')).toBeTruthy();
  });

  it('voice channels list their people with speaking rings and mute glyphs', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
      new MobileEvent.Speaking({
        serverKey: NIGHT_OWLS,
        channelId: CHANNELS.lounge,
        changes: [{ userId: '1001', speaking: true }],
      }),
    ]);
    await settle();

    const { getByText, getByTestId } = await render(<ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={jest.fn()} />);

    // Kai is speaking in Lounge
    expect(getByText('Kai')).toBeTruthy();
    expect(getByTestId('speaking-ring')).toBeTruthy();

    // Mira is muted in Lounge
    expect(getByText('Mira')).toBeTruthy();
    expect(getByTestId('member-muted')).toBeTruthy();
  });

  it('a voice channel shows what plays', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
      new MobileEvent.MusicNowPlaying({
        serverKey: NIGHT_OWLS,
        nowPlaying: {
          channelId: CHANNELS.lounge,
          title: 'Midnight City',
          artist: 'M83',
          artworkId: '',
          playing: true,
          ended: false,
          sourceDisplayName: 'opencord',
        },
      }),
    ]);
    await settle();

    const { findByText } = await render(<ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={jest.fn()} />);

    // The row is past the list's first ten (the plugin channels came
    // before it): it draws in a later batch.
    expect(await findByText('Midnight City — M83')).toBeTruthy();
  });

  it('channels the user can\'t view are left out', async () => {
    const snap = snapshot(NIGHT_OWLS);
    const restrictedSnapshot = {
      ...snap,
      access: snap.access.map((entry: ChannelAccess) =>
        entry.channelId === CHANNELS.musicRequests ? { ...entry, view: false } : entry,
      ),
    };

    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: restrictedSnapshot }),
    ]);
    await settle();

    const { queryByText, getByText } = await render(<ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={jest.fn()} />);

    expect(queryByText('music-requests')).toBeNull();
    expect(getByText('general')).toBeTruthy();
  });

  it('the open channel is highlighted in the sidebar', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();

    const { getByRole } = await render(
      <ChannelList
        serverKey={NIGHT_OWLS}
        variant="sidebar"
        selectedId={CHANNELS.general}
        onSelect={jest.fn()}
      />,
    );

    const generalRow = getByRole('button', { name: 'general' });
    expect(generalRow.props.accessibilityState).toEqual({ selected: true });

    const offTopicRow = getByRole('button', { name: 'off-topic' });
    expect(offTopicRow.props.accessibilityState).toEqual({ selected: false });
  });

  // The header's old menu (Mark all as read, Copy address, Leave server)
  // became the server sheet (design §6.1): Mark as read and Leave are
  // tested there (features/serverMenu); Copy address stays in the servers
  // list's long-press menu.
  // Fails if the name stops opening the sheet, or the old menu comes back.
  it('the header opens the server sheet from the name, and has no menu of its own', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();

    const { getByRole, queryByTestId } = await render(<ChannelListHeader serverKey={NIGHT_OWLS} backLabel="Servers" onBack={jest.fn()} />);

    expect(queryByTestId('channel-list-menu')).toBeNull();
    await fireEvent.press(getByRole('button', { name: 'Night Owls' }));
    await fireEvent.press(getByRole('button', { name: 'Night Owls' }));
    expect(jest.mocked(router.push).mock.calls).toEqual([['/server/fake-night-owls/settings'], ['/server/fake-night-owls/settings']]);
  });
});
