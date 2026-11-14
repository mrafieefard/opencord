// The activity this device has open (plugins M4 plan, Task 15): at most one,
// named by the store's `openActivity`, its ViewSession kept here beside it.
// Opening another closes the first. It goes when the device leaves the
// call, when its view closes, and 3 s after its session ends (the screen
// says so meanwhile). It's paused while no stage draws it; its session
// and engine stay. Its view hears who is in the call and who speaks.
import type { ActivitySessionRecord } from '@opencord/core-native';
import * as PluginArea from '@opencord/plugin-area';
import { router } from 'expo-router';
import { useSyncExternalStore } from 'react';

import { getCore } from '@/core';
import { buildVoiceMembers } from '@/state/selectors';
import { useAppStore } from '@/state/store';
import type { AppState } from '@/state/types';
import { tokens } from '@/ui/theme/tokens';

import { areaSetup } from './areaSetup';
import { startCallInput } from './callInput';
import { answerHostCalls } from './hostCalls';
import { phoneHostDeps } from './phoneHost';
import { pluginViewPump } from './usePluginSession';
import { ViewSession } from './viewSession';

/** How long `This activity ended` shows before the grid comes back. */
export const ENDED_SHOWN_MS = 3000;

export interface Held {
  key: string;
  serverKey: string;
  channelId: string;
  sessionId: string;
  pluginId: string;
  /** The session as it was opened: kept while its screen says it ended. */
  record: ActivitySessionRecord;
  session: ViewSession;
  stop: () => void;
}

let held: Held | null = null;
let endedTimer: ReturnType<typeof setTimeout> | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of [...listeners]) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The open activity's session and plugin, or null. */
export function useOpenActivity(): Held | null {
  return useSyncExternalStore(subscribe, () => held);
}

/** Closes this device's view of the open activity; the tile returns. */
export function closeOpenActivity(): void {
  useAppStore.setState({ openActivity: null });
}

// The stages that would draw it: the highest priority one does.
const stages = new Map<symbol, number>();
let told: 'pause' | 'resume' = 'resume';

function stageOwner(): symbol | null {
  let best: [symbol, number] | null = null;
  for (const entry of stages) if (!best || entry[1] > best[1]) best = entry;
  return best?.[0] ?? null;
}

function tellPaused() {
  if (!held) return;
  const now = stages.size === 0 ? 'pause' : 'resume';
  if (now === told) return;
  told = now;
  held.session.send({ kind: 'lifecycle', event: now });
}

/** Claims the stage while mounted; whether this claim draws now. */
export function claimStage(owner: symbol, priority: number): () => void {
  stages.set(owner, priority);
  tellPaused();
  emit();
  return () => {
    stages.delete(owner);
    tellPaused();
    emit();
  };
}

export function useStageOwner(): symbol | null {
  return useSyncExternalStore(subscribe, stageOwner);
}

function release() {
  if (endedTimer !== null) clearTimeout(endedTimer);
  endedTimer = null;
  const old = held;
  held = null;
  told = 'resume';
  old?.stop();
  old?.session.close();
  emit();
}

function open(state: AppState, target: NonNullable<AppState['openActivity']>): Held | null {
  const record = state.servers[target.serverKey]?.snapshot?.activitySessions.find((s) => s.id === target.sessionId);
  if (!record) return null;
  const scheme = state.prefs.theme === 'light' ? 'light' : 'dark';
  const session = new ViewSession({
    core: getCore(),
    pump: pluginViewPump(),
    area: PluginArea,
    place: {
      kind: 'activity',
      serverKey: target.serverKey,
      sessionId: target.sessionId,
      channelId: target.channelId,
      activityKey: record.activityKey,
    },
    setupJson: areaSetup({
      colors: tokens[scheme],
      scheme,
      insets: { top: 0, right: 0, bottom: 0, left: 0 },
      textScale: 1,
      reduceMotion: false,
      serverKey: target.serverKey,
      pluginId: record.pluginId,
    }),
  });
  const stopAnswering = answerHostCalls(
    session,
    PluginArea,
    phoneHostDeps({
      serverKey: target.serverKey,
      pluginId: record.pluginId,
      openChannel: (id) => router.push({ pathname: '/servers/[serverKey]', params: { serverKey: target.serverKey, channel: id } }),
    }),
  );
  const stopCall = startCallInput({
    read: () => {
      const now = useAppStore.getState();
      const server = now.servers[target.serverKey];
      const members = server ? buildVoiceMembers(server, target.channelId, []).map((member) => member.userId) : [];
      return { members, speaking: now.speaking[`${target.serverKey}/${target.channelId}`] ?? [] };
    },
    subscribe: (changed) => useAppStore.subscribe(changed),
    send: (json) => session.send(JSON.parse(json) as object),
  });
  const stopStatus = session.subscribe((status) => {
    if (status.state !== 'closed') return;
    if (status.reason === 'activity_ended') ended();
    else closeOpenActivity();
  });
  void session.start();
  // A stage claims it as the screen draws it: paused only if none has by then.
  setTimeout(tellPaused, 0);
  return {
    key: `${target.serverKey}/${target.sessionId}`,
    serverKey: target.serverKey,
    channelId: target.channelId,
    sessionId: target.sessionId,
    pluginId: record.pluginId,
    record,
    session,
    stop: () => {
      stopAnswering();
      stopCall();
      stopStatus();
    },
  };
}

/** The session ended: its screen says so for a moment, then the grid. */
function ended() {
  if (endedTimer !== null) return;
  endedTimer = setTimeout(() => {
    endedTimer = null;
    closeOpenActivity();
  }, ENDED_SHOWN_MS);
}

function follow(state: AppState) {
  const target = state.openActivity;
  const voice = state.myVoice;
  if (target && (voice?.serverKey !== target.serverKey || voice.channelId !== target.channelId)) {
    // Out of the call: nothing of it stays open.
    closeOpenActivity();
    return;
  }
  const key = target ? `${target.serverKey}/${target.sessionId}` : null;
  if (held && held.key !== key) release();
  if (target && !held) {
    held = open(state, target);
    if (!held) closeOpenActivity();
    emit();
    return;
  }
  if (held && target) {
    const live = state.servers[target.serverKey]?.snapshot?.activitySessions.some((s) => s.id === target.sessionId);
    if (!live) ended();
  }
}

/** Follows the store's `openActivity`; returns the stop. Started with the app. */
export function watchOpenActivity(): () => void {
  follow(useAppStore.getState());
  const unsubscribe = useAppStore.subscribe(follow);
  return () => {
    unsubscribe();
    release();
  };
}
