// Where a command comes from: the Opencord mark, a plugin's icon, or its
// initials when it has none.
import { Image } from 'expo-image';
import { StyleSheet, View } from 'react-native';

import { useImageFile } from '@/features/servers/useImageFile';
import { Icon, Text, initials, useTheme } from '@/ui';

import type { CommandSource } from './commandRows';

export interface SourceIconProps {
  serverKey: string;
  source: CommandSource;
  size: number;
  radius: number;
}

export function SourceIcon({ serverKey, source, size, radius }: SourceIconProps) {
  const { colors } = useTheme();
  const file = useImageFile(serverKey, source.avatarHash);
  const box = { width: size, height: size, borderRadius: radius };
  return (
    <View style={[styles.box, box, { backgroundColor: colors.avatarShades[0] }]}>
      {file ? (
        <Image source={{ uri: file }} style={box} accessibilityIgnoresInvertColors />
      ) : source.id === '' ? (
        <Icon name="commands" size={size * 0.62} color="text" />
      ) : (
        <Text variant="subhead" style={{ fontSize: size * 0.36, lineHeight: size * 0.46 }}>
          {initials(source.name)}
        </Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  box: { alignItems: 'center', justifyContent: 'center', overflow: 'hidden' },
});
