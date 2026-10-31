import { fireEvent, render } from '@testing-library/react-native';
import { Alert } from 'react-native';
import * as Clipboard from 'expo-clipboard';
import {
  ConnectionRecord,
  MobileEvent,
  type ChannelAccess,
} from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import { onEvents } from '@/state/store';
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

    const { getByText } = await render(<ChannelList serverKey={NIGHT_OWLS} variant="full" onSelect={jest.fn()} />);

    expect(getByText('Midnight City — M83')).toBeTruthy();
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

  it('the header menu marks all read and leaves the server', async () => {
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();

    const onBack = jest.fn();
    const { getByText, getByTestId } = await render(<ChannelListHeader serverKey={NIGHT_OWLS} backLabel="Servers" onBack={onBack} />);

    expect(getByText('Night Owls')).toBeTruthy();

    // Menu contains Mark all as read, Copy address, Leave server
    const menu = getByTestId('channel-list-menu');
    expect(menu).toBeTruthy();

    // Trigger Mark all as read
    fireEvent(menu, 'onAction', 'mark-read');
    await settle();

    // Trigger Copy address
    fireEvent(menu, 'onAction', 'copy-address');
    expect(Clipboard.setStringAsync).toHaveBeenCalled();

    // Trigger Leave server
    fireEvent(menu, 'onAction', 'leave');
    expect(Alert.alert).toHaveBeenCalledWith(
      'Leave Night Owls?',
      'You can come back with an invite.',
      expect.any(Array),
    );
  });
});
