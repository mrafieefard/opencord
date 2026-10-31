import { useLocalSearchParams } from 'expo-router';

import { ImageViewer } from '@/features/viewer/ImageViewer';

export default function ViewerRoute() {
  const { serverKey, attachmentId } = useLocalSearchParams<{
    serverKey: string;
    attachmentId: string;
  }>();

  return (
    <ImageViewer
      serverKey={serverKey ?? ''}
      attachmentId={attachmentId ?? ''}
    />
  );
}
