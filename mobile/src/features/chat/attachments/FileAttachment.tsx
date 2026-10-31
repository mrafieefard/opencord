import { memo, useCallback, useRef, useState } from 'react';
import { ActivityIndicator, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import * as Sharing from 'expo-sharing';
import { AttachmentKind, AttachmentVariant, type AttachmentRecord } from '@opencord/core-native';

import { report } from '@/core';
import { Icon, type IconName, Text, space, useMenuOpener, useTheme } from '@/ui';

import { fetchAttachmentFile } from './useAttachmentFile';

export interface FileAttachmentProps {
  serverKey: string;
  attachment: AttachmentRecord;
  /** The room the bubble has for it. */
  maxWidth: number;
}

/** Cards are as wide as images can be (spec §8.5), or the bubble's room. */
const CARD_MAX = 260;
const ICON_CIRCLE = 40;

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

export const FileAttachment = memo(function FileAttachment({ serverKey, attachment, maxWidth }: FileAttachmentProps) {
  const { colors } = useTheme();
  const openMenu = useMenuOpener();
  const [downloading, setDownloading] = useState(false);
  const inProgress = useRef(false);

  // One download at a time per card; the share sheet opens on the file.
  const open = useCallback(() => {
    if (inProgress.current) return;
    inProgress.current = true;
    setDownloading(true);
    const done = fetchAttachmentFile(serverKey, attachment.id, AttachmentVariant.Original)
      .then((uri) => Sharing.shareAsync(uri))
      .finally(() => {
        inProgress.current = false;
        setDownloading(false);
      });
    report(done, 'open file');
  }, [serverKey, attachment.id]);

  const sizeText = formatFileSize(attachment.sizeBytes);
  const kindText = formatAttachmentKind(attachment.kind, attachment.durationMs);

  return (
    <Touchable
      testID={`file-attachment-${attachment.id}`}
      onPress={open}
      onLongPress={openMenu}
      style={[styles.card, { width: Math.min(CARD_MAX, maxWidth) }]}
    >
      <View
        accessible
        accessibilityRole="button"
        accessibilityLabel={`${attachment.filename}, ${sizeText}, ${kindText}`}
        style={styles.content}
      >
        <View style={[styles.iconCircle, { backgroundColor: colors.mentionBg }]}>
          {downloading ? (
            <ActivityIndicator size="small" color={colors.text} />
          ) : (
            <Icon name={attachmentIcon(attachment.kind)} size={20} color="text" />
          )}
        </View>
        <View style={styles.info}>
          <Text variant="subhead" numberOfLines={1} ellipsizeMode="middle">
            {attachment.filename}
          </Text>
          <Text variant="footnote" color="textMuted" numberOfLines={1} tabular>
            {sizeText} · {kindText}
          </Text>
        </View>
      </View>
    </Touchable>
  );
});

const styles = StyleSheet.create({
  card: {
    paddingVertical: space.xs,
  },
  content: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
  },
  iconCircle: {
    width: ICON_CIRCLE,
    height: ICON_CIRCLE,
    borderRadius: ICON_CIRCLE / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
  info: {
    flex: 1,
    gap: 2,
  },
});
