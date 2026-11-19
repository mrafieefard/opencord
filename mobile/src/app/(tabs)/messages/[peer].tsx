import { router, useLocalSearchParams } from 'expo-router';

import { PrivateChat } from '@/features/private/PrivateChat';

export default function PrivateChatRoute() {
  const { peer } = useLocalSearchParams<{ peer: string }>();
  return <PrivateChat key={peer} peer={peer ?? ''} onBack={router.back} onLeave={router.back} />;
}
