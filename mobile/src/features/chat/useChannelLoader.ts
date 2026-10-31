import { useCallback, useEffect } from 'react';

import { closeChannel, loadOlder, openChannel } from '@/state/actions/chat';

export function useChannelLoader(serverKey: string, channelId: string) {
  useEffect(() => {
    openChannel(serverKey, channelId);

    return () => {
      closeChannel();
    };
  }, [serverKey, channelId]);

  const handleLoadOlder = useCallback(() => {
    loadOlder(serverKey, channelId);
  }, [serverKey, channelId]);

  return {
    loadOlder: handleLoadOlder,
  };
}
