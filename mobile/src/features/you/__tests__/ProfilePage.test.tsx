import { act, cleanup, fireEvent, render, screen } from '@testing-library/react-native';
import * as Clipboard from 'expo-clipboard';
import * as Sharing from 'expo-sharing';
import { ConnectionRecord, MobileError, MobileEvent, PresenceStatus } from '@opencord/core-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { saveIdentity } from '@/core/identity';
import { pickPhoto } from '@/features/media/pickPhoto';
import { ProfilePage } from '@/features/you/ProfilePage';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

const mockPush = jest.fn();
jest.mock('expo-router', () => ({
  useRouter: () => ({ push: mockPush }),
  useNavigation: () => ({ setOptions: jest.fn(), addListener: jest.fn(() => () => {}) }),
  useFocusEffect: (cb: () => void) => cb(),
}));

jest.mock('@/features/media/pickPhoto', () => ({
  pickPhoto: jest.fn(),
}));

jest.mock('expo-clipboard', () => ({
  setStringAsync: jest.fn().mockResolvedValue(true),
}));

jest.mock('expo-sharing', () => ({
  shareAsync: jest.fn().mockResolvedValue(true),
}));

async function draw() {
  return await render(
    <GestureHandlerRootView style={{ flex: 1 }}>
      <ProfilePage />
    </GestureHandlerRootView>,
  );
}

describe('ProfilePage', () => {
  beforeEach(async () => {
    freshStore();
    mockPush.mockClear();
    jest.clearAllMocks();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();
  });

  afterEach(() => {
    cleanup();
  });

  // Fails if any row or button does not trigger its respective call or action.
  it('each control does what it shows', async () => {
    const core = freshStore();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();

    (pickPhoto as jest.Mock).mockResolvedValue({
      kind: 'picked',
      file: { path: '/tmp/new_avatar.png', filename: 'new_avatar.png', sizeBytes: 1024, mime: 'image/png' },
    });

    await draw();

    // 1. Edit photo button triggers pickPhoto and updates avatar
    const editBtn = screen.getByText('Edit');
    await act(async () => {
      fireEvent.press(editBtn);
    });
    expect(pickPhoto).toHaveBeenCalledWith({ title: 'Profile picture', allowRemove: false });
    expect(core.calls.some((c) => c.method === 'avatarSet' && c.args[0] === '/tmp/new_avatar.png')).toBe(true);

    // 2. Display name row navigates to /you/name
    const nameRow = screen.getByText('Display name');
    await act(async () => {
      fireEvent.press(nameRow);
    });
    expect(mockPush).toHaveBeenCalledWith('/you/name');

    // 3. Status rows: Online, Idle, Do not disturb
    const onlineRow = screen.getByText('Online');
    await act(async () => {
      fireEvent.press(onlineRow);
    });
    expect(useAppStore.getState().prefs.status).toBe('online');
    expect(core.calls.some((c) => c.method === 'presenceUpdate' && c.args[1] === PresenceStatus.Online)).toBe(true);

    const idleRow = screen.getByText('Idle');
    await act(async () => {
      fireEvent.press(idleRow);
    });
    expect(useAppStore.getState().prefs.status).toBe('idle');
    expect(core.calls.some((c) => c.method === 'presenceUpdate' && c.args[1] === PresenceStatus.Idle)).toBe(true);

    const dndRow = screen.getByText('Do not disturb');
    await act(async () => {
      fireEvent.press(dndRow);
    });
    expect(useAppStore.getState().prefs.status).toBe('dnd');
    expect(core.calls.some((c) => c.method === 'presenceUpdate' && c.args[1] === PresenceStatus.Dnd)).toBe(true);

    // 4. Copy public key copies to clipboard and shows toast
    const copyKeyBtn = screen.getByText('Copy public key');
    await act(async () => {
      fireEvent.press(copyKeyBtn);
    });
    expect(Clipboard.setStringAsync).toHaveBeenCalled();
    expect(useAppStore.getState().toasts.some((t) => t.text === 'Public key copied')).toBe(true);

    // 5. Back up identity shares backup file
    const secret = new Uint8Array(32).fill(42).buffer;
    await saveIdentity(core, secret, 'Ava');
    const backupRow = screen.getByText('Back up your identity');
    await act(async () => {
      fireEvent.press(backupRow);
    });
    expect(Sharing.shareAsync).toHaveBeenCalled();
  });

  // Fails if a server refusal during photo upload does not toast the failure naming the server.
  it('one refusal names its server', async () => {
    const core = freshStore();
    core.failNext('avatarUpload', new MobileError.Server({ code: 'file_too_large', message: 'too big' }));
    (pickPhoto as jest.Mock).mockResolvedValue({
      kind: 'picked',
      file: { path: '/tmp/oversized.png', filename: 'oversized.png', sizeBytes: 10_000_000, mime: 'image/png' },
    });

    await draw();

    const editBtn = screen.getByText('Edit');
    await act(async () => {
      fireEvent.press(editBtn);
    });
    await settle();

    const toasts = useAppStore.getState().toasts;
    expect(toasts.some((t) => t.text.includes('Night Owls kept your old picture: That file is bigger than this server allows.'))).toBe(true);
  });
});
