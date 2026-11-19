import {
  ConversationStatus,
  MobileEvent,
  PrivateEventRecord,
  PrivateMessageKind,
  PrivateSendState,
  type PrivateContactRecord,
  type PrivateMessageRecord,
} from '@opencord/core-native';
import { act, fireEvent, render, screen } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';

import { startEventLoop, type EventLoop } from '@/core';
import type { FakeCore } from '@/core/fake/FakeCore';
import { MIRA_PEER } from '@/core/fake/privateWorld';
import { initialState, onEvents, useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';
import * as ui from '@/ui';

import { PrivateChat } from '../PrivateChat';
import { buildPrivateItems } from '../privateAdapter';

jest.mock('@/ui', () => ({ ...jest.requireActual<object>('@/ui'), confirm: jest.fn() }));

// Mira has no messages in the fake world, so every bubble here is the
// test's own, and her relay (Night Owls) is a shared, connected server:
// the composer shows.
const PEER = MIRA_PEER;

let core: FakeCore;
let loop: EventLoop;

/** The core's events reach the store the app's way: the event loop. */
function startLoop() {
  loop = startEventLoop(core, onEvents);
}

async function flush() {
  await act(async () => {
    await settle();
  });
}

async function advance(ms: number) {
  await act(async () => {
    await jest.advanceTimersByTimeAsync(ms);
    await settle();
  });
}

async function openChat() {
  const view = await render(
    <GestureHandlerRootView style={{ flex: 1 }}>
      <PrivateChat peer={PEER} />
    </GestureHandlerRootView>,
  );
  await flush();
  return view;
}

function callsOf(method: string): unknown[][] {
  return core.calls.filter((call) => call.method === method).map((call) => call.args);
}

async function sendFromComposer(text: string) {
  await fireEvent.changeText(screen.getByTestId('composer-input'), text);
  await fireEvent.press(screen.getByTestId('composer-send'));
  await flush();
}

/** A bubble's text. */
function bubbleWith(text: string) {
  return screen.queryByText(new RegExp(`^${text.replace(/[!?.]/g, '\\$&')}`));
}

function lastSent(): PrivateMessageRecord {
  const sent = (useAppStore.getState().private.messages[PEER] ?? []).at(-1);
  if (!sent) throw new Error('nothing sent');
  return sent;
}

/** The core says a message's state changed, as it does after a failed delivery. */
async function coreChanges(message: PrivateMessageRecord) {
  core.emit(MobileEvent.Private.new({ event: PrivateEventRecord.MessageChanged.new({ message }) }));
  await flush();
}

describe('PrivateChat', () => {
  beforeEach(async () => {
    // setImmediate stays real: settle() waits on it.
    jest.useFakeTimers({ doNotFake: ['setImmediate'], now: 1_761_500_000_000 });
    core = freshStore();
    startLoop();
    await flush();
    jest.clearAllMocks();
  });

  afterEach(() => {
    loop.stop();
    // Ends the stopped loop's wait.
    core.eventsAttach();
    jest.useRealTimers();
  });

  // Fails if a message loaded in Sending shows no clock (the adapter drops
  // the state), or if the phone sends it again itself instead of the core.
  it('a waiting message sends after restart', async () => {
    const first = await openChat();
    await sendFromComposer('see you tomorrow');
    expect(screen.getByTestId('bubble-sending')).toBeTruthy();
    const waiting = lastSent();
    expect(waiting.state).toBe(PrivateSendState.Sending);

    // The app is killed: the store goes, the core keeps its outbox.
    await first.unmount();
    loop.stop();
    useAppStore.setState(initialState(), true);
    startLoop();

    await openChat();
    expect(bubbleWith('see you tomorrow')).toBeTruthy();
    expect(screen.getByTestId('bubble-sending')).toBeTruthy();

    // The core delivers it after the start: nothing on the phone resends.
    await advance(400);
    expect(screen.queryByTestId('bubble-sending')).toBeNull();
    expect(bubbleWith('see you tomorrow')).toBeTruthy();
    expect(callsOf('privateSend')).toEqual([[PEER, 'see you tomorrow']]);
    expect(callsOf('privateRetry')).toEqual([]);
  });

  // Fails if Retry doesn't call private_retry with the message's id, or the
  // bubble doesn't follow the core back to Sending and Sent. Twice in a row.
  it('a failed message retries', async () => {
    await openChat();
    await sendFromComposer('are you there?');
    const message = lastSent();
    // Delivery fails instead of the fake's Sent.
    jest.clearAllTimers();

    for (let attempt = 1; attempt <= 2; attempt += 1) {
      await coreChanges({ ...message, state: PrivateSendState.Failed });
      expect(screen.getByText('Not sent. Tap to retry')).toBeTruthy();

      await fireEvent.press(screen.getByText('Not sent. Tap to retry'));
      await flush();
      expect(callsOf('privateRetry')).toHaveLength(attempt);
      expect(callsOf('privateRetry')[attempt - 1]).toEqual([message.id]);
      expect(screen.queryByText('Not sent. Tap to retry')).toBeNull();
      expect(screen.getByTestId('bubble-sending')).toBeTruthy();

      await advance(400);
      expect(screen.queryByTestId('bubble-sending')).toBeNull();
    }
  });

  // Fails if Edit doesn't fill the composer and save through private_edit,
  // or Delete doesn't ask and then call private_delete.
  it('edit and delete reach the core', async () => {
    await openChat();
    await sendFromComposer('helo');
    await advance(400);
    const message = lastSent();

    for (const text of ['hello', 'hello!']) {
      await fireEvent(screen.getByTestId('private-message-menu'), 'onAction', 'edit');
      expect(screen.getByText('Editing message')).toBeTruthy();
      await fireEvent.changeText(screen.getByTestId('composer-input'), text);
      await fireEvent.press(screen.getByTestId('composer-send'));
      await flush();
      expect(callsOf('privateEdit').at(-1)).toEqual([PEER, message.id, text]);
      expect(screen.queryByText('Editing message')).toBeNull();
      expect(bubbleWith(text)).toBeTruthy();
    }
    expect(callsOf('privateEdit')).toHaveLength(2);

    const confirm = jest.mocked(ui.confirm);
    await fireEvent(screen.getByTestId('private-message-menu'), 'onAction', 'delete');
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(confirm.mock.calls[0]?.[0]).toMatchObject({
      title: 'Delete message?',
      message: 'Are you sure you want to delete this message?',
      confirmTitle: 'Delete',
    });
    expect(callsOf('privateDelete')).toEqual([]);
    await act(async () => confirm.mock.calls[0]?.[0].onConfirm());
    await flush();
    expect(callsOf('privateDelete')).toEqual([[PEER, message.id]]);
    expect(bubbleWith('hello!')).toBeNull();
  });

  // Fails if opening the chat doesn't read it, or what arrives while it's
  // open stays unread.
  it('opening marks read', async () => {
    await openChat();
    expect(callsOf('privateMarkRead')).toEqual([[PEER]]);

    for (const [index, unread] of [1, 2].entries()) {
      const contact = useAppStore.getState().private.contacts[PEER];
      if (!contact) throw new Error('no contact');
      const incoming: PrivateMessageRecord = {
        id: `00000000000090${index}`,
        peer: PEER,
        outgoing: false,
        kind: PrivateMessageKind.Text,
        content: `ping ${unread}`,
        sentAtMs: Date.now(),
        state: PrivateSendState.Received,
      };
      const changed: PrivateContactRecord = { ...contact, unread };
      core.emit(
        MobileEvent.Private.new({ event: PrivateEventRecord.MessageAdded.new({ message: incoming }) }),
        MobileEvent.Private.new({ event: PrivateEventRecord.ContactChanged.new({ contact: changed }) }),
      );
      await flush();
      expect(callsOf('privateMarkRead')).toHaveLength(index + 2);
      expect(useAppStore.getState().private.contacts[PEER]?.unread).toBe(0);
    }
  });

  // Fails if a request shows the composer.
  it('a request shows the request bar', async () => {
    await openChat();
    const current = useAppStore.getState().private.contacts[PEER];
    if (!current) throw new Error('no contact');
    core.emit(
      MobileEvent.Private.new({
        event: PrivateEventRecord.ContactChanged.new({ contact: { ...current, conversation: ConversationStatus.Request } }),
      }),
    );
    await flush();
    expect(screen.getByText('Mira wants to message you.')).toBeTruthy();
    expect(screen.getByTestId('request-bar')).toBeTruthy();
    expect(screen.queryByTestId('composer-input')).toBeNull();
  });
});

describe('privateAdapter', () => {
  const base: PrivateMessageRecord = {
    id: '01',
    peer: PEER,
    outgoing: true,
    kind: PrivateMessageKind.Text,
    content: 'a',
    sentAtMs: 1_761_500_000_000,
    state: PrivateSendState.Sent,
  };

  // Fails if a relay change doesn't end a group, or a key change loses the desktop's words.
  it('rows other than text end a group and say what the desktop says', () => {
    const items = buildPrivateItems(
      [
        base,
        { ...base, id: '02', content: 'b', sentAtMs: base.sentAtMs + 1000 },
        { ...base, id: '03', kind: PrivateMessageKind.Divider, content: 'Night Owls', sentAtMs: base.sentAtMs + 2000 },
        { ...base, id: '04', content: 'c', sentAtMs: base.sentAtMs + 3000 },
        { ...base, id: '05', outgoing: false, kind: PrivateMessageKind.KeysChanged, content: '', sentAtMs: base.sentAtMs + 4000 },
        { ...base, id: '06', state: PrivateSendState.Failed, sentAtMs: base.sentAtMs + 5000 },
      ],
      'Mira',
    );
    expect(items.map((item) => item.type)).toEqual(['day', 'message', 'message', 'relay', 'message', 'note', 'message']);
    const positions = items.flatMap((item) => (item.type === 'message' ? [item.position] : []));
    expect(positions).toEqual(['first', 'last', 'single', 'single']);
    expect(items[5]).toMatchObject({ type: 'note', text: "Mira's keys changed." });
    expect(items[6]).toMatchObject({ type: 'message', sendState: 'failed', own: true });
  });
});
