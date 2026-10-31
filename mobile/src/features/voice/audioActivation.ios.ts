// Turning iOS's audio session on and off around the core's own audio. The
// core opens the microphone and speaker through cpal, which reads the
// microphone's format from the session: on a session that isn't active and
// set to record, iOS reports no input channels and the microphone can't be
// opened. Turning it off afterwards lets other apps play at full volume again.
import { setIsAudioActiveAsync } from 'expo-audio';

export async function activateAudio(): Promise<void> {
  try {
    await setIsAudioActiveAsync(true);
  } catch (error: unknown) {
    console.warn('the audio session could not be turned on', error);
  }
}

/** Fails harmlessly while something still plays or records: iOS keeps it on. */
export async function deactivateAudio(): Promise<void> {
  try {
    await setIsAudioActiveAsync(false);
  } catch {
    // Still in use; it goes off with the next give-back.
  }
}
