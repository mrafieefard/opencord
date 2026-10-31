// The tablet's content pane with no channel open (spec §7.2).
import { StyleSheet, View } from 'react-native';

import { EmptyState, useTheme } from '@/ui';

export function NoChannel() {
  const { colors } = useTheme();
  return (
    <View style={[styles.pane, { backgroundColor: colors.chat }]}>
      <EmptyState title="Choose a channel" />
    </View>
  );
}

const styles = StyleSheet.create({
  pane: { flex: 1 },
});
