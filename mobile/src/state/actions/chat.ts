import type { MessageRecord } from '@opencord/core-native';

import { describeError, getCore } from '@/core';
import { errorKind } from '@/core/errors';
import { EMPTY_MESSAGES, mergeMessages } from '@/state/events';
import { readPositions, useAppStore } from '@/state/store';
import { channelKey, type PendingSend, type PickedFile } from '@/state/types';

import { toast } from './ui';

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
      markReadIfAtBottom(serverKey, channelId, true);
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
    // Otherwise the chat just stays empty.
    toast(describeError(error), 'error');
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
    toast(describeError(error), 'error');
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

export async function sendMessage(serverKey: string, channelId: string, content: string): Promise<void> {
  const nonce = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  const key = channelKey(serverKey, channelId);
  const pendingSend: PendingSend = {
    nonce,
    serverKey,
    channelId,
    content,
    files: [],
    state: 'sending',
    createdAtMs: Date.now(),
  };

  useAppStore.setState((state) => {
    const cur = state.messages[key] ?? EMPTY_MESSAGES;
    return {
      messages: {
        ...state.messages,
        [key]: { ...cur, pending: [...cur.pending, pendingSend] },
      },
    };
  });

  try {
    const sent = await getCore().messageSend(serverKey, channelId, content, nonce);
    useAppStore.setState((state) => {
      const cur = state.messages[key];
      if (!cur) return state;
      const nextPending = cur.pending.filter((p) => p.nonce !== nonce);
      const nextItems = mergeMessages(cur.items, [sent]);
      return {
        messages: {
          ...state.messages,
          [key]: { ...cur, items: nextItems, pending: nextPending },
        },
      };
    });
  } catch (error: unknown) {
    useAppStore.setState((state) => {
      const cur = state.messages[key];
      if (!cur) return state;
      return {
        messages: {
          ...state.messages,
          [key]: {
            ...cur,
            pending: cur.pending.map((p) =>
              p.nonce === nonce ? { ...p, state: 'failed', error: describeError(error) } : p,
            ),
          },
        },
      };
    });
  }
}

export async function sendWithFiles(
  serverKey: string,
  channelId: string,
  content: string,
  files: PickedFile[],
): Promise<void> {
  const nonce = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  const key = channelKey(serverKey, channelId);
  const pendingSend: PendingSend = {
    nonce,
    serverKey,
    channelId,
    content,
    files,
    state: 'sending',
    createdAtMs: Date.now(),
  };

  useAppStore.setState((state) => {
    const cur = state.messages[key] ?? EMPTY_MESSAGES;
    return {
      messages: {
        ...state.messages,
        [key]: { ...cur, pending: [...cur.pending, pendingSend] },
      },
    };
  });

  try {
    const outgoing = files.map((f) => ({ path: f.path, filename: f.filename }));
    const sent = await getCore().messageSendFiles(serverKey, channelId, content, nonce, outgoing);
    useAppStore.setState((state) => {
      const cur = state.messages[key];
      if (!cur) return state;
      const nextPending = cur.pending.filter((p) => p.nonce !== nonce);
      const nextItems = mergeMessages(cur.items, [sent]);
      return {
        messages: {
          ...state.messages,
          [key]: { ...cur, items: nextItems, pending: nextPending },
        },
      };
    });
  } catch (error: unknown) {
    useAppStore.setState((state) => {
      const cur = state.messages[key];
      if (!cur) return state;
      const isCancelled = (error as Error)?.message === 'Cancelled';
      return {
        messages: {
          ...state.messages,
          [key]: {
            ...cur,
            pending: cur.pending.map((p) =>
              p.nonce === nonce ? { ...p, state: 'failed', error: isCancelled ? 'Cancelled' : describeError(error) } : p,
            ),
          },
        },
      };
    });
  }
}

function findPending(nonce: string): { key: string; pending: PendingSend } | null {
  const messages = useAppStore.getState().messages;
  for (const [k, channelMsgs] of Object.entries(messages)) {
    const match = channelMsgs.pending.find((p) => p.nonce === nonce);
    if (match) return { key: k, pending: match };
  }
  return null;
}

export async function retrySend(nonce: string): Promise<void> {
  const found = findPending(nonce);
  if (!found) return;

  const { key, pending } = found;
  useAppStore.setState((state) => {
    const cur = state.messages[key];
    if (!cur) return state;
    return {
      messages: {
        ...state.messages,
        [key]: {
          ...cur,
          pending: cur.pending.map((p) =>
            p.nonce === nonce ? { ...p, state: 'sending', error: undefined } : p,
          ),
        },
      },
    };
  });

  try {
    let sent: MessageRecord;
    if (pending.files.length > 0) {
      const outgoing = pending.files.map((f) => ({ path: f.path, filename: f.filename }));
      sent = await getCore().messageSendFiles(
        pending.serverKey,
        pending.channelId,
        pending.content,
        nonce,
        outgoing,
      );
    } else {
      sent = await getCore().messageSend(pending.serverKey, pending.channelId, pending.content, nonce);
    }
    useAppStore.setState((state) => {
      const cur = state.messages[key];
      if (!cur) return state;
      const nextPending = cur.pending.filter((p) => p.nonce !== nonce);
      const nextItems = mergeMessages(cur.items, [sent]);
      return {
        messages: {
          ...state.messages,
          [key]: { ...cur, items: nextItems, pending: nextPending },
        },
      };
    });
  } catch (error: unknown) {
    useAppStore.setState((state) => {
      const cur = state.messages[key];
      if (!cur) return state;
      return {
        messages: {
          ...state.messages,
          [key]: {
            ...cur,
            pending: cur.pending.map((p) =>
              p.nonce === nonce ? { ...p, state: 'failed', error: describeError(error) } : p,
            ),
          },
        },
      };
    });
  }
}

export function discardSend(nonce: string): void {
  const found = findPending(nonce);
  if (!found) return;
  const { key } = found;
  useAppStore.setState((state) => {
    const cur = state.messages[key];
    if (!cur) return state;
    return {
      messages: {
        ...state.messages,
        [key]: {
          ...cur,
          pending: cur.pending.filter((p) => p.nonce !== nonce),
        },
      },
    };
  });
}

export function cancelSend(nonce: string): void {
  const found = findPending(nonce);
  if (!found) return;
  const { key, pending } = found;
  try {
    getCore().sendCancel(pending.serverKey, nonce);
  } catch (error: unknown) {
    console.warn(`sendCancel failed: ${errorKind(error)}`);
  }
  useAppStore.setState((state) => {
    const cur = state.messages[key];
    if (!cur) return state;
    return {
      messages: {
        ...state.messages,
        [key]: {
          ...cur,
          pending: cur.pending.map((p) =>
            p.nonce === nonce ? { ...p, state: 'failed', error: 'Cancelled' } : p,
          ),
        },
      },
    };
  });
}

export async function editMessage(serverKey: string, messageId: string, content: string): Promise<void> {
  const updated = await getCore().messageEdit(serverKey, messageId, content);
  const key = channelKey(serverKey, updated.channelId);
  useAppStore.setState((state) => {
    const cur = state.messages[key];
    if (!cur) return state;
    return {
      messages: {
        ...state.messages,
        [key]: {
          ...cur,
          items: cur.items.map((item) => (item.id === messageId ? updated : item)),
        },
      },
    };
  });
}

export async function deleteMessage(serverKey: string, messageId: string): Promise<void> {
  await getCore().messageDelete(serverKey, messageId);
  useAppStore.setState((state) => {
    const nextMessages = { ...state.messages };
    for (const [k, channelMsgs] of Object.entries(nextMessages)) {
      if (channelMsgs.items.some((item) => item.id === messageId)) {
        nextMessages[k] = {
          ...channelMsgs,
          items: channelMsgs.items.filter((item) => item.id !== messageId),
        };
      }
    }
    return { messages: nextMessages };
  });
}

const lastTypingSent = new Map<string, number>();
const TYPING_THROTTLE_MS = 8000;

export async function notifyTyping(serverKey: string, channelId: string): Promise<void> {
  const key = channelKey(serverKey, channelId);
  const now = Date.now();
  const last = lastTypingSent.get(key) ?? 0;
  if (now - last < TYPING_THROTTLE_MS) return;
  lastTypingSent.set(key, now);
  try {
    await getCore().typingStart(serverKey, channelId);
  } catch (error: unknown) {
    console.warn(`typingStart failed: ${errorKind(error)}`);
  }
}

export function startEditingMessage(
  serverKey: string,
  channelId: string,
  messageId: string,
  text: string,
): void {
  useAppStore.setState({ editingMessage: { serverKey, channelId, messageId, text } });
}

export function cancelEditingMessage(): void {
  useAppStore.setState({ editingMessage: null });
}
