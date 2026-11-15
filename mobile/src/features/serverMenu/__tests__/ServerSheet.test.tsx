// The server sheet (design §6.1): its header, Mark as read, the admin rows
// this user may open (permitted by the desktop's gates and registered as a
// page), Leave server, and the ways in from the server's screen.
import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { router } from 'expo-router';
import type { ReactElement } from 'react';
import { Alert, Dimensions } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import { CHANNELS, NIGHT_OWLS, RUST_CLUB, snapshot } from '@/core/fake/world';
import { ChannelListHeader } from '@/features/server/ChannelListHeader';
import { ServerScreen } from '@/features/server/ServerScreen';
import { chrome } from '@/features/servers/chrome';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { ServerSheet } from '../ServerSheet';
import { LOST_ACCESS, registerServerPage, useOpenPageGuard } from '../serverRows';

jest.mock('expo-router', () => ({
  router: { push: jest.fn(), back: jest.fn(), replace: jest.fn(), dismissTo: jest.fn() },
  useNavigation: () => ({ setOptions: jest.fn() }),
  useFocusEffect: (effect: () => undefined | (() => void)) =>
    jest.requireActual<typeof import('react')>('react').useEffect(effect, [effect]),
}));

// One object, so a test can draw Android's header (no tab bar search slot).
jest.mock('@/features/servers/chrome', () => ({
  chrome: { serversSearch: false, serversVoiceBar: false, tabBarSearch: true, floatingVoice: true },
}));

const alertSpy = jest.spyOn(Alert, 'alert');

// P0 builds no admin page: this file registers four, each opening a route
// that exists, so a press can be followed. Bans stays unregistered.
registerServerPage('overview', { label: 'Overview', route: (key) => `/server/${key}/member/overview` });
registerServerPage('channels', { label: 'Channels', route: (key) => `/server/${key}/member/channels` });
registerServerPage('roles', { label: 'Roles', route: (key) => `/server/${key}/member/roles` });
registerServerPage('members', { label: 'Members', route: (key) => `/server/${key}/member/members` });

const push = jest.mocked(router.push);
const back = jest.mocked(router.back);
const dismissTo = jest.mocked(router.dismissTo);

function windowWidth(width: number) {
  const size = { width, height: 900, scale: 2, fontScale: 1 };
  Dimensions.set({ window: size, screen: size });
}

async function seed() {
  freshStore();
  onEvents([
    new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    new MobileEvent.Connection({ serverKey: RUST_CLUB, state: new ConnectionRecord.Connected() }),
    new MobileEvent.Ready({ snapshot: snapshot(RUST_CLUB) }),
  ]);
  await settle();
}

const draw = (element: ReactElement) => render(<GestureHandlerRootView style={{ flex: 1 }}>{element}</GestureHandlerRootView>);

const lostAccessToasts = () => useAppStore.getState().toasts.filter((entry) => entry.text === LOST_ACCESS).length;

/** The sheet's stack, which runs the guard with where it is (settings/_layout.tsx). */
function SheetStack({ pathname }: { pathname: string }) {
  useOpenPageGuard(NIGHT_OWLS, pathname);
  return <ServerSheet serverKey={NIGHT_OWLS} />;
}

describe('ServerSheet', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    chrome.tabBarSearch = true;
  });

  // Fails if a row shows without its permission (Rust Club: a plain
  // member), or without a page (Bans: permitted, never registered), if the
  // owner's Hosting row is missing or pressable, or a row doesn't open its
  // page each time.
  it('rows follow permissions', async () => {
    await seed();
    const view = await draw(<ServerSheet serverKey={NIGHT_OWLS} />);
    expect(screen.getByText('Night Owls')).toBeTruthy();
    // 8 members; two of them (the 4th and 8th) are offline in the fake world.
    expect(screen.getByText('8 members · 6 online')).toBeTruthy();
    for (const label of ['Mark as read', 'Overview', 'Channels', 'Roles', 'Members', 'Leave server']) {
      expect(screen.getByRole('button', { name: label })).toBeTruthy();
    }
    expect(screen.queryByText('Bans')).toBeNull();
    expect(screen.getByText('Hosting')).toBeTruthy();
    expect(screen.getByText('Open on a computer')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Hosting' })).toBeNull();

    await fireEvent.press(screen.getByRole('button', { name: 'Overview' }));
    await fireEvent.press(screen.getByRole('button', { name: 'Overview' }));
    await fireEvent.press(screen.getByRole('button', { name: 'Roles' }));
    expect(push.mock.calls).toEqual([
      ['/server/fake-night-owls/member/overview'],
      ['/server/fake-night-owls/member/overview'],
      ['/server/fake-night-owls/member/roles'],
    ]);

    // The second server: no permission, not the owner. Only the sheet's own rows.
    await view.rerender(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ServerSheet serverKey={RUST_CLUB} />
      </GestureHandlerRootView>,
    );
    expect(screen.getByText('Rust Club')).toBeTruthy();
    for (const label of ['Overview', 'Channels', 'Roles', 'Members', 'Bans', 'Hosting', 'Open on a computer']) {
      expect(screen.queryByText(label)).toBeNull();
    }
    expect(screen.getByRole('button', { name: 'Mark as read' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Leave server' })).toBeTruthy();
  });

  // Fails if an AccessChanged leaves the row in the open sheet, leaves its
  // page open, says so more than once, or closes a page the user still may
  // open; and if a page the user closes themselves says anything.
  it('a row goes when its permission goes', async () => {
    await seed();
    const tree = (pathname: string) => (
      <GestureHandlerRootView style={{ flex: 1 }}>
        <SheetStack pathname={pathname} />
      </GestureHandlerRootView>
    );
    const before = snapshot(NIGHT_OWLS);
    const change = (serverAccess: typeof before.serverAccess) =>
      act(async () => {
        // The server sends the channels' access with it.
        onEvents([new MobileEvent.AccessChanged({ serverKey: NIGHT_OWLS, access: before.access, serverAccess })]);
      });

    // The Channels page is open, then the user closes it: nothing is said.
    const view = await render(tree('/server/fake-night-owls/member/channels'));
    await view.rerender(tree('/server/fake-night-owls/settings'));
    expect(lostAccessToasts()).toBe(0);

    // A page the Roles page pushed is open when Manage roles goes.
    await view.rerender(tree('/server/fake-night-owls/member/roles/2001'));
    expect(screen.getByRole('button', { name: 'Roles' })).toBeTruthy();
    const withoutRoles = { ...before.serverAccess, administrator: false, manageRoles: false };
    await change(withoutRoles);
    expect(screen.queryByRole('button', { name: 'Roles' })).toBeNull();
    // Members still shows: kick and ban still allow it.
    for (const label of ['Overview', 'Channels', 'Members']) expect(screen.getByRole('button', { name: label })).toBeTruthy();
    expect(dismissTo.mock.calls).toEqual([['/server/fake-night-owls/settings']]);
    expect(lostAccessToasts()).toBe(1);

    // Another change while the page is still closing: said once only.
    await change({ ...withoutRoles, manageServer: false });
    expect(screen.queryByRole('button', { name: 'Overview' })).toBeNull();
    expect(dismissTo).toHaveBeenCalledTimes(1);
    expect(lostAccessToasts()).toBe(1);

    // Back on the list: a page still allowed stays open through a change.
    await view.rerender(tree('/server/fake-night-owls/settings'));
    await view.rerender(tree('/server/fake-night-owls/member/channels'));
    await change({ ...withoutRoles, manageServer: false, kickMembers: false });
    expect(dismissTo).toHaveBeenCalledTimes(1);
    expect(lostAccessToasts()).toBe(1);
  });

  // Fails if the sheet's Close doesn't close it.
  it('close closes the sheet', async () => {
    await seed();
    await draw(<ServerSheet serverKey={NIGHT_OWLS} />);
    await fireEvent.press(screen.getByRole('button', { name: 'Close' }));
    expect(back).toHaveBeenCalledTimes(1);
  });

  // Fails if Leave server leaves without asking, or the sheet stays open
  // on a server that's gone once the user says Leave.
  it('leave asks first', async () => {
    await seed();
    await draw(<ServerSheet serverKey={NIGHT_OWLS} />);

    await fireEvent.press(screen.getByRole('button', { name: 'Leave server' }));
    expect(alertSpy).toHaveBeenCalledWith('Leave Night Owls?', 'You can come back with an invite.', expect.any(Array));
    expect(useAppStore.getState().order.includes(NIGHT_OWLS)).toBe(true);

    // Cancel, then ask again: still there, still asking.
    const answer = (index: number, text: string) =>
      (alertSpy.mock.calls[index]?.[2] as { text: string; onPress?: () => void }[] | undefined)?.find((button) => button.text === text)?.onPress?.();
    await act(async () => answer(0, 'Cancel'));
    expect(useAppStore.getState().order.includes(NIGHT_OWLS)).toBe(true);
    await fireEvent.press(screen.getByRole('button', { name: 'Leave server' }));
    expect(alertSpy).toHaveBeenCalledTimes(2);
    expect(back).not.toHaveBeenCalled();

    await act(async () => {
      answer(1, 'Leave');
      await settle();
    });
    expect(useAppStore.getState().order.includes(NIGHT_OWLS)).toBe(false);
    expect(screen.queryByTestId('server-sheet')).toBeNull();
    expect(back).toHaveBeenCalledTimes(1);
  });

  // Fails if Mark as read doesn't clear the server's unread count each time.
  it('mark as read clears the server, each time', async () => {
    await seed();
    const message = (id: string) =>
      new MobileEvent.MessageCreated({
        serverKey: NIGHT_OWLS,
        message: { id, channelId: CHANNELS.general, authorId: '1001', content: 'evening all', createdAtMs: 1_761_500_000_000, attachments: [] },
      });
    const unread = () => useAppStore.getState().servers[NIGHT_OWLS]?.activity[CHANNELS.general]?.unread;
    await draw(<ServerSheet serverKey={NIGHT_OWLS} />);

    await act(async () => onEvents([message('400001')]));
    expect(unread()).toBe(1);
    await fireEvent.press(screen.getByRole('button', { name: 'Mark as read' }));
    expect(unread()).toBe(0);
    await act(async () => onEvents([message('400002')]));
    expect(unread()).toBe(1);
    await fireEvent.press(screen.getByRole('button', { name: 'Mark as read' }));
    expect(unread()).toBe(0);
  });

  // Fails if, with a chat in front (the tab bar hidden), the server's name
  // in the channel sidebar doesn't open the sheet.
  it("the server's name opens the sheet while a chat is in front", async () => {
    await seed();
    windowWidth(412);
    await draw(<ServerScreen serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />);
    expect(useAppStore.getState().chatInFront).toBe(true);

    // As a person does: the sidebar, then the name at its top.
    await fireEvent.press(screen.getByLabelText('Show channels'));
    await fireEvent.press(screen.getByRole('button', { name: 'Night Owls' }));
    await fireEvent.press(screen.getByRole('button', { name: 'Night Owls' }));
    expect(push.mock.calls).toEqual([['/server/fake-night-owls/settings'], ['/server/fake-night-owls/settings']]);
  });

  // Fails if Android's header loses its server button, or iOS's grows one
  // (it has the tab slot).
  it("Android's header has the server button", async () => {
    await seed();
    chrome.tabBarSearch = false;
    await draw(<ChannelListHeader serverKey={NIGHT_OWLS} backLabel="Servers" onBack={jest.fn()} />);
    await fireEvent.press(screen.getByRole('button', { name: 'Server' }));
    await fireEvent.press(screen.getByRole('button', { name: 'Server' }));
    expect(push.mock.calls).toEqual([['/server/fake-night-owls/settings'], ['/server/fake-night-owls/settings']]);
  });

  it("iOS's header has no server button", async () => {
    await seed();
    await draw(<ChannelListHeader serverKey={NIGHT_OWLS} backLabel="Servers" onBack={jest.fn()} />);
    expect(screen.queryByRole('button', { name: 'Server' })).toBeNull();
  });
});
