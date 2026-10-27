import { memo } from 'react';
import { StyleSheet, View } from 'react-native';

import { Text, space, useTheme } from '@/ui';

export const NewMessagesDivider = memo(function NewMessagesDivider() {
  const { colors } = useTheme();

  return (
    <View style={styles.container} testID="new-messages-divider">
      <View style={[styles.line, { backgroundColor: colors.recording }]} />
      <View style={[styles.pill, { backgroundColor: colors.recording }]}>
        <Text variant="caption" color="onAccent" style={styles.text}>
          NEW
        </Text>
      </View>
      <View style={[styles.line, { backgroundColor: colors.recording }]} />
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
    height: 1,
  },
  pill: {
    paddingHorizontal: space.sm,
    paddingVertical: 2,
    borderRadius: 8,
    marginHorizontal: space.xs,
  },
  text: {
    fontWeight: '700',
    fontSize: 10,
    letterSpacing: 0.5,
  },
});
