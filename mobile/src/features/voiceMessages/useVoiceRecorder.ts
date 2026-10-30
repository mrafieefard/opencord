// The composer's voice recorder (media uploads design §11, as Telegram's):
// hold the mic to record and let go to send; slide left past 120 pt to
// cancel; slide up past 60 pt to lock, then send, stop to hear it first,
// or delete. A tap alone says to hold. At the server's maximum a held
// recording sends and a locked one stops.
import type { VoiceDraftRecord } from '@opencord/core-native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { Gesture } from 'react-native-gesture-handler';
import { useSharedValue } from 'react-native-reanimated';

import { report } from '@/core';
import { toast } from '@/state/actions/ui';
import {
  cancelRecording,
  discardDraft,
  sendVoiceDraft,
  startRecording,
  stopRecording,
} from '@/state/actions/voiceMessages';
import { useAppStore } from '@/state/store';

export type RecorderMode = 'idle' | 'holding' | 'locked' | 'draft';

export const CANCEL_DISTANCE = 120;
export const LOCK_DISTANCE = 60;
/** Shorter than this, a recording is taken for a tap and dropped. */
const SHORTEST_MS = 700;
const HOLD_HINT = 'Hold to record a voice message';

export interface VoiceRecorder {
  mode: RecorderMode;
  /** Slid far enough left that letting go cancels. */
  cancelArmed: boolean;
  draft: VoiceDraftRecord | null;
  /** On the mic button: hold, slide, let go; or a tap. */
  gesture: ReturnType<typeof Gesture.Exclusive>;
  send: () => void;
  stop: () => void;
  remove: () => void;
}

export function useVoiceRecorder(serverKey: string, channelId: string): VoiceRecorder {
  const [mode, setModeState] = useState<RecorderMode>('idle');
  const [cancelArmed, setCancelArmedState] = useState(false);
  const [draft, setDraft] = useState<VoiceDraftRecord | null>(null);
  // The gesture's callbacks outlive renders: they read these, as the
  // panes' gestures read their shared values.
  const modeNow = useSharedValue<RecorderMode>('idle');
  const armedNow = useSharedValue(false);
  /** Let go before recording had started: finish once it has. */
  const releasedEarly = useSharedValue(false);
  const starting = useSharedValue(false);

  const setMode = useCallback((next: RecorderMode) => {
    modeNow.set(next);
    setModeState(next);
  }, [modeNow]);
  const setArmed = useCallback((next: boolean) => {
    if (armedNow.get() === next) return;
    armedNow.set(next);
    setCancelArmedState(next);
  }, [armedNow]);

  /** Stops and sends what was recorded, or drops it when it was only a tap. */
  const finishAndSend = useCallback(async () => {
    setMode('idle');
    setArmed(false);
    const recorded = await stopRecording();
    if (!recorded) return;
    if (recorded.durationMs < SHORTEST_MS) {
      discardDraft(recorded);
      toast(HOLD_HINT);
      return;
    }
    await sendVoiceDraft(serverKey, channelId, recorded);
  }, [serverKey, channelId, setMode, setArmed]);

  const cancel = useCallback(async () => {
    setMode('idle');
    setArmed(false);
    await cancelRecording();
  }, [setMode, setArmed]);

  const begin = useCallback(async () => {
    releasedEarly.set(false);
    starting.set(true);
    const started = await startRecording(serverKey);
    starting.set(false);
    if (!started) return;
    setMode('holding');
    if (releasedEarly.get()) await finishAndSend();
  }, [serverKey, setMode, finishAndSend, releasedEarly, starting]);

  const release = useCallback(() => {
    if (starting.get()) {
      releasedEarly.set(true);
      return;
    }
    if (modeNow.get() !== 'holding') return;
    if (armedNow.get()) report(cancel(), 'cancel a voice message');
    else report(finishAndSend(), 'send a voice message');
  }, [cancel, finishAndSend, starting, releasedEarly, modeNow, armedNow]);

  const gesture = useMemo(
    () =>
      Gesture.Exclusive(
        Gesture.Pan()
          .runOnJS(true)
          .activateAfterLongPress(200)
          .onStart(() => report(begin(), 'record a voice message'))
          .onUpdate((event) => {
            if (modeNow.get() !== 'holding') return;
            if (event.translationY < -LOCK_DISTANCE && !armedNow.get()) {
              setMode('locked');
              return;
            }
            setArmed(event.translationX < -CANCEL_DISTANCE);
          })
          .onFinalize(release),
        Gesture.Tap()
          .runOnJS(true)
          .onEnd(() => toast(HOLD_HINT)),
      ),
    [begin, release, setMode, setArmed, modeNow, armedNow],
  );

  // At the server's maximum (or the microphone gone): a held recording
  // sends, a locked one stops.
  useEffect(
    () =>
      useAppStore.subscribe((state, previous) => {
        if (!state.voiceRecording?.stopped || previous.voiceRecording?.stopped) return;
        if (modeNow.get() === 'holding') report(finishAndSend(), 'send a voice message');
        else if (modeNow.get() === 'locked') {
          setMode('draft');
          report(stopRecording().then(setDraft), 'stop a voice message');
        }
      }),
    [finishAndSend, setMode, modeNow],
  );

  const send = useCallback(() => {
    if (modeNow.get() === 'locked') report(finishAndSend(), 'send a voice message');
    else if (modeNow.get() === 'draft' && draft) {
      const toSend = draft;
      setDraft(null);
      setMode('idle');
      report(sendVoiceDraft(serverKey, channelId, toSend), 'send a voice message');
    }
  }, [draft, serverKey, channelId, setMode, finishAndSend, modeNow]);

  const stop = useCallback(() => {
    if (modeNow.get() !== 'locked') return;
    setMode('draft');
    report(stopRecording().then(setDraft), 'stop a voice message');
  }, [setMode, modeNow]);

  const remove = useCallback(() => {
    if (modeNow.get() === 'locked') report(cancel(), 'delete a voice message');
    else if (modeNow.get() === 'draft') {
      if (draft) discardDraft(draft);
      setDraft(null);
      setMode('idle');
    }
  }, [draft, cancel, setMode, modeNow]);

  // Leaving the chat mid-recording drops it.
  useEffect(
    () => () => {
      if (modeNow.get() === 'holding' || modeNow.get() === 'locked') void cancelRecording();
    },
    [modeNow],
  );

  return { mode, cancelArmed, draft, gesture, send, stop, remove };
}
