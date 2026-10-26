// All of the app's text: a variant of the type scale, a color token.
import { Text as NativeText, type StyleProp, type TextProps as NativeTextProps, type TextStyle } from 'react-native';

import { MAX_FONT_SCALE, typography, type TextVariant } from './theme/typography';
import type { ColorToken } from './theme/tokens';
import { useTheme } from './theme/useTheme';

export interface TextProps extends Pick<NativeTextProps, 'numberOfLines' | 'selectable' | 'ellipsizeMode' | 'onTextLayout' | 'accessibilityRole'> {
  variant: TextVariant;
  color?: ColorToken;
  /** Times and counts that change in place. */
  tabular?: boolean;
  style?: StyleProp<TextStyle>;
  children?: React.ReactNode;
}

export function Text({ variant, color = 'text', tabular = false, style, children, ...rest }: TextProps) {
  const { colors } = useTheme();
  return (
    <NativeText
      maxFontSizeMultiplier={MAX_FONT_SCALE}
      style={[typography[variant], { color: colors[color] }, tabular && { fontVariant: ['tabular-nums'] }, style]}
      {...rest}
    >
      {children}
    </NativeText>
  );
}
