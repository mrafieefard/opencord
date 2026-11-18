import { memo, useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Modal, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { Image } from 'expo-image';
import { AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';

import { useVideoPlayer, VideoView } from './videoPlayer';

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

  // Thumbnail poster if available
  const thumb = useAttachmentFile(serverKey, attachment.id, AttachmentVariant.Thumbnail);
  const posterUri = thumb.path;

  const size = fitSize(attachment.width ?? 0, attachment.height ?? 0, Math.min(260, maxWidth));

  const player = useVideoPlayer(localUri ? { uri: localUri } : null, (p) => {
    p.loop = false;
    p.play();
  });

  // When another video becomes active, pause and dismiss this one
  useEffect(() => {
    const onActiveChange = (activeKey: string | null) => {
      if (activeKey !== videoKey && isPlaying) {
        player.pause();
        setIsPlaying(false);
      }
    };
    listeners.add(onActiveChange);
    return () => {
      listeners.delete(onActiveChange);
    };
  }, [videoKey, isPlaying, player]);

  const handlePlayPress = useCallback(() => {
    if (downloading) return;
    setDownloadFailed(false);

    if (localUri) {
      setActiveVideoKey(videoKey);
      setIsPlaying(true);
      player.play();
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
  }, [downloading, localUri, serverKey, attachment.id, videoKey, player]);

  const handleClose = useCallback(() => {
    player.pause();
    setIsPlaying(false);
    if (currentActiveVideoKey === videoKey) {
      setActiveVideoKey(null);
    }
  }, [player, videoKey]);

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
        <View style={[StyleSheet.absoluteFill, { backgroundColor: colors.surface }]} />
      )}

      {/* Dim overlay for contrast */}
      <View style={[StyleSheet.absoluteFill, { backgroundColor: colors.scrim }]} />

      {/* Center play / loading / retry button */}
      <View style={styles.centerSlot}>
        {downloading ? (
          <ActivityIndicator
            size="large"
            color={colors.text}
            testID={`video-loading-${attachment.id}`}
          />
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

      {/* Full screen video player modal */}
      {isPlaying && (
        <Modal
          visible={isPlaying}
          animationType="fade"
          transparent={false}
          onRequestClose={handleClose}
          testID={`video-player-modal-${attachment.id}`}
        >
          <View style={styles.playerContainer}>
            <VideoView
              testID={`video-player-view-${attachment.id}`}
              player={player}
              style={StyleSheet.absoluteFill}
              allowsPictureInPicture
              nativeControls
              contentFit="contain"
            />
            <View style={styles.modalHeader}>
              <IconButton icon="close" label="Close" onPress={handleClose} size={44} tone="plain" />
            </View>
          </View>
        </Modal>
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
