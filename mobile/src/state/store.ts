// The app's one store (design §6), and the event loop's callback. A Ready
// starts seeding recent activity; the server's other events wait until it
// ends, then apply in order (rust_repository.dart `_held`). A newer Ready
// meanwhile throws the older seeding's result away.
import { MobileEvent, MobileEvent_Tags, type ServerSnapshot } from '@opencord/core-native';
import { create } from 'zustand';

import { getCore } from '@/core';
import { errorKind } from '@/core/errors';

import { seedActivity } from './activity';
import { applyEvents, newServer } from './events';
import { ReadPositions } from './readPositions';
import type { AppState } from './types';
export { channelKey } from './types';

export const DEFAULT_MUSIC = { volume: 0.5, muted: false, duck: false } as const;
export const DEFAULT_USER_AUDIO = { volume: 1, mutedForMe: false } as const;

export function initialState(): AppState {
  return {
    boot: { state: 'starting' },
    self: null,
    servers: {},
    order: [],
    messages: {},
    openChannel: null,
    appActive: true,
    myVoice: null,
    speaking: {},
    music: {},
    uploads: {},
    userAudio: {},
    prefs: { theme: 'system', music: { ...DEFAULT_MUSIC } },
    toasts: [],
    editingMessage: null,
  };
}

export const useAppStore = create<AppState>(() => initialState());

export const readPositions = new ReadPositions(getCore);

/** Shown for a few seconds; see actions/ui.ts. */
type Toaster = (text: string) => void;
let toaster: Toaster = () => {};
export function setToaster(next: Toaster): void {
  toaster = next;
}

/** Runs after a server's Ready has been applied (the name catch-up). */
type ReadyHook = (serverKey: string) => void;
let readyHook: ReadyHook = () => {};
export function setReadyHook(next: ReadyHook): void {
  readyHook = next;
}

function serverOf(event: MobileEvent): string | null {
  switch (event.tag) {
    case MobileEvent_Tags.Ready:
      return event.inner.snapshot.serverKey;
    case MobileEvent_Tags.AudioProblem:
    case MobileEvent_Tags.SpeakingWhileMuted:
      return null;
    default:
      return event.inner.serverKey;
  }
}

/** A server the store doesn't know yet (just added): taken from the core's list. */
function ensureServer(serverKey: string): void {
  if (useAppStore.getState().servers[serverKey]) return;
  const entry = getCore()
    .serversList()
    .find((candidate) => candidate.key === serverKey);
  if (!entry) return;
  useAppStore.setState((state) =>
    state.servers[serverKey]
      ? state
      : {
          servers: { ...state.servers, [serverKey]: newServer(entry) },
          order: state.order.includes(serverKey) ? state.order : [...state.order, serverKey],
        },
  );
}

/** The event loop's callback. */
export function onEvents(events: MobileEvent[]): void {
  for (const event of events) {
    const serverKey = serverOf(event);
    if (serverKey !== null) ensureServer(serverKey);
  }
  let run: MobileEvent[] = [];
  const flush = () => {
    if (run.length === 0) return;
    const batch = run;
    run = [];
    useAppStore.setState((state) => applyEvents(state, batch, Date.now()));
  };
  for (const event of events) {
    if (MobileEvent.Ready.instanceOf(event)) {
      flush();
      startReady(event.inner.snapshot);
      continue;
    }
    const serverKey = serverOf(event);
    const server = serverKey === null ? undefined : useAppStore.getState().servers[serverKey];
    // A connection state passes: the row shows it at once.
    if (server?.seeding && !MobileEvent.Connection.instanceOf(event)) {
      flush();
      useAppStore.setState((state) => {
        const current = state.servers[server.key];
        if (!current) return state;
        return { servers: { ...state.servers, [server.key]: { ...current, held: [...current.held, event] } } };
      });
      continue;
    }
    run.push(event);
  }
  flush();
  for (const event of events) {
    if (MobileEvent.MusicNotice.instanceOf(event)) toaster(event.inner.text);
  }
}

function startReady(snapshot: ServerSnapshot): void {
  const serverKey = snapshot.serverKey;
  let count = 0;
  useAppStore.setState((state) => {
    const current = state.servers[serverKey];
    if (!current) return state;
    count = current.readyCount + 1;
    return {
      servers: { ...state.servers, [serverKey]: { ...current, seeding: true, readyCount: count, held: [] } },
    };
  });
  if (count === 0) return;
  seedActivity(getCore(), snapshot, readPositions).then(
    (activity) => finishReady(snapshot, count, activity),
    (error: unknown) => {
      console.warn(`seeding failed: ${errorKind(error)}`);
      finishReady(snapshot, count, {});
    },
  );
}

function finishReady(snapshot: ServerSnapshot, count: number, activity: Awaited<ReturnType<typeof seedActivity>>): void {
  const serverKey = snapshot.serverKey;
  let applied = false;
  useAppStore.setState((state) => {
    const current = state.servers[serverKey];
    // A newer Ready replaced this one meanwhile.
    if (!current || current.readyCount !== count) return state;
    applied = true;
    const voice = Object.fromEntries(
      snapshot.voiceStates.filter((voiceState) => voiceState.channelId !== undefined).map((voiceState) => [voiceState.userId, voiceState]),
    );
    const ready: AppState = {
      ...state,
      servers: {
        ...state.servers,
        [serverKey]: { ...current, snapshot, activity, voice, typing: {}, seeding: false, held: [] },
      },
    };
    return applyEvents(ready, current.held, Date.now());
  });
  if (applied) readyHook(serverKey);
}
