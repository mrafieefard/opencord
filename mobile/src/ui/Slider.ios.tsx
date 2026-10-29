// SwiftUI's own slider, tinted with the text color (design §10).
import { Host, Slider as SwiftSlider } from '@expo/ui/swift-ui';
import { accessibilityLabel as label, disabled as disabledModifier, tint } from '@expo/ui/swift-ui/modifiers';
import { useRef } from 'react';

import type { SliderProps } from './Slider.types';
import { useTheme } from './theme/useTheme';

export type { SliderProps } from './Slider.types';

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
  return (
    <Host matchContents={{ vertical: true }} colorScheme={scheme} style={{ flex: 1 }}>
      <SwiftSlider
        value={value}
        min={min}
        max={max}
        step={step}
        modifiers={[tint(trackColor), disabledModifier(disabled), label(accessibilityLabel)]}
        onValueChange={(next) => {
          last.current = next;
          onChange(next);
        }}
        onEditingChanged={(editing) => {
          if (!editing) onCommit?.(last.current);
        }}
      />
    </Host>
  );
}
