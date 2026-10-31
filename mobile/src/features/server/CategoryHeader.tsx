import { memo } from 'react';
import { StyleSheet, View } from 'react-native';

import { Text, space } from '@/ui';

export interface CategoryHeaderProps {
  name: string;
}

export const CategoryHeader = memo(function CategoryHeader({ name }: CategoryHeaderProps) {
  return (
    <View style={styles.header}>
      <Text variant="caption" color="textMuted">
        {name.toUpperCase()}
      </Text>
    </View>
  );
});

const styles = StyleSheet.create({
  header: {
    paddingHorizontal: space.lg,
    paddingTop: space.md,
    paddingBottom: space.xs,
  },
});
