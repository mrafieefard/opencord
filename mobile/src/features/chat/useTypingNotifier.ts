import { useCallback, useRef } from 'react';

import { notifyTyping } from '@/state/actions/chat';

/** How often typing is said again while it goes on. */
export const TYPING_EVERY_MS = 8000;

/** Calls `send` at most once every 8 seconds, as the user types. */
export function useThrottledTyping(send: () => void): () => void {
  const lastNotified = useRef(0);

  return useCallback(() => {
    const now = Date.now();
    if (now - lastNotified.current >= TYPING_EVERY_MS) {
      lastNotified.current = now;
      send();
    }
  }, [send]);
}

/** Throttles typing notifications to at most once every 8 seconds. */
export function useTypingNotifier(serverKey: string, channelId: string): () => void {
  const send = useCallback(() => {
    // Best effort: notifyTyping logs a failure itself.
    void notifyTyping(serverKey, channelId);
  }, [serverKey, channelId]);
  return useThrottledTyping(send);
}
