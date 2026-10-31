import { MaterialSymbols_400Regular_Filled } from '@expo-google-fonts/material-symbols/400Regular_Filled';
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

/**
 * Material Symbols' filled face. Android draws a symbol filled where iOS's
 * is a `.fill` one (play, pause, the mic), so the two match; expo-symbols'
 * own default is the outlined face.
 */
const FILLED = { ios: 'unspecified', android: { name: 'MaterialSymbols_400Regular_Filled', font: MaterialSymbols_400Regular_Filled } } as const;

export function Icon({ name, size = 20, color = 'text', tint, testID }: IconProps) {
  const { colors } = useTheme();
  const symbol = icons[name];
  return (
    <SymbolView
      testID={testID}
      name={symbol}
      size={size}
      weight={symbol.ios.endsWith('.fill') ? FILLED : undefined}
      tintColor={tint ?? colors[color]}
      resizeMode="scaleAspectFit"
    />
  );
}
