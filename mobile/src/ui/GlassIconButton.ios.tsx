// A round Liquid Glass button with one SF Symbol, as Apple's sheets close
// (Maps, Find My): the sheet's close button, and its other round actions.
import { Button as SwiftButton, Host } from '@expo/ui/swift-ui';
import { accessibilityLabel, buttonBorderShape, buttonStyle, controlSize, labelStyle, tint } from '@expo/ui/swift-ui/modifiers';

import { icons, type IconName } from './icons';
import { useTheme } from './theme/useTheme';

export interface GlassIconButtonProps {
  icon: IconName;
  label: string;
  onPress: () => void;
}

export function GlassIconButton({ icon, label, onPress }: GlassIconButtonProps) {
  const { colors, scheme } = useTheme();
  return (
    <Host matchContents colorScheme={scheme} ignoreSafeArea="all">
      <SwiftButton
        label={label}
        systemImage={icons[icon].ios}
        onPress={onPress}
        modifiers={[
          buttonStyle('glass'),
          buttonBorderShape('circle'),
          controlSize('large'),
          labelStyle('iconOnly'),
          tint(colors.text),
          accessibilityLabel(label),
        ]}
      />
    </Host>
  );
}
