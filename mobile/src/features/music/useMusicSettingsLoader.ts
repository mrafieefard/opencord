// A server's music settings, fetched while something shows them (the voice
// pane's Music button, the player's sources), as the desktop's
// musicSettingsProvider does: again after each reconnect, since a change
// made while away came as no event.
import { ConnectionRecord_Tags } from '@opencord/core-native';
import { useEffect } from 'react';

import { errorKind } from '@/core';
import { loadMusicSettings } from '@/state/actions/music';
import { useServer } from '@/state/selectors';

export function useMusicSettingsLoader(serverKey: string): void {
  const server = useServer(serverKey);
  const connected = server?.connection.tag === ConnectionRecord_Tags.Connected;
  const readyCount = server?.readyCount ?? 0;

  useEffect(() => {
    if (!connected) return;
    loadMusicSettings(serverKey).catch((error: unknown) => {
      console.warn(`music settings for ${serverKey} failed: ${errorKind(error)}`);
    });
  }, [serverKey, connected, readyCount]);
}
