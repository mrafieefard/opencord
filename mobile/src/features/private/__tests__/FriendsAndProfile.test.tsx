import { act, fireEvent, render, screen, within } from '@testing-library/react-native';
import { router } from 'expo-router';
import {
  ConnectionRecord,
  ConversationStatus,
  FriendStatus,
  MobileEvent,
  PrivateContactRecord,
  PrivateEventRecord,
  PrivateVerification,
} from '@opencord/core-native';

import { startEventLoop, type EventLoop } from '@/core';
import type { FakeCore } from '@/core/fake/FakeCore';
import { KAI_PEER } from '@/core/fake/privateWorld';
import { NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { MemberProfile } from '@/features/members/MemberProfile';
import { onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { FriendsPage } from '../FriendsPage';

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

beforeEach(async () => {
  jest.useFakeTimers({ doNotFake: ['setImmediate'], now: 1_761_500_000_000 });
  core = freshStore();
  const snap = snapshot(NIGHT_OWLS);
  useAppStore.setState({
    servers: {
      [NIGHT_OWLS]: {
        key: NIGHT_OWLS,
        entry: {
          key: NIGHT_OWLS,
          host: 'test.opencord.org',
          port: 443,
          name: 'Night Owls',
          userId: '1001',
        },
        connection: new ConnectionRecord.Connected(),
        snapshot: snap,
        seeding: false,
        readyCount: 1,
        held: [],
        activity: {},
        typing: {},
        voice: {},
        nowPlaying: {},
        musicSettings: null,
      },
    },
  });
  startLoop();
  await flush();
  jest.clearAllMocks();
});

afterEach(() => {
  loop?.stop();
});

describe('Friends and Profile messaging', () => {
  // Fails if pressing Message does not call privateOpenConversation or does not navigate.
  it('message opens the conversation', async () => {
    core.emit(
      MobileEvent.Private.new({
        event: PrivateEventRecord.ContactChanged.new({
          contact: {
            peer: KAI_PEER,
            name: 'Kai',
            unread: 0,
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
        }),
      }),
    );
    await flush();

    await render(<MemberProfile serverKey={NIGHT_OWLS} userId="1001" />);
    await flush();

    const messageBtn = screen.getByLabelText('Message');
    expect(messageBtn).toBeTruthy();

    await fireEvent.press(messageBtn);
    await flush();

    expect(callsOf('privateOpenConversation')).toContainEqual([NIGHT_OWLS, '1001']);
    expect(router.push).toHaveBeenCalled();
  });

  // Fails if Message button is shown when private_messages is false or friends-only condition fails.
  it('a server with private messages off shows no button', async () => {
    core.setPrivateSharedServers([
      {
        serverKey: NIGHT_OWLS,
        name: 'Night Owls',
        connected: true,
        privateMessages: false,
        friendsOnly: false,
        peerUserId: '1001',
      },
    ]);

    await render(<MemberProfile serverKey={NIGHT_OWLS} userId="1001" />);
    await flush();

    expect(screen.queryByLabelText('Message')).toBeNull();
  });

  // Fails if Online, All, Pending, or Blocked tabs do not render correct titles or actions do not reach core.
  it('each friends section does what it says', async () => {
    const contacts: PrivateContactRecord[] = [
      {
        peer: 'friend-online',
        name: 'Online Friend',
        unread: 0,
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
      {
        peer: 'incoming-peer',
        name: 'Incoming Requester',
        unread: 0,
        muted: false,
        requestSpam: false,
        blocked: false,
        reachable: true,
        conversation: ConversationStatus.None,
        friend: FriendStatus.Incoming,
        verification: PrivateVerification.Unverified,
        newKeys: false,
        hidden: false,
        lastActivityMs: 500,
      },
      {
        peer: 'sent-peer',
        name: 'Sent Target',
        unread: 0,
        muted: false,
        requestSpam: false,
        blocked: false,
        reachable: true,
        conversation: ConversationStatus.None,
        friend: FriendStatus.Outgoing,
        verification: PrivateVerification.Unverified,
        newKeys: false,
        hidden: false,
        lastActivityMs: 400,
      },
      {
        peer: 'blocked-peer',
        name: 'Blocked User',
        unread: 0,
        muted: false,
        requestSpam: false,
        blocked: true,
        reachable: false,
        conversation: ConversationStatus.None,
        friend: FriendStatus.None,
        verification: PrivateVerification.Unverified,
        newKeys: false,
        hidden: false,
        lastActivityMs: 300,
      },
    ];

    for (const c of contacts) {
      core.emit(
        MobileEvent.Private.new({
          event: PrivateEventRecord.ContactChanged.new({ contact: c }),
        }),
      );
    }
    await flush();

    await render(<FriendsPage />);
    await flush();

    // 1. Online tab (default)
    expect(screen.getByText(/^ONLINE —/)).toBeTruthy();
    expect(screen.getByText('Online Friend')).toBeTruthy();

    // 2. All tab
    const allTab = screen.getByText('All');
    await fireEvent.press(allTab);
    await flush();
    expect(screen.getByText(/^ALL —/)).toBeTruthy();
    expect(screen.getByText('Online Friend')).toBeTruthy();

    // 3. Pending tab
    const pendingTab = screen.getByText('Pending');
    await fireEvent.press(pendingTab);
    await flush();
    expect(screen.getByText(/^INCOMING —/)).toBeTruthy();
    expect(screen.getByText('Incoming Requester')).toBeTruthy();
    expect(screen.getByText(/^SENT —/)).toBeTruthy();
    expect(screen.getByText('Sent Target')).toBeTruthy();

    // Accept incoming
    const incomingCard = screen.getByTestId('incoming-card-incoming-peer');
    const acceptBtn = within(incomingCard).getByLabelText('Accept');
    await fireEvent.press(acceptBtn);
    await flush();
    expect(callsOf('friendRequestAnswer')).toContainEqual(['incoming-peer', true]);

    // Cancel sent
    const outgoingCard = screen.getByTestId('outgoing-card-sent-peer');
    const cancelBtn = within(outgoingCard).getByLabelText('Cancel');
    await fireEvent.press(cancelBtn);
    await flush();
    expect(callsOf('friendRequestCancel')).toContainEqual(['sent-peer']);

    // 4. Blocked tab
    const blockedTab = screen.getByText('Blocked');
    await fireEvent.press(blockedTab);
    await flush();
    expect(screen.getByText(/^BLOCKED —/)).toBeTruthy();
    expect(screen.getByText('Blocked User')).toBeTruthy();

    // Unblock
    const blockedCard = screen.getByTestId('blocked-card-blocked-peer');
    const unblockBtn = within(blockedCard).getByLabelText('Unblock');
    await fireEvent.press(unblockBtn);
    await flush();
    expect(callsOf('privateUnblock')).toContainEqual(['blocked-peer']);
  });
});
