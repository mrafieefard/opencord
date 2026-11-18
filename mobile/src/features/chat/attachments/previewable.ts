import { AttachmentKind, type AttachmentRecord } from '@opencord/core-native';

/** Files whose text the Preview button shows, up to [MAX_PREVIEW_BYTES]. Matches desktop textPreviewExtensions. */
export const TEXT_PREVIEW_EXTENSIONS = new Set([
  'txt',
  'md',
  'log',
  'json',
  'csv',
  'xml',
  'yaml',
  'yml',
  'toml',
  'ini',
  'cfg',
  'conf',
  'rs',
  'py',
  'js',
  'ts',
  'dart',
  'c',
  'h',
  'cpp',
  'java',
  'go',
  'sh',
]);

export const MAX_PREVIEW_BYTES = 256 * 1024; // 256 KB

export const AUDIO_EXTENSIONS = new Set(['mp3', 'ogg', 'm4a', 'wav', 'flac']);
export const VIDEO_EXTENSIONS = new Set(['mp4', 'm4v', 'mov', 'webm', 'mkv']);

/** The lowercase text after a filename's last dot, or ''. */
export function fileExtension(filename: string): string {
  const dot = filename.lastIndexOf('.');
  return dot === -1 ? '' : filename.substring(dot + 1).toLowerCase();
}

/** Returns true if the file extension is previewable text and the file is <= 256 KB. */
export function isTextPreviewable(filename: string, sizeBytes: number): boolean {
  if (sizeBytes > MAX_PREVIEW_BYTES) return false;
  const ext = fileExtension(filename);
  return TEXT_PREVIEW_EXTENSIONS.has(ext);
}

/** Returns true if the attachment is an audio file (not a voice message). */
export function isAudioAttachment(attachment: AttachmentRecord): boolean {
  if (attachment.kind === AttachmentKind.Audio) return true;
  if (attachment.kind === AttachmentKind.File) {
    return AUDIO_EXTENSIONS.has(fileExtension(attachment.filename));
  }
  return false;
}

/** Returns true if the attachment is a video file. */
export function isVideoAttachment(attachment: AttachmentRecord): boolean {
  if (attachment.kind === AttachmentKind.Video) return true;
  if (attachment.kind === AttachmentKind.File) {
    return VIDEO_EXTENSIONS.has(fileExtension(attachment.filename));
  }
  return false;
}
