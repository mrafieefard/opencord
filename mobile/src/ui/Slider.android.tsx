// Jetpack Compose's own slider, in the text color (design §10).
import { Host, Slider as ComposeSlider } from '@expo/ui/jetpack-compose';
import { semantics } from '@expo/ui/jetpack-compose/modifiers';
import { useRef } from 'react';

import type { SliderProps } from './Slider.types';
import { useTheme } from './theme/useTheme';

export type { SliderProps } from './Slider.types';

const MAX_DRAWN_STOPS = 10;

export function Slider({
  value,
  min,
  max,
  step,
  accent = 'text',
  onChange,
  onCommit,
  disabled = false,
  accessibilityLabel,
}: SliderProps) {
  const { colors, scheme } = useTheme();
  const trackColor = accent === 'voice' ? colors.voice : colors.text;
  const last = useRef(value);
  // Compose counts the stops between the ends, not the step, and draws a
  // dot for each: a fine step (volume's 1 %) slides freely and rounds here.
  const stops = step ? Math.max(Math.round((max - min) / step) - 1, 0) : 0;
  const steps = stops <= MAX_DRAWN_STOPS ? stops : 0;
  const snap = (next: number) => (step && steps === 0 ? Math.round((next - min) / step) * step + min : next);
  return (
    <Host matchContents={{ vertical: true }} colorScheme={scheme} style={{ flex: 1 }}>
      <ComposeSlider
        value={value}
        min={min}
        max={max}
        steps={steps}
        enabled={!disabled}
        modifiers={[semantics({ contentDescription: accessibilityLabel })]}
        colors={{
          thumbColor: trackColor,
          activeTrackColor: trackColor,
          inactiveTrackColor: colors.selected,
          activeTickColor: colors.onAccent,
          inactiveTickColor: colors.textMuted,
        }}
        onValueChange={(raw) => {
          const next = snap(raw);
          if (next === last.current) return;
          last.current = next;
          onChange(next);
        }}
        onValueChangeFinished={() => onCommit?.(last.current)}
      />
    </Host>
  );
}
