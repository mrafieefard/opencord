import { useLocalSearchParams } from 'expo-router';

import { InviteSheet } from '@/features/invites/InviteSheet';

export default function InviteRoute() {
  const { serverKey } = useLocalSearchParams<{ serverKey: string }>();
  return <InviteSheet serverKey={serverKey ?? ''} />;
}
