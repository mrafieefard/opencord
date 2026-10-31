// Unread counts: nothing at 0, "99+" above 99, "@3" for mentions.
import { StyleSheet } from 'react-native';
import Animated, { ZoomIn } from 'react-native-reanimated';

import { Text } from './Text';
import { motion } from './theme/space';
import { useTheme } from './theme/useTheme';

export function badgeText(count: number, mention = false): string | null {
  if (count <= 0) return null;
  const number = count > 99 ? '99+' : String(count);
  return mention ? `@${number}` : number;
}

export function Badge({ count, mention = false, testID }: { count: number; mention?: boolean; testID?: string }) {
  const { colors } = useTheme();
  const text = badgeText(count, mention);
  if (text === null) return null;
  return (
    <Animated.View testID={testID} entering={ZoomIn.duration(motion.badge)} style={[styles.badge, { backgroundColor: colors.accent }]}>
      <Text variant="caption" color="onAccent" tabular style={styles.text}>
        {text}
      </Text>
    </Animated.View>
  );
}

const styles = StyleSheet.create({
  badge: { minWidth: 20, height: 20, borderRadius: 10, paddingHorizontal: 6, alignItems: 'center', justifyContent: 'center' },
  text: { textTransform: 'none', letterSpacing: 0, fontSize: 12, lineHeight: 16 },
});
