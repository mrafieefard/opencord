import { Pressable, StyleSheet, View } from 'react-native';

import { Icon, Text, space, useTheme } from '@/ui';

export interface EditingBarProps {
  onCancel: () => void;
}

export function EditingBar({ onCancel }: EditingBarProps) {
  const { colors } = useTheme();

  return (
    <View style={[styles.container, { backgroundColor: colors.surface }]}>
      <View style={styles.left}>
        <Icon name="edit" size={16} color="accent" />
        <Text variant="subhead" color="text" style={styles.label}>
          Editing message
        </Text>
      </View>
      <Pressable
        onPress={onCancel}
        hitSlop={8}
        accessibilityLabel="Cancel editing"
        accessibilityRole="button"
        style={styles.cancelBtn}
      >
        <Icon name="close" size={16} color="textSecondary" />
      </Pressable>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    paddingHorizontal: space.sm,
    paddingVertical: space.xs,
    borderTopLeftRadius: 12,
    borderTopRightRadius: 12,
  },
  left: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
  },
  label: {
    fontWeight: '600',
  },
  cancelBtn: {
    padding: space.xs,
  },
});
