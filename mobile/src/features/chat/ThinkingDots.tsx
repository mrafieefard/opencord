// Three dots fading in turn while a command is thinking (design §26.6).
// With reduced motion they stand still.
import { StyleSheet, View } from 'react-native';
import Animated, { useReducedMotion } from 'react-native-reanimated';

import { useTheme } from '@/ui';

const FADE = {
  '0%': { opacity: 0.25 },
  '30%': { opacity: 1 },
  '70%': { opacity: 0.25 },
  '100%': { opacity: 0.25 },
};

const DOTS = [0, 180, 360];

export function ThinkingDots() {
  const { colors } = useTheme();
  const reduced = useReducedMotion();
  return (
    <View style={styles.row} testID="thinking-dots">
      {DOTS.map((delay) => (
        <Animated.View
          key={delay}
          style={[
            styles.dot,
            { backgroundColor: colors.textMuted },
            reduced
              ? styles.still
              : {
                  animationName: FADE,
                  animationDuration: '1100ms',
                  animationDelay: `${delay}ms`,
                  animationIterationCount: 'infinite',
                  animationTimingFunction: 'ease-in-out',
                },
          ]}
        />
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  row: { flexDirection: 'row', alignItems: 'center', gap: 3, marginLeft: 6 },
  dot: { width: 5, height: 5, borderRadius: 2.5 },
  still: { opacity: 0.6 },
});
