import { UploadPhaseRecord_Tags } from '@opencord/core-native';
import { memo, useEffect, useRef } from 'react';
import { Animated, StyleSheet, View } from 'react-native';

import { cancelUpload, dismissUpload, retryUpload } from '@/state/actions/music';
import type { MusicUploadState } from '@/state/types';
import { IconButton, Text, radius, space, useTheme } from '@/ui';

import { uploadText } from './uploadText';

export interface UploadRowProps {
  upload: MusicUploadState;
}

export const UploadRow = memo(function UploadRow({ upload }: UploadRowProps) {
  const { colors } = useTheme();
  const opacity = useRef(new Animated.Value(1)).current;

  const isDone = upload.phase.tag === UploadPhaseRecord_Tags.Done;
  const isFailed = upload.phase.tag === UploadPhaseRecord_Tags.Failed;
  const isCancelled = upload.phase.tag === UploadPhaseRecord_Tags.Cancelled;
  const isActive = !isDone && !isFailed && !isCancelled;

  useEffect(() => {
    if (isDone) {
      const timer = setTimeout(() => {
        Animated.timing(opacity, {
          toValue: 0,
          duration: 300,
          useNativeDriver: true,
        }).start(() => {
          dismissUpload(upload.uploadId);
        });
      }, 2700);
      return () => clearTimeout(timer);
    }
  }, [isDone, upload.uploadId, opacity]);

  const percent =
    upload.totalBytes > 0
      ? Math.min(100, Math.max(0, Math.round((upload.sentBytes / upload.totalBytes) * 100)))
      : 0;

  const statusText = uploadText(
    upload.phase,
    upload.sentBytes,
    upload.totalBytes,
  );

  return (
    <Animated.View style={[styles.container, { opacity, borderBottomColor: colors.border }]}>
      <View style={styles.content}>
        <View style={styles.infoCol}>
          <Text variant="subhead" numberOfLines={1}>
            {upload.filename}
          </Text>
          <Text
            variant="footnote"
            color={isFailed ? 'recording' : 'textSecondary'}
            numberOfLines={1}
          >
            {statusText}
          </Text>
          {isActive ? (
            <View style={[styles.progressBarTrack, { backgroundColor: colors.border }]}>
              <View
                style={[
                  styles.progressBarFill,
                  { backgroundColor: colors.voice, width: `${percent}%` },
                ]}
              />
            </View>
          ) : null}
        </View>

        <View style={styles.actions}>
          {isActive ? (
            <IconButton
              icon="close"
              label={`Cancel upload ${upload.filename}`}
              size={36}
              onPress={() => cancelUpload(upload.uploadId)}
            />
          ) : null}
          {isFailed ? (
            <>
              <IconButton
                icon="retry"
                label={`Retry ${upload.filename}`}
                size={36}
                onPress={() => retryUpload(upload.uploadId)}
              />
              <IconButton
                icon="close"
                label={`Dismiss ${upload.filename}`}
                size={36}
                onPress={() => dismissUpload(upload.uploadId)}
              />
            </>
          ) : null}
        </View>
      </View>
    </Animated.View>
  );
});

const styles = StyleSheet.create({
  container: {
    width: '100%',
    paddingVertical: space.sm,
    paddingHorizontal: space.md,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  content: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
  },
  infoCol: {
    flex: 1,
    gap: space.xs,
  },
  progressBarTrack: {
    width: '100%',
    height: 3,
    borderRadius: radius.pill,
    overflow: 'hidden',
    marginTop: space.xs,
  },
  progressBarFill: {
    height: '100%',
    borderRadius: radius.pill,
  },
  actions: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
  },
});
