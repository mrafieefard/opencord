// A thin bar, 0–1, that grows with a CSS transition on its scale (no
// layout animation).
import { StyleSheet, View } from 'react-native';
import Animated from 'react-native-reanimated';

import { useTheme } from './theme/useTheme';

export function ProgressBar({ progress, tone = 'neutral' }: { progress: number; tone?: 'neutral' | 'error' }) {
  const { colors } = useTheme();
  const clamped = Math.min(Math.max(progress, 0), 1);
  return (
    <View
      accessibilityRole="progressbar"
      accessibilityValue={{ min: 0, max: 100, now: Math.round(clamped * 100) }}
      style={[styles.track, { backgroundColor: colors.selected }]}
    >
      <Animated.View
        style={[
          styles.fill,
          {
            backgroundColor: tone === 'error' ? colors.recording : colors.text,
            transform: [{ scaleX: clamped }],
            transformOrigin: 'left',
            transitionProperty: 'transform',
            transitionDuration: 150,
          },
        ]}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  track: { height: 3, borderRadius: 1.5, overflow: 'hidden' },
  fill: { flex: 1 },
});
