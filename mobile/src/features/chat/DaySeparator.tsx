import { memo } from 'react';
import { StyleSheet, View } from 'react-native';

import { deviceLocale } from '@/features/servers/formatTime';
import { Text, space, useTheme, radius } from '@/ui';

export function formatDay(date: Date, nowMs = Date.now(), locale = deviceLocale()): string {
  const atMidnight = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const now = new Date(nowMs);
  const nowMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate());

  const diffDays = Math.round((nowMidnight.getTime() - atMidnight.getTime()) / (24 * 60 * 60 * 1000));

  if (diffDays <= 0) return 'Today';
  if (diffDays === 1) return 'Yesterday';

  return new Intl.DateTimeFormat(locale, {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
  }).format(date);
}

export interface DaySeparatorProps {
  date: Date;
}

export const DaySeparator = memo(function DaySeparator({ date }: DaySeparatorProps) {
  const { colors } = useTheme();
  const label = formatDay(date);

  return (
    <View style={styles.container} testID="day-separator">
      <View style={[styles.pill, { backgroundColor: colors.surface }]}>
        <Text variant="footnote" color="textSecondary">
          {label}
        </Text>
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    alignItems: 'center',
    marginVertical: space.md,
  },
  pill: {
    paddingHorizontal: space.md,
    paddingVertical: 3,
    borderRadius: radius.icon,
  },
});
