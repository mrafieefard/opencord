import type { MessageRecord } from '@opencord/core-native';

import { getCore } from '@/core';
import { errorKind } from '@/core/errors';
import { mergeMessages } from '@/state/events';
import { readPositions, useAppStore } from '@/state/store';
import { channelKey } from '@/state/types';

const PAGE_SIZE = 50;
const MAX_KEPT_AFTER_CLOSE = 100;

export async function openChannel(serverKey: string, channelId: string): Promise<void> {
  const currentOpen = useAppStore.getState().openChannel;
  if (!currentOpen || currentOpen.serverKey !== serverKey || currentOpen.channelId !== channelId) {
    useAppStore.setState({
      openChannel: { serverKey, channelId, atBottom: true },
    });
  }

  const key = channelKey(serverKey, channelId);
  const existing = useAppStore.getState().messages[key];
  if (existing?.loaded) {
    if (useAppStore.getState().openChannel?.atBottom) {
      if (useAppStore.getState().openChannel?.atBottom) {
    markReadIfAtBottom(serverKey, channelId, true);
  }
    }
    return;
  }

  useAppStore.setState((state) => {
    const prev = state.messages[key];
    return {
      messages: {
        ...state.messages,
        [key]: {
          items: prev?.items ?? [],
          hasOlder: prev?.hasOlder ?? true,
          loadingOlder: false,
          loaded: false,
          pending: prev?.pending ?? [],
        },
      },
    };
  });

  let fetched: MessageRecord[] = [];
  try {
    fetched = await getCore().messagesFetch(serverKey, channelId, undefined, PAGE_SIZE);
  } catch (error: unknown) {
    console.warn(`failed to fetch messages for ${channelId}: ${errorKind(error)}`);
    return;
  }

  useAppStore.setState((state) => {
    const current = state.messages[key];
    if (!current) return state;

    const merged = mergeMessages(current.items, fetched);
    const hasOlder = fetched.length >= PAGE_SIZE;

    return {
      messages: {
        ...state.messages,
        [key]: {
          ...current,
          items: merged,
          hasOlder,
          loaded: true,
        },
      },
    };
  });

  if (useAppStore.getState().openChannel?.atBottom) {
    markReadIfAtBottom(serverKey, channelId, true);
  }
}

export function closeChannel(): void {
  const open = useAppStore.getState().openChannel;
  if (!open) return;

  const key = channelKey(open.serverKey, open.channelId);
  useAppStore.setState((state) => {
    const current = state.messages[key];
    if (!current) return { ...state, openChannel: null };

    const keptItems = current.items.slice(-MAX_KEPT_AFTER_CLOSE);
    return {
      ...state,
      openChannel: null,
      messages: {
        ...state.messages,
        [key]: {
          ...current,
          items: keptItems,
          hasOlder: true,
          loaded: false,
        },
      },
    };
  });
}

export async function loadOlder(serverKey: string, channelId: string): Promise<void> {
  const key = channelKey(serverKey, channelId);
  const current = useAppStore.getState().messages[key];
  if (!current || current.loadingOlder || !current.hasOlder) return;

  const before = current.items[0]?.id;
  if (!before) return;

  useAppStore.setState((state) => {
    const cur = state.messages[key];
    if (!cur) return state;
    return {
      messages: {
        ...state.messages,
        [key]: { ...cur, loadingOlder: true },
      },
    };
  });

  let fetched: MessageRecord[] = [];
  try {
    fetched = await getCore().messagesFetch(serverKey, channelId, before, PAGE_SIZE);
  } catch (error: unknown) {
    console.warn(`failed to load older messages for ${channelId}: ${errorKind(error)}`);
    useAppStore.setState((state) => {
      const cur = state.messages[key];
      if (!cur) return state;
      return {
        messages: {
          ...state.messages,
          [key]: { ...cur, loadingOlder: false },
        },
      };
    });
    return;
  }

  useAppStore.setState((state) => {
    const cur = state.messages[key];
    if (!cur) return state;

    const merged = mergeMessages(cur.items, fetched);
    const hasOlder = fetched.length >= PAGE_SIZE;

    return {
      messages: {
        ...state.messages,
        [key]: {
          ...cur,
          items: merged,
          hasOlder,
          loadingOlder: false,
        },
      },
    };
  });
}

export function markReadIfAtBottom(serverKey: string, channelId: string, atBottom: boolean): void {
  useAppStore.setState((state) => {
    const open = state.openChannel;
    if (open && open.serverKey === serverKey && open.channelId === channelId) {
      return { openChannel: { ...open, atBottom } };
    }
    return state;
  });

  const state = useAppStore.getState();
  if (!atBottom || !state.appActive) return;

  const key = channelKey(serverKey, channelId);
  const channelMsgs = state.messages[key];
  const newest = channelMsgs?.items[channelMsgs.items.length - 1];
  if (!newest) return;

  readPositions.save(serverKey, channelId, newest.id);

  useAppStore.setState((s) => {
    const server = s.servers[serverKey];
    if (!server) return s;

    const activity = server.activity[channelId];
    if (!activity) return s;

    return {
      servers: {
        ...s.servers,
        [serverKey]: {
          ...server,
          activity: {
            ...server.activity,
            [channelId]: {
              ...activity,
              unread: 0,
              mentions: 0,
              lastReadId: newest.id,
            },
          },
        },
      },
    };
  });
}
