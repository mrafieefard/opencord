// A full-width action in Liquid Glass (iOS 26), as Apple's own apps draw
// their main actions: prominent glass tinted with the variant's colour,
// plain glass for the secondary one, text alone for `plain`.
import {
  Button as SwiftButton,
  HStack,
  Host,
  Image,
  ProgressView,
  Text as SwiftText,
} from '@expo/ui/swift-ui';
import {
  accessibilityLabel,
  buttonBorderShape,
  buttonStyle,
  controlSize,
  disabled as disabledModifier,
  font,
  foregroundStyle,
  frame,
  tint,
} from '@expo/ui/swift-ui/modifiers';

import type { ButtonProps } from './Button.types';
import { icons } from './icons';
import { onHue } from './theme/tokens';
import { fonts } from './theme/typography';
import { useTheme } from './theme/useTheme';

export type { ButtonProps } from './Button.types';

export function Button({ title, onPress, variant, loading = false, disabled = false, icon }: ButtonProps) {
  const { colors, scheme } = useTheme();
  const look =
    variant === 'primary'
      ? { style: 'glassProminent' as const, tint: colors.accent, text: colors.onAccent }
      : variant === 'voice'
        ? { style: 'glassProminent' as const, tint: colors.voice, text: onHue }
        : variant === 'danger'
          ? { style: 'glassProminent' as const, tint: colors.recording, text: onHue }
          : variant === 'secondary'
            ? { style: 'glass' as const, tint: undefined, text: colors.text }
            : { style: 'plain' as const, tint: undefined, text: colors.text };
  // Loading keeps its colour and ignores presses; disabled is the system's grey.
  const label = disabled ? colors.textMuted : look.text;
  return (
    <Host matchContents={{ vertical: true }} colorScheme={scheme} ignoreSafeArea="all" style={{ alignSelf: 'stretch' }}>
      <SwiftButton
        onPress={loading || disabled ? () => {} : onPress}
        modifiers={[
          buttonStyle(look.style),
          buttonBorderShape('capsule'),
          controlSize('large'),
          ...(look.tint ? [tint(look.tint)] : []),
          disabledModifier(disabled),
          accessibilityLabel(title),
        ]}
      >
        <HStack spacing={8} modifiers={[frame({ maxWidth: Infinity, minHeight: 30 })]}>
          {loading ? (
            <ProgressView modifiers={[tint(label)]} />
          ) : (
            <>
              {icon && <Image systemName={icons[icon].ios} modifiers={[foregroundStyle(label)]} />}
              <SwiftText modifiers={[font({ family: fonts.semiBold, size: 17 }), foregroundStyle(label)]}>{title}</SwiftText>
            </>
          )}
        </HStack>
      </SwiftButton>
    </Host>
  );
}
