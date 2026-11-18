import { useLocalSearchParams } from 'expo-router';

import { IdentityChangedScreen } from '@/features/servers/IdentityChangedScreen';

export default function IdentityChangedRoute() {
  const { serverKey } = useLocalSearchParams<{ serverKey: string }>();
  return <IdentityChangedScreen serverKey={serverKey ?? ''} />;
}
