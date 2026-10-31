// Jetpack Compose's own switch, monochrome.
import { Host, Switch as ComposeSwitch } from '@expo/ui/jetpack-compose';
import { semantics } from '@expo/ui/jetpack-compose/modifiers';

import type { SwitchProps } from './Switch.types';
import { useTheme } from './theme/useTheme';

export type { SwitchProps } from './Switch.types';

export function Switch({ value, onChange, disabled = false, accessibilityLabel }: SwitchProps) {
  const { colors, scheme } = useTheme();
  return (
    <Host matchContents colorScheme={scheme}>
      <ComposeSwitch
        value={value}
        enabled={!disabled}
        modifiers={[semantics({ contentDescription: accessibilityLabel })]}
        onCheckedChange={onChange}
        colors={{
          checkedThumbColor: colors.onAccent,
          checkedTrackColor: colors.accent,
          checkedBorderColor: colors.accent,
          uncheckedThumbColor: colors.textMuted,
          uncheckedTrackColor: colors.surface,
          uncheckedBorderColor: colors.border,
        }}
      />
    </Host>
  );
}
