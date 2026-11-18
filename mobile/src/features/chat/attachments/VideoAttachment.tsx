import { memo, useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Modal, StyleSheet, View } from 'react-native';
import { GestureHandlerRootView, Touchable } from 'react-native-gesture-handler';
import { Image } from 'expo-image';
import { useVideoPlayer, VideoView } from 'expo-video';
import { AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';

import { report } from '@/core';
import { Icon, IconButton, Text, radius, space, useTheme } from '@/ui';
import { onMedia } from '@/ui/theme/tokens';
import { formatDurationMs } from './FileAttachment';
import { fitSize } from './fitSize';
import { fetchAttachmentFile, useAttachmentFile } from './useAttachmentFile';

export interface VideoAttachmentProps {
  serverKey: string;
  attachment: AttachmentRecord;
  maxWidth: number;
}

const PLAY_BUTTON_SIZE = 48;

/** Active video key tracked so only one video player exists at any time across the app. */
let currentActiveVideoKey: string | null = null;
const listeners = new Set<(activeKey: string | null) => void>();

function setActiveVideoKey(key: string | null) {
  currentActiveVideoKey = key;
  for (const listener of listeners) {
    listener(key);
  }
}

interface ActiveVideoModalProps {
  uri: string;
  attachmentId: string;
  onClose: () => void;
}

function ActiveVideoModal({ uri, attachmentId, onClose }: ActiveVideoModalProps) {
  const player = useVideoPlayer({ uri }, (p) => {
    p.loop = false;
    p.play();
  });

  return (
    <Modal
      visible
      animationType="fade"
      transparent={false}
      onRequestClose={onClose}
      testID={`video-player-modal-${attachmentId}`}
    >
      <GestureHandlerRootView style={StyleSheet.absoluteFill}>
        <View style={styles.playerContainer}>
          <VideoView
            testID={`video-player-view-${attachmentId}`}
            player={player}
            style={StyleSheet.absoluteFill}
            allowsPictureInPicture
            nativeControls
            contentFit="contain"
          />
          <View style={styles.modalHeader}>
            <IconButton icon="close" label="Close" onPress={onClose} size={44} tone="plain" />
          </View>
        </View>
      </GestureHandlerRootView>
    </Modal>
  );
}

export const VideoAttachment = memo(function VideoAttachment({
  serverKey,
  attachment,
  maxWidth,
}: VideoAttachmentProps) {
  const { colors } = useTheme();
  const videoKey = `${serverKey}/${attachment.id}`;

  const [downloading, setDownloading] = useState(false);
  const [downloadFailed, setDownloadFailed] = useState(false);
  const [localUri, setLocalUri] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);

  // Thumbnail poster only if attachment.hasThumbnail is true
  const thumb = useAttachmentFile(
    serverKey,
    attachment.hasThumbnail ? attachment.id : undefined,
    AttachmentVariant.Thumbnail,
  );
  const posterUri = attachment.hasThumbnail ? thumb.path : null;

  const size = fitSize(attachment.width ?? 0, attachment.height ?? 0, Math.min(260, maxWidth));

  // When another video becomes active, pause and dismiss this one
  useEffect(() => {
    const onActiveChange = (activeKey: string | null) => {
      if (activeKey !== videoKey && isPlaying) {
        setIsPlaying(false);
      }
    };
    listeners.add(onActiveChange);
    return () => {
      listeners.delete(onActiveChange);
    };
  }, [videoKey, isPlaying]);

  const handlePlayPress = useCallback(() => {
    if (downloading) return;
    setDownloadFailed(false);

    if (localUri) {
      setActiveVideoKey(videoKey);
      setIsPlaying(true);
      return;
    }

    setDownloading(true);
    const fetch = fetchAttachmentFile(serverKey, attachment.id, AttachmentVariant.Original)
      .then((uri) => {
        setLocalUri(uri);
        setActiveVideoKey(videoKey);
        setIsPlaying(true);
      })
      .catch((error: unknown) => {
        setDownloadFailed(true);
        throw error;
      })
      .finally(() => {
        setDownloading(false);
      });

    report(fetch, 'download video');
  }, [downloading, localUri, serverKey, attachment.id, videoKey]);

  const handleClose = useCallback(() => {
    setIsPlaying(false);
    if (currentActiveVideoKey === videoKey) {
      setActiveVideoKey(null);
    }
  }, [videoKey]);

  return (
    <View style={[styles.container, { width: size.width, height: size.height }]}>
      {/* Poster image or dark tile */}
      {posterUri ? (
        <Image
          source={{ uri: posterUri }}
          style={StyleSheet.absoluteFill}
          contentFit="cover"
          accessibilityLabel={attachment.filename}
        />
      ) : (
        <View style={[StyleSheet.absoluteFill, { backgroundColor: colors.surface }]} pointerEvents="none" />
      )}

      {/* Dim overlay for contrast */}
      <View style={[StyleSheet.absoluteFill, { backgroundColor: colors.scrim }]} pointerEvents="none" />

      {/* Center play / loading progress / retry button */}
      <View style={styles.centerSlot}>
        {downloading ? (
          <View style={[styles.progressBox, { backgroundColor: colors.scrim }]}>
            <ActivityIndicator
              size="small"
              color={colors.text}
              testID={`video-loading-${attachment.id}`}
            />
            <Text variant="caption" color="text">
              Downloading…
            </Text>
          </View>
        ) : downloadFailed ? (
          <Touchable
            onPress={handlePlayPress}
            testID={`video-retry-${attachment.id}`}
            hitSlop={8}
            style={[styles.playButton, { backgroundColor: colors.surface }]}
          >
            <View accessible accessibilityRole="button" accessibilityLabel="Retry downloading video" style={styles.iconCenter}>
              <Icon name="retry" size={24} color="text" />
            </View>
          </Touchable>
        ) : (
          <Touchable
            onPress={handlePlayPress}
            testID={`video-play-${attachment.id}`}
            hitSlop={8}
            style={[styles.playButton, { backgroundColor: colors.elevated }]}
          >
            <View accessible accessibilityRole="button" accessibilityLabel={`Play ${attachment.filename}`} style={styles.iconCenter}>
              <Icon name="play" size={24} color="text" />
            </View>
          </Touchable>
        )}
      </View>

      {/* Duration badge */}
      {attachment.durationMs !== undefined && attachment.durationMs > 0 && (
        <View style={[styles.durationBadge, { backgroundColor: colors.scrim }]}>
          <Text variant="caption" color="text" tabular>
            {formatDurationMs(attachment.durationMs)}
          </Text>
        </View>
      )}

      {/* Full screen video player modal — only instantiated while active (Review Focus 4) */}
      {isPlaying && localUri && (
        <ActiveVideoModal
          uri={localUri}
          attachmentId={attachment.id}
          onClose={handleClose}
        />
      )}
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    borderRadius: radius.image,
    overflow: 'hidden',
    position: 'relative',
    justifyContent: 'center',
    alignItems: 'center',
  },
  centerSlot: {
    justifyContent: 'center',
    alignItems: 'center',
  },
  progressBox: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
    paddingHorizontal: space.sm,
    paddingVertical: space.xs,
    borderRadius: radius.pill,
  },
  playButton: {
    width: PLAY_BUTTON_SIZE,
    height: PLAY_BUTTON_SIZE,
    borderRadius: PLAY_BUTTON_SIZE / 2,
    justifyContent: 'center',
    alignItems: 'center',
  },
  iconCenter: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  durationBadge: {
    position: 'absolute',
    bottom: space.xs,
    right: space.xs,
    paddingHorizontal: space.xs,
    paddingVertical: 2,
    borderRadius: radius.pill,
  },
  playerContainer: {
    flex: 1,
    backgroundColor: onMedia.chat,
  },
  modalHeader: {
    position: 'absolute',
    top: space.xl,
    left: space.md,
    zIndex: 10,
  },
});
