import { cleanup, fireEvent, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';
import { NameScreen } from '../NameScreen';
import { YouScreen } from '../YouScreen';

const mockBack = jest.fn();
const mockRouter = {
  back: () => mockBack(),
  push: jest.fn(),
};

jest.mock('expo-router', () => ({
  router: mockRouter,
  useRouter: () => mockRouter,
  useNavigation: () => ({
    setOptions: jest.fn(),
  }),
}));

async function renderScreen(ui: React.ReactElement) {
  return await render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

describe('You tab', () => {
  beforeEach(() => {
    freshStore();
    mockBack.mockClear();
  });

  afterEach(() => {
    cleanup();
  });

  it('the Profile card shows the name and status, and navigates to /you/profile', async () => {
    mockRouter.push.mockClear();
    useAppStore.setState({
      self: {
        displayName: 'Ava',
        fingerprint: 'ab12cd34ef567890',
        publicKeyHex: '0123456789abcdef',
      },
    });

    await renderScreen(<YouScreen />);

    expect(screen.getByText('Ava')).toBeTruthy();
    expect(screen.getByText('Online')).toBeTruthy();

    await fireEvent.press(screen.getByLabelText('Profile'));
    expect(mockRouter.push).toHaveBeenCalledWith('/you/profile');
  });

  it('changing the name saves it and sends it to every connected server', async () => {
    const core = freshStore();
    const serverB = {
      key: 'server-b',
      name: 'Server B',
      host: '127.0.0.1',
      port: 7710,
      userId: 'user-1',
    };
    // Add serverB to fake core servers so requireServer doesn't reject it
    (core as unknown as { servers: unknown[] }).servers.push(serverB);

    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
      new MobileEvent.Connection({ serverKey: 'server-b', state: new ConnectionRecord.Connected() }),
    ]);
    await settle();

    await renderScreen(<NameScreen />);

    const input = screen.getByLabelText('Display name');
    expect(input.props.value).toBe('Ava');

    await fireEvent.changeText(input, 'Avery');
    await fireEvent.press(screen.getByLabelText('Save'));
    await settle();

    expect(mockBack).toHaveBeenCalled();
    expect(useAppStore.getState().self?.displayName).toBe('Avery');

    const updateCalls = core.calls.filter((c) => c.method === 'profileUpdate');
    expect(updateCalls.map((c) => c.args)).toEqual([
      [NIGHT_OWLS, 'Avery'],
      ['server-b', 'Avery'],
    ]);
  });

  it('the theme choice is kept and applied', async () => {
    const core = freshStore();
    await renderScreen(<YouScreen />);

    expect(useAppStore.getState().prefs.theme).toBe('system');

    await fireEvent.press(screen.getByLabelText('Theme Dark'));
    await settle();

    expect(useAppStore.getState().prefs.theme).toBe('dark');
    expect(core.settingsGet('mobile.theme')).toBe('dark');
  });

  it('the music volume shows a percentage and applies through musicSetListening', async () => {
    const core = freshStore();
    await renderScreen(<YouScreen />);

    expect(screen.getByText('50 %')).toBeTruthy();

    const slider = screen.getByLabelText('Music volume');
    fireEvent(slider, 'onChange', 1.5);
    await settle();

    expect(screen.getByText('150 %')).toBeTruthy();
    expect(useAppStore.getState().prefs.music.volume).toBe(1.5);

    const muteSwitch = screen.getByLabelText('Mute music');
    await fireEvent(muteSwitch, 'onChange', true);
    await settle();

    expect(useAppStore.getState().prefs.music.muted).toBe(true);

    const duckSwitch = screen.getByLabelText('Lower music while people talk');
    await fireEvent(duckSwitch, 'onChange', true);
    await settle();

    expect(useAppStore.getState().prefs.music.duck).toBe(true);

    const setListeningCalls = core.calls.filter((c) => c.method === 'musicSetListening');
    const lastCall = setListeningCalls[setListeningCalls.length - 1];
    expect(lastCall?.args).toEqual([1.5, true, true]);
  });

  // Fails if the row is missing or opens anything but Voice & audio's route (app/(tabs)/you/voice.tsx).
  it('the Voice & audio row opens its page', async () => {
    freshStore();
    mockRouter.push.mockClear();
    await renderScreen(<YouScreen />);
    await fireEvent.press(screen.getByLabelText('Voice & audio'));
    await fireEvent.press(screen.getByLabelText('Voice & audio'));
    expect(mockRouter.push.mock.calls).toEqual([['/you/voice'], ['/you/voice']]);
  });
});
