import { StyleSheet, View } from 'react-native';
import { ScrollView, Touchable } from 'react-native-gesture-handler';

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
          style={[styles.chip, { backgroundColor: colors.hover }]}
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
            <Touchable onPress={() => onRemove(index)} hitSlop={12} style={styles.removeBtn}>
              <View accessible accessibilityRole="button" accessibilityLabel={`Remove ${file.filename}`}>
                <Icon name="close" size={14} color="textSecondary" />
              </View>
            </Touchable>
          )}
        </View>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    gap: space.sm,
    paddingHorizontal: space.md,
    paddingBottom: space.sm,
  },
  chip: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    height: 32,
    borderRadius: 16,
    paddingHorizontal: space.md,
    maxWidth: 220,
  },
  filename: {
    maxWidth: 130,
  },
  removeBtn: {
    padding: 2,
    marginLeft: 2,
  },
});
