import { memo, useCallback } from 'react';
import { Pressable, StyleSheet, View } from 'react-native';
import { Image } from 'expo-image';
import { router } from 'expo-router';
import { AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';

import { useTheme } from '@/ui';

import { fitSize } from './fitSize';
import { useAttachmentFile } from './useAttachmentFile';

export interface ImageAttachmentProps {
  serverKey: string;
  attachment: AttachmentRecord;
}

export const ImageAttachment = memo(function ImageAttachment({
  serverKey,
  attachment,
}: ImageAttachmentProps) {
  const { colors } = useTheme();
  const dims = fitSize(
    attachment.width ?? attachment.thumbnailWidth ?? 0,
    attachment.height ?? attachment.thumbnailHeight ?? 0,
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
    <Pressable
      testID={`attachment-pressable-${attachment.id}`}
      onPress={handlePress}
      accessibilityRole="imagebutton"
      accessibilityLabel={attachment.filename}
      style={styles.pressable}
    >
      <Image
        testID={`attachment-image-${attachment.id}`}
        source={{ uri: path }}
        style={[styles.image, { width: dims.width, height: dims.height }]}
        contentFit="cover"
        accessibilityLabel={attachment.filename}
      />
    </Pressable>
  );
});

const styles = StyleSheet.create({
  pressable: {
    borderRadius: 12,
    overflow: 'hidden',
  },
  placeholder: {
    borderRadius: 12,
  },
  image: {
    borderRadius: 12,
  },
});
