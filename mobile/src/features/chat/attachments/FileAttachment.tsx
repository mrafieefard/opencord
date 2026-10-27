import { memo, useCallback, useRef, useState } from 'react';
import { ActivityIndicator, Pressable, StyleSheet, View } from 'react-native';
import * as Sharing from 'expo-sharing';
import { AttachmentKind, AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';

import { Icon, type IconName, Text, space, useTheme } from '@/ui';

import { fetchAttachmentFile } from './useAttachmentFile';

export interface FileAttachmentProps {
  serverKey: string;
  attachment: AttachmentRecord;
}

export function formatDurationMs(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = (totalSeconds % 60).toString().padStart(2, '0');
  return `${minutes}:${seconds}`;
}

export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) {
    const fixed = kb >= 100 ? Math.round(kb).toString() : kb.toFixed(1);
    return `${fixed} KB`;
  }
  const mb = kb / 1024;
  if (mb < 1024) {
    const fixed = mb >= 100 ? Math.round(mb).toString() : mb.toFixed(1);
    return `${fixed} MB`;
  }
  const gb = mb / 1024;
  const fixed = gb >= 100 ? Math.round(gb).toString() : gb.toFixed(1);
  return `${fixed} GB`;
}

export function formatAttachmentKind(kind: AttachmentKind, durationMs?: number): string {
  switch (kind) {
    case AttachmentKind.Video:
      return durationMs !== undefined && durationMs > 0 ? `Video, ${formatDurationMs(durationMs)}` : 'Video';
    case AttachmentKind.VoiceMessage:
      return durationMs !== undefined && durationMs > 0
        ? `Voice message, ${formatDurationMs(durationMs)}`
        : 'Voice message';
    case AttachmentKind.Audio:
      return durationMs !== undefined && durationMs > 0 ? `Audio, ${formatDurationMs(durationMs)}` : 'Audio';
    case AttachmentKind.Image:
      return 'Image';
    default:
      return 'File';
  }
}

export function attachmentIcon(kind: AttachmentKind): IconName {
  switch (kind) {
    case AttachmentKind.VoiceMessage:
      return 'mic';
    case AttachmentKind.Audio:
      return 'music';
    case AttachmentKind.Video:
      return 'play';
    default:
      return 'file';
  }
}

export const FileAttachment = memo(function FileAttachment({
  serverKey,
  attachment,
}: FileAttachmentProps) {
  const { colors } = useTheme();
  const [downloading, setDownloading] = useState(false);
  const inProgress = useRef(false);

  const handlePress = useCallback(async () => {
    if (inProgress.current) return;
    inProgress.current = true;
    setDownloading(true);
    try {
      const uri = await fetchAttachmentFile(serverKey, attachment.id, AttachmentVariant.Original);
      await Sharing.shareAsync(uri);
    } catch (err: unknown) {
      console.warn(`download file ${attachment.id} failed:`, err);
    } finally {
      inProgress.current = false;
      setDownloading(false);
    }
  }, [serverKey, attachment.id]);

  const iconName = attachmentIcon(attachment.kind);
  const sizeText = formatFileSize(attachment.sizeBytes);
  const kindText = formatAttachmentKind(attachment.kind, attachment.durationMs);

  return (
    <Pressable
      testID={`file-attachment-${attachment.id}`}
      onPress={handlePress}
      accessibilityRole="button"
      accessibilityLabel={`${attachment.filename}, ${sizeText}, ${kindText}`}
      style={[
        styles.card,
        {
          backgroundColor: colors.surface,
          borderColor: colors.border,
        },
      ]}
    >
      <View style={styles.iconSlot}>
        {downloading ? (
          <ActivityIndicator size="small" color={colors.accent} />
        ) : (
          <Icon name={iconName} size={22} color="textMuted" />
        )}
      </View>
      <View style={styles.info}>
        <Text variant="subhead" numberOfLines={1} ellipsizeMode="middle" style={styles.filename}>
          {attachment.filename}
        </Text>
        <Text variant="footnote" color="textMuted" numberOfLines={1}>
          {sizeText} · {kindText}
        </Text>
      </View>
    </Pressable>
  );
});

const styles = StyleSheet.create({
  card: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: space.sm,
    paddingVertical: space.sm,
    borderRadius: 12,
    borderWidth: StyleSheet.hairlineWidth,
    maxWidth: 260,
    minWidth: 180,
  },
  iconSlot: {
    width: 28,
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: space.xs,
  },
  info: {
    flex: 1,
    justifyContent: 'center',
  },
  filename: {
    fontWeight: '600',
    marginBottom: 2,
  },
});
