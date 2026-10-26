// A full-width action: 52 pt, 14 pt corners.
import { ActivityIndicator, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Icon } from './Icon';
import type { IconName } from './icons';
import { Text } from './Text';
import { radius, space } from './theme/space';
import type { ColorToken } from './theme/tokens';
import { useTheme } from './theme/useTheme';

export interface ButtonProps {
  title: string;
  onPress: () => void;
  variant: 'primary' | 'secondary' | 'voice' | 'danger';
  loading?: boolean;
  disabled?: boolean;
  icon?: IconName;
}

export function Button({ title, onPress, variant, loading = false, disabled = false, icon }: ButtonProps) {
  const { colors } = useTheme();
  const look: { background: string; text: ColorToken; border?: string } =
    variant === 'primary'
      ? { background: colors.accent, text: 'onAccent' }
      : variant === 'voice'
        ? { background: colors.voice, text: 'onAccent' }
        : variant === 'danger'
          ? { background: colors.recording, text: 'onAccent' }
          : { background: colors.surface, text: 'text', border: colors.border };
  const off = disabled || loading;
  return (
    <Touchable
      onPress={onPress}
      disabled={off}
      underlayColor={colors.text}
      activeUnderlayOpacity={0.12}
      style={[
        styles.button,
        { backgroundColor: look.background, opacity: disabled ? 0.45 : 1 },
        look.border !== undefined && { borderWidth: StyleSheet.hairlineWidth, borderColor: look.border },
      ]}
    >
      <View accessible accessibilityRole="button" accessibilityLabel={title} accessibilityState={{ disabled: off, busy: loading }} style={styles.content}>
        {loading ? (
          <ActivityIndicator color={colors[look.text]} />
        ) : (
          <>
            {icon && <Icon name={icon} size={18} color={look.text} />}
            <Text variant="headline" color={look.text}>
              {title}
            </Text>
          </>
        )}
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  button: { height: 52, borderRadius: radius.icon, overflow: 'hidden', justifyContent: 'center' },
  content: { flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: space.sm, paddingHorizontal: space.lg },
});
