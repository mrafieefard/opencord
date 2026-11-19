import { memo, useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, ScrollView, StyleSheet, View } from 'react-native';
import { File } from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import { router } from 'expo-router';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { AttachmentVariant } from '@opencord/core-native';

import { report } from '@/core';
import { IconButton, Text, space, useTheme } from '@/ui';
import { fetchAttachmentFile } from './useAttachmentFile';

export interface TextPreviewProps {
  serverKey: string;
  attachmentId: string;
  filename: string;
  onClose?: () => void;
}

export const TextPreview = memo(function TextPreview({
  serverKey,
  attachmentId,
  filename,
  onClose,
}: TextPreviewProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  const [loading, setLoading] = useState(true);
  const [content, setContent] = useState<string>('');
  const [filePath, setFilePath] = useState<string | null>(null);

  const handleClose = useCallback(() => {
    if (onClose) {
      onClose();
    } else {
      router.back();
    }
  }, [onClose]);

  const handleShare = useCallback(() => {
    if (!filePath) return;
    report(Sharing.shareAsync(filePath), 'share text file');
  }, [filePath]);

  useEffect(() => {
    let mounted = true;
    setLoading(true);

    fetchAttachmentFile(serverKey, attachmentId, AttachmentVariant.Original)
      .then(async (uri) => {
        if (!mounted) return;
        setFilePath(uri);
        const text = await new File(uri).text();
        if (!mounted) return;
        setContent(text);
      })
      .catch((error: unknown) => {
        if (!mounted) return;
        setContent(`Failed to load text preview: ${error instanceof Error ? error.message : String(error)}`);
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, [serverKey, attachmentId]);

  return (
    <View style={[styles.container, { backgroundColor: colors.chat }]}>
      {/* Header */}
      <View
        style={[
          styles.header,
          {
            paddingTop: Math.max(insets.top, space.sm),
            borderBottomColor: colors.border,
            backgroundColor: colors.elevated,
          },
        ]}
      >
        <IconButton icon="close" label="Close" onPress={handleClose} size={40} tone="plain" />
        <View style={styles.titleSlot}>
          <Text variant="subhead" numberOfLines={1} ellipsizeMode="middle">
            {filename}
          </Text>
        </View>
        <IconButton icon="share" label="Share" onPress={handleShare} size={40} tone="plain" />
      </View>

      {/* Content */}
      {loading ? (
        <View style={styles.center}>
          <ActivityIndicator size="large" color={colors.text} />
        </View>
      ) : (
        <ScrollView style={styles.scrollView} contentContainerStyle={styles.scrollContent}>
          <Text variant="body" style={[styles.monoText, { color: colors.text }]} selectable>
            {content}
          </Text>
        </ScrollView>
      )}
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: space.sm,
    paddingBottom: space.sm,
    borderBottomWidth: 1,
  },
  titleSlot: {
    flex: 1,
    alignItems: 'center',
    paddingHorizontal: space.sm,
  },
  center: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  scrollView: {
    flex: 1,
  },
  scrollContent: {
    padding: space.md,
  },
  monoText: {
    fontFamily: 'monospace',
    fontSize: 13,
    lineHeight: 18,
  },
});
