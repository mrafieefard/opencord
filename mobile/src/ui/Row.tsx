// A pressable row in a list or pane: Gesture Handler's Touchable, whose
// native underlay waits until the system knows it's a press, not a scroll
// (gestures skill, tap-handling).
import type { ReactNode } from 'react';
import { StyleSheet, View, type AccessibilityRole, type StyleProp, type ViewStyle } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { radius, TOUCH } from './theme/space';
import { useTheme } from './theme/useTheme';

export interface RowProps {
  onPress?: () => void;
  onLongPress?: () => void;
  height?: number;
  selected?: boolean;
  disabled?: boolean;
  /** For a row that is one of a set (a checkmark row): told to screen readers, not drawn. */
  checked?: boolean;
  accessibilityLabel: string;
  accessibilityRole?: AccessibilityRole;
  style?: StyleProp<ViewStyle>;
  children: ReactNode;
}

export function Row({
  onPress,
  onLongPress,
  height = TOUCH,
  selected = false,
  disabled = false,
  checked,
  accessibilityLabel,
  accessibilityRole = 'button',
  style,
  children,
}: RowProps) {
  const { colors } = useTheme();
  return (
    <Touchable
      onPress={disabled ? undefined : onPress}
      onLongPress={disabled ? undefined : onLongPress}
      disabled={disabled}
      underlayColor={colors.selected}
      activeUnderlayOpacity={1}
      style={[styles.row, { minHeight: height }, selected && { backgroundColor: colors.selected, borderRadius: radius.row }, style]}
    >
      <View
        accessible
        accessibilityRole={accessibilityRole}
        accessibilityLabel={accessibilityLabel}
        accessibilityState={{
          selected,
          ...(checked !== undefined ? { checked } : {}),
          ...(disabled ? { disabled } : {}),
        }}
        style={styles.content}
      >
        {children}
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  row: { justifyContent: 'center' },
  content: { flex: 1, flexDirection: 'row', alignItems: 'center' },
});
