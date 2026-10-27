import { useLocalSearchParams } from 'expo-router';

import { ServerScreen } from '@/features/server/ServerScreen';

export default function ServerRoute() {
  const { serverKey, channel, sidebar } = useLocalSearchParams<{ serverKey: string; channel?: string; sidebar?: string }>();
  return (
    <ServerScreen
      key={`${serverKey}:${channel ?? ''}:${sidebar ?? ''}`}
      serverKey={serverKey ?? ''}
      channelId={channel}
      startWithSidebar={sidebar === '1'}
    />
  );
}
