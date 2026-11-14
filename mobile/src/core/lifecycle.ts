// What runs once the identity is loaded: prefs, servers, the event loop,
// the voice service following the store, and coming back to the foreground
// (retrying what isn't connected).
import { AppState, type AppStateStatus } from 'react-native';

import { getCore, report, setErrorShower, startEventLoop, type EventLoop } from '@/core';
import { loadPrefs } from '@/state/actions/prefs';
import { catchUpName } from '@/state/actions/profile';
import { loadServers, retryDisconnected } from '@/state/actions/servers';
import { setAppActive, toast } from '@/state/actions/ui';
import { watchVoiceService } from '@/state/actions/voice';
import { watchVoiceNoteAudio } from '@/state/actions/voiceMessages';
import { reloadAfterReady } from '@/state/actions/chat';
import { onEvents, setReadyHook, setToaster } from '@/state/store';
import { watchOpenActivity } from '@/features/plugins/activityHolder';
import { watchCallWarmth } from '@/features/plugins/warmUp';

let loop: EventLoop | null = null;
let appStateSubscription: { remove(): void } | null = null;
let unwatchVoice: (() => void) | null = null;
let unwatchVoiceNotes: (() => void) | null = null;
let unwatchCallWarmth: (() => void) | null = null;
let unwatchOpenActivity: (() => void) | null = null;

export function startApp(): void {
  setToaster((text) => toast(text));
  setErrorShower((text) => toast(text, 'error'));
  setReadyHook((serverKey) => {
    catchUpName(serverKey);
    report(reloadAfterReady(serverKey), 'reload messages');
  });
  loadPrefs();
  loadServers();
  loop?.stop();
  loop = startEventLoop(getCore(), onEvents);
  unwatchVoice?.();
  unwatchVoice = watchVoiceService();
  unwatchCallWarmth?.();
  unwatchCallWarmth = watchCallWarmth();
  unwatchOpenActivity?.();
  unwatchOpenActivity = watchOpenActivity();
  unwatchVoiceNotes?.();
  unwatchVoiceNotes = watchVoiceNoteAudio();
  appStateSubscription?.remove();
  appStateSubscription = AppState.addEventListener('change', (state: AppStateStatus) => {
    setAppActive(state === 'active');
    if (state === 'active') report(retryDisconnected(), 'retrying servers');
  });
}
