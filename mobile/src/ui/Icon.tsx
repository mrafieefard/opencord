import { SymbolView } from 'expo-symbols';

import { icons, type IconName } from './icons';
import type { ColorToken } from './theme/tokens';
import { useTheme } from './theme/useTheme';

export interface IconProps {
  name: IconName;
  size?: number;
  color?: ColorToken;
  /** A color from tokens.ts that isn't the theme's (`onHue`, `onMedia`); wins over `color`. */
  tint?: string;
  testID?: string;
}

export function Icon({ name, size = 20, color = 'text', tint, testID }: IconProps) {
  const { colors } = useTheme();
  return <SymbolView testID={testID} name={icons[name]} size={size} tintColor={tint ?? colors[color]} resizeMode="scaleAspectFit" />;
}
