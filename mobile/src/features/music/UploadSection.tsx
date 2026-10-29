import { useCallback } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { report } from '@/core';
import { uploadSongs } from '@/state/actions/music';
import { useChannelInfo, useMusicUploads, useMyVoice } from '@/state/selectors';
import { Icon, Text, space, useTheme } from '@/ui';

import { pickAudio } from './pickAudio';
import { UploadRow } from './UploadRow';

export interface UploadSectionProps {
  serverKey: string;
  channelId: string;
}

export function UploadSection({ serverKey, channelId }: UploadSectionProps) {
  const { colors } = useTheme();
  const info = useChannelInfo(serverKey, channelId);
  const myVoice = useMyVoice();
  const uploads = useMusicUploads(serverKey, channelId);

  const channelName = info?.channel.name ?? 'channel';
  const inVoice = myVoice?.serverKey === serverKey && myVoice.channelId === channelId;
  const canManage = !!info?.access.musicManage;
  const canAdd = !!info?.access.musicAdd;
  const canAttach = !!info?.access.attach;
  const canUpload = canManage || (canAdd && canAttach && inVoice);

  const getMissingReason = () => {
    if (!canAdd) return 'You need Add to music queue to upload songs.';
    if (!canAttach) return 'You need Attach files to upload songs.';
    if (!inVoice) return `Join ${channelName} to upload songs.`;
    return 'You cannot upload songs here.';
  };

  const handlePickFiles = useCallback(async () => {
    report(
      (async () => {
        const files = await pickAudio();
        if (files.length > 0) {
          await uploadSongs(serverKey, channelId, files);
        }
      })(),
      'choose audio files',
    );
  }, [serverKey, channelId]);

  if (!canUpload) {
    return (
      <View style={styles.notAllowedContainer}>
        <Text variant="callout" color="textSecondary" style={styles.centerText}>
          {getMissingReason()}
        </Text>
      </View>
    );
  }

  return (
    <View style={styles.container}>
      <Touchable
        onPress={handlePickFiles}
        accessible
        accessibilityRole="button"
        accessibilityLabel="Choose audio files"
        style={[
          styles.pickButton,
          {
            borderColor: colors.border,
            backgroundColor: colors.surface,
          },
        ]}
      >
        <Icon name="upload" size={20} color="textSecondary" />
        <Text variant="callout" style={styles.buttonText}>
          Choose audio files
        </Text>
      </Touchable>

      <Text variant="footnote" color="textMuted" style={styles.helpText}>
        MP3, FLAC, M4A, AAC, OGG or WAV. Songs you upload stay on this server: find them in Search, marked Local.
      </Text>

      {uploads.length > 0 ? (
        <View style={styles.uploadsList}>
          {uploads.map((upload) => (
            <UploadRow key={upload.uploadId} upload={upload} />
          ))}
        </View>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    width: '100%',
    paddingHorizontal: space.md,
  },
  notAllowedContainer: {
    width: '100%',
    paddingHorizontal: space.lg,
    paddingVertical: space.xl,
    alignItems: 'center',
    justifyContent: 'center',
  },
  centerText: {
    textAlign: 'center',
  },
  pickButton: {
    width: '100%',
    height: 56,
    borderRadius: 14,
    borderWidth: 1.5,
    borderStyle: 'dashed',
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: space.sm,
  },
  buttonText: {
    textAlign: 'center',
  },
  helpText: {
    marginTop: space.sm,
    marginBottom: space.md,
    lineHeight: 18,
  },
  uploadsList: {
    width: '100%',
  },
});
