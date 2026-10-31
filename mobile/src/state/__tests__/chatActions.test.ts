import { ConnectionRecord, MobileEvent, type MessageRecord } from '@opencord/core-native';

import { getCore } from '@/core';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';

import {
  closeChannel,
  loadOlder,
  markReadIfAtBottom,
  openChannel,
} from '../actions/chat';
import { onEvents, readPositions, useAppStore } from '../store';
import { channelKey } from '../types';

describe('chat actions', () => {
  beforeEach(async () => {
    freshStore();
    onEvents([
      new MobileEvent.Connection({ serverKey: NIGHT_OWLS, state: new ConnectionRecord.Connected() }),
      new MobileEvent.Ready({ snapshot: snapshot(NIGHT_OWLS) }),
    ]);
    await settle();
  });

  it('opening a channel loads the newest 50 once', async () => {
    const core = useAppStore.getState().servers[NIGHT_OWLS];
    expect(core).toBeDefined();

    const fetchSpy = jest.spyOn(getCore(), 'messagesFetch');

    await openChannel(NIGHT_OWLS, CHANNELS.general);
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    expect(fetchSpy).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.general, undefined, 50);

    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    const channelMsgs = useAppStore.getState().messages[key];
    expect(channelMsgs).toBeDefined();
    expect(channelMsgs?.loaded).toBe(true);

    // Opening it again when already loaded does not fetch again
    await openChannel(NIGHT_OWLS, CHANNELS.general);
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    fetchSpy.mockRestore();
  });

  it('a message that arrives during the first load shows once, in order', async () => {
    const core = getCore();
    let resolveFetch: (msgs: MessageRecord[]) => void = () => {};
    const fetchPromise = new Promise<MessageRecord[]>((resolve) => {
      resolveFetch = resolve;
    });

    jest.spyOn(core, 'messagesFetch').mockImplementationOnce(() => fetchPromise);

    const openPromise = openChannel(NIGHT_OWLS, CHANNELS.general);

    // While fetch is in flight, a live message arrives
    const liveMsg: MessageRecord = {
      id: '200',
      channelId: CHANNELS.general,
      authorId: '1001',
      content: 'live message',
      createdAtMs: 1761500000000,
      attachments: [],
    };
    onEvents([new MobileEvent.MessageCreated({ serverKey: NIGHT_OWLS, message: liveMsg })]);

    // Now resolve the fetch with an older message and the live message
    const fetchedMsg1: MessageRecord = {
      id: '100',
      channelId: CHANNELS.general,
      authorId: '1001',
      content: 'older message',
      createdAtMs: 1761400000000,
      attachments: [],
    };
    resolveFetch([liveMsg, fetchedMsg1]);

    await openPromise;

    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    const items = useAppStore.getState().messages[key]?.items ?? [];

    expect(items).toHaveLength(2);
    expect(items[0]?.id).toBe('100');
    expect(items[1]?.id).toBe('200');
  });

  it('an older page merges with live messages', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    useAppStore.setState({
      messages: {
        [key]: {
          items: [
            {
              id: '500',
              channelId: CHANNELS.general,
              authorId: '1001',
              content: 'm500',
              createdAtMs: 1761500000000,
              attachments: [],
            },
          ],
          hasOlder: true,
          loadingOlder: false,
          loaded: true,
          pending: [],
        },
      },
    });

    const core = getCore();
    jest.spyOn(core, 'messagesFetch').mockResolvedValueOnce([
      {
        id: '400',
        channelId: CHANNELS.general,
        authorId: '1001',
        content: 'm400',
        createdAtMs: 1761400000000,
        attachments: [],
      },
    ]);

    await loadOlder(NIGHT_OWLS, CHANNELS.general);

    const items = useAppStore.getState().messages[key]?.items ?? [];
    expect(items).toHaveLength(2);
    expect(items[0]?.id).toBe('400');
    expect(items[1]?.id).toBe('500');
  });

  it('switching channels mid-load shows each channel\'s own messages', async () => {
    const core = getCore();
    let resolveGeneral: (msgs: MessageRecord[]) => void = () => {};
    const generalPromise = new Promise<MessageRecord[]>((resolve) => {
      resolveGeneral = resolve;
    });

    jest.spyOn(core, 'messagesFetch').mockImplementation((_server, channelId) => {
      if (channelId === CHANNELS.general) return generalPromise;
      return Promise.resolve([
        {
          id: '900',
          channelId: CHANNELS.offTopic,
          authorId: '1001',
          content: 'offtopic msg',
          createdAtMs: 1761500000000,
          attachments: [],
        },
      ]);
    });

    const p1 = openChannel(NIGHT_OWLS, CHANNELS.general);
    const p2 = openChannel(NIGHT_OWLS, CHANNELS.offTopic);

    resolveGeneral([
      {
        id: '100',
        channelId: CHANNELS.general,
        authorId: '1001',
        content: 'general msg',
        createdAtMs: 1761400000000,
        attachments: [],
      },
    ]);

    await Promise.all([p1, p2]);

    expect(useAppStore.getState().openChannel?.channelId).toBe(CHANNELS.offTopic);

    const genItems = useAppStore.getState().messages[channelKey(NIGHT_OWLS, CHANNELS.general)]?.items ?? [];
    const offItems = useAppStore.getState().messages[channelKey(NIGHT_OWLS, CHANNELS.offTopic)]?.items ?? [];

    expect(genItems).toHaveLength(1);
    expect(genItems[0]?.content).toBe('general msg');

    expect(offItems).toHaveLength(1);
    expect(offItems[0]?.content).toBe('offtopic msg');
  });

  it('loading older stops when a page comes back short', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    useAppStore.setState({
      messages: {
        [key]: {
          items: [
            {
              id: '500',
              channelId: CHANNELS.general,
              authorId: '1001',
              content: 'm500',
              createdAtMs: 1761500000000,
              attachments: [],
            },
          ],
          hasOlder: true,
          loadingOlder: false,
          loaded: true,
          pending: [],
        },
      },
    });

    const core = getCore();
    const fetchSpy = jest.spyOn(core, 'messagesFetch').mockResolvedValueOnce([
      // Only 2 messages returned when limit was 50 (short page)
      { id: '10', channelId: CHANNELS.general, authorId: '1001', content: 'm10', createdAtMs: 100, attachments: [] },
      { id: '20', channelId: CHANNELS.general, authorId: '1001', content: 'm20', createdAtMs: 200, attachments: [] },
    ]);

    await loadOlder(NIGHT_OWLS, CHANNELS.general);

    const channelState = useAppStore.getState().messages[key];
    expect(channelState?.hasOlder).toBe(false);

    // Further loadOlder does nothing
    await loadOlder(NIGHT_OWLS, CHANNELS.general);
    expect(fetchSpy).toHaveBeenCalledTimes(1);

    fetchSpy.mockRestore();
  });

  it('reading is marked only at the bottom while the app is active', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    const initialPos = readPositions.of(NIGHT_OWLS, CHANNELS.general);

    useAppStore.setState({
      appActive: true,
      openChannel: { serverKey: NIGHT_OWLS, channelId: CHANNELS.general, atBottom: false },
      messages: {
        [key]: {
          items: [
            {
              id: '999999',
              channelId: CHANNELS.general,
              authorId: '1001',
              content: 'm999999',
              createdAtMs: 1761500000000,
              attachments: [],
            },
          ],
          hasOlder: false,
          loadingOlder: false,
          loaded: true,
          pending: [],
        },
      },
    });

    // When not at bottom, read position is not updated to the new message
    markReadIfAtBottom(NIGHT_OWLS, CHANNELS.general, false);
    expect(readPositions.of(NIGHT_OWLS, CHANNELS.general)).toBe(initialPos);

    // When at bottom but app inactive, read position is not updated
    useAppStore.setState({ appActive: false });
    markReadIfAtBottom(NIGHT_OWLS, CHANNELS.general, true);
    expect(readPositions.of(NIGHT_OWLS, CHANNELS.general)).toBe(initialPos);

    // When at bottom and app active, read position is updated to newest
    useAppStore.setState({ appActive: true });
    markReadIfAtBottom(NIGHT_OWLS, CHANNELS.general, true);
    expect(readPositions.of(NIGHT_OWLS, CHANNELS.general)).toBe('999999');
  });

  it('closing a channel keeps its newest 100 and lets older ones load again', async () => {
    const key = channelKey(NIGHT_OWLS, CHANNELS.general);
    // Create 150 messages
    const items: MessageRecord[] = Array.from({ length: 150 }, (_, i) => ({
      id: `${i + 1}`,
      channelId: CHANNELS.general,
      authorId: '1001',
      content: `msg ${i + 1}`,
      createdAtMs: 1761500000000 + i * 1000,
      attachments: [],
    }));

    useAppStore.setState({
      openChannel: { serverKey: NIGHT_OWLS, channelId: CHANNELS.general, atBottom: true },
      messages: {
        [key]: {
          items,
          hasOlder: false,
          loadingOlder: false,
          loaded: true,
          pending: [],
        },
      },
    });

    closeChannel();

    expect(useAppStore.getState().openChannel).toBeNull();
    const stored = useAppStore.getState().messages[key];
    expect(stored).toBeDefined();
    expect(stored?.items).toHaveLength(100);
    expect(stored?.items[0]?.id).toBe('51');
    expect(stored?.items[99]?.id).toBe('150');
    expect(stored?.hasOlder).toBe(true);
  });
});
