// SwiftUI's own toggle, its label hidden: rows say what it is.
import { Host, Toggle } from '@expo/ui/swift-ui';
import { disabled as disabledModifier, labelsHidden, tint } from '@expo/ui/swift-ui/modifiers';

import type { SwitchProps } from './Switch.types';
import { useTheme } from './theme/useTheme';

export type { SwitchProps } from './Switch.types';

export function Switch({ value, onChange, disabled = false, accessibilityLabel }: SwitchProps) {
  const { colors, scheme } = useTheme();
  return (
    <Host matchContents colorScheme={scheme}>
      <Toggle
        isOn={value}
        label={accessibilityLabel}
        onIsOnChange={onChange}
        modifiers={[labelsHidden(), tint(colors.text), disabledModifier(disabled)]}
      />
    </Host>
  );
}
