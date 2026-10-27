import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { Alert } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent, VoiceLinkState } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, RUST_CLUB, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';
import { onEvents, useAppStore } from '@/state/store';
import { ServerListScreen } from '../ServerListScreen';

function renderScreen(ui: React.ReactElement) {
  return render(<GestureHandlerRootView style={{ flex: 1 }}>{ui}</GestureHandlerRootView>);
}

const mockPush = jest.fn();
let mockNavOptions: Record<string, unknown> = {};
jest.mock('expo-router', () => ({
  router: { push: (...args: unknown[]) => mockPush(...args), replace: jest.fn() },
  useNavigation: () => ({
    setOptions: (opts: Record<string, unknown>) => {
      Object.assign(mockNavOptions, opts);
    },
  }),
}));

const now = 1_761_500_000_000;

async function setupServers() {
  const core = freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    new MobileEvent.Connection({ serverKey: RUST_CLUB, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(RUST_CLUB) }),
  ]);
  await settle();
  return core;
}

describe('ServerList', () => {
  beforeEach(() => {
    mockPush.mockReset();
    mockNavOptions = {};
    jest.clearAllMocks();
  });

  it('rows show name, preview, time and badge', async () => {
    await setupServers();
    onEvents([
      new MobileEvent.MessageCreated({
        serverKey: NIGHT_OWLS,
        message: {
          id: '400001',
          channelId: CHANNELS.general,
          authorId: '1001',
          content: 'evening all',
          createdAtMs: now,
          attachments: [],
        },
      }),
    ]);
    await settle();

    await renderScreen(<ServerListScreen />);
    expect(screen.getByText('Night Owls')).toBeTruthy();
    expect(screen.getByText(/#general · Kai: evening all/)).toBeTruthy();
    expect(screen.getByText('1')).toBeTruthy();
  });

  it('a row shows the voice glyph while the user is in voice there', async () => {
    await setupServers();
    useAppStore.setState({
      myVoice: {
        serverKey: NIGHT_OWLS,
        channelId: CHANNELS.lounge,
        link: new VoiceLinkState.Connected(),
        muted: false,
        deafened: false,
      },
    });

    await renderScreen(<ServerListScreen />);
    expect(screen.getByTestId(`voice-glyph-${NIGHT_OWLS}`)).toBeTruthy();
    expect(screen.queryByTestId(`voice-glyph-${RUST_CLUB}`)).toBeNull();
  });

  it('search filters by name', async () => {
    await setupServers();
    await renderScreen(<ServerListScreen query="rust" />);
    expect(screen.getByText('Rust Club')).toBeTruthy();
    expect(screen.queryByText('Night Owls')).toBeNull();
  });

  it('leave asks first and removes the server', async () => {
    await setupServers();
    const alertSpy = jest.spyOn(Alert, 'alert');

    await renderScreen(<ServerListScreen />);
    const leaveButton = screen.getByTestId(`leave-server-${NIGHT_OWLS}`);
    fireEvent.press(leaveButton);

    expect(alertSpy).toHaveBeenCalledWith(
      'Leave Night Owls?',
      'You can come back with an invite.',
      expect.any(Array),
    );

    const buttons = alertSpy.mock.calls[0]?.[2] as { text: string; onPress?: () => void }[] | undefined;
    const leaveConfirm = buttons?.find((b) => b.text === 'Leave');
    expect(leaveConfirm).toBeDefined();

    const leavePromise = leaveConfirm?.onPress?.();
    await leavePromise;
    await settle();

    expect(useAppStore.getState().order.includes(NIGHT_OWLS)).toBe(false);
  });

  it('the menu offers Retry now only when not connected', async () => {
    await setupServers();
    onEvents([
      new MobileEvent.Connection({
        serverKey: RUST_CLUB,
        state: new ConnectionRecord.Reconnecting({ attempt: 1, retryInMs: 2000 }),
      }),
    ]);
    await settle();

    await renderScreen(<ServerListScreen />);
    const nightOwlsMenu = screen.getByTestId(`server-menu-${NIGHT_OWLS}`);
    expect(nightOwlsMenu.props.actions.some((a: { id: string }) => a.id === 'retry')).toBe(false);

    const rustClubMenu = screen.getByTestId(`server-menu-${RUST_CLUB}`);
    expect(rustClubMenu.props.actions.some((a: { id: string }) => a.id === 'retry')).toBe(true);
  });

  it("pull to refresh retries the servers that aren't connected", async () => {
    const core = await setupServers();
    onEvents([
      new MobileEvent.Connection({
        serverKey: RUST_CLUB,
        state: new ConnectionRecord.Connecting(),
      }),
    ]);
    await settle();

    await renderScreen(<ServerListScreen />);
    const list = screen.getByTestId('server-list');
    const refreshControl = list.props.refreshControl;
    expect(refreshControl).toBeDefined();

    await act(async () => {
      refreshControl.props.onRefresh();
    });
    await settle();

    expect(core.calls.some((c) => c.method === 'serverRetryNow' && c.args[0] === RUST_CLUB)).toBe(true);
  });

  it('with no servers it offers Add a server', async () => {
    freshStore({ scenario: 'empty' });
    useAppStore.setState({ servers: {}, order: [] });

    await renderScreen(<ServerListScreen />);
    expect(screen.getByText('No servers yet')).toBeTruthy();
    expect(screen.getByText('Add one with an invite link or its address.')).toBeTruthy();

    const addButton = screen.getByLabelText('Add a server');
    fireEvent.press(addButton);
    expect(mockPush).toHaveBeenCalledWith('/add-server');
  });

  it('tapping a row opens the server', async () => {
    await setupServers();
    await renderScreen(<ServerListScreen />);

    fireEvent.press(screen.getByText('Night Owls'));
    expect(mockPush).toHaveBeenCalledWith('/server/fake-night-owls');
  });
});
