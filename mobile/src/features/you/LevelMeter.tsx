// The microphone's live level under Voice & audio's test: a 6 pt bar in the
// accent colour, filling evenly in dB from −60 to 0 dBFS, as the desktop's
// InputLevelMeter does. The core reports the level every 50 ms
// (`REPORT_EVERY`); the bar redraws at most 30 times a second.
import { useEffect, useState } from 'react';
import { StyleSheet, View } from 'react-native';
import Animated from 'react-native-reanimated';

import { onInputLevel } from '@/state/actions/audio';
import { useTheme } from '@/ui';

/** The quietest level the bar shows. */
const FLOOR_DBFS = -60;
/** 30 fps. */
const FRAME_MS = 1000 / 30;

/** How full the bar is for a level: 0 at −60 dBFS and below, 1 at 0. */
export function levelFill(dbfs: number): number {
  if (!Number.isFinite(dbfs)) return 0;
  return Math.min(Math.max((dbfs - FLOOR_DBFS) / -FLOOR_DBFS, 0), 1);
}

export function LevelMeter() {
  const { colors } = useTheme();
  // Empty until the first level arrives.
  const [fill, setFill] = useState(0);
  useEffect(() => {
    let drawnAt = -Infinity;
    return onInputLevel((dbfs) => {
      const now = Date.now();
      if (now - drawnAt < FRAME_MS) return;
      drawnAt = now;
      setFill(levelFill(dbfs));
    });
  }, []);
  return (
    <View
      accessibilityRole="progressbar"
      accessibilityLabel="Input level"
      accessibilityValue={{ min: 0, max: 100, now: Math.round(fill * 100) }}
      style={[styles.track, { backgroundColor: colors.selected }]}
    >
      <Animated.View
        testID="level-fill"
        style={[
          styles.fill,
          {
            backgroundColor: colors.accent,
            transform: [{ scaleX: fill }],
            transformOrigin: 'left',
            transitionProperty: 'transform',
            transitionDuration: 50,
          },
        ]}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  track: { height: 6, borderRadius: 3, overflow: 'hidden' },
  fill: { flex: 1 },
});
