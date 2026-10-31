// A floating surface on Android (GlassSurface.ios.tsx is Liquid Glass): the
// raised surface Material gives a bar that floats over content.
import { View } from 'react-native';

import type { GlassSurfaceProps } from './GlassSurface.types';
import { useTheme } from './theme/useTheme';

export type { GlassSurfaceProps } from './GlassSurface.types';

export function GlassSurface({ style, testID, children, ref }: GlassSurfaceProps) {
  const { colors } = useTheme();
  return (
    <View ref={ref} style={[{ backgroundColor: colors.elevated, elevation: 3 }, style]} testID={testID}>
      {children}
    </View>
  );
}
