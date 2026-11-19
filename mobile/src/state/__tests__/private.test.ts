import {
  ConversationStatus,
  FriendStatus,
  MobileEvent,
  PrivateContactRecord,
  PrivateEventRecord,
  PrivateMessageKind,
  PrivateMessageRecord,
  PrivateSendState,
  PrivateVerification,
} from '@opencord/core-native';

import { act, renderHook } from '@testing-library/react-native';
import { startEventLoop, type EventLoop } from '@/core';
import type { FakeCore } from '@/core/fake/FakeCore';
import { KAI_PEER, MIRA_PEER } from '@/core/fake/privateWorld';
import { freshStore, settle } from '@/test/store';
import {
  hidePrivate,
  loadOlder,
  loadPrivate,
  setPrivateMuted,
} from '../actions/private';
import {
  selectPrivateContacts,
  selectPrivateUnreadTotal,
  useOwnDevices,
  usePrivateContacts,
  usePrivateMessages,
  usePrivateTyping,
  usePrivateUnreadTotal,
} from '../selectors';
import { onEvents, useAppStore } from '../store';

function createContact(peer: string, overrides: Partial<PrivateContactRecord> = {}): PrivateContactRecord {
  return {
    peer,
    name: `User ${peer}`,
    friend: FriendStatus.None,
    requestSpam: false,
    blocked: false,
    conversation: ConversationStatus.Accepted,
    unread: 0,
    muted: false,
    hidden: false,
    newKeys: false,
    verification: PrivateVerification.Unverified,
    reachable: true,
    ...overrides,
  };
}

function createMessage(peer: string, id: string, overrides: Partial<PrivateMessageRecord> = {}): PrivateMessageRecord {
  return {
    id,
    peer,
    outgoing: true,
    kind: PrivateMessageKind.Text,
    content: 'hello',
    sentAtMs: 1000,
    state: PrivateSendState.Sending,
    ...overrides,
  };
}

describe('private store', () => {
  let core: FakeCore;
  let loop: EventLoop;

  beforeEach(() => {
    core = freshStore();
    loop = startEventLoop(core, onEvents);
  });

  afterEach(() => {
    loop?.stop();
  });

  async function flush() {
    await act(async () => {
      await settle();
    });
  }

  // Fails if loadPrivate overwrites contacts that were updated by events prior to initial load.
  it('events before the first load apply after it', async () => {
    // 1. Send a ContactChanged event for KAI_PEER (which exists in fake core) before loadPrivate()
    const eventContact = createContact(KAI_PEER, { name: 'Kai (Updated Via Event)', unread: 42, muted: true });
    onEvents([
      MobileEvent.Private.new({
        event: PrivateEventRecord.ContactChanged.new({ contact: eventContact }),
      }),
    ]);

    // Ensure the event modified store state
    expect(useAppStore.getState().private.contacts[KAI_PEER]?.name).toBe('Kai (Updated Via Event)');
    expect(useAppStore.getState().private.contacts[KAI_PEER]?.unread).toBe(42);

    // 2. Now call loadPrivate(), which returns the core's baseline contacts
    await loadPrivate();

    // 3. The contact touched by the earlier event must retain its event-updated state
    const contactAfter = useAppStore.getState().private.contacts[KAI_PEER];
    expect(contactAfter).toBeDefined();
    expect(contactAfter?.name).toBe('Kai (Updated Via Event)');
    expect(contactAfter?.unread).toBe(42);
    expect(contactAfter?.muted).toBe(true);
  });

  // Fails if an event arriving while loadPrivate is awaiting core overwrites the event-updated contact.
  it('events during the load apply after it', async () => {
    // Start loadPrivate (in flight)
    const loadPromise = loadPrivate();

    // An event arrives for MIRA_PEER while load is in flight
    const eventMira = createContact(MIRA_PEER, { name: 'Mira (In Flight Event)', unread: 77, muted: true });
    onEvents([
      MobileEvent.Private.new({
        event: PrivateEventRecord.ContactChanged.new({ contact: eventMira }),
      }),
    ]);

    await loadPromise;

    const miraAfter = useAppStore.getState().private.contacts[MIRA_PEER];
    expect(miraAfter).toBeDefined();
    expect(miraAfter?.name).toBe('Mira (In Flight Event)');
    expect(miraAfter?.unread).toBe(77);
    expect(miraAfter?.muted).toBe(true);
  });

  // Fails if message state updates do not apply in order or discard subsequent state transitions.
  it('a message changing state twice ends in the last', async () => {
    const peer = 'peer-2';
    const msgId = 'msg-1';

    // 1. Initial message added with Sending state
    onEvents([
      MobileEvent.Private.new({
        event: PrivateEventRecord.MessageAdded.new({
          message: createMessage(peer, msgId, { state: PrivateSendState.Sending }),
        }),
      }),
    ]);
    expect(useAppStore.getState().private.messages[peer]?.[0]?.state).toBe(PrivateSendState.Sending);

    // 2. First state change: Sent
    onEvents([
      MobileEvent.Private.new({
        event: PrivateEventRecord.MessageChanged.new({
          message: createMessage(peer, msgId, { state: PrivateSendState.Sent }),
        }),
      }),
    ]);
    expect(useAppStore.getState().private.messages[peer]?.[0]?.state).toBe(PrivateSendState.Sent);

    // 3. Second state change: Failed (or edited)
    onEvents([
      MobileEvent.Private.new({
        event: PrivateEventRecord.MessageChanged.new({
          message: createMessage(peer, msgId, { state: PrivateSendState.Failed }),
        }),
      }),
    ]);
    expect(useAppStore.getState().private.messages[peer]?.[0]?.state).toBe(PrivateSendState.Failed);
  });

  // Fails if hiding does not remove a contact from chats or if a new message does not unhide it.
  it('a hidden chat leaves the list and comes back with a new message', async () => {
    const peer = 'kai-peer';
    // Load baseline contacts (which includes Kai as accepted and unhidden)
    await loadPrivate();

    const initialChats = selectPrivateContacts(useAppStore.getState().private.contacts, 'chats');
    expect(initialChats.some((c) => c.peer === peer)).toBe(true);

    // Hide the chat
    await hidePrivate(peer);
    await flush();
    const chatsAfterHide = selectPrivateContacts(useAppStore.getState().private.contacts, 'chats');
    expect(chatsAfterHide.some((c) => c.peer === peer)).toBe(false);

    // A new message arrives for Kai
    onEvents([
      MobileEvent.Private.new({
        event: PrivateEventRecord.MessageAdded.new({
          message: createMessage(peer, 'new-msg-1', { content: 'Hey there!' }),
        }),
      }),
    ]);

    // The chat returns to the chats list
    const chatsAfterNewMsg = selectPrivateContacts(useAppStore.getState().private.contacts, 'chats');
    expect(chatsAfterNewMsg.some((c) => c.peer === peer)).toBe(true);
  });

  // Fails if unread total includes unread messages from muted contacts.
  it('unread total skips muted', async () => {
    // Add two contacts with unread messages: one unmuted, one muted
    const contact1 = createContact('peer-unmuted', { unread: 4, muted: false });
    const contact2 = createContact('peer-muted', { unread: 7, muted: true });

    // Seed into fake core's contacts map as well so setPrivateMuted emits ContactChanged
    core.privateContactsMap.set('peer-unmuted', contact1);
    core.privateContactsMap.set('peer-muted', contact2);

    onEvents([
      MobileEvent.Private.new({
        event: PrivateEventRecord.ContactChanged.new({ contact: contact1 }),
      }),
      MobileEvent.Private.new({
        event: PrivateEventRecord.ContactChanged.new({ contact: contact2 }),
      }),
    ]);

    expect(selectPrivateUnreadTotal(useAppStore.getState().private.contacts)).toBe(4);

    // Muting the first contact drops the total to 0
    await setPrivateMuted('peer-unmuted', true);
    await flush();
    expect(selectPrivateUnreadTotal(useAppStore.getState().private.contacts)).toBe(0);

    // Unmuting the second contact raises the total to 7
    await setPrivateMuted('peer-muted', false);
    await flush();
    expect(selectPrivateUnreadTotal(useAppStore.getState().private.contacts)).toBe(7);
  });

  // Fails if hook selectors or loadOlder pagination fail to retrieve store data.
  it('hooks and loadOlder provide reactive views over private state', async () => {
    await loadPrivate();

    // 1. usePrivateContacts filters
    const { result: chats } = await renderHook(() => usePrivateContacts('chats'));
    expect(chats.current.length).toBeGreaterThan(0);

    const { result: friends } = await renderHook(() => usePrivateContacts('friends'));
    expect(friends.current.some((c) => c.name === 'Kai')).toBe(true);

    const { result: pending } = await renderHook(() => usePrivateContacts('pending'));
    expect(pending.current.some((c) => c.name === 'Rowan' || c.name === 'Sol')).toBe(true);

    const { result: blocked } = await renderHook(() => usePrivateContacts('blocked'));
    expect(blocked.current.some((c) => c.name === 'Vex')).toBe(true);

    const { result: requests } = await renderHook(() => usePrivateContacts('requests'));
    expect(requests.current.some((c) => c.name === 'Ash')).toBe(true);

    // 2. useOwnDevices
    const { result: ownDevices } = await renderHook(() => useOwnDevices());
    expect(ownDevices.current.hasChats).toBe(true);
    expect(ownDevices.current.devices.length).toBeGreaterThan(0);

    // 3. usePrivateUnreadTotal
    const { result: unreadTotal } = await renderHook(() => usePrivateUnreadTotal());
    expect(typeof unreadTotal.current).toBe('number');

    // 4. usePrivateTyping
    const { result: isTyping } = await renderHook(() => usePrivateTyping('kai-peer'));
    expect(isTyping.current).toBe(false);

    await act(async () => {
      onEvents([
        MobileEvent.Private.new({
          event: PrivateEventRecord.Typing.new({ peer: 'kai-peer' }),
        }),
      ]);
    });
    const { result: isTypingNow } = await renderHook(() => usePrivateTyping('kai-peer'));
    expect(isTypingNow.current).toBe(true);

    // 5. usePrivateMessages and loadOlder
    const { result: msgs } = await renderHook(() => usePrivateMessages('kai-peer'));
    expect(msgs.current).toBeDefined();

    // loadOlder for Kai
    await act(async () => {
      await loadOlder('kai-peer');
    });
    const { result: msgsAfterOlder } = await renderHook(() => usePrivateMessages('kai-peer'));
    expect(msgsAfterOlder.current.length).toBeGreaterThanOrEqual(1);
  });
});
