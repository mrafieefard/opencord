// A thin bar that thickens under the finger, as Apple Music's and Material
// 3's expressive sliders do: the player's position and the music volume.
// Drag or tap anywhere on it; a screen reader steps it. It shows `value`,
// so the owner moves it with `onChange` while the finger does.
import * as Haptics from 'expo-haptics';
import { useRef, useState } from 'react';
import { type LayoutChangeEvent, StyleSheet, View } from 'react-native';
import { GestureDetector, useExclusiveGestures, usePanGesture, useTapGesture } from 'react-native-gesture-handler';
import Animated, { useReducedMotion } from 'react-native-reanimated';

import { withAlpha } from './theme/tokens';
import { useTheme } from './theme/useTheme';

export interface ScrubberProps {
  value: number;
  min?: number;
  max: number;
  /** What values round to. */
  step: number;
  /** A screen reader's step; `step` when not given. */
  accessibilityStep?: number;
  /** While the finger moves. */
  onChange: (value: number) => void;
  /** When the finger lifts, or on a tap. */
  onCommit?: (value: number) => void;
  /** A value it settles on when near, with a tick (100 % volume). */
  detent?: number;
  /** Glides between values, for a position that ticks. */
  smooth?: boolean;
  disabled?: boolean;
  /** A tap while disabled, to say why. */
  onDisabledPress?: () => void;
  accessibilityLabel: string;
  /** Read out for the value ("1:12 of 3:41"). */
  accessibilityValue: string;
  testID?: string;
}

const HEIGHT = 32;
const BAR = 8;
/** The bar at rest is half as thick as under the finger. */
const REST_SCALE = 0.5;
const DETENT_REACH = 0.025;
const START_X = 4;
const FAIL_Y = 12;
const TICK_MS = 500;
const QUICK_MS = 150;

export function Scrubber({
  value,
  min = 0,
  max,
  step,
  accessibilityStep = step,
  onChange,
  onCommit,
  detent,
  smooth = false,
  disabled = false,
  onDisabledPress,
  accessibilityLabel,
  accessibilityValue,
  testID,
}: ScrubberProps) {
  const { colors } = useTheme();
  const reduceMotion = useReducedMotion();
  const [width, setWidth] = useState(0);
  const [active, setActive] = useState(false);
  const last = useRef(value);
  const range = max - min > 0 ? max - min : 1;

  const clamp = (next: number) => Math.min(Math.max(next, min), max);
  const valueAt = (x: number) => {
    const raw = min + (Math.min(Math.max(x, 0), width) / (width || 1)) * range;
    if (detent !== undefined && Math.abs(raw - detent) <= range * DETENT_REACH) return detent;
    return clamp(Math.round(raw / step) * step);
  };
  const move = (x: number) => {
    const next = valueAt(x);
    if (next === last.current) return;
    if (next === detent) void Haptics.selectionAsync();
    last.current = next;
    onChange(next);
  };
  const commit = (next: number) => {
    last.current = next;
    onChange(next);
    onCommit?.(next);
  };

  const pan = usePanGesture({
    testID: testID ? `${testID}-pan` : undefined,
    enabled: !disabled,
    runOnJS: true,
    activeOffsetX: [-START_X, START_X],
    failOffsetY: [-FAIL_Y, FAIL_Y],
    onActivate: (event) => {
      last.current = value;
      setActive(true);
      move(event.x);
    },
    onUpdate: (event) => move(event.x),
    onDeactivate: (event) => {
      setActive(false);
      if (!event.canceled) onCommit?.(last.current);
    },
  });
  const tap = useTapGesture({
    testID: testID ? `${testID}-tap` : undefined,
    runOnJS: true,
    onActivate: (event) => {
      if (disabled) onDisabledPress?.();
      else commit(valueAt(event.x));
    },
  });
  const gesture = useExclusiveGestures(pan, tap);

  const fraction = Math.min(Math.max((value - min) / range, 0), 1);
  const glide = reduceMotion || active ? 0 : smooth ? TICK_MS : QUICK_MS;
  const fill = disabled ? colors.textMuted : colors.text;

  return (
    <GestureDetector gesture={gesture}>
      <View
        testID={testID}
        accessible
        accessibilityRole="adjustable"
        accessibilityLabel={accessibilityLabel}
        accessibilityValue={{ text: accessibilityValue }}
        accessibilityState={{ disabled }}
        accessibilityActions={[{ name: 'increment' }, { name: 'decrement' }]}
        onAccessibilityAction={(event) => {
          if (disabled) return;
          const delta = event.nativeEvent.actionName === 'increment' ? accessibilityStep : -accessibilityStep;
          commit(clamp(value + delta));
        }}
        onLayout={(event: LayoutChangeEvent) => setWidth(event.nativeEvent.layout.width)}
        style={styles.hit}
      >
        <Animated.View
          style={[
            styles.bar,
            {
              backgroundColor: withAlpha(colors.text, 0.16),
              transform: [{ scaleY: active ? 1 : REST_SCALE }],
              transitionProperty: 'transform',
              transitionDuration: reduceMotion ? 0 : QUICK_MS,
            },
          ]}
        >
          <Animated.View
            testID={testID ? `${testID}-fill` : undefined}
            style={[
              styles.fill,
              {
                backgroundColor: fill,
                transform: [{ translateX: (fraction - 1) * width }],
                transitionProperty: 'transform',
                transitionDuration: glide,
                transitionTimingFunction: smooth ? 'linear' : 'ease-out',
              },
            ]}
          />
        </Animated.View>
      </View>
    </GestureDetector>
  );
}

const styles = StyleSheet.create({
  hit: { height: HEIGHT, justifyContent: 'center' },
  bar: { height: BAR, borderRadius: BAR / 2, overflow: 'hidden' },
  fill: { ...StyleSheet.absoluteFill },
});
