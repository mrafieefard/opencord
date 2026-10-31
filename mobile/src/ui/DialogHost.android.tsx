// The question confirm() holds, as a card in `elevated` in the middle of
// the screen over the scrim: the title, the message, and Cancel beside the
// confirming button. Back and a tap outside cancel.
import { useEffect } from 'react';
import { Modal, StyleSheet, View, useWindowDimensions } from 'react-native';
import { GestureHandlerRootView, Touchable } from 'react-native-gesture-handler';
import Animated, { useAnimatedStyle, useSharedValue, withTiming } from 'react-native-reanimated';

import { Button } from './Button.android';
import { closeConfirm, useOpenConfirm, type ConfirmOptions } from './confirm.android';
import { Text } from './Text';
import { radius, space } from './theme/space';
import { useTheme } from './theme/useTheme';

/** The card's widest, and its distance from the screen's sides. */
export const DIALOG_MAX_WIDTH = 400;
const DIALOG_MARGIN = space.xxl;
const OPEN_MS = 160;

export function DialogHost() {
  const options = useOpenConfirm();
  if (options === null) return null;
  return <Dialog options={options} />;
}

function Dialog({ options }: { options: ConfirmOptions }) {
  const { colors } = useTheme();
  const window = useWindowDimensions();
  const width = Math.min(window.width - DIALOG_MARGIN * 2, DIALOG_MAX_WIDTH);

  const progress = useSharedValue(0);
  useEffect(() => {
    progress.set(withTiming(1, { duration: OPEN_MS }));
  }, [progress]);
  const scrimStyle = useAnimatedStyle(() => ({ opacity: progress.get() }));
  const cardStyle = useAnimatedStyle(() => ({
    opacity: progress.get(),
    transform: [{ scale: 0.95 + 0.05 * progress.get() }],
  }));

  const accept = () => {
    closeConfirm();
    options.onConfirm();
  };

  return (
    <Modal transparent visible statusBarTranslucent navigationBarTranslucent animationType="none" onRequestClose={closeConfirm}>
      <GestureHandlerRootView style={styles.center}>
        <Touchable onPress={closeConfirm} style={StyleSheet.absoluteFill} accessibilityLabel="Close">
          <Animated.View style={[StyleSheet.absoluteFill, { backgroundColor: colors.scrim }, scrimStyle]} />
        </Touchable>
        <Animated.View
          accessibilityRole="alert"
          accessibilityViewIsModal
          style={[styles.card, { width, backgroundColor: colors.elevated }, cardStyle]}
        >
          <View style={styles.text}>
            <Text variant="headline">{options.title}</Text>
            {options.message !== undefined && (
              <Text variant="callout" color="textSecondary">
                {options.message}
              </Text>
            )}
          </View>
          <View style={styles.actions}>
            <View style={styles.action}>
              <Button title="Cancel" variant="plain" onPress={closeConfirm} />
            </View>
            <View style={styles.action}>
              <Button
                title={options.confirmTitle}
                variant={options.destructive ? 'danger' : 'primary'}
                onPress={accept}
              />
            </View>
          </View>
        </Animated.View>
      </GestureHandlerRootView>
    </Modal>
  );
}

const styles = StyleSheet.create({
  center: { flex: 1, alignItems: 'center', justifyContent: 'center' },
  card: { borderRadius: radius.sheet, padding: space.xxl, paddingBottom: space.lg, gap: space.xl },
  text: { gap: space.sm },
  actions: { flexDirection: 'row', gap: space.sm },
  action: { flex: 1 },
});
