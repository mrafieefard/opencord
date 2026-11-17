// Voice & audio's live side (phone P1): the microphone test, the level it
// shows, and the core falling back from High noise suppression.
//
// The test's calls run one after another, each waiting for the one before,
// as the desktop's MicTestNotifier does (mic_test.dart): `micTestSet` runs
// on a blocking thread, so an "off" not waited for could land after a later
// "on", or an "on" after the "off" sent when the page closed, and leave the
// microphone open. Outside a call the test opens the microphone itself, so
// it takes the audio session first, as recording a voice message does
// (voiceMessages.ts), and gives it back after: unless a call is starting,
// which has taken the session for itself.
import { MobileEvent, NoiseSuppression } from '@opencord/core-native';

import { getCore } from '@/core';
import { enterVoiceNoteAudio, leaveVoiceAudio } from '@/features/voice/audioSession';
import { ensureMicrophone, explainMicrophone } from '@/features/voice/micPermission';

import { setAudioHook, useAppStore } from '../store';
import { setAudio } from './prefs';
import { toast } from './ui';

/** Words new on the phone (plan P1, Global Constraints). */
export const FELL_BACK_TEXT = 'High noise suppression is too heavy for this phone. Using Standard.';

/** What was asked last; the work catches up with it. */
let wanted = false;
/** Whether the core has the test on. */
let on = false;
/** The work under way, which every caller until it ends waits for. */
let settling: Promise<void> | null = null;
/** Whether the test took the audio session, and so gives it back. */
let tookSession = false;
/** A call is starting and has the session: stopping keeps it on. */
let keepSession = false;

const inCall = () => useAppStore.getState().myVoice !== null;

function show(testing: boolean): void {
  if (useAppStore.getState().micTest !== testing) useAppStore.setState({ micTest: testing });
}

/**
 * Starts the test (and the meter); resolves once the core has it, or it was
 * refused. The microphone is asked for first, before the test counts as
 * asked: the system's prompt takes the app out of the foreground, which
 * ends a test (VoiceAudioPage), and would end this one before it began.
 */
export async function startMicTest(): Promise<void> {
  if (!(await ensureMicrophone())) {
    explainMicrophone();
    return;
  }
  await ask(true);
}

/** Ends the test; nothing happens when it is off. */
export function stopMicTest(): Promise<void> {
  return ask(false);
}

/**
 * Ends the test before a call starts, keeping the audio session for the
 * call. The core would otherwise move the test into the call's engine
 * (`start_session` copies it), and the person would hear themselves there.
 */
export async function stopMicTestForCall(): Promise<void> {
  keepSession = true;
  try {
    await ask(false);
  } catch (error: unknown) {
    // A start that failed meanwhile was shown to whoever pressed Test; the
    // test is off either way, and the call goes on.
    console.warn('the microphone test did not stop cleanly before the call', error);
  }
}

/** Quick calls end on the last one asked. */
function ask(testing: boolean): Promise<void> {
  wanted = testing;
  show(testing);
  if (settling) return settling;
  if (on === testing) {
    keepSession = false;
    return Promise.resolve();
  }
  settling = settle();
  return settling;
}

async function settle(): Promise<void> {
  try {
    while (on !== wanted) {
      if (wanted) await begin();
      else await end();
    }
  } finally {
    settling = null;
    keepSession = false;
  }
}

/** The test stays off: the button goes back to Test. */
function refuse(): void {
  wanted = false;
  show(false);
}

async function begin(): Promise<void> {
  // The button is disabled in a call; this covers a call that started
  // while the press was on its way.
  if (inCall()) return refuse();
  const core = getCore();
  try {
    tookSession = true;
    await enterVoiceNoteAudio();
    core.levelMeterSet(true);
    await core.micTestSet(true);
  } catch (error: unknown) {
    refuse();
    turnMeterOff();
    await giveSessionBack();
    throw error;
  }
  on = true;
}

async function end(): Promise<void> {
  turnMeterOff();
  await getCore().micTestSet(false);
  on = false;
  await giveSessionBack();
}

/** The meter off; a failure here is the core's, and the test's stop still goes ahead. */
function turnMeterOff(): void {
  try {
    getCore().levelMeterSet(false);
  } catch (error: unknown) {
    console.warn('the level meter did not turn off', error);
  }
}

async function giveSessionBack(): Promise<void> {
  if (!tookSession) return;
  tookSession = false;
  // A call holds it now: turning it off would cut the call's audio on iOS.
  if (keepSession || inCall()) return;
  // A voice message still playing holds it too, and gives it back when it
  // ends (watchVoiceNoteAudio).
  if (useAppStore.getState().voicePlayback?.playing) return;
  await leaveVoiceAudio();
}

// The level ------------------------------------------------------------

type LevelListener = (dbfs: number) => void;
const levelListeners = new Set<LevelListener>();

/** Hears each `InputLevel` (dBFS) while the meter is on; returns the unsubscribe. */
export function onInputLevel(listener: LevelListener): () => void {
  levelListeners.add(listener);
  return () => {
    levelListeners.delete(listener);
  };
}

// Events ---------------------------------------------------------------

/** The core's media events Voice & audio acts on, from the store's event loop. */
function onAudioEvent(event: MobileEvent): void {
  if (MobileEvent.InputLevel.instanceOf(event)) {
    for (const listener of levelListeners) listener(event.inner.dbfs);
    return;
  }
  if (MobileEvent.NoiseSuppressionFellBack.instanceOf(event)) {
    // A newer choice than High made meanwhile stands.
    if (useAppStore.getState().prefs.audio.noiseSuppression !== NoiseSuppression.High) return;
    setAudio({ noiseSuppression: NoiseSuppression.Standard });
    toast(FELL_BACK_TEXT);
    return;
  }
  // The test's own microphone couldn't open (an AudioProblem that isn't a
  // fallback): say why, and the button goes back to Test.
  if (MobileEvent.AudioProblem.instanceOf(event) && !event.inner.output && !event.inner.fellBack && wanted && !inCall()) {
    toast(event.inner.message, 'error');
    stopMicTest().catch((error: unknown) => console.warn('the microphone test did not stop', error));
  }
}

/** Routes the store's media events here; once, at start (lifecycle.ts). */
export function watchAudioEvents(): void {
  setAudioHook(onAudioEvent);
}
