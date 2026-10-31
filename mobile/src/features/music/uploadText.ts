import { UploadPhaseRecord, UploadPhaseRecord_Tags } from '@opencord/core-native';

export function formatUploadSize(bytes: number, locale = 'en-US'): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) {
    const formatted = kb.toLocaleString(locale, {
      minimumFractionDigits: kb % 1 === 0 ? 0 : 1,
      maximumFractionDigits: 1,
    });
    return `${formatted} KB`;
  }
  const mb = kb / 1024;
  if (mb < 1024) {
    const formatted = mb.toLocaleString(locale, {
      minimumFractionDigits: mb % 1 === 0 ? 0 : 1,
      maximumFractionDigits: 1,
    });
    return `${formatted} MB`;
  }
  const gb = mb / 1024;
  const formatted = gb.toLocaleString(locale, {
    minimumFractionDigits: gb % 1 === 0 ? 0 : 1,
    maximumFractionDigits: 1,
  });
  return `${formatted} GB`;
}

export function uploadText(
  phase: UploadPhaseRecord,
  sent: number,
  total: number,
  locale = 'en-US',
): string {
  switch (phase.tag) {
    case UploadPhaseRecord_Tags.Preparing:
      return 'Checking…';
    case UploadPhaseRecord_Tags.Uploading: {
      const percent = total > 0 ? Math.round((sent / total) * 100) : 0;
      return `Uploading · ${percent} % of ${formatUploadSize(total, locale)}`;
    }
    case UploadPhaseRecord_Tags.Processing:
    case UploadPhaseRecord_Tags.Sending:
      return 'The server is checking it…';
    case UploadPhaseRecord_Tags.Done:
      return 'Queued';
    case UploadPhaseRecord_Tags.Failed: {
      const failed = phase as { message?: string; inner?: { message?: string } };
      const msg = failed.inner?.message ?? failed.message ?? 'Upload failed';
      return `Failed: ${msg}`;
    }
    case UploadPhaseRecord_Tags.Cancelled:
      return 'Cancelled';
  }
}
