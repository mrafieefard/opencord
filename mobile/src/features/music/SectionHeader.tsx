// A list's heading in the player (spec §8.7): the name, a quiet detail
// under it, and one text action at the end. And the line a list shows
// when it has nothing.
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Text, TOUCH, space } from '@/ui';

import { GUTTER } from './TrackRow';

export interface SectionHeaderProps {
  title: string;
  detail?: string;
  action?: { title: string; label: string; onPress: () => void };
}

export function SectionHeader({ title, detail, action }: SectionHeaderProps) {
  return (
    <View style={styles.header}>
      <View style={styles.titles}>
        <Text variant="headline" accessibilityRole="header">
          {title}
        </Text>
        {detail !== undefined && (
          <Text variant="footnote" color="textMuted" tabular>
            {detail}
          </Text>
        )}
      </View>
      {action && (
        <Touchable
          onPress={action.onPress}
          accessible
          accessibilityRole="button"
          accessibilityLabel={action.label}
          activeOpacity={0.5}
          hitSlop={space.sm}
          style={styles.action}
        >
          <Text variant="callout" color="textSecondary">
            {action.title}
          </Text>
        </Touchable>
      )}
    </View>
  );
}

export function SectionNote({ title, body }: { title: string; body?: string }) {
  return (
    <View style={styles.note}>
      <Text variant="callout" color="textSecondary">
        {title}
      </Text>
      {body !== undefined && (
        <Text variant="footnote" color="textMuted">
          {body}
        </Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  header: {
    flexDirection: 'row',
    alignItems: 'flex-end',
    paddingHorizontal: GUTTER,
    paddingTop: space.xxl,
    paddingBottom: space.sm,
  },
  titles: { flex: 1, gap: 2 },
  action: { minHeight: TOUCH, justifyContent: 'flex-end', paddingBottom: 2 },
  note: { paddingHorizontal: GUTTER, paddingVertical: space.md, gap: 2 },
});
