// Voice messages on this device (media uploads design §10–11): recording
// one (held, or locked hands-free), sending or discarding it, and playing
// voice messages and drafts with seeking and 1×, 1.5× and 2× speed. Outside
// a call they take the audio session for themselves (audioSession.ts).
import { PlaybackItemRecord, type VoiceDraftRecord } from '@opencord/core-native';
import * as Haptics from 'expo-haptics';

import { describeError, getCore } from '@/core';
import { enterVoiceNoteAudio, leaveVoiceAudio } from '@/features/voice/audioSession';
import { ensureMicrophone, explainMicrophone } from '@/features/voice/micPermission';

import { mergeMessages, playbackKey } from '../events';
import { markFreshSend } from '../freshSends';
import { useAppStore } from '../store';
import { channelKey } from '../types';
import { toast } from './ui';

/** The server's longest voice message, when it hasn't said: the default. */
const DEFAULT_MAX_SECONDS = 300;
const SPEEDS = [1, 1.5, 2];

const inCall = () => useAppStore.getState().myVoice !== null;

/** Takes the audio session for a voice message, unless a call holds it. */
async function takeAudio(): Promise<void> {
  if (!inCall()) await enterVoiceNoteAudio();
}

/** Gives the audio session back, unless a call holds it. */
async function giveAudioBack(): Promise<void> {
  if (!inCall()) await leaveVoiceAudio();
}

/** Starts recording; false when the microphone was refused or it couldn't start. */
export async function startRecording(serverKey: string): Promise<boolean> {
  if (!(await ensureMicrophone())) {
    explainMicrophone();
    return false;
  }
  stopVoice();
  await takeAudio();
  const maxSeconds = useAppStore.getState().servers[serverKey]?.snapshot?.voiceMessageMaxSeconds || DEFAULT_MAX_SECONDS;
  try {
    getCore().voiceRecordStart(maxSeconds);
  } catch (error: unknown) {
    await giveAudioBack();
    toast(describeError(error), 'error');
    return false;
  }
  useAppStore.setState({ voiceRecording: { levels: [], startedAtMs: Date.now(), stopped: null } });
  void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);
  return true;
}

/** Stops recording and keeps it as a draft; null when nothing was kept. */
export async function stopRecording(): Promise<VoiceDraftRecord | null> {
  useAppStore.setState({ voiceRecording: null });
  try {
    return getCore().voiceRecordStop();
  } catch (error: unknown) {
    toast(describeError(error), 'error');
    return null;
  } finally {
    await giveAudioBack();
  }
}

export async function cancelRecording(): Promise<void> {
  useAppStore.setState({ voiceRecording: null });
  try {
    getCore().voiceRecordCancel();
  } finally {
    await giveAudioBack();
  }
  void Haptics.notificationAsync(Haptics.NotificationFeedbackType.Warning);
}

export function discardDraft(draft: VoiceDraftRecord): void {
  stopVoiceIf(draftItem(draft));
  getCore().voiceDraftDiscard(draft.draftId);
}

/** Sends a draft as a voice message; it joins the chat once the server has it. */
export async function sendVoiceDraft(serverKey: string, channelId: string, draft: VoiceDraftRecord): Promise<void> {
  stopVoiceIf(draftItem(draft));
  const nonce = `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  markFreshSend(nonce);
  try {
    const sent = await getCore().voiceMessageSend(serverKey, channelId, nonce, draft.draftId);
    const key = channelKey(serverKey, channelId);
    useAppStore.setState((state) => {
      const current = state.messages[key];
      if (!current) return state;
      return { messages: { ...state.messages, [key]: { ...current, items: mergeMessages(current.items, [sent]) } } };
    });
  } catch (error: unknown) {
    toast(describeError(error), 'error');
  }
}

export const attachmentItem = (serverKey: string, attachmentId: string) =>
  new PlaybackItemRecord.Attachment({ serverKey, attachmentId });
export const draftItem = (draft: VoiceDraftRecord) => new PlaybackItemRecord.Draft({ draftId: draft.draftId });

/** Plays from `positionMs`; whatever played before stops. */
export async function playVoice(item: PlaybackItemRecord, positionMs = 0): Promise<void> {
  await takeAudio();
  try {
    await getCore().voicePlay(item, Math.max(0, Math.round(positionMs)), useAppStore.getState().voiceSpeed);
  } catch (error: unknown) {
    toast(describeError(error), 'error');
  }
}

export function pauseVoice(): void {
  getCore().voicePause();
}

export function resumeVoice(): void {
  getCore().voiceResume();
}

export function seekVoice(positionMs: number): void {
  getCore().voiceSeek(Math.max(0, Math.round(positionMs)));
}

/** 1× → 1.5× → 2× → 1×, for what plays now and what plays next. */
export function nextVoiceSpeed(): void {
  const current = useAppStore.getState().voiceSpeed;
  const speed = SPEEDS[(SPEEDS.indexOf(current) + 1) % SPEEDS.length] ?? 1;
  useAppStore.setState({ voiceSpeed: speed });
  if (useAppStore.getState().voicePlayback?.playing) {
    try {
      getCore().voiceSetSpeed(speed);
    } catch (error: unknown) {
      toast(describeError(error), 'error');
    }
  }
}

export function stopVoice(): void {
  if (!useAppStore.getState().voicePlayback) return;
  getCore().voiceStop();
  useAppStore.setState({ voicePlayback: null });
}

function stopVoiceIf(item: PlaybackItemRecord): void {
  const playing = useAppStore.getState().voicePlayback;
  if (playing?.key === playbackKey(item)) stopVoice();
}

/**
 * Gives the audio session back once a voice message stops playing (ended,
 * paused or failed) and nothing is recording, outside a call: other apps
 * come back to full volume. Returns the unsubscribe.
 */
export function watchVoiceNoteAudio(): () => void {
  return useAppStore.subscribe((state, previous) => {
    const wasPlaying = previous.voicePlayback?.playing ?? false;
    const playing = state.voicePlayback?.playing ?? false;
    if (wasPlaying && !playing && state.voiceRecording === null && state.myVoice === null) void leaveVoiceAudio();
  });
}
