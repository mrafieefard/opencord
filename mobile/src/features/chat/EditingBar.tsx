// Above the field while a message is being edited (spec §8.5).
import { StyleSheet, View } from 'react-native';

import { Icon, IconButton, Text, space } from '@/ui';

export interface EditingBarProps {
  onCancel: () => void;
}

export function EditingBar({ onCancel }: EditingBarProps) {
  return (
    <View style={styles.container}>
      <Icon name="edit" size={18} color="textSecondary" />
      <Text variant="subhead" style={styles.label}>
        Editing message
      </Text>
      <IconButton icon="close" label="Cancel editing" size={36} onPress={onCancel} />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    paddingLeft: space.lg,
    paddingRight: space.xs,
    paddingBottom: space.xs,
  },
  label: {
    flex: 1,
  },
});
