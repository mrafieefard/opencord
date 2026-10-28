import { memo } from 'react';
import { AttachmentKind, type AttachmentRecord } from '@opencord/core-native';

import { ImageAttachment } from './ImageAttachment';
import { FileAttachment } from './FileAttachment';

export interface AttachmentViewProps {
  serverKey: string;
  attachment: AttachmentRecord;
  /** The room the bubble has for it. */
  maxWidth: number;
}

export const AttachmentView = memo(function AttachmentView({
  serverKey,
  attachment,
  maxWidth,
}: AttachmentViewProps) {
  if (attachment.kind === AttachmentKind.Image) {
    return <ImageAttachment serverKey={serverKey} attachment={attachment} maxWidth={maxWidth} />;
  }
  return <FileAttachment serverKey={serverKey} attachment={attachment} maxWidth={maxWidth} />;
});
