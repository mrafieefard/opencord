// Toasts from the store, over everything at the bottom, for 3 s each.
import { StyleSheet, View } from 'react-native';
import Animated, { FadeInDown, FadeOut } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { useToasts } from '@/state/selectors';

import { Text } from './Text';
import { radius, space } from './theme/space';
import { useTheme } from './theme/useTheme';

export function ToastHost() {
  const toasts = useToasts();
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  return (
    <View pointerEvents="none" style={[styles.host, { bottom: insets.bottom + 72 }]}>
      {toasts.map((toast) => (
        <Animated.View
          key={toast.id}
          entering={FadeInDown.duration(160)}
          exiting={FadeOut.duration(160)}
          accessibilityLiveRegion="polite"
          style={[styles.toast, { backgroundColor: colors.elevated, borderColor: colors.border }]}
        >
          <Text variant="callout" color={toast.tone === 'error' ? 'recording' : 'text'}>
            {toast.text}
          </Text>
        </Animated.View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  host: { position: 'absolute', left: space.lg, right: space.lg, alignItems: 'center', gap: space.sm },
  toast: {
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
    borderRadius: radius.tile,
    borderWidth: StyleSheet.hairlineWidth,
    maxWidth: 480,
  },
});
