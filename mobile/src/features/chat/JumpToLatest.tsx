import { memo } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import { Badge, Icon, space, useTheme } from '@/ui';

export interface JumpToLatestProps {
  count?: number;
  onPress: () => void;
  /** What floats over the list's bottom (the chat's dock): it sits above that. */
  bottom?: number;
}

export const JumpToLatest = memo(function JumpToLatest({ count = 0, onPress, bottom = 0 }: JumpToLatestProps) {
  const { colors } = useTheme();

  return (
    <View style={[styles.anchor, { bottom: bottom + space.lg }]} pointerEvents="box-none">
      <Touchable
        onPress={onPress}
        underlayColor={colors.selected}
        style={[styles.button, { backgroundColor: colors.elevated, borderColor: colors.border }]}
        testID="jump-to-latest"
      >
        <View style={styles.center} accessible accessibilityRole="button" accessibilityLabel="Jump to latest">
          <Icon name="jumpDown" size={22} color="text" />
          {count > 0 && (
            <View style={styles.badgeWrapper}>
              <Badge count={count} />
            </View>
          )}
        </View>
      </Touchable>
    </View>
  );
});

const styles = StyleSheet.create({
  anchor: {
    position: 'absolute',
    right: space.lg,
    zIndex: 10,
  },
  button: {
    width: 44,
    height: 44,
    borderRadius: 22,
    // No shadows (design §9.3): a hairline sets it off the messages.
    borderWidth: StyleSheet.hairlineWidth,
    justifyContent: 'center',
    alignItems: 'center',
  },
  center: {
    width: '100%',
    height: '100%',
    justifyContent: 'center',
    alignItems: 'center',
  },
  badgeWrapper: {
    position: 'absolute',
    top: -6,
    right: -6,
  },
});
