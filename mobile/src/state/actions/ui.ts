// What the screens tell the store about themselves: the chat on screen,
// whether the app is in front, and toasts.
import { useAppStore } from '../store';
import type { Toast } from '../types';

export const TOAST_MS = 3000;
let nextToast = 1;

export function toast(text: string, tone: Toast['tone'] = 'neutral'): void {
  const id = nextToast;
  nextToast += 1;
  useAppStore.setState((state) => ({ toasts: [...state.toasts, { id, text, tone }] }));
  setTimeout(() => {
    useAppStore.setState((state) => ({ toasts: state.toasts.filter((entry) => entry.id !== id) }));
  }, TOAST_MS);
}

export function setOpenChannel(serverKey: string, channelId: string, atBottom: boolean): void {
  const open = useAppStore.getState().openChannel;
  if (open?.serverKey === serverKey && open.channelId === channelId && open.atBottom === atBottom) return;
  useAppStore.setState({ openChannel: { serverKey, channelId, atBottom } });
}

export function clearOpenChannel(): void {
  useAppStore.setState({ openChannel: null });
}

export function setAppActive(active: boolean): void {
  useAppStore.setState({ appActive: active });
}

/** Whether a text channel fills the screen, which hides the tab bar. */
export function setChatInFront(chatInFront: boolean): void {
  if (useAppStore.getState().chatInFront !== chatInFront) useAppStore.setState({ chatInFront });
}

/** Which voice channel's page fills the screen, if one does. */
export function setVoicePageInFront(page: { serverKey: string; channelId: string } | null): void {
  const held = useAppStore.getState().voicePageInFront;
  if (held?.serverKey === page?.serverKey && held?.channelId === page?.channelId) return;
  useAppStore.setState({ voicePageInFront: page });
}

/** The call's glass has left its bar (or come back into it). */
export function setCallUp(callUp: boolean): void {
  if (useAppStore.getState().callUp !== callUp) useAppStore.setState({ callUp });
}

/** The call is wholly open (or no longer). */
export function setCallFull(callFull: boolean): void {
  if (useAppStore.getState().callFull !== callFull) useAppStore.setState({ callFull });
}
