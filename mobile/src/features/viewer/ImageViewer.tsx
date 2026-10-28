import { memo, useCallback, useState } from 'react';
import { ActivityIndicator, StyleSheet, View } from 'react-native';
import Animated, { useAnimatedStyle, withTiming } from 'react-native-reanimated';
import { GestureDetector } from 'react-native-gesture-handler';
import { Image } from 'expo-image';
import { router } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import * as Sharing from 'expo-sharing';
import { AttachmentVariant } from '@opencord/core-native';

import { IconButton, space } from '@/ui';
import { report } from '@/core';
import { onMedia, tokens } from '@/ui/theme/tokens';
import { ThemeContext } from '@/ui/theme/ThemeProvider';
import { useAttachmentFile } from '@/features/chat/attachments/useAttachmentFile';

import { useZoomPan } from './useZoomPan';

export interface ImageViewerProps {
  serverKey: string;
  attachmentId: string;
  onClose?: () => void;
}

export const ImageViewer = memo(function ImageViewer({
  serverKey,
  attachmentId,
  onClose,
}: ImageViewerProps) {
  const insets = useSafeAreaInsets();
  const [controlsVisible, setControlsVisible] = useState(true);

  const handleClose = useCallback(() => {
    if (onClose) {
      onClose();
    } else {
      router.back();
    }
  }, [onClose]);

  const handleToggleControls = useCallback(() => {
    setControlsVisible((prev) => !prev);
  }, []);

  const { gesture, imageAnimatedStyle, backdropAnimatedStyle } = useZoomPan({
    onClose: handleClose,
    onToggleControls: handleToggleControls,
  });

  const original = useAttachmentFile(serverKey, attachmentId, AttachmentVariant.Original);
  const thumbnail = useAttachmentFile(serverKey, attachmentId, AttachmentVariant.Thumbnail);

  const filePath = original.path ?? thumbnail.path;

  const handleShare = useCallback(() => {
    if (!filePath) return;
    report(Sharing.shareAsync(filePath), 'share image');
  }, [filePath]);

  const controlsAnimatedStyle = useAnimatedStyle(() => ({
    opacity: withTiming(controlsVisible ? 1 : 0, { duration: 150 }),
  }));

  return (
    <ThemeContext.Provider value={{ colors: tokens.dark, scheme: 'dark' }}>
      <Animated.View
        style={[styles.container, { backgroundColor: onMedia.chat }, backdropAnimatedStyle]}
        testID="image-viewer-container"
      >
        <GestureDetector gesture={gesture}>
          <Animated.View style={styles.imageWrapper}>
            {filePath ? (
              <Animated.View style={[styles.imageContainer, imageAnimatedStyle]}>
                <Image
                  source={{ uri: filePath }}
                  style={styles.image}
                  contentFit="contain"
                  accessibilityLabel="Full size image"
                  testID="viewer-image"
                />
              </Animated.View>
            ) : (
              <View style={styles.spinnerSlot}>
                <ActivityIndicator size="large" color={onMedia.accent} />
              </View>
            )}
          </Animated.View>
        </GestureDetector>

        <Animated.View
          pointerEvents={controlsVisible ? 'box-none' : 'none'}
          style={[styles.topBar, { paddingTop: Math.max(insets.top, space.sm) }, controlsAnimatedStyle]}
        >
          <IconButton icon="close" label="Close" onPress={handleClose} size={44} tone="plain" />
          <IconButton
            icon="share"
            label="Share"
            onPress={handleShare}
            size={44}
            tone="plain"
            disabled={!filePath}
          />
        </Animated.View>
      </Animated.View>
    </ThemeContext.Provider>
  );
});

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  imageWrapper: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  imageContainer: {
    width: '100%',
    height: '100%',
  },
  image: {
    width: '100%',
    height: '100%',
  },
  spinnerSlot: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  topBar: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    flexDirection: 'row',
    justifyContent: 'space-between',
    paddingHorizontal: space.md,
    paddingBottom: space.sm,
  },
});
