// A floating surface on iOS: Liquid Glass, as Telegram's and Apple's own
// bars over content, in the app's scheme.
import { GlassView } from 'expo-glass-effect';

import type { GlassSurfaceProps } from './GlassSurface.types';
import { useTheme } from './theme/useTheme';

export type { GlassSurfaceProps } from './GlassSurface.types';

export function GlassSurface({ style, interactive = false, testID, children }: GlassSurfaceProps) {
  const { scheme } = useTheme();
  return (
    <GlassView glassEffectStyle="regular" isInteractive={interactive} colorScheme={scheme} style={style} testID={testID}>
      {children}
    </GlassView>
  );
}
