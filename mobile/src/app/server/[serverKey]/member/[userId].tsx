import { useLocalSearchParams } from 'expo-router';

import { ParticipantSheet } from '@/features/voice/ParticipantSheet';

export default function MemberRoute() {
  const { serverKey, userId } = useLocalSearchParams<{ serverKey: string; userId: string }>();
  return <ParticipantSheet serverKey={serverKey ?? ''} userId={userId ?? ''} />;
}
