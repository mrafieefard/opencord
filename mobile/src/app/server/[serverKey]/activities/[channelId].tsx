import { useLocalSearchParams } from 'expo-router';

import { ActivityPicker } from '@/features/plugins/ActivityPicker';

// Starting an activity in the call this device is in; the route names the
// call's channel, as the soundboard's does.
export default function ActivitiesRoute() {
  const { serverKey, channelId } = useLocalSearchParams<{ serverKey: string; channelId: string }>();
  return <ActivityPicker serverKey={serverKey} channelId={channelId} />;
}
