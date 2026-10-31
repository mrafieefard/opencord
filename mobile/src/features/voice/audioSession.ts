// The audio session around a voice call (design §11). The core opens the
// microphone and the speaker itself through cpal; this tells the system
// what the app is doing with sound, and on iOS turns the session on before
// the core opens anything and off after (audioActivation.ios.ts).
import { setAudioModeAsync } from 'expo-audio';

import { activateAudio, deactivateAudio } from './audioActivation';
import { VOICE_INTERRUPTION_MODE } from './voiceMixing';

/** Before joining: record, play when the ringer is off, keep going in the background; other apps pause on Android, mix on iOS. */
export async function enterVoiceAudio(): Promise<void> {
  await setAudioModeAsync({
    allowsRecording: true,
    playsInSilentMode: true,
    shouldPlayInBackground: true,
    interruptionMode: VOICE_INTERRUPTION_MODE,
  });
  await activateAudio();
}

/** After leaving: other apps play again, and nothing is held in the background. */
export async function leaveVoiceAudio(): Promise<void> {
  await setAudioModeAsync({
    allowsRecording: false,
    shouldPlayInBackground: false,
    interruptionMode: 'mixWithOthers',
  });
  await deactivateAudio();
}

/**
 * Before recording or playing a voice message outside a call: record and
 * play even with the ringer off, lowering other apps meanwhile. In a call
 * the call's session already does both.
 */
export async function enterVoiceNoteAudio(): Promise<void> {
  await setAudioModeAsync({
    allowsRecording: true,
    playsInSilentMode: true,
    shouldPlayInBackground: false,
    interruptionMode: 'duckOthers',
  });
  await activateAudio();
}
