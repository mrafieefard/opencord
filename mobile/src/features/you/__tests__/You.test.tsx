import { cleanup, fireEvent, render, screen } from '@testing-library/react-native';
import * as Clipboard from 'expo-clipboard';
import * as Sharing from 'expo-sharing';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { File, Paths } from 'expo-file-system';
import { NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { saveIdentity } from '@/core/identity';
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
    (Sharing.shareAsync as jest.Mock).mockClear();
  });

  afterEach(() => {
    cleanup();
  });

  it('the header shows the name and fingerprint, and a tap copies the fingerprint', async () => {
    useAppStore.setState({
      self: {
        displayName: 'Ava',
        fingerprint: 'ab12cd34ef567890',
        publicKeyHex: '0123456789abcdef',
      },
    });

    await renderScreen(<YouScreen />);

    expect(screen.getAllByText('Ava').length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText('ab12cd34ef567890')).toBeTruthy();

    await fireEvent.press(screen.getByLabelText('Copy fingerprint'));
    await settle();

    const clipboardText = await Clipboard.getStringAsync();
    expect(clipboardText).toBe('ab12cd34ef567890');
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

  it("backing up writes the file and opens the share sheet: the file's text equals identityBackupEncode's", async () => {
    const core = freshStore();
    const secret = new Uint8Array(32).fill(42).buffer;
    await saveIdentity(core, secret, 'Ava');

    await renderScreen(<YouScreen />);

    await fireEvent.press(screen.getByLabelText('Back up your identity'));
    await settle();

    const expectedBackup = core.identityBackupEncode(secret);
    const file = new File(Paths.cache, 'opencord-identity.txt');
    expect(await file.text()).toBe(expectedBackup);

    expect(Sharing.shareAsync).toHaveBeenCalledWith(
      file.uri,
      expect.objectContaining({ dialogTitle: 'Back up your identity' }),
    );
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
});
