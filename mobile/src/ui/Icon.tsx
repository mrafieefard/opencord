import { SymbolView } from 'expo-symbols';

import { icons, type IconName } from './icons';
import type { ColorToken } from './theme/tokens';
import { useTheme } from './theme/useTheme';

export interface IconProps {
  name: IconName;
  size?: number;
  color?: ColorToken;
}

export function Icon({ name, size = 20, color = 'text' }: IconProps) {
  const { colors } = useTheme();
  return <SymbolView name={icons[name]} size={size} tintColor={colors[color]} resizeMode="scaleAspectFit" />;
}
