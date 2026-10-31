import { memo, useCallback } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import { Image } from 'expo-image';
import { router } from 'expo-router';
import { AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';

import { motion, radius, useMenuOpener, useTheme } from '@/ui';

import { fitSize } from './fitSize';
import { useAttachmentFile } from './useAttachmentFile';

export interface ImageAttachmentProps {
  serverKey: string;
  attachment: AttachmentRecord;
  /** The room the bubble has for it. */
  maxWidth: number;
}

const MAX_W = 260;
const MAX_H = 320;

export const ImageAttachment = memo(function ImageAttachment({
  serverKey,
  attachment,
  maxWidth,
}: ImageAttachmentProps) {
  const { colors } = useTheme();
  const openMenu = useMenuOpener();
  const dims = fitSize(
    attachment.width ?? attachment.thumbnailWidth ?? 0,
    attachment.height ?? attachment.thumbnailHeight ?? 0,
    Math.min(MAX_W, maxWidth),
    MAX_H,
  );

  const { path } = useAttachmentFile(serverKey, attachment.id, AttachmentVariant.Thumbnail);

  const handlePress = useCallback(() => {
    router.push({
      pathname: '/viewer',
      params: { serverKey, attachmentId: attachment.id },
    });
  }, [serverKey, attachment.id]);

  if (!path) {
    return (
      <View
        testID={`attachment-placeholder-${attachment.id}`}
        style={[
          styles.placeholder,
          {
            width: dims.width,
            height: dims.height,
            backgroundColor: colors.surface,
          },
        ]}
      />
    );
  }

  return (
    <Touchable testID={`attachment-pressable-${attachment.id}`} onPress={handlePress} onLongPress={openMenu} style={styles.pressable}>
      <Image
        testID={`attachment-image-${attachment.id}`}
        source={{ uri: path }}
        style={[styles.image, { width: dims.width, height: dims.height, backgroundColor: colors.surface }]}
        contentFit="cover"
        transition={motion.message}
        accessible
        accessibilityRole="imagebutton"
        accessibilityLabel={attachment.filename}
      />
    </Touchable>
  );
});

const styles = StyleSheet.create({
  pressable: {
    borderRadius: radius.image,
    overflow: 'hidden',
  },
  placeholder: {
    borderRadius: radius.image,
  },
  image: {
    borderRadius: radius.image,
  },
});
