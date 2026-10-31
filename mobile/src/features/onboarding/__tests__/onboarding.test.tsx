import { act, fireEvent, render, screen } from '@testing-library/react-native';
import * as SecureStore from 'expo-secure-store';

import Index from '@/app/index';
import { setCore } from '@/core';
import { FakeCore } from '@/core/fake/FakeCore';
import { BootGate, boot } from '@/features/boot/BootGate';
import { initialState, useAppStore } from '@/state/store';
import { Text } from '@/ui';

import { CreateScreen } from '../CreateScreen';
import { ImportScreen, NOT_A_BACKUP, pickBackup } from '../ImportScreen';
import { createIdentity } from '../identity';
import { holdSecret, peekSecret } from '../pending';
import { WelcomeScreen } from '../WelcomeScreen';

const mockPush = jest.fn();
const mockReplace = jest.fn();
let mockParams: Record<string, string> = {};
jest.mock('expo-router', () => ({
  router: { push: (...args: unknown[]) => mockPush(...args), replace: (...args: unknown[]) => mockReplace(...args) },
  useLocalSearchParams: () => mockParams,
  Redirect: ({ href }: { href: string }) => {
    const { Text: NativeText } = jest.requireActual('react-native');
    return <NativeText>redirect:{href}</NativeText>;
  },
}));
jest.mock('expo-splash-screen', () => ({ preventAutoHideAsync: jest.fn(async () => {}), hideAsync: jest.fn(async () => {}) }));

let mockBackupText = '';
jest.mock('expo-document-picker', () => ({
  getDocumentAsync: jest.fn(async () => ({ canceled: false, assets: [{ uri: 'file:///cache/backup.txt' }] })),
}));
jest.mock('expo-file-system', () => ({
  File: class MockFile {
    async text() {
      return mockBackupText;
    }
    delete() {}
  },
  Paths: { document: { uri: 'file:///data/' }, cache: { uri: 'file:///cache/' } },
}));
jest.mock('@/core/lifecycle', () => ({ startApp: jest.fn() }));

const items = (SecureStore as unknown as { __items: Map<string, string> }).__items;

function fresh(scenario: 'empty' | 'default' = 'empty'): FakeCore {
  const core = new FakeCore({ scenario });
  setCore(core);
  items.clear();
  useAppStore.setState(initialState(), true);
  mockPush.mockReset();
  mockReplace.mockReset();
  mockParams = {};
  holdSecret(null);
  return core;
}

describe('starting and onboarding', () => {
  it('a first start shows Welcome', async () => {
    fresh();
    await boot();
    expect(useAppStore.getState().boot).toEqual({ state: 'onboarding' });
    await render(<Index />);
    expect(screen.getByText('redirect:/welcome')).toBeTruthy();
    await render(<WelcomeScreen />);
    expect(screen.getByText('Chat and voice on servers you trust.')).toBeTruthy();
  });

  it('creating an identity saves it and opens Servers', async () => {
    const core = fresh();
    await createIdentity('  Ava ');
    expect(items.get('opencord.identity')).toBeDefined();
    expect(core.settingsGet('mobile.displayName')).toBe('Ava');
    expect(core.calls.some((call) => call.method === 'identityLoad')).toBe(true);
    expect(useAppStore.getState().boot).toEqual({ state: 'ready' });
    expect(useAppStore.getState().self?.displayName).toBe('Ava');
  });

  it("an empty name is refused with the core's sentence", async () => {
    fresh();
    await render(<CreateScreen />);
    await fireEvent.press(screen.getByLabelText('Continue'));
    expect(await screen.findByText('display name must not be empty')).toBeTruthy();
    expect(useAppStore.getState().boot).toEqual({ state: 'starting' });
  });

  it('importing a backup decodes it and asks for a name', async () => {
    const core = fresh();
    mockBackupText = core.identityBackupEncode(new Uint8Array(32).fill(5).buffer);
    expect(await pickBackup()).toBe('picked');
    expect(new Uint8Array(peekSecret() ?? new ArrayBuffer(0))).toEqual(new Uint8Array(32).fill(5));
    await render(<CreateScreen />);
    expect(screen.getByText('Your identity is ready. Pick the name servers show.')).toBeTruthy();
    await fireEvent.changeText(screen.getByLabelText('Display name'), 'Ava');
    await fireEvent.press(screen.getByLabelText('Continue'));
    const loaded = core.calls.find((call) => call.method === 'identityLoad');
    expect(new Uint8Array(loaded?.args[0] as ArrayBuffer)).toEqual(new Uint8Array(32).fill(5));
  });

  it('a wrong backup file says so', async () => {
    fresh();
    mockBackupText = 'hello';
    await render(<WelcomeScreen />);
    await fireEvent.press(screen.getByLabelText('Import a backup'));
    await act(async () => {});
    expect(mockPush).toHaveBeenCalledWith({ pathname: '/welcome/import', params: { failed: '1' } });
    mockParams = { failed: '1' };
    await render(<ImportScreen />);
    expect(screen.getByText(NOT_A_BACKUP)).toBeTruthy();
  });

  it('a saved identity skips onboarding', async () => {
    const core = fresh();
    items.set('opencord.identity', btoa(String.fromCharCode(...new Uint8Array(32).fill(9))));
    core.settingsSet('mobile.displayName', 'Ava');
    await boot();
    expect(useAppStore.getState().boot).toEqual({ state: 'ready' });
    await render(<Index />);
    expect(screen.getByText('redirect:/servers')).toBeTruthy();
  });

  it('a failed start shows its sentence and Try again', async () => {
    const core = fresh();
    core.failNext('coreInit', new (jest.requireActual('@opencord/core-native').MobileError.Storage)({ message: 'full' }));
    await render(
      <BootGate>
        <Text variant="body">inside</Text>
      </BootGate>,
    );
    expect(await screen.findByText("Couldn't save on this phone. It may be out of space.")).toBeTruthy();
    await fireEvent.press(screen.getByLabelText('Try again'));
    expect(await screen.findByText('inside')).toBeTruthy();
  });
});
