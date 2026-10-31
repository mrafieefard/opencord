// The composer's mic, as Telegram's: glass at rest; held, it swells into a
// filled circle that follows the finger, with a soft ring around it that
// breathes with the voice. It turns red once letting go would cancel.
import { useEffect } from 'react';
import { StyleSheet, View } from 'react-native';
import Animated, { type SharedValue, useAnimatedStyle, useSharedValue, withSpring, withTiming } from 'react-native-reanimated';

import { useAppStore } from '@/state/store';
import { GlassSurface, Icon, useTheme } from '@/ui';

export interface MicButtonProps {
  holding: boolean;
  cancelArmed: boolean;
  dragX: SharedValue<number>;
  dragY: SharedValue<number>;
  size: number;
}

/** How much bigger the held circle is, and how far it follows the finger. */
const GROWN = 0.6;
const FOLLOW = 0.35;
/** The ring at full voice, beyond the grown circle. */
const BREATH = 0.9;

export function MicButton({ holding, cancelArmed, dragX, dragY, size }: MicButtonProps) {
  const { colors } = useTheme();
  const level = useAppStore((state) => (holding ? (state.voiceRecording?.levels.at(-1) ?? 0) : 0));
  const grow = useSharedValue(0);
  const voice = useSharedValue(0);
  useEffect(() => {
    grow.set(withSpring(holding ? 1 : 0, { damping: 14, stiffness: 180 }));
  }, [holding, grow]);
  useEffect(() => {
    voice.set(withTiming(Math.min(Math.sqrt(level), 1), { duration: 90 }));
  }, [level, voice]);

  const circle = useAnimatedStyle(() => ({
    transform: [
      { translateX: dragX.get() * FOLLOW },
      { translateY: dragY.get() * FOLLOW },
      { scale: 1 + grow.get() * GROWN },
    ],
  }));
  const ring = useAnimatedStyle(() => ({
    opacity: grow.get() * 0.25,
    transform: [
      { translateX: dragX.get() * FOLLOW },
      { translateY: dragY.get() * FOLLOW },
      { scale: 1 + grow.get() * (GROWN + voice.get() * BREATH) },
    ],
  }));
  const fill = cancelArmed ? colors.recording : colors.voice;
  const round = { width: size, height: size, borderRadius: size / 2 };

  return (
    <View style={round}>
      <Animated.View pointerEvents="none" style={[StyleSheet.absoluteFill, round, { backgroundColor: fill }, ring]} />
      {holding ? (
        <Animated.View style={[round, styles.center, { backgroundColor: fill }, circle]}>
          <Icon name="mic" size={22} color="onAccent" />
        </Animated.View>
      ) : (
        <GlassSurface interactive style={round}>
          <View
            style={[round, styles.center]}
            accessible
            accessibilityRole="button"
            accessibilityLabel="Record a voice message"
            accessibilityHint="Hold to record, let go to send"
            testID="composer-mic"
          >
            <Icon name="mic" size={22} color="text" />
          </View>
        </GlassSurface>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  center: { alignItems: 'center', justifyContent: 'center' },
});
