// What runs once the identity is loaded: prefs, servers, the event loop,
// and coming back to the foreground (retrying what isn't connected).
import { AppState, type AppStateStatus } from 'react-native';

import { getCore, report, setErrorShower, startEventLoop, type EventLoop } from '@/core';
import { loadPrefs } from '@/state/actions/prefs';
import { catchUpName } from '@/state/actions/profile';
import { loadServers, retryDisconnected } from '@/state/actions/servers';
import { setAppActive, toast } from '@/state/actions/ui';
import { onEvents, setReadyHook, setToaster } from '@/state/store';

let loop: EventLoop | null = null;
let appStateSubscription: { remove(): void } | null = null;

export function startApp(): void {
  setToaster((text) => toast(text));
  setErrorShower((text) => toast(text, 'error'));
  setReadyHook(catchUpName);
  loadPrefs();
  loadServers();
  loop?.stop();
  loop = startEventLoop(getCore(), onEvents);
  appStateSubscription?.remove();
  appStateSubscription = AppState.addEventListener('change', (state: AppStateStatus) => {
    setAppActive(state === 'active');
    if (state === 'active') report(retryDisconnected(), 'retrying servers');
  });
}
