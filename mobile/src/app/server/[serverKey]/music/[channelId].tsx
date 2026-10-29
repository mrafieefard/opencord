// Route for the music player sheet: /server/[serverKey]/music/[channelId]
import { useLocalSearchParams } from 'expo-router';

import { MusicPlayer } from '@/features/music/MusicPlayer';
import { SheetToasts } from '@/ui';

export default function MusicRoute() {
  const { serverKey, channelId } = useLocalSearchParams<{ serverKey: string; channelId: string }>();

  if (!serverKey || !channelId) return null;

  return (
    <>
      <MusicPlayer serverKey={serverKey} channelId={channelId} />
      <SheetToasts />
    </>
  );
}
