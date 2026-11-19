import { router } from 'expo-router';
import { View } from 'react-native';

import { EmptyState, PaneHeader } from '@/ui';

export default function FriendsRoute() {
  return (
    <View style={{ flex: 1 }}>
      <PaneHeader title="Friends" left={{ icon: 'back', label: 'Back', onPress: () => router.back() }} />
      <EmptyState title="Friends" body="No pending requests." />
    </View>
  );
}
