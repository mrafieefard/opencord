// A full-width action: 52 pt, 14 pt corners.
import { ActivityIndicator, StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Icon } from './Icon';
import type { ButtonProps } from './Button.types';
import { Text } from './Text';
import { radius, space } from './theme/space';
import { onHue, type ColorToken } from './theme/tokens';
import { useTheme } from './theme/useTheme';

export type { ButtonProps } from './Button.types';

export function Button({ title, onPress, variant, loading = false, disabled = false, icon }: ButtonProps) {
  const { colors } = useTheme();
  // On the hues (voice, danger) the text is white in both themes.
  const look: { background: string; text: ColorToken; tint?: string; border?: string } =
    variant === 'primary'
      ? { background: colors.accent, text: 'onAccent' }
      : variant === 'voice'
        ? { background: colors.voice, text: 'onAccent', tint: onHue }
        : variant === 'danger'
          ? { background: colors.recording, text: 'onAccent', tint: onHue }
          : variant === 'plain'
            ? { background: 'transparent', text: 'text' }
            : { background: colors.surface, text: 'text', border: colors.border };
  const off = disabled || loading;
  // Disabled is flat and quiet in every variant, not a faded color.
  const shown =
    disabled && variant !== 'plain'
      ? { background: colors.selected, text: 'textMuted' as const, tint: undefined, border: undefined }
      : disabled
        ? { ...look, text: 'textMuted' as const, tint: undefined }
        : look;
  const foreground = shown.tint ?? colors[shown.text];
  return (
    <Touchable
      onPress={onPress}
      disabled={off}
      underlayColor={colors.text}
      activeUnderlayOpacity={0.12}
      style={[
        styles.button,
        { backgroundColor: shown.background },
        shown.border !== undefined && { borderWidth: StyleSheet.hairlineWidth, borderColor: shown.border },
      ]}
    >
      <View accessible accessibilityRole="button" accessibilityLabel={title} accessibilityState={{ disabled: off, busy: loading }} style={styles.content}>
        {loading ? (
          <ActivityIndicator color={foreground} />
        ) : (
          <>
            {icon && <Icon name={icon} size={18} tint={foreground} />}
            <Text variant="headline" style={{ color: foreground }}>
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
