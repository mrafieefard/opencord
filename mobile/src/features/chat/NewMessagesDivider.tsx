import { memo } from 'react';
import { StyleSheet, View } from 'react-native';

import { Text, space, useTheme } from '@/ui';

export const NewMessagesDivider = memo(function NewMessagesDivider() {
  const { colors } = useTheme();

  return (
    <View style={styles.container} testID="new-messages-divider">
      <View style={[styles.line, { backgroundColor: colors.border }]} />
      <Text variant="footnote" color="textSecondary" style={styles.label}>
        New messages
      </Text>
      <View style={[styles.line, { backgroundColor: colors.border }]} />
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    alignItems: 'center',
    marginVertical: space.sm,
    paddingHorizontal: space.md,
  },
  line: {
    flex: 1,
    height: StyleSheet.hairlineWidth,
  },
  label: {
    marginHorizontal: space.sm,
  },
});
