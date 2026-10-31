// A voice message's waveform (media uploads design §11): 3 pt bars with
// 2 pt gaps, as many as fit, the played part in the voice blue and the
// rest muted. A tap or a drag along it seeks, given `onSeek`.
import { useState } from 'react';
import { type LayoutChangeEvent, StyleSheet, View } from 'react-native';
import { Gesture, GestureDetector } from 'react-native-gesture-handler';

import { useTheme } from '@/ui';

export interface WaveformProps {
  /** 0–255 each (an attachment's), or 0–1 (live levels) with `unit`. */
  levels: readonly number[];
  /** The levels' full scale: 255 for a stored waveform, 1 for live levels. */
  unit?: number;
  /** 0–1: how much has played. */
  progress?: number;
  height?: number;
  /** A fraction 0–1 of the length, from a tap or a drag. */
  onSeek?: (fraction: number) => void;
  /** Bars at the right edge, the newest last (the live recording). */
  alignEnd?: boolean;
  color?: string;
  testID?: string;
}

export const BAR = 3;
export const GAP = 2;
const MIN_BAR = 3;

/** The loudest level in each of `count` equal slices. */
export function resample(levels: readonly number[], count: number): number[] {
  if (count <= 0) return [];
  if (levels.length === 0) return new Array<number>(count).fill(0);
  const bars: number[] = [];
  for (let index = 0; index < count; index += 1) {
    const from = Math.floor((index * levels.length) / count);
    const to = Math.max(from + 1, Math.floor(((index + 1) * levels.length) / count));
    let loudest = 0;
    for (let at = from; at < to && at < levels.length; at += 1) loudest = Math.max(loudest, levels[at] ?? 0);
    bars.push(loudest);
  }
  return bars;
}

export function Waveform({ levels, unit = 255, progress = 0, height = 28, onSeek, alignEnd = false, color, testID }: WaveformProps) {
  const { colors } = useTheme();
  const [width, setWidth] = useState(0);
  const count = Math.max(0, Math.floor((width + GAP) / (BAR + GAP)));
  // A live recording shows its last levels as they come, one bar each;
  // a stored waveform is spread over the whole width.
  const bars = alignEnd ? levels.slice(-count) : resample(levels, count);
  const played = Math.round(progress * bars.length);

  const onLayout = (event: LayoutChangeEvent) => setWidth(Math.floor(event.nativeEvent.layout.width));
  const seekTo = (x: number) => {
    if (onSeek && width > 0) onSeek(Math.min(Math.max(x / width, 0), 1));
  };
  const gesture = Gesture.Race(
    Gesture.Tap()
      .runOnJS(true)
      .onEnd((event) => seekTo(event.x)),
    Gesture.Pan()
      .runOnJS(true)
      .activeOffsetX([-4, 4])
      .onUpdate((event) => seekTo(event.x)),
  );

  const content = (
    <View
      style={[styles.row, { height }, alignEnd && styles.end]}
      onLayout={onLayout}
      testID={testID}
      accessible={!!onSeek}
      accessibilityRole={onSeek ? 'adjustable' : undefined}
      accessibilityLabel={onSeek ? 'Position' : undefined}
      accessibilityValue={onSeek ? { min: 0, max: 100, now: Math.round(progress * 100) } : undefined}
      onAccessibilityAction={(event) => {
        if (!onSeek) return;
        if (event.nativeEvent.actionName === 'increment') onSeek(Math.min(progress + 0.1, 1));
        if (event.nativeEvent.actionName === 'decrement') onSeek(Math.max(progress - 0.1, 0));
      }}
      accessibilityActions={onSeek ? [{ name: 'increment' }, { name: 'decrement' }] : undefined}
    >
      {bars.map((level, index) => (
        <View
          // Bars are positions, not items: the index is their identity.
          key={index}
          style={[
            styles.bar,
            {
              height: Math.max(MIN_BAR, Math.round((Math.min(level, unit) / unit) * height)),
              backgroundColor: color ?? (index < played ? colors.voice : colors.textMuted),
            },
          ]}
        />
      ))}
    </View>
  );
  return onSeek ? <GestureDetector gesture={gesture}>{content}</GestureDetector> : content;
}

const styles = StyleSheet.create({
  row: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: GAP, overflow: 'hidden' },
  end: { justifyContent: 'flex-end' },
  bar: { width: BAR, borderRadius: BAR / 2 },
});
