// A person: their picture, or the first letters of their name on a shade
// picked by the name. A ring in `voice` while they speak.
import { Image } from 'expo-image';
import { StyleSheet, View } from 'react-native';
import Animated from 'react-native-reanimated';

import { Text } from './Text';
import { motion } from './theme/space';
import { useTheme } from './theme/useTheme';

export function nameHash(name: string): number {
  return [...name].reduce((hash, char) => (Math.imul(hash, 31) + (char.codePointAt(0) ?? 0)) >>> 0, 7);
}

export function initials(name: string): string {
  return name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((word) => [...word][0] ?? '')
    .join('')
    .toUpperCase();
}

export interface AvatarProps {
  name: string;
  file?: string | null;
  size: number;
  speaking?: boolean;
}

export function Avatar({ name, file, size, speaking = false }: AvatarProps) {
  const { colors } = useTheme();
  const ring = size >= 64 ? 3 : 2;
  const shade = colors.avatarShades[nameHash(name) % colors.avatarShades.length];
  const round = { width: size, height: size, borderRadius: size / 2 };
  return (
    <View style={{ width: size + ring * 2, height: size + ring * 2 }}>
      <Animated.View
        testID="avatar-ring"
        style={[
          StyleSheet.absoluteFill,
          {
            borderRadius: size / 2 + ring,
            borderWidth: ring,
            borderColor: colors.voice,
            opacity: speaking ? 1 : 0,
            transitionProperty: 'opacity',
            transitionDuration: motion.speaking,
          },
        ]}
      />
      <View style={[round, styles.face, { margin: ring, backgroundColor: shade }]}>
        {file ? (
          <Image source={{ uri: file }} style={round} accessibilityIgnoresInvertColors />
        ) : (
          <Text variant={size >= 48 ? 'headline' : 'subhead'} style={{ fontSize: Math.round(size * 0.4), lineHeight: Math.round(size * 0.5) }}>
            {initials(name)}
          </Text>
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  face: { alignItems: 'center', justifyContent: 'center', overflow: 'hidden' },
});
