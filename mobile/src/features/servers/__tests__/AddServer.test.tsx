import { cleanup, fireEvent, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { MobileError } from '@opencord/core-native';

import { describeError } from '@/core';
import { freshStore, settle } from '@/test/store';
import { useAppStore } from '@/state/store';
import { AddServerForm } from '../AddServerForm';

const mockBack = jest.fn();
let mockParams: { link?: string } = {};

const mockRouter = {
  back: () => mockBack(),
  push: jest.fn(),
};

jest.mock('expo-router', () => ({
  router: mockRouter,
  useRouter: () => mockRouter,
  useLocalSearchParams: () => mockParams,
}));

async function renderScreen(ui: React.ReactElement) {
  return await render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

describe('AddServer', () => {
  beforeEach(() => {
    freshStore();
    mockBack.mockClear();
    mockParams = {};
  });

  afterEach(() => {
    cleanup();
  });

  it('adds a server and closes', async () => {
    const core = freshStore();
    core.serverTrust('127.0.0.1:7710', 'test');
    await renderScreen(<AddServerForm />);

    const input = screen.getByLabelText('Invite link or address');
    await fireEvent.changeText(input, '127.0.0.1:7710');

    await fireEvent.press(screen.getByLabelText('Add'));
    await settle();

    expect(mockBack).toHaveBeenCalled();
    expect(useAppStore.getState().order).toContain('127.0.0.1:7710');
  });

  it('asks to trust an unknown certificate, then adds', async () => {
    const core = freshStore();
    await renderScreen(<AddServerForm />);

    const input = screen.getByLabelText('Invite link or address');
    await fireEvent.changeText(input, '127.0.0.1:7710');

    await fireEvent.press(screen.getByLabelText('Add'));
    await settle();

    expect(screen.getByText('Trust this server?')).toBeTruthy();
    expect(screen.getByText(/Its certificate isn't from a public authority/)).toBeTruthy();

    // Fingerprint in groups of four, four to a line
    expect(screen.getAllByText(/^[0-9a-f]{4}( [0-9a-f]{4}){3}$/).map((line) => line.props.children)).toEqual([
      'ab12 cd34 ef56 ab12',
      'cd34 ef56 ab12 cd34',
      'ef56 ab12 cd34 ef56',
      'ab12 cd34 ef56 ab12',
    ]);

    await fireEvent.press(screen.getByLabelText('Trust and add'));
    await settle();

    const trustCalls = core.calls.filter((c) => c.method === 'serverTrust');
    expect(trustCalls).toHaveLength(1);
    expect(trustCalls[0]?.args).toEqual([
      '127.0.0.1:7710',
      'ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12cd34ef56ab12',
    ]);

    expect(mockBack).toHaveBeenCalled();
    expect(useAppStore.getState().order).toContain('127.0.0.1:7710');
  });

  it('cancelling the trust step adds nothing', async () => {
    await renderScreen(<AddServerForm />);

    const input = screen.getByLabelText('Invite link or address');
    await fireEvent.changeText(input, '127.0.0.1:7710');

    await fireEvent.press(screen.getByLabelText('Add'));
    await settle();

    expect(screen.getByText('Trust this server?')).toBeTruthy();

    await fireEvent.press(screen.getByLabelText('Cancel'));
    await settle();

    expect(useAppStore.getState().order).not.toContain('127.0.0.1:7710');
  });

  it('the claim token is sent only when filled in', async () => {
    const core = freshStore();
    core.serverTrust('127.0.0.1:7710', 'one');
    core.serverTrust('127.0.0.1:7711', 'two');

    // 1. Without claim token
    const { unmount } = await renderScreen(<AddServerForm />);
    const input = screen.getByLabelText('Invite link or address');
    await fireEvent.changeText(input, '127.0.0.1:7710');

    await fireEvent.press(screen.getByLabelText('Add'));
    await settle();

    const firstAdd = core.calls.find((c) => c.method === 'serverAdd');
    expect(firstAdd?.args).toEqual(['127.0.0.1:7710', undefined]);

    unmount();
    await settle();

    // 2. With claim token
    await renderScreen(<AddServerForm />);
    const input2 = screen.getByLabelText('Invite link or address');
    await fireEvent.changeText(input2, '127.0.0.1:7711');

    // Expand "I own this server"
    await fireEvent.press(screen.getByLabelText('I own this server'));
    await settle();

    const tokenInput = screen.getByLabelText('Claim token');
    await fireEvent.changeText(tokenInput, 'secret-claim-token-123');

    await fireEvent.press(screen.getByLabelText('Add'));
    await settle();

    const secondAdd = core.calls.filter((c) => c.method === 'serverAdd')[1];
    expect(secondAdd?.args).toEqual(['127.0.0.1:7711', 'secret-claim-token-123']);
  });

  it('errors show under the field', async () => {
    await renderScreen(<AddServerForm />);

    const input = screen.getByLabelText('Invite link or address');
    await fireEvent.changeText(input, 'not an address');

    await fireEvent.press(screen.getByLabelText('Add'));
    await settle();

    expect(screen.getByRole('alert')).toBeTruthy();
    expect(screen.getByText(/not an invite link or address/)).toBeTruthy();
    expect(mockBack).not.toHaveBeenCalled();

    // Editing the address clears the stale error.
    await fireEvent.changeText(input, '127.0.0.1:7710');
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('an error while trusting shows in the trust step', async () => {
    const core = freshStore();
    await renderScreen(<AddServerForm />);
    await fireEvent.changeText(screen.getByLabelText('Invite link or address'), '127.0.0.1:7710');
    await fireEvent.press(screen.getByLabelText('Add'));
    await settle();

    core.failNext('serverAdd', new MobileError.NotConnected());
    await fireEvent.press(screen.getByLabelText('Trust and add'));
    await settle();

    expect(screen.getByText('Trust this server?')).toBeTruthy();
    expect(screen.getByRole('alert')).toBeTruthy();
    expect(screen.getByText(describeError(new MobileError.NotConnected()))).toBeTruthy();
    expect(mockBack).not.toHaveBeenCalled();
  });

  it('a link parameter fills the field', async () => {
    mockParams = { link: 'opencord://10.0.2.2:7710/invite/ABC#fp=123' };
    await renderScreen(<AddServerForm />);

    const input = screen.getByLabelText('Invite link or address');
    expect(input.props.value).toBe('opencord://10.0.2.2:7710/invite/ABC#fp=123');
  });
});
