// Voice on this device: joining (microphone, audio session, the
// foreground service, then the core), leaving, its toggles, and others'
// volume. The service follows the store: it stops whenever this device's
// voice ends, however it ended, and its notification follows the channel.
import { VoiceService, type VoiceNotification } from '@opencord/voice-service';

import { getCore } from '@/core';
import { enterVoiceAudio, leaveVoiceAudio } from '@/features/voice/audioSession';
import { askForNotifications, ensureMicrophone, explainMicrophone } from '@/features/voice/micPermission';

import { useAppStore } from '../store';
import type { AppState, MyVoice } from '../types';

/** The notification the service shows, or null while it isn't running. */
let shown: string | null = null;

function showService(notification: VoiceNotification): void {
  if (shown === notification.title) return;
  shown = notification.title;
  VoiceService.start(notification);
}

function stopService(): void {
  if (shown === null) return;
  shown = null;
  VoiceService.stop();
}

/** "In voice: Lounge · Night Owls". */
export function voiceNotification(state: Pick<AppState, 'servers'>, serverKey: string, channelId: string): VoiceNotification {
  const server = state.servers[serverKey];
  const snapshot = server?.snapshot ?? null;
  const channel = snapshot?.channels.find((candidate) => candidate.id === channelId)?.name ?? 'Voice';
  const name = snapshot?.name ?? server?.entry.name ?? 'Opencord';
  return { title: `In voice: ${channel} · ${name}`, text: '' };
}

/** False when the microphone was refused; throws what the core throws. */
export async function joinVoice(serverKey: string, channelId: string): Promise<boolean> {
  if (!(await ensureMicrophone())) {
    explainMicrophone();
    return false;
  }
  await askForNotifications();
  await enterVoiceAudio();
  // Started before the join, while the app is in the foreground, as
  // Android 14 requires for a microphone service.
  showService(voiceNotification(useAppStore.getState(), serverKey, channelId));
  try {
    await getCore().voiceJoin(serverKey, channelId);
  } catch (error: unknown) {
    // Still in voice elsewhere: the service stays, back on that channel.
    const still = useAppStore.getState().myVoice;
    if (still) showService(voiceNotification(useAppStore.getState(), still.serverKey, still.channelId));
    else stopService();
    throw error;
  }
  return true;
}

export async function leaveVoice(): Promise<void> {
  try {
    await getCore().voiceLeave();
  } finally {
    stopService();
    await leaveVoiceAudio();
  }
}

export function setMuted(muted: boolean): void {
  getCore().voiceSetMute(muted);
}

export function setDeafened(deafened: boolean): void {
  getCore().voiceSetDeaf(deafened);
}

/** 0–2, 1 being as sent. */
export function setUserVolume(serverKey: string, userId: string, volume: number): void {
  getCore().voiceSetUserVolume(serverKey, userId, volume);
}

export function setUserLocalMute(serverKey: string, userId: string, muted: boolean): void {
  getCore().voiceSetUserLocalMute(serverKey, userId, muted);
}

/** Keeps the service in step with this device's voice; returns the unsubscribe. */
export function watchVoiceService(): () => void {
  shown = null;
  return useAppStore.subscribe((state, previous) => {
    const now: MyVoice | null = state.myVoice;
    const before: MyVoice | null = previous.myVoice;
    if (before && !now) stopService();
    else if (now) showService(voiceNotification(state, now.serverKey, now.channelId));
  });
}
