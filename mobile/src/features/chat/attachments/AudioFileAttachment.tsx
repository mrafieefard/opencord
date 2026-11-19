import { memo, useCallback, useState } from 'react';
import { ActivityIndicator, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { useAudioPlayer, useAudioPlayerStatus } from 'expo-audio';
import { AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';

import { report } from '@/core';
import { clock, voiceWidth } from '@/features/chat/attachments/VoiceMessageAttachment';
import { Icon, Scrubber, Text, onHue, radius, space, useTheme } from '@/ui';
import { fetchAttachmentFile } from './useAttachmentFile';

export interface AudioFileAttachmentProps {
  serverKey: string;
  attachment: AttachmentRecord;
  maxWidth: number;
}

const BUTTON = 36;

export const AudioFileAttachment = memo(function AudioFileAttachment({
  serverKey,
  attachment,
  maxWidth,
}: AudioFileAttachmentProps) {
  const { colors } = useTheme();
  const [downloading, setDownloading] = useState(false);
  const [localUri, setLocalUri] = useState<string | null>(null);

  const player = useAudioPlayer(null);
  const status = useAudioPlayerStatus(player);

  const durationSec =
    status.duration > 0
      ? status.duration
      : attachment.durationMs
        ? attachment.durationMs / 1000
        : 30;

  const currentSec = status.currentTime ?? 0;
  const isPlaying = status.playing;

  const toggle = useCallback(() => {
    if (downloading) return;
    if (localUri) {
      if (isPlaying) {
        player.pause();
      } else {
        player.play();
      }
      return;
    }

    setDownloading(true);
    const fetch = fetchAttachmentFile(serverKey, attachment.id, AttachmentVariant.Original)
      .then((uri) => {
        setLocalUri(uri);
        player.replace({ uri });
        player.play();
      })
      .finally(() => {
        setDownloading(false);
      });
    report(fetch, 'play audio file');
  }, [downloading, localUri, isPlaying, player, serverKey, attachment.id]);

  const seek = useCallback(
    (seconds: number) => {
      const clamped = Math.min(Math.max(seconds, 0), durationSec);
      void player.seekTo(clamped);
    },
    [player, durationSec],
  );

  const durationMs = Math.round(durationSec * 1000);
  const width = Math.min(voiceWidth(durationMs), maxWidth);

  return (
    <View
      style={[styles.container, { width, backgroundColor: colors.surface }]}
      testID={`audio-attachment-${attachment.id}`}
    >
      <Touchable
        onPress={toggle}
        hitSlop={4}
        testID={`audio-play-${attachment.id}`}
        style={[styles.button, { backgroundColor: colors.voice }]}
      >
        <View
          accessible
          accessibilityRole="button"
          accessibilityLabel={isPlaying ? 'Pause' : `Play ${attachment.filename}`}
          style={styles.center}
        >
          {downloading ? (
            <ActivityIndicator size="small" color={onHue} />
          ) : (
            <Icon name={isPlaying ? 'pause' : 'play'} size={16} tint={onHue} />
          )}
        </View>
      </Touchable>

      <View style={styles.body}>
        <Text variant="subhead" numberOfLines={1} ellipsizeMode="middle">
          {attachment.filename}
        </Text>

        <Scrubber
          testID={`audio-scrubber-${attachment.id}`}
          value={currentSec}
          min={0}
          max={durationSec}
          step={0.5}
          accessibilityStep={5}
          onChange={() => {}}
          onCommit={seek}
          accessibilityLabel="Audio progress"
          accessibilityValue={`${clock(currentSec * 1000)} of ${clock(durationMs)}`}
        />

        <View style={styles.meta}>
          <Text variant="caption" color="textSecondary" tabular>
            {isPlaying || currentSec > 0
              ? `${clock(currentSec * 1000)} / ${clock(durationMs)}`
              : clock(durationMs)}
          </Text>
        </View>
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    padding: space.sm,
    borderRadius: radius.image,
  },
  button: {
    width: BUTTON,
    height: BUTTON,
    borderRadius: BUTTON / 2,
    overflow: 'hidden',
  },
  center: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
  body: {
    flex: 1,
    gap: 2,
  },
  meta: {
    flexDirection: 'row',
    justifyContent: 'space-between',
    alignItems: 'center',
  },
});
