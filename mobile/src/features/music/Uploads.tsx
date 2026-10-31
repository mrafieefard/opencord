// Uploading songs from the phone (spec §8.7). The upload button beside
// the search bar picks audio files, several at once; each file is a row
// above Up next while it goes. Allowed with Manage music, or Add to music
// queue and Attach files while in the channel; otherwise the button is
// dimmed and says what's missing.
import { useCallback } from 'react';
import { View } from 'react-native';

import { report } from '@/core';
import { uploadSongs } from '@/state/actions/music';
import { toast } from '@/state/actions/ui';
import { useChannelInfo, useMusicUploads, useMyVoice } from '@/state/selectors';

import { pickAudio } from './pickAudio';
import { SectionHeader } from './SectionHeader';
import { UploadRow } from './UploadRow';

export interface UploadPicker {
  /** Why the user can't upload here; null when they can. */
  missing: string | null;
  pick: () => void;
}

export function useUploadPicker(serverKey: string, channelId: string): UploadPicker {
  const info = useChannelInfo(serverKey, channelId);
  const myVoice = useMyVoice();
  const inVoice = myVoice?.serverKey === serverKey && myVoice.channelId === channelId;
  const access = info?.access;
  const channelName = info?.channel.name ?? 'the channel';

  const missing = access?.musicManage
    ? null
    : !access?.musicAdd
      ? 'You need Add to music queue to upload songs.'
      : !access.attach
        ? 'You need Attach files to upload songs.'
        : !inVoice
          ? `Join ${channelName} to upload songs.`
          : null;

  const pick = useCallback(() => {
    if (missing !== null) {
      toast(missing);
      return;
    }
    report(
      pickAudio().then((files) => (files.length > 0 ? uploadSongs(serverKey, channelId, files) : undefined)),
      'choose audio files',
    );
  }, [missing, serverKey, channelId]);

  return { missing, pick };
}

export function Uploads({ serverKey, channelId }: { serverKey: string; channelId: string }) {
  const uploads = useMusicUploads(serverKey, channelId);
  if (uploads.length === 0) return null;
  return (
    <View>
      <SectionHeader title="Uploading" />
      {uploads.map((upload) => (
        <UploadRow key={upload.uploadId} upload={upload} />
      ))}
    </View>
  );
}
