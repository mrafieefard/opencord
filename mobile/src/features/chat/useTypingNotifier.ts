import { useCallback, useRef } from 'react';

import { notifyTyping } from '@/state/actions/chat';

/** Throttles typing notifications to at most once every 8 seconds. */
export function useTypingNotifier(serverKey: string, channelId: string): () => void {
  const lastNotified = useRef(0);

  return useCallback(() => {
    const now = Date.now();
    if (now - lastNotified.current >= 8000) {
      lastNotified.current = now;
      notifyTyping(serverKey, channelId).catch(() => {});
    }
  }, [serverKey, channelId]);
}
