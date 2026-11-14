import { useLocalSearchParams } from 'expo-router';

import { ActivityPage } from '@/features/plugins/ActivityPage';

// An activity's own page, over the call (showActivity).
export default function ActivityRoute() {
  const { serverKey, sessionId } = useLocalSearchParams<{ serverKey: string; sessionId: string }>();
  return <ActivityPage serverKey={serverKey} sessionId={sessionId} />;
}
