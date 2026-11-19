import {
  ConversationStatus,
  FriendStatus,
  MobileEvent,
  PrivateContactRecord,
  PrivateEventRecord,
  PrivateVerification,
} from '@opencord/core-native';
import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { startEventLoop, type EventLoop } from '@/core';
import type { FakeCore } from '@/core/fake/FakeCore';
import { KAI_PEER, MIRA_PEER, ASH_PEER } from '@/core/fake/privateWorld';
import { selectPrivateUnreadTotal } from '@/state/selectors';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';
import * as ui from '@/ui';

import { ChatList } from '../ChatList';
import { RequestsScreen } from '../../../app/(tabs)/messages/requests';

jest.mock('@/ui', () => ({ ...jest.requireActual<object>('@/ui'), confirm: jest.fn() }));

let core: FakeCore;
let loop: EventLoop;

function startLoop() {
  loop = startEventLoop(core, onEvents);
}

async function flush() {
  await act(async () => {
    await settle();
  });
}

function callsOf(method: string): unknown[][] {
  return core.calls.filter((call) => call.method === method).map((call) => call.args);
}

describe('Messages tab', () => {
  beforeEach(async () => {
    jest.useFakeTimers({ doNotFake: ['setImmediate'], now: 1_761_500_000_000 });
    core = freshStore();
    startLoop();
    await flush();
    jest.clearAllMocks();
  });

  afterEach(() => {
    loop.stop();
    core.eventsAttach();
    jest.useRealTimers();
  });

  // Fails if contacts are not sorted by lastActivityMs descending.
  it('chats are newest first', async () => {
    await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ChatList />
      </GestureHandlerRootView>,
    );
    await flush();

    const kaiRow = screen.getByTestId(`chat-row-${KAI_PEER}`);
    const miraRow = screen.getByTestId(`chat-row-${MIRA_PEER}`);
    expect(kaiRow).toBeTruthy();
    expect(miraRow).toBeTruthy();

    const kaiContact = useAppStore.getState().private.contacts[KAI_PEER];
    const miraContact = useAppStore.getState().private.contacts[MIRA_PEER];
    expect(kaiContact?.lastActivityMs).toBeGreaterThan(miraContact?.lastActivityMs ?? 0);

    const rows = screen.getAllByTestId(/^chat-row-/);
    expect(rows[0]).toBe(kaiRow);
  });

  // Fails if muted unread counts are included in the badge total.
  it('the badge skips muted', async () => {
    const contacts: Record<string, PrivateContactRecord> = {
      [KAI_PEER]: {
        peer: KAI_PEER,
        name: 'Kai',
        unread: 3,
        muted: false,
        requestSpam: false,
        blocked: false,
        reachable: true,
        conversation: ConversationStatus.Accepted,
        friend: FriendStatus.Friends,
        verification: PrivateVerification.Verified,
        newKeys: false,
        hidden: false,
        lastActivityMs: 1000,
      },
      [MIRA_PEER]: {
        peer: MIRA_PEER,
        name: 'Mira',
        unread: 5,
        muted: true,
        requestSpam: false,
        blocked: false,
        reachable: true,
        conversation: ConversationStatus.Accepted,
        friend: FriendStatus.Friends,
        verification: PrivateVerification.Unverified,
        newKeys: false,
        hidden: false,
        lastActivityMs: 500,
      },
    };
    expect(selectPrivateUnreadTotal(contacts)).toBe(3);
  });

  // Fails if swipe read/close or menu mute/close/block don't reach the core.
  it('each swipe and menu item does what it says', async () => {
    core.emit(
      MobileEvent.Private.new({
        event: PrivateEventRecord.ContactChanged.new({
          contact: {
            peer: 'test-peer',
            name: 'Test Peer',
            conversation: ConversationStatus.Accepted,
            friend: FriendStatus.Friends,
            requestSpam: false,
            blocked: false,
            lastActivityMs: 500,
            unread: 2,
            muted: false,
            hidden: false,
            newKeys: false,
            verification: PrivateVerification.Verified,
            reachable: true,
          },
        }),
      }),
    );
    await flush();

    await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <ChatList />
      </GestureHandlerRootView>,
    );
    await flush();

    // Swipe Read on Kai
    const readBtn = screen.getByTestId(`read-chat-${KAI_PEER}`);
    await fireEvent.press(readBtn);
    await flush();
    expect(callsOf('privateMarkRead')).toContainEqual([KAI_PEER]);

    // Swipe Close on Kai
    const closeBtn = screen.getByTestId(`close-chat-${KAI_PEER}`);
    await fireEvent.press(closeBtn);
    await flush();
    expect(callsOf('privateHide')).toContainEqual([KAI_PEER]);

    // Menu: Mute on Mira
    await fireEvent(screen.getByTestId(`chat-menu-${MIRA_PEER}`), 'onAction', 'mute');
    await flush();
    expect(callsOf('privateSetMuted')).toContainEqual([MIRA_PEER, true]);

    // Menu: Close on Mira
    await fireEvent(screen.getByTestId(`chat-menu-${MIRA_PEER}`), 'onAction', 'close');
    await flush();
    expect(callsOf('privateHide')).toContainEqual([MIRA_PEER]);

    // Menu: Block on Test Peer
    const confirm = jest.mocked(ui.confirm);
    await fireEvent(screen.getByTestId('chat-menu-test-peer'), 'onAction', 'block');
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm.mock.calls[0]?.[0]).toMatchObject({
      title: 'Block Test Peer?',
      message: "They won't be able to message you or see your presence.",
      confirmTitle: 'Block',
      destructive: true,
    });
    confirm.mock.calls[0]?.[0].onConfirm();
    await flush();
    expect(callsOf('privateBlock')).toContainEqual(['test-peer']);
  });

  // Fails if requests accept doesn't call message_request_accept or decline doesn't call message_request_ignore.
  it('requests accept and decline', async () => {
    // Add a second request contact to test both accept and decline
    core.emit(
      MobileEvent.Private.new({
        event: PrivateEventRecord.ContactChanged.new({
          contact: {
            peer: 'second-peer',
            name: 'Second Request',
            conversation: ConversationStatus.Request,
            friend: FriendStatus.None,
            requestSpam: false,
            blocked: false,
            lastActivityMs: 1000,
            unread: 1,
            muted: false,
            hidden: false,
            newKeys: false,
            verification: PrivateVerification.Unverified,
            reachable: true,
          },
        }),
      }),
    );
    await flush();

    await render(
      <GestureHandlerRootView style={{ flex: 1 }}>
        <RequestsScreen />
      </GestureHandlerRootView>,
    );
    await flush();

    expect(screen.getAllByText('Message requests').length).toBeGreaterThan(0);
    expect(screen.getByText('They wrote first. Nothing reaches your chats until you accept.')).toBeTruthy();
    expect(screen.getByText('Ash')).toBeTruthy();
    expect(screen.getByText('Second Request')).toBeTruthy();

    // Accept Ash
    const acceptBtns = screen.getAllByLabelText('Accept');
    const firstAccept = acceptBtns[0];
    expect(firstAccept).toBeDefined();
    if (firstAccept) await fireEvent.press(firstAccept);
    await flush();
    expect(callsOf('messageRequestAccept')).toContainEqual([ASH_PEER]);

    // Decline Second Request
    const declineBtns = screen.getAllByLabelText('Decline');
    const firstDecline = declineBtns[0];
    expect(firstDecline).toBeDefined();
    if (firstDecline) await fireEvent.press(firstDecline);
    await flush();
    expect(callsOf('messageRequestIgnore')).toContainEqual(['second-peer']);
  });
});
