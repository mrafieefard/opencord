import { memo } from 'react';
import { AttachmentKind, type AttachmentRecord } from '@opencord/core-native';

import { ImageAttachment } from './ImageAttachment';
import { FileAttachment } from './FileAttachment';

export interface AttachmentViewProps {
  serverKey: string;
  attachment: AttachmentRecord;
}

export const AttachmentView = memo(function AttachmentView({
  serverKey,
  attachment,
}: AttachmentViewProps) {
  if (attachment.kind === AttachmentKind.Image) {
    return <ImageAttachment serverKey={serverKey} attachment={attachment} />;
  }
  return <FileAttachment serverKey={serverKey} attachment={attachment} />;
});
