import { useLocalSearchParams } from 'expo-router';

import { TextPreview } from '@/features/chat/attachments/TextPreview';

export default function TextPreviewRoute() {
  const { serverKey, attachmentId, filename } = useLocalSearchParams<{
    serverKey: string;
    attachmentId: string;
    filename: string;
  }>();

  return (
    <TextPreview
      serverKey={serverKey ?? ''}
      attachmentId={attachmentId ?? ''}
      filename={filename ?? 'Text Preview'}
    />
  );
}
