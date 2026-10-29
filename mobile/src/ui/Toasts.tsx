// Toasts from the store, over everything at the bottom, for 3 s each.
// Inverse, as Material's snackbars are, so they stand out on any surface
// (a sheet is `elevated` too); an error leads with a warning in `recording`.
import { StyleSheet, View } from 'react-native';
import Animated, { FadeInDown, FadeOut } from 'react-native-reanimated';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { useToasts } from '@/state/selectors';

import { Icon } from './Icon';
import { Text } from './Text';
import { radius, space } from './theme/space';
import { useTheme } from './theme/useTheme';

export interface ToastsProps {
  /** Where the toasts stand, from the bottom of what holds them. */
  bottom?: number;
}

export function Toasts({ bottom }: ToastsProps) {
  const toasts = useToasts();
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  return (
    <View pointerEvents="none" style={[styles.host, { bottom: bottom ?? insets.bottom + 72 }]}>
      {toasts.map((toast) => (
        <Animated.View
          key={toast.id}
          entering={FadeInDown.duration(160)}
          exiting={FadeOut.duration(160)}
          accessibilityLiveRegion="polite"
          style={[styles.toast, { backgroundColor: colors.accent }]}
        >
          {toast.tone === 'error' && <Icon name="warning" size={18} color="recording" />}
          <Text variant="callout" color="onAccent" style={styles.text}>
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
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.sm,
    paddingHorizontal: space.lg,
    paddingVertical: space.md,
    borderRadius: radius.tile,
    maxWidth: 480,
  },
  text: { flexShrink: 1 },
});
