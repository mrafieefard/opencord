import { act, cleanup, fireEvent, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { SoundsPage } from '@/features/you/SoundsPage';
import { useAppStore } from '@/state/store';
import { freshStore } from '@/test/store';

const mockPush = jest.fn();
jest.mock('expo-router', () => ({
  useRouter: () => ({ push: mockPush }),
  useNavigation: () => ({ setOptions: jest.fn(), addListener: jest.fn(() => () => {}) }),
  useFocusEffect: (cb: () => void) => cb(),
}));

async function draw() {
  return await render(
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SoundsPage />
    </GestureHandlerRootView>,
  );
}

describe('SoundsPage', () => {
  beforeEach(() => {
    freshStore();
    jest.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  // Fails if any slider or switch does not update prefs or notify the core.
  it('each control does what it shows', async () => {
    const core = freshStore();
    await draw();

    // 1. Soundboard volume slider
    const volumeSlider = screen.getByLabelText('Soundboard volume');
    expect(screen.getByText('100 %')).toBeTruthy();
    await act(async () => {
      fireEvent(volumeSlider, 'onChange', 50);
    });
    expect(useAppStore.getState().prefs.soundboard.volume).toBe(50);
    expect(core.calls.some((c) => c.method === 'soundboardSetListening' && c.args[0] === 0.5 && c.args[1] === false)).toBe(true);

    // 2. Mute all soundboards switch
    const muteSwitch = screen.getByLabelText('Mute all soundboards');
    await act(async () => {
      fireEvent(muteSwitch, 'onChange', true);
    });
    expect(useAppStore.getState().prefs.soundboard.muted).toBe(true);
    expect(core.calls.some((c) => c.method === 'soundboardSetListening' && c.args[0] === 0.5 && c.args[1] === true)).toBe(true);

    // 3. Voice sounds switches
    expect(screen.queryByText('Streams starting and ending')).toBeNull();

    const joinLeaveSwitch = screen.getByLabelText('Joining and leaving');
    expect(useAppStore.getState().prefs.cuesOff).not.toContain('joinLeave');

    // Turn off Joining and leaving
    await act(async () => {
      fireEvent(joinLeaveSwitch, 'onChange', false);
    });
    expect(useAppStore.getState().prefs.cuesOff).toContain('joinLeave');

    // Turn back on
    await act(async () => {
      fireEvent(joinLeaveSwitch, 'onChange', true);
    });
    expect(useAppStore.getState().prefs.cuesOff).not.toContain('joinLeave');
  });
});
