import { useLocalSearchParams } from 'expo-router';

import { VoiceChannelSheet } from '@/features/voice/VoiceChannelSheet';

export default function VoiceChannelRoute() {
  const { serverKey, channelId } = useLocalSearchParams<{ serverKey: string; channelId: string }>();
  return <VoiceChannelSheet serverKey={serverKey ?? ''} channelId={channelId ?? ''} />;
}
