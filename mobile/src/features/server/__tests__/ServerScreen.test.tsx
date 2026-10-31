import { fireEvent, render, screen } from '@testing-library/react-native';
import { Dimensions } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, RUST_CLUB, snapshot } from '@/core/fake/world';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { ServerScreen } from '../ServerScreen';

const mockBack = jest.fn();
const mockSetOptions = jest.fn();
jest.mock('expo-router', () => ({
  router: { back: () => mockBack(), push: jest.fn(), replace: jest.fn() },
  useNavigation: () => ({ setOptions: mockSetOptions }),
  useFocusEffect: (effect: () => undefined | (() => void)) =>
    jest.requireActual<typeof import('react')>('react').useEffect(effect, [effect]),
}));

function windowWidth(width: number) {
  const size = { width, height: 900, scale: 2, fontScale: 1 };
  Dimensions.set({ window: size, screen: size });
}

async function renderServer(channelId?: string) {
  freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    new MobileEvent.Connection({ serverKey: RUST_CLUB, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(RUST_CLUB) }),
  ]);
  await settle();
  await render(
    <GestureHandlerRootView style={{ flex: 1 }}>
      <ServerScreen serverKey={NIGHT_OWLS} channelId={channelId} />
    </GestureHandlerRootView>,
  );
}

const lastGestureEnabled = () => {
  const calls = mockSetOptions.mock.calls.filter(([options]) => 'gestureEnabled' in options);
  return calls[calls.length - 1]?.[0].gestureEnabled;
};

describe('ServerScreen', () => {
  beforeEach(() => {
    jest.clearAllMocks();
  });

  it('the sidebar lists only this server\'s channels, never servers', async () => {
    windowWidth(412);
    await renderServer(CHANNELS.general);
    await fireEvent.press(screen.getByLabelText('Show channels'));

    expect(screen.getByText('Night Owls')).toBeTruthy();
    expect(screen.getByLabelText('All channels')).toBeTruthy();
    for (const name of ['general', 'music-requests', 'off-topic', 'Lounge', 'Quiet room']) {
      expect(screen.getByRole('button', { name })).toBeTruthy();
    }
    expect(screen.getByRole('button', { name: 'general' }).props.accessibilityState).toMatchObject({ selected: true });
    expect(screen.queryByText('Rust Club')).toBeNull();
    expect(screen.queryByText('Weekend Raid')).toBeNull();
  });

  it('the stack\'s swipe back is on only in channels', async () => {
    windowWidth(412);
    await renderServer();
    expect(lastGestureEnabled()).toBe(true);

    await fireEvent.press(screen.getByRole('button', { name: 'general' }));
    expect(lastGestureEnabled()).toBe(false);
    expect(screen.getByLabelText('Show channels')).toBeTruthy();

    await fireEvent.press(screen.getByLabelText('Show channels'));
    expect(lastGestureEnabled()).toBe(false);

    await fireEvent.press(screen.getByLabelText('All channels'));
    expect(lastGestureEnabled()).toBe(true);

    await fireEvent.press(screen.getByLabelText('Servers'));
    expect(mockBack).toHaveBeenCalledTimes(1);
  });

  it('the tab bar steps aside only while a text channel fills the screen', async () => {
    windowWidth(412);
    await renderServer();
    expect(useAppStore.getState().chatInFront).toBe(false);

    await fireEvent.press(screen.getByRole('button', { name: 'general' }));
    expect(useAppStore.getState().chatInFront).toBe(true);

    await fireEvent.press(screen.getByLabelText('Show channels'));
    expect(useAppStore.getState().chatInFront).toBe(false);
  });

  it('at tablet width the list and the content sit side by side', async () => {
    windowWidth(1024);
    await renderServer();
    expect(lastGestureEnabled()).toBe(true);
    expect(screen.getByText('Choose a channel')).toBeTruthy();
    expect(screen.queryByLabelText('Show channels')).toBeNull();

    await fireEvent.press(screen.getByRole('button', { name: 'general' }));
    expect(screen.queryByText('Choose a channel')).toBeNull();
    expect(screen.getByText('The usual chatter')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Lounge' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'general' }).props.accessibilityState).toMatchObject({ selected: true });
    expect(lastGestureEnabled()).toBe(true);

    await fireEvent.press(screen.getByLabelText('Servers'));
    expect(mockBack).toHaveBeenCalledTimes(1);
  });
});
