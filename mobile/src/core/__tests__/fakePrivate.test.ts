import {
  ConversationStatus,
  FriendStatus,
  MobileEvent,
  OwnDeviceState,
  PrivateEventRecord,
  PrivateSendState,
  PrivateVerification,
} from '@opencord/core-native';

import { FakeCore } from '../fake/FakeCore';

async function drain(core: FakeCore, generation: bigint): Promise<MobileEvent[]> {
  const seen: MobileEvent[] = [];
  while ((core as unknown as { queue: MobileEvent[] }).queue.length > 0) {
    const batch = await core.nextEvents(generation, 256);
    seen.push(...batch);
  }
  return seen;
}

describe('FakeCore private world', () => {
  // Fails if privateSend does not emit MessageAdded in Sending or does not transition to Sent.
  it('the fake echoes a send through both states', async () => {
    jest.useFakeTimers();
    try {
      const core = new FakeCore();
      const generation = core.eventsAttach();

      const sent = await core.privateSend('kai-peer', 'Hello Kai!');
      expect(sent.content).toBe('Hello Kai!');
      expect(sent.state).toBe(PrivateSendState.Sending);

      // Event 1: MessageAdded in Sending state
      const initialEvents = await drain(core, generation);
      const addedEvent = initialEvents
        .filter(MobileEvent.Private.instanceOf)
        .find((e) => PrivateEventRecord.MessageAdded.instanceOf(e.inner.event));
      expect(addedEvent).toBeDefined();
      if (addedEvent && PrivateEventRecord.MessageAdded.instanceOf(addedEvent.inner.event)) {
        expect(addedEvent.inner.event.inner.message.id).toBe(sent.id);
        expect(addedEvent.inner.event.inner.message.state).toBe(PrivateSendState.Sending);
      }

      // After 400 ms: MessageChanged in Sent state
      await jest.advanceTimersByTimeAsync(400);
      const changedEvents = await drain(core, generation);
      const changedEvent = changedEvents
        .filter(MobileEvent.Private.instanceOf)
        .find((e) => PrivateEventRecord.MessageChanged.instanceOf(e.inner.event));
      expect(changedEvent).toBeDefined();
      if (changedEvent && PrivateEventRecord.MessageChanged.instanceOf(changedEvent.inner.event)) {
        expect(changedEvent.inner.event.inner.message.id).toBe(sent.id);
        expect(changedEvent.inner.event.inner.message.state).toBe(PrivateSendState.Sent);
      }

      // After 2 s: Kai types back
      await jest.advanceTimersByTimeAsync(1600);
      const typingEvents = await drain(core, generation);
      const typingEvent = typingEvents
        .filter(MobileEvent.Private.instanceOf)
        .find((e) => PrivateEventRecord.Typing.instanceOf(e.inner.event));
      expect(typingEvent).toBeDefined();
      if (typingEvent && PrivateEventRecord.Typing.instanceOf(typingEvent.inner.event)) {
        expect(typingEvent.inner.event.inner.peer).toBe('kai-peer');
      }
    } finally {
      jest.useRealTimers();
    }
  });

  // Fails if any expected contact kind (Kai, Mira, Rowan, Sol, Spam, Vex, Ash, or own device) is missing or mismatched.
  it('the fake has every contact kind', async () => {
    const core = new FakeCore();
    const contacts = await core.privateContacts();

    // Kai: online, verified, friend
    const kai = contacts.find((c) => c.name === 'Kai');
    expect(kai).toBeDefined();
    expect(kai?.friend).toBe(FriendStatus.Friends);
    expect(kai?.verification).toBe(PrivateVerification.Verified);
    expect(kai?.conversation).toBe(ConversationStatus.Accepted);
    expect(kai?.reachable).toBe(true);

    // Kai has a 60-message chat with an edited and a deleted message and one failed send
    if (kai) {
      const messages = await core.privateMessages(kai.peer, undefined, 100);
      expect(messages.length).toBe(60);
      expect(messages.some((m) => m.editedAtMs !== undefined)).toBe(true);
      expect(messages.some((m) => m.state === PrivateSendState.Failed)).toBe(true);
    }

    // Mira: not verified, new_keys: true
    const mira = contacts.find((c) => c.name === 'Mira');
    expect(mira).toBeDefined();
    expect(mira?.friend).toBe(FriendStatus.Friends);
    expect(mira?.verification).toBe(PrivateVerification.Unverified);
    expect(mira?.newKeys).toBe(true);

    // Rowan: incoming friend request
    const rowan = contacts.find((c) => c.name === 'Rowan');
    expect(rowan).toBeDefined();
    expect(rowan?.friend).toBe(FriendStatus.Incoming);
    expect(rowan?.requestSpam).toBe(false);

    // Sol: outgoing friend request
    const sol = contacts.find((c) => c.name === 'Sol');
    expect(sol).toBeDefined();
    expect(sol?.friend).toBe(FriendStatus.Outgoing);
    expect(sol?.requestSpam).toBe(false);

    // Spam request
    const spam = contacts.find((c) => c.requestSpam);
    expect(spam).toBeDefined();
    expect(spam?.friend).toBe(FriendStatus.Incoming);

    // Vex: blocked
    const vex = contacts.find((c) => c.name === 'Vex');
    expect(vex).toBeDefined();
    expect(vex?.blocked).toBe(true);

    // Ash: message request
    const ash = contacts.find((c) => c.name === 'Ash');
    expect(ash).toBeDefined();
    expect(ash?.conversation).toBe(ConversationStatus.Request);

    // Own devices waiting
    const own = await core.privateOwnDevices();
    expect(own.hasChats).toBe(true);
    expect(own.devices.some((d) => d.state === OwnDeviceState.Offering)).toBe(true);
  });
});
