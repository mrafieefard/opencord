import { useLocalSearchParams } from 'expo-router';

import { ServerSheet } from '@/features/serverMenu/ServerSheet';

export default function ServerSheetRoute() {
  const { serverKey } = useLocalSearchParams<{ serverKey: string }>();
  return <ServerSheet serverKey={serverKey ?? ''} />;
}
