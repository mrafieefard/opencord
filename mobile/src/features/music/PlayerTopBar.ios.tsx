// The player's top on iOS, the same as every sheet's (SheetHeader.ios):
// where the music plays, leading; Join in voice-tinted glass while the user
// isn't listening there; the round glass close button, trailing.
import { Button as SwiftButton, Host } from '@expo/ui/swift-ui';
import { accessibilityLabel, buttonBorderShape, buttonStyle, controlSize, font, foregroundStyle, tint } from '@expo/ui/swift-ui/modifiers';
import { StyleSheet, View } from 'react-native';

import { GlassIconButton } from '@/ui/GlassIconButton.ios';
import { icons } from '@/ui/icons';
import { fonts, onHue, space, Text, useTheme } from '@/ui';

import type { PlayerTopBarProps } from './PlayerTopBar.types';

export type { PlayerTopBarProps } from './PlayerTopBar.types';

export function PlayerTopBar({ channelName, onJoin, onClose }: PlayerTopBarProps) {
  const { colors, scheme } = useTheme();
  return (
    <View style={styles.bar}>
      <View style={styles.where}>
        <Text variant="caption" color="textMuted">
          Playing in
        </Text>
        <Text variant="headline" numberOfLines={1}>
          {channelName}
        </Text>
      </View>
      {onJoin && (
        <Host matchContents colorScheme={scheme} ignoreSafeArea="all">
          <SwiftButton
            label="Join"
            systemImage={icons.listen.ios}
            onPress={onJoin}
            modifiers={[
              buttonStyle('glassProminent'),
              buttonBorderShape('capsule'),
              controlSize('large'),
              tint(colors.voice),
              foregroundStyle(onHue),
              font({ family: fonts.semiBold, size: 15 }),
              accessibilityLabel(`Join ${channelName} to listen`),
            ]}
          />
        </Host>
      )}
      <GlassIconButton icon="close" label="Close" onPress={onClose} />
    </View>
  );
}

const styles = StyleSheet.create({
  bar: { flexDirection: 'row', alignItems: 'center', gap: space.sm, paddingHorizontal: space.lg, paddingTop: space.lg, paddingBottom: space.sm },
  where: { flex: 1 },
});
