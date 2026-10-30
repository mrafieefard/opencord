// The soundboard on this device (Phase 2 plan §11.7): playing a sound for
// everyone in this device's voice channel, previewing one here only. Why a
// play failed shows as a toast, as on the desktop.
import { MobileError, MobileError_Tags, type DefaultSoundRecord } from '@opencord/core-native';
import * as Haptics from 'expo-haptics';

import { describeError, getCore } from '@/core';
import type { PickerSound } from '@/features/soundboard/picker';

import { useAppStore } from '../store';
import { toast } from './ui';

let defaults: DefaultSoundRecord[] | null = null;

/** The sounds every client has; they never change while the app runs. */
export function defaultSounds(): DefaultSoundRecord[] {
  defaults ??= getCore().soundboardDefaultSounds();
  return defaults;
}

/** Tests start again from the core they set. */
export function forgetDefaultSounds(): void {
  defaults = null;
}

/** Plays a sound in this device's voice channel; false when it didn't. */
export async function playSound(sound: PickerSound): Promise<boolean> {
  const voice = useAppStore.getState().myVoice;
  if (!voice) return false;
  if (sound.reason !== null) {
    toast(sound.reason);
    return false;
  }
  const { serverKey, channelId } = voice;
  void Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
  try {
    await getCore().soundboardPlay(serverKey, channelId, sound.ref, sound.from);
  } catch (error: unknown) {
    const forbidden = MobileError.instanceOf(error) && error.tag === MobileError_Tags.Server && error.inner.code === 'forbidden';
    toast(forbidden ? "You can't play that sound here." : describeError(error), 'error');
    return false;
  }
  useAppStore.setState((state) => ({ lastSound: { ...state.lastSound, [serverKey]: Date.now() } }));
  return true;
}

/** Plays a sound here only, from the server it lives on. */
export async function previewSound(sound: PickerSound): Promise<void> {
  try {
    await getCore().soundboardPreview(sound.previewServer, sound.previewRef);
  } catch (error: unknown) {
    toast(describeError(error), 'error');
  }
}
