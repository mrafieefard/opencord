import { memo } from 'react';
import { AttachmentKind, type AttachmentRecord } from '@opencord/core-native';

import { ImageAttachment } from './ImageAttachment';
import { VideoAttachment } from './VideoAttachment';
import { AudioFileAttachment } from './AudioFileAttachment';
import { FileAttachment } from './FileAttachment';
import { VoiceMessageAttachment } from './VoiceMessageAttachment';
import { isAudioAttachment, isVideoAttachment } from './previewable';

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
  if (attachment.kind === AttachmentKind.VoiceMessage) {
    return <VoiceMessageAttachment serverKey={serverKey} attachment={attachment} maxWidth={maxWidth} />;
  }
  if (isVideoAttachment(attachment)) {
    return <VideoAttachment serverKey={serverKey} attachment={attachment} maxWidth={maxWidth} />;
  }
  if (isAudioAttachment(attachment)) {
    return <AudioFileAttachment serverKey={serverKey} attachment={attachment} maxWidth={maxWidth} />;
  }
  return <FileAttachment serverKey={serverKey} attachment={attachment} maxWidth={maxWidth} />;
});
