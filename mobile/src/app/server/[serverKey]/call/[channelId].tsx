import { useLocalSearchParams } from 'expo-router';

import { VoicePane } from '@/features/voice/VoicePane';

// The call, as a sheet over everything (openCall).
export default function CallRoute() {
  const { serverKey, channelId } = useLocalSearchParams<{ serverKey: string; channelId: string }>();
  return <VoicePane serverKey={serverKey ?? ''} channelId={channelId ?? ''} asCall />;
}
