import { useLocalSearchParams } from 'expo-router';

import { useServer } from '@/state/selectors';
import { EmptyState } from '@/ui';

// later: Task 11 puts the server screen here.
export default function ServerScreen() {
  const { serverKey } = useLocalSearchParams<{ serverKey: string }>();
  const server = useServer(serverKey ?? '');
  const name = server?.snapshot?.name ?? server?.entry.name ?? 'Server';
  return <EmptyState title={name} />;
}
