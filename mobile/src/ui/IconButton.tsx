// A round button with one icon. Toggles are `surface` when off and
// `accent` when on (design §8.6).
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Icon } from './Icon';
import type { IconName } from './icons';
import type { ColorToken } from './theme/tokens';
import { useTheme } from './theme/useTheme';

export type IconButtonTone = 'plain' | 'toggle' | 'accent' | 'danger' | 'voice';

export interface IconButtonProps {
  icon: IconName;
  label: string;
  onPress: () => void;
  size?: 36 | 44 | 56 | 64;
  tone?: IconButtonTone;
  active?: boolean;
  disabled?: boolean;
}

export function IconButton({ icon, label, onPress, size = 44, tone = 'plain', active = false, disabled = false }: IconButtonProps) {
  const { colors } = useTheme();
  let background = 'transparent';
  let foreground: ColorToken = 'text';
  if (tone === 'toggle') {
    background = active ? colors.accent : colors.surface;
    foreground = active ? 'onAccent' : 'text';
  } else if (tone === 'accent') {
    background = colors.accent;
    foreground = 'onAccent';
  } else if (tone === 'danger') {
    background = colors.recording;
    foreground = 'onAccent';
  } else if (tone === 'voice') {
    background = colors.voice;
    foreground = 'onAccent';
  }
  return (
    <Touchable
      onPress={onPress}
      disabled={disabled}
      underlayColor={colors.selected}
      activeUnderlayOpacity={tone === 'plain' ? 1 : 0.3}
      style={[styles.button, { width: size, height: size, borderRadius: size / 2, backgroundColor: background, opacity: disabled ? 0.4 : 1 }]}
    >
      <View accessible accessibilityRole="button" accessibilityLabel={label} accessibilityState={{ disabled, selected: active }} style={styles.center}>
        <Icon name={icon} size={Math.round(size * 0.45)} color={tone === 'danger' || tone === 'voice' ? 'onAccent' : foreground} />
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  button: { alignItems: 'center', justifyContent: 'center', overflow: 'hidden' },
  center: { flex: 1, alignSelf: 'stretch', alignItems: 'center', justifyContent: 'center' },
});
