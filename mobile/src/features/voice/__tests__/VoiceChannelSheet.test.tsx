import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { router } from 'expo-router';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import type { FakeCore } from '@/core/fake/FakeCore';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { ServerScreen } from '@/features/server/ServerScreen';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { VoiceChannelSheet } from '../VoiceChannelSheet';

jest.mock('expo-router', () => ({
  router: { back: jest.fn(), push: jest.fn(), replace: jest.fn() },
  useNavigation: () => ({ setOptions: jest.fn() }),
  useFocusEffect: (effect: () => undefined | (() => void)) =>
    jest.requireActual<typeof import('react')>('react').useEffect(effect, [effect]),
}));

async function ready(): Promise<FakeCore> {
  const core = freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
  ]);
  await settle();
  return core;
}

async function pump(core: FakeCore): Promise<void> {
  onEvents(await core.nextEvents(core.eventsAttach(), 100));
}

describe('a voice channel as a sheet', () => {
  beforeEach(() => jest.clearAllMocks());

  it("shows who is in it and joins, then closes", async () => {
    const core = await ready();
    await render(<VoiceChannelSheet serverKey={NIGHT_OWLS} channelId={CHANNELS.lounge} />);

    expect(screen.getByText('Lounge')).toBeTruthy();
    expect(screen.getByText('2 in voice')).toBeTruthy();
    expect(screen.getByText('Kai')).toBeTruthy();
    expect(screen.getByLabelText('Mira, muted')).toBeTruthy();

    await act(async () => {
      fireEvent.press(screen.getByLabelText('Join Voice'));
    });
    await pump(core);
    expect(useAppStore.getState().myVoice).toMatchObject({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge });
    expect(router.back).toHaveBeenCalled();
  });

  it('an empty channel says so', async () => {
    await ready();
    await render(<VoiceChannelSheet serverKey={NIGHT_OWLS} channelId={CHANNELS.quiet} />);
    expect(screen.getByText("No one's here yet")).toBeTruthy();
  });

  it('a voice channel opens the sheet; the one this device is in opens the call', async () => {
    const core = await ready();
    await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ServerScreen serverKey={NIGHT_OWLS} />
      </GestureHandlerRootView>,
    );
    fireEvent.press(screen.getByRole('button', { name: 'Lounge' }));
    expect(router.push).toHaveBeenCalledWith(`/server/${NIGHT_OWLS}/voice/${CHANNELS.lounge}`);

    jest.mocked(router.push).mockClear();
    await core.voiceJoin(NIGHT_OWLS, CHANNELS.lounge);
    await pump(core);
    fireEvent.press(screen.getByRole('button', { name: 'Lounge' }));
    expect(router.push).not.toHaveBeenCalled();
    expect(useAppStore.getState().call).toEqual({ serverKey: NIGHT_OWLS, channelId: CHANNELS.lounge });
  });
});
