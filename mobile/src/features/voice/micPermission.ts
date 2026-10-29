// The permissions a call needs (design §8.6): the microphone, asked on the
// first Join, and on Android 13 and later the notification that keeps the
// call going, whose refusal doesn't stop the call.
import { getRecordingPermissionsAsync, requestNotificationPermissionsAsync, requestRecordingPermissionsAsync } from 'expo-audio';
import { Linking, Platform } from 'react-native';

import { report } from '@/core';
import { confirm } from '@/ui';

/** Android 13: notifications need asking for. */
const ASKS_FOR_NOTIFICATIONS = 33;

/**
 * Whether the microphone may be used, asking if it isn't. Always asks:
 * where the system won't show its prompt again it answers no at once, and
 * Expo's `canAskAgain` can be stale (it remembers asking before a revoke).
 */
export async function ensureMicrophone(): Promise<boolean> {
  if ((await getRecordingPermissionsAsync()).granted) return true;
  return (await requestRecordingPermissionsAsync()).granted;
}

/** Says why voice can't start, with the way to Settings. */
export function explainMicrophone(): void {
  confirm({
    title: 'Opencord needs the microphone for voice.',
    message: 'You can allow it in Settings.',
    confirmTitle: 'Open Settings',
    onConfirm: () => report(Linking.openSettings(), 'open settings'),
  });
}

/** Asks for notifications where Android requires it; the answer changes nothing else. */
export async function askForNotifications(): Promise<void> {
  if (Platform.OS !== 'android' || Platform.Version < ASKS_FOR_NOTIFICATIONS) return;
  await requestNotificationPermissionsAsync();
}
