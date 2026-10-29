// The audio session around a voice call (design §11). The core opens the
// microphone and the speaker itself through cpal; this only tells the
// system what the app is doing with sound.
import { setAudioModeAsync } from 'expo-audio';

/** Before joining: record, play when the ringer is off, keep going in the background, pause other apps. */
export async function enterVoiceAudio(): Promise<void> {
  await setAudioModeAsync({
    allowsRecording: true,
    playsInSilentMode: true,
    shouldPlayInBackground: true,
    interruptionMode: 'doNotMix',
  });
}

/** After leaving: other apps play again, and nothing is held in the background. */
export async function leaveVoiceAudio(): Promise<void> {
  await setAudioModeAsync({
    allowsRecording: false,
    shouldPlayInBackground: false,
    interruptionMode: 'mixWithOthers',
  });
}
