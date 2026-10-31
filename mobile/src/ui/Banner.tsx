// A line across a pane: the voice link's state, a failure with an action.
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Text } from './Text';
import { space } from './theme/space';
import { useTheme } from './theme/useTheme';

export interface BannerProps {
  text: string;
  tone: 'neutral' | 'error';
  action?: { title: string; onPress: () => void };
}

export function Banner({ text, tone, action }: BannerProps) {
  const { colors } = useTheme();
  return (
    <View
      accessibilityRole="alert"
      style={[styles.banner, { backgroundColor: colors.surface, borderBottomColor: colors.border }]}
    >
      <Text variant="footnote" color={tone === 'error' ? 'recording' : 'textSecondary'} style={styles.text}>
        {text}
      </Text>
      {action && (
        <Touchable onPress={action.onPress} hitSlop={8}>
          <View accessible accessibilityRole="button" accessibilityLabel={action.title}>
            <Text variant="subhead">{action.title}</Text>
          </View>
        </Touchable>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  banner: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.md,
    paddingHorizontal: space.lg,
    paddingVertical: space.sm,
    borderBottomWidth: StyleSheet.hairlineWidth,
  },
  text: { flex: 1 },
});
