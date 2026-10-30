import { fireEvent, render } from '@testing-library/react-native';
import {
  ConnectionRecord,
  MobileEvent,
  VoiceLinkState,
} from '@opencord/core-native';
import { router } from 'expo-router';

import { CHANNELS, NIGHT_OWLS, SELF_ID, snapshot } from '@/core/fake/world';
import * as voiceActions from '@/state/actions/voice';
import { onEvents } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { ConnectedBar, ConnectedVoiceRow } from '../ConnectedBar';

jest.mock('expo-router', () => ({
  router: { back: jest.fn(), push: jest.fn() },
}));

/** This device in Lounge on Night Owls. */
async function inLounge(): Promise<void> {
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    new MobileEvent.VoiceStateChanged({
      serverKey: NIGHT_OWLS,
      state: {
        userId: SELF_ID,
        channelId: CHANNELS.lounge,
        thisDevice: true,
        selfMute: false,
        selfDeaf: false,
        serverMute: false,
        serverDeaf: false,
      },
    }),
  ]);
  await settle();
}

describe('ConnectedBar', () => {
  beforeEach(() => {
    freshStore();
    jest.clearAllMocks();
  });

  it('not connected to voice, it renders nothing', async () => {
    const snap = snapshot(NIGHT_OWLS);
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snap }),
    ]);
    await settle();

    const { toJSON } = await render(<ConnectedBar />);
    expect(toJSON()).toBeNull();
  });

  it('the connected bar shows what plays and opens the voice channel', async () => {
    const snap = snapshot(NIGHT_OWLS);
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snap }),
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: {
          userId: SELF_ID,
          channelId: CHANNELS.lounge,
          thisDevice: true,
          selfMute: false,
          selfDeaf: false,
          serverMute: false,
          serverDeaf: false,
        },
      }),
      new MobileEvent.VoiceLink({
        serverKey: NIGHT_OWLS,
        channelId: CHANNELS.lounge,
        state: new VoiceLinkState.Connected(),
      }),
      new MobileEvent.MusicNowPlaying({
        serverKey: NIGHT_OWLS,
        nowPlaying: {
          channelId: CHANNELS.lounge,
          title: 'Sunflower',
          artist: 'Post Malone',
          artworkId: '',
          playing: true,
          ended: false,
          sourceDisplayName: 'YouTube',
        },
      }),
    ]);
    await settle();

    const { getByText, getByTestId } = await render(<ConnectedBar />);

    expect(getByText('Lounge')).toBeTruthy();
    expect(getByText('♪ Sunflower — Post Malone')).toBeTruthy();

    // Tapping the main bar opens the channel
    const barTap = getByTestId('connected-bar-pressable');
    await fireEvent.press(barTap);
    expect(router.push).toHaveBeenCalledWith({
      pathname: '/servers/[serverKey]',
      params: { serverKey: NIGHT_OWLS, channel: CHANNELS.lounge },
    });
  });

  it('on its own server the bar opens the channel in place', async () => {
    await inLounge();
    const openChannel = jest.fn();

    const { getByTestId } = await render(<ConnectedBar serverKey={NIGHT_OWLS} onOpenChannel={openChannel} />);
    await fireEvent.press(getByTestId('connected-bar-pressable'));

    // Not a second server screen on top of this one.
    expect(openChannel).toHaveBeenCalledWith(CHANNELS.lounge);
    expect(router.push).not.toHaveBeenCalled();
  });

  it("on another server's screen the bar opens the voice channel's server", async () => {
    await inLounge();
    const openChannel = jest.fn();

    const { getByTestId } = await render(<ConnectedBar serverKey="other" onOpenChannel={openChannel} />);
    await fireEvent.press(getByTestId('connected-bar-pressable'));

    expect(openChannel).not.toHaveBeenCalled();
    expect(router.push).toHaveBeenCalledWith({
      pathname: '/servers/[serverKey]',
      params: { serverKey: NIGHT_OWLS, channel: CHANNELS.lounge },
    });
  });

  it('beside the voice channel itself the bar hides', async () => {
    await inLounge();
    const { toJSON } = await render(<ConnectedBar serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} onOpenChannel={() => {}} />);
    expect(toJSON()).toBeNull();
  });

  it('tapping mute or leave in the connected bar acts on voice', async () => {
    const snap = snapshot(NIGHT_OWLS);
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snap }),
      new MobileEvent.VoiceStateChanged({
        serverKey: NIGHT_OWLS,
        state: {
          userId: SELF_ID,
          channelId: CHANNELS.lounge,
          thisDevice: true,
          selfMute: false,
          selfDeaf: false,
          serverMute: false,
          serverDeaf: false,
        },
      }),
      new MobileEvent.VoiceLink({
        serverKey: NIGHT_OWLS,
        channelId: CHANNELS.lounge,
        state: new VoiceLinkState.Connected(),
      }),
    ]);
    await settle();

    const setMutedSpy = jest.spyOn(voiceActions, 'setMuted').mockImplementation(() => {});
    const leaveVoiceSpy = jest.spyOn(voiceActions, 'leaveVoice').mockResolvedValue();

    const { getByTestId } = await render(<ConnectedBar />);

    const muteBtn = getByTestId('connected-bar-mute');
    await fireEvent.press(muteBtn);
    expect(setMutedSpy).toHaveBeenCalledWith(true);

    const leaveBtn = getByTestId('connected-bar-leave');
    await fireEvent.press(leaveBtn);
    expect(leaveVoiceSpy).toHaveBeenCalled();
  });

  it('compact, the row keeps the channel and the mic, and still opens the channel', async () => {
    await inLounge();
    const { getByText, queryByText, getByTestId, queryByTestId } = await render(
      <ConnectedVoiceRow serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} muted={false} compact />,
    );
    expect(getByText('Lounge')).toBeTruthy();
    expect(queryByText('Night Owls')).toBeNull();
    expect(getByTestId('connected-bar-mute')).toBeTruthy();
    expect(queryByTestId('connected-bar-leave')).toBeNull();
    await fireEvent.press(getByTestId('connected-bar-pressable'));
    expect(router.push).toHaveBeenCalledWith({ pathname: '/servers/[serverKey]', params: { serverKey: NIGHT_OWLS, channel: CHANNELS.lounge } });
  });
});
