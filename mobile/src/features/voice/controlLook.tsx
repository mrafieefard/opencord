// How the call's controls look on Android (ControlBar.ios.tsx draws iOS's
// own, in SwiftUI glass): one raised bar, with round buttons in it that fill
// with a colour when on (accent) or for leaving (recording).
import type { ReactNode } from 'react';
import { StyleSheet, View } from 'react-native';

import { GlassSurface, space, useTheme } from '@/ui';

export const BUTTON_SIZE = 56;

/** On, the colour a control takes; leave, the recording colour. */
export type ControlTone = 'plain' | 'on' | 'leave';

export function ControlFrame({ children }: { children: ReactNode }) {
  return (
    <GlassSurface style={styles.bar} testID="control-bar">
      {children}
    </GlassSurface>
  );
}

export function ControlCircle({ tone, children }: { tone: ControlTone; children: ReactNode }) {
  const { colors } = useTheme();
  const fill = tone === 'on' ? colors.accent : tone === 'leave' ? colors.recording : colors.surface;
  return <View style={[styles.button, { backgroundColor: fill }]}>{children}</View>;
}

const styles = StyleSheet.create({
  bar: {
    flexDirection: 'row',
    justifyContent: 'space-evenly',
    alignItems: 'center',
    paddingVertical: space.sm,
    marginHorizontal: space.md,
    marginBottom: space.sm,
    borderRadius: BUTTON_SIZE / 2 + space.sm,
  },
  button: {
    width: BUTTON_SIZE,
    height: BUTTON_SIZE,
    borderRadius: BUTTON_SIZE / 2,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
