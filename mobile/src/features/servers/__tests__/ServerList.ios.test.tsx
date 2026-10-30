// iOS's server list: a SwiftUI List whose rows take the system's swipe
// actions, context menu and pull to refresh (ServerList.ios.tsx).
import { act, fireEvent, render, screen, within } from '@testing-library/react-native';
import { Alert } from 'react-native';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, RUST_CLUB, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';
import { onEvents, useAppStore } from '@/state/store';
import { deviceLocale, formatTime } from '../formatTime';
import { ServerListScreen } from '../ServerListScreen';

const mockPush = jest.fn();
jest.mock('expo-router', () => ({
  router: { push: (...args: unknown[]) => mockPush(...args), replace: jest.fn() },
  useNavigation: () => ({ setOptions: jest.fn() }),
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

const row = (serverKey: string) => screen.getByTestId(`server-menu-${serverKey}`);

describe('ServerList on iOS', () => {
  beforeEach(() => {
    mockPush.mockReset();
    jest.clearAllMocks();
  });

  it('rows show name, preview, time and badge', async () => {
    await setupServers();
    onEvents([
      new MobileEvent.MessageCreated({
        serverKey: NIGHT_OWLS,
        message: { id: '400001', channelId: CHANNELS.general, authorId: '1001', content: 'evening all', createdAtMs: now, attachments: [] },
      }),
    ]);
    await settle();

    await render(<ServerListScreen searchable={false} voiceBar={false} />);
    const time = formatTime(now, Date.now(), deviceLocale());
    expect(screen.getByText('Night Owls')).toBeTruthy();
    expect(screen.getByText(time)).toBeTruthy();
    expect(screen.getByLabelText(`Night Owls, #general · Kai: evening all, 1 unread, ${time}`)).toBeTruthy();
  });

  it('tapping a row opens the server', async () => {
    await setupServers();
    await render(<ServerListScreen searchable={false} voiceBar={false} />);

    fireEvent.press(row(NIGHT_OWLS));
    expect(mockPush).toHaveBeenCalledWith('/server/fake-night-owls');
  });

  it('the trailing swipe action asks before leaving', async () => {
    await setupServers();
    const alertSpy = jest.spyOn(Alert, 'alert');
    await render(<ServerListScreen searchable={false} voiceBar={false} />);

    fireEvent.press(within(row(NIGHT_OWLS)).getByRole('button', { name: 'Leave' }));
    expect(alertSpy).toHaveBeenCalledWith('Leave Night Owls?', 'You can come back with an invite.', expect.any(Array));
    // Still there until the question is answered.
    expect(useAppStore.getState().order.includes(NIGHT_OWLS)).toBe(true);

    const buttons = alertSpy.mock.calls[0]?.[2] as { text: string; onPress?: () => void }[] | undefined;
    await buttons?.find((b) => b.text === 'Leave')?.onPress?.();
    await settle();
    expect(useAppStore.getState().order.includes(NIGHT_OWLS)).toBe(false);
  });

  it('the leading swipe action marks the server read', async () => {
    await setupServers();
    onEvents([
      new MobileEvent.MessageCreated({
        serverKey: NIGHT_OWLS,
        message: { id: '400001', channelId: CHANNELS.general, authorId: '1001', content: 'evening all', createdAtMs: now, attachments: [] },
      }),
    ]);
    await settle();
    await render(<ServerListScreen searchable={false} voiceBar={false} />);

    expect(useAppStore.getState().servers[NIGHT_OWLS]?.activity[CHANNELS.general]?.unread).toBe(1);

    fireEvent.press(within(row(NIGHT_OWLS)).getByRole('button', { name: 'Read' }));
    expect(useAppStore.getState().servers[NIGHT_OWLS]?.activity[CHANNELS.general]?.unread).toBe(0);
  });

  it('the context menu offers Retry now only when not connected', async () => {
    await setupServers();
    onEvents([
      new MobileEvent.Connection({ serverKey: RUST_CLUB, state: new ConnectionRecord.Reconnecting({ attempt: 1, retryInMs: 2000 }) }),
    ]);
    await settle();
    await render(<ServerListScreen searchable={false} voiceBar={false} />);

    const items = (serverKey: string) =>
      within(row(serverKey)).getAllByRole('menuitem').map((item) => item.props.accessibilityLabel);
    expect(items(NIGHT_OWLS)).toEqual(['Mark as read', 'Copy address', 'Leave server']);
    expect(items(RUST_CLUB)).toContain('Retry now');
  });

  it("pull to refresh retries the servers that aren't connected", async () => {
    const core = await setupServers();
    onEvents([new MobileEvent.Connection({ serverKey: RUST_CLUB, state: new ConnectionRecord.Connecting() })]);
    await settle();
    await render(<ServerListScreen searchable={false} voiceBar={false} />);

    await act(async () => {
      await screen.getByRole('list').props.onRefresh();
    });
    await settle();
    expect(core.calls.some((c) => c.method === 'serverRetryNow' && c.args[0] === RUST_CLUB)).toBe(true);
  });
});
