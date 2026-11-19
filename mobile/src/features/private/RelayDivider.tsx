// Where a private chat started or moved (design §6.3, the desktop's
// relay_divider.dart): a dashed line each side of the server's name. The
// name opens the server when this device has it.
import { router } from 'expo-router';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { useAppStore } from '@/state/store';
import { Text, space, useTheme } from '@/ui';

export interface RelayDividerProps {
  serverName: string;
  serverKey?: string;
}

export function RelayDivider({ serverName, serverKey }: RelayDividerProps) {
  const { colors } = useTheme();
  const saved = useAppStore((state) => serverKey !== undefined && state.servers[serverKey] !== undefined);
  const line = <View style={[styles.line, { borderColor: colors.selected }]} />;
  const label = (
    <Text variant="footnote" color="textSecondary" numberOfLines={1}>
      {serverName}
    </Text>
  );
  return (
    <View style={styles.row} testID="relay-divider">
      {line}
      {saved && serverKey !== undefined ? (
        <Touchable onPress={() => router.push(`/servers/${serverKey}`)} style={styles.label}>
          <View accessible accessibilityRole="button" accessibilityLabel={`Open ${serverName}`}>
            {label}
          </View>
        </Touchable>
      ) : (
        <View style={styles.label}>{label}</View>
      )}
      {line}
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
    paddingHorizontal: space.lg,
    paddingTop: space.xs,
    paddingBottom: space.sm,
  },
  line: { flex: 1, borderTopWidth: 1, borderStyle: 'dashed' },
  label: { flexShrink: 1, paddingVertical: space.xs },
});
