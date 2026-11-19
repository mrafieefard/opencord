// The app's one store (design §6), and the event loop's callback. A Ready
// starts seeding recent activity; the server's other events wait until it
// ends, then apply in order (rust_repository.dart `_held`). A newer Ready
// meanwhile throws the older seeding's result away.
import { MobileEvent, MobileEvent_Tags, NoiseSuppression, type PhoneAudioSettings, type ServerSnapshot } from '@opencord/core-native';
import { create } from 'zustand';

import { getCore } from '@/core';
import { errorKind } from '@/core/errors';

import { seedActivity } from './activity';
import { applyEvents, newServer } from './events';
import { ReadPositions } from './readPositions';
import type { AppState, CueGroup, SoundboardPrefs, UserStatus } from './types';
export { channelKey } from './types';

export const DEFAULT_STATUS: UserStatus = 'online';
export const DEFAULT_MUSIC = { volume: 0.5, muted: false, duck: false } as const;
export const DEFAULT_USER_AUDIO = { volume: 1, mutedForMe: false } as const;
/**
 * The core's own defaults (`AudioSettings::default()`), which every phone
 * call ran on before Voice & audio: unchanged, applying them changes nothing.
 */
export const DEFAULT_AUDIO: Readonly<PhoneAudioSettings> = {
  noiseSuppression: NoiseSuppression.Standard,
  inputVolume: 1,
  outputVolume: 1,
};

export const DEFAULT_SOUNDBOARD: Readonly<SoundboardPrefs> = {
  volume: 100,
  muted: false,
};

export const DEFAULT_CUES_OFF: readonly CueGroup[] = [];

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
    soundRings: {},
    lastSound: {},
    voicePlayback: null,
    chatInFront: false,
    serverInFront: null,
    voicePageInFront: null,
    call: null,
    openActivity: null,
    tabInset: 0,
    voiceSlot: null,
    voiceSpeed: 1,
    voiceRecording: null,
    micTest: false,
    prefs: {
      theme: 'system',
      music: { ...DEFAULT_MUSIC },
      soundFavorites: [],
      audio: { ...DEFAULT_AUDIO },
      status: DEFAULT_STATUS,
      soundboard: { ...DEFAULT_SOUNDBOARD },
      cuesOff: [...DEFAULT_CUES_OFF],
      recentEmoji: [],
    },
    toasts: [],
    editingMessage: null,
    recentChannels: [],
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

/** Takes the media events Voice & audio acts on (actions/audio.ts). */
type AudioHook = (event: MobileEvent) => void;
let audioHook: AudioHook = () => {};
export function setAudioHook(next: AudioHook): void {
  audioHook = next;
}

function serverOf(event: MobileEvent): string | null {
  switch (event.tag) {
    case MobileEvent_Tags.Ready:
      return event.inner.snapshot.serverKey;
    case MobileEvent_Tags.AudioProblem:
    case MobileEvent_Tags.SpeakingWhileMuted:
    case MobileEvent_Tags.InputLevel:
    case MobileEvent_Tags.NoiseSuppressionFellBack:
    case MobileEvent_Tags.VoiceRecordLevel:
    case MobileEvent_Tags.VoiceRecordStopped:
    case MobileEvent_Tags.Playback:
    case MobileEvent_Tags.PlaybackEnded:
    case MobileEvent_Tags.PlaybackFailed:
    case MobileEvent_Tags.Private:
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
    if (MobileEvent.PlaybackFailed.instanceOf(event)) toaster("Couldn't play that voice message.");
    if (
      MobileEvent.InputLevel.instanceOf(event) ||
      MobileEvent.NoiseSuppressionFellBack.instanceOf(event) ||
      MobileEvent.AudioProblem.instanceOf(event)
    ) {
      audioHook(event);
    }
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
    const pluginUsers = Object.fromEntries(
      (snapshot.pluginUsers ?? []).map((u) => [u.userId, u]),
    );
    const ready: AppState = {
      ...state,
      servers: {
        ...state.servers,
        [serverKey]: { ...current, snapshot, activity, voice, typing: {}, pluginUsers, seeding: false, held: [] },
      },
    };
    return applyEvents(ready, current.held, Date.now());
  });
  if (applied) readyHook(serverKey);
}
