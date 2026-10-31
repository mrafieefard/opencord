// One file going up (spec §8.7): the filename, what's happening and a thin
// bar while it uploads. An arrow, a tick or a warning where the artwork
// would be. Cancel while it goes; retry and dismiss after a failure. A
// queued row fades out within 3 s.
import { UploadPhaseRecord_Tags } from '@opencord/core-native';
import { memo, useEffect, useRef } from 'react';
import { Animated, StyleSheet, View } from 'react-native';

import { cancelUpload, dismissUpload, retryUpload } from '@/state/actions/music';
import type { MusicUploadState } from '@/state/types';
import { Icon, IconButton, ProgressBar, Text, motion, radius, space, useTheme, weightFamily } from '@/ui';

import { GUTTER, TRACK_ROW_HEIGHT } from './TrackRow';
import { uploadText } from './uploadText';

export interface UploadRowProps {
  upload: MusicUploadState;
}

const LEADING = 48;
const QUEUED_SHOWN_MS = 2700;

export const UploadRow = memo(function UploadRow({ upload }: UploadRowProps) {
  const { colors } = useTheme();
  const opacity = useRef(new Animated.Value(1)).current;

  const tag = upload.phase.tag;
  const isDone = tag === UploadPhaseRecord_Tags.Done;
  const isFailed = tag === UploadPhaseRecord_Tags.Failed;
  const isCancelled = tag === UploadPhaseRecord_Tags.Cancelled;
  const isActive = !isDone && !isFailed && !isCancelled;
  const isSending = tag === UploadPhaseRecord_Tags.Uploading;

  useEffect(() => {
    if (!isDone) return;
    const timer = setTimeout(() => {
      Animated.timing(opacity, { toValue: 0, duration: motion.fade, useNativeDriver: true }).start(() =>
        dismissUpload(upload.uploadId),
      );
    }, QUEUED_SHOWN_MS);
    return () => clearTimeout(timer);
  }, [isDone, upload.uploadId, opacity]);

  const progress = upload.totalBytes > 0 ? upload.sentBytes / upload.totalBytes : 0;

  return (
    <Animated.View style={[styles.row, { opacity }]}>
      <View style={[styles.leading, { backgroundColor: colors.selected }]}>
        <Icon
          name={isActive ? 'upload' : isDone ? 'check' : isFailed ? 'warning' : 'close'}
          size={20}
          color={isFailed ? 'recording' : 'textSecondary'}
        />
      </View>
      <View style={styles.text}>
        <Text variant="callout" numberOfLines={1} style={styles.name}>
          {upload.filename}
        </Text>
        <Text variant="footnote" color={isFailed ? 'recording' : 'textSecondary'} numberOfLines={2} tabular>
          {uploadText(upload.phase, upload.sentBytes, upload.totalBytes)}
        </Text>
        {isSending && (
          <View style={styles.bar}>
            <ProgressBar progress={progress} />
          </View>
        )}
      </View>
      {isActive && (
        <IconButton icon="close" label={`Cancel upload ${upload.filename}`} size={36} onPress={() => cancelUpload(upload.uploadId)} />
      )}
      {isFailed && (
        <>
          <IconButton icon="retry" label={`Retry ${upload.filename}`} size={36} onPress={() => void retryUpload(upload.uploadId)} />
          <IconButton icon="close" label={`Dismiss ${upload.filename}`} size={36} onPress={() => dismissUpload(upload.uploadId)} />
        </>
      )}
      {isCancelled && (
        <IconButton icon="close" label={`Dismiss ${upload.filename}`} size={36} onPress={() => dismissUpload(upload.uploadId)} />
      )}
    </Animated.View>
  );
});

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', minHeight: TRACK_ROW_HEIGHT, paddingHorizontal: GUTTER, paddingVertical: space.sm, gap: space.md },
  leading: { width: LEADING, height: LEADING, borderRadius: radius.row, alignItems: 'center', justifyContent: 'center' },
  text: { flex: 1, gap: 2 },
  name: { fontFamily: weightFamily(500) },
  bar: { marginTop: space.xs },
});
