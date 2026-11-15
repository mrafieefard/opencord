// iOS's search slot (design §6.1, Review Focus 4): Search on the Servers
// tab; the server's button, opening its sheet, while a server's screen is in
// front; Search again once it leaves. Jest draws with iOS's files, so
// `chrome.tabBarSearch` is on, as on an iPhone.
import { act, render, screen, within } from '@testing-library/react-native';
import type { ReactNode } from 'react';
import { Dimensions } from 'react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, MobileEvent } from '@opencord/core-native';

import TabsLayout from '@/app/(tabs)/_layout';
import { NIGHT_OWLS, RUST_CLUB, snapshot } from '@/core/fake/world';
import { ServerScreen } from '@/features/server/ServerScreen';
import { clearServerInFront, setServerInFront } from '@/state/actions/ui';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

const mockPush = jest.fn();
jest.mock('expo-router', () => ({
  router: { push: (...args: unknown[]) => mockPush(...args), back: jest.fn(), replace: jest.fn(), dismissTo: jest.fn() },
  useNavigation: () => ({ setOptions: jest.fn() }),
  // Focus is the screen's life here: it comes on mount and goes on unmount.
  useFocusEffect: (effect: () => undefined | (() => void)) =>
    jest.requireActual<typeof import('react')>('react').useEffect(effect, [effect]),
}));

// The native tab bar is native: each trigger is a view holding its props.
jest.mock('expo-router/native-tabs', () => {
  const React = jest.requireActual<typeof import('react')>('react');
  const { Text, View } = jest.requireActual<typeof import('react-native')>('react-native');
  const NativeTabs = ({ children }: { children: ReactNode }) => React.createElement(View, null, children);
  // The props ride on the view under names a View doesn't have, so they're
  // built apart from the call (no excess-property check on a held object).
  const Trigger = (props: { name: string; children?: ReactNode }) => {
    const host = { testID: `tab-${props.name}`, trigger: props };
    return React.createElement(View, host, props.children);
  };
  const TriggerIcon = (props: { sf: string; md: string }) => {
    const host = { testID: 'tab-icon', sf: props.sf, md: props.md };
    return React.createElement(View, host);
  };
  const TriggerLabel = ({ children }: { children: ReactNode }) => React.createElement(Text, null, children);
  Trigger.Icon = TriggerIcon;
  Trigger.Label = TriggerLabel;
  NativeTabs.Trigger = Trigger;
  return { NativeTabs };
});

interface TriggerProps {
  name: string;
  hidden?: boolean;
  disabled?: boolean;
  accessibilityLabel?: string;
  listeners?: { tabPress?: () => void };
}

const slot = (): TriggerProps => screen.getByTestId('tab-search').props.trigger;
const slotIcon = () => within(screen.getByTestId('tab-search')).getByTestId('tab-icon').props.sf;
const slotLabel = () => within(screen.getByTestId('tab-search')).getByText(/^(Search|Server)$/).props.children;
const pressSlot = () => act(() => slot().listeners?.tabPress?.());

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

function App({ server }: { server: string | null }) {
  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <TabsLayout />
      {server !== null && <ServerScreen key={server} serverKey={server} />}
    </GestureHandlerRootView>
  );
}

describe('the iOS tab slot', () => {
  beforeEach(() => {
    jest.clearAllMocks();
    windowWidth(412);
  });

  // Fails if the slot stops being Search outside a server, stays the
  // server's button after the server's screen leaves, or acts on a press
  // there; and if entering a server changes the trigger's name or hidden,
  // which rebuilds the tabs.
  it('the tab slot is search outside a server', async () => {
    await seed();
    const view = await render(<App server={null} />);
    expect(slot()).toMatchObject({ name: 'search', hidden: false, disabled: false, accessibilityLabel: 'Search' });
    expect(slotIcon()).toBe('magnifyingglass');
    expect(slotLabel()).toBe('Search');
    await pressSlot();
    expect(mockPush).not.toHaveBeenCalled();

    // In a server, then out of it again: Search comes back.
    await view.rerender(<App server={NIGHT_OWLS} />);
    expect(slotLabel()).toBe('Server');
    await view.rerender(<App server={null} />);
    expect(useAppStore.getState().serverInFront).toBeNull();
    expect(slot()).toMatchObject({ name: 'search', hidden: false, disabled: false, accessibilityLabel: 'Search' });
    expect(slotIcon()).toBe('magnifyingglass');
    expect(slotLabel()).toBe('Search');
    await pressSlot();
    expect(mockPush).not.toHaveBeenCalled();
  });

  // Fails if the slot doesn't become this server's button (disabled, so
  // the tab doesn't switch, with the ellipsis and `Server`), or a press
  // doesn't open this server's sheet each time.
  it('the tab slot opens the server sheet inside a server', async () => {
    await seed();
    const view = await render(<App server={NIGHT_OWLS} />);
    expect(useAppStore.getState().serverInFront).toBe(NIGHT_OWLS);
    expect(slot()).toMatchObject({ name: 'search', hidden: false, disabled: true, accessibilityLabel: 'Server' });
    expect(slotIcon()).toBe('ellipsis');
    expect(slotLabel()).toBe('Server');
    await pressSlot();
    await pressSlot();
    expect(mockPush.mock.calls).toEqual([['/server/fake-night-owls/settings'], ['/server/fake-night-owls/settings']]);

    // Another server: its own sheet.
    mockPush.mockClear();
    await view.rerender(<App server={RUST_CLUB} />);
    await pressSlot();
    expect(mockPush.mock.calls).toEqual([['/server/fake-rust-club/settings']]);
  });

  // Fails if a server leaving the front clears a slot another server took meanwhile.
  it('only the server holding the slot gives it back', async () => {
    await seed();
    setServerInFront(NIGHT_OWLS);
    setServerInFront(RUST_CLUB);
    clearServerInFront(NIGHT_OWLS);
    expect(useAppStore.getState().serverInFront).toBe(RUST_CLUB);
    clearServerInFront(RUST_CLUB);
    expect(useAppStore.getState().serverInFront).toBeNull();
  });
});
