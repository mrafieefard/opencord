// A server: its icon, or its first two letters, in a 14 pt squircle.
import { Image } from 'expo-image';
import { StyleSheet, View } from 'react-native';

import { initials, nameHash } from './Avatar';
import { Text } from './Text';
import { radius } from './theme/space';
import { useTheme } from './theme/useTheme';

export function ServerIcon({ name, file, size = 52 }: { name: string; file?: string | null; size?: number }) {
  const { colors } = useTheme();
  const shape = { width: size, height: size, borderRadius: radius.icon };
  const shade = colors.avatarShades[nameHash(name) % colors.avatarShades.length];
  return (
    <View style={[shape, styles.box, { backgroundColor: shade }]}>
      {file ? (
        <Image source={{ uri: file }} style={shape} accessibilityIgnoresInvertColors />
      ) : (
        <Text variant="headline">{initials(name)}</Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({ box: { alignItems: 'center', justifyContent: 'center', overflow: 'hidden' } });
