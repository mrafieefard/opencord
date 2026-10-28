import { Pressable, ScrollView, StyleSheet, View } from 'react-native';

import type { PickedFile } from '@/state/types';
import { Icon, Text, space, useTheme } from '@/ui';

export interface AttachmentChipsProps {
  files: PickedFile[];
  onRemove?: (index: number) => void;
  progress?: { sentBytes: number; totalBytes: number };
}

export function AttachmentChips({ files, onRemove, progress }: AttachmentChipsProps) {
  const { colors } = useTheme();

  if (files.length === 0) return null;

  const progressPercent =
    progress && progress.totalBytes > 0
      ? Math.min(100, Math.round((progress.sentBytes / progress.totalBytes) * 100))
      : null;

  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={styles.container}
    >
      {files.map((file, index) => (
        <View
          key={`${file.path}-${index}`}
          style={[styles.chip, { backgroundColor: colors.surface }]}
          testID="attachment-chip"
        >
          <Icon name="file" size={14} color="textSecondary" />
          <Text variant="footnote" color="text" numberOfLines={1} style={styles.filename}>
            {file.filename}
          </Text>
          {progressPercent !== null && (
            <Text variant="footnote" color="textMuted">
              {progressPercent}%
            </Text>
          )}
          {onRemove && (
            <Pressable
              onPress={() => onRemove(index)}
              hitSlop={8}
              accessibilityLabel={`Remove ${file.filename}`}
              accessibilityRole="button"
              style={styles.removeBtn}
            >
              <Icon name="close" size={12} color="textMuted" />
            </Pressable>
          )}
        </View>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    gap: space.xs,
    paddingHorizontal: space.sm,
    paddingTop: space.xs,
    paddingBottom: space.xs,
  },
  chip: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
    borderRadius: 16,
    paddingHorizontal: space.sm,
    paddingVertical: 4,
    maxWidth: 200,
  },
  filename: {
    maxWidth: 130,
  },
  removeBtn: {
    padding: 2,
    marginLeft: 2,
  },
});
