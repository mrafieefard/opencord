// Subscribes to music session updates while the sheet is open (spec §8.7).
import { useEffect } from 'react';

import { report } from '@/core';
import { subscribeMusic, unsubscribeMusic } from '@/state/actions/music';

export function useMusicSessionSubscription(serverKey: string, channelId: string): void {
  useEffect(() => {
    if (!serverKey || !channelId) return;
    void report(subscribeMusic(serverKey, channelId), 'subscribe to music');
    return () => {
      void report(unsubscribeMusic(serverKey, channelId), 'unsubscribe from music');
    };
  }, [serverKey, channelId]);
}
