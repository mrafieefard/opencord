import { useLocalSearchParams } from 'expo-router';

import { MembersSheet } from '@/features/members/MembersSheet';

export default function MembersRoute() {
  const { serverKey } = useLocalSearchParams<{ serverKey: string }>();
  return <MembersSheet serverKey={serverKey ?? ''} />;
}
