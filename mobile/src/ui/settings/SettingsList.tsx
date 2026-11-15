// The scroll view under a settings page's sections. A field the keyboard
// would cover scrolls up above it. It scrolls inside a sheet with detents
// too: on Android pulling up raises the sheet before the list scrolls.
//
// The background is `chat` on both systems: on iOS the groups are `surface`
// cards on it, as on the You tab (in the light theme `sidebar` and `surface`
// are both white, so the cards would vanish on `sidebar`); on Android the
// rows sit on it directly.
import type { ReactNode } from 'react';
import { StyleSheet } from 'react-native';
import { KeyboardAwareScrollView } from 'react-native-keyboard-controller';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { space } from '../theme/space';
import { useTheme } from '../theme/useTheme';

export function SettingsList({ children }: { children: ReactNode }) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  return (
    <KeyboardAwareScrollView
      testID="settings-list"
      style={[styles.list, { backgroundColor: colors.chat }]}
      contentContainerStyle={{ paddingBottom: insets.bottom + space.xxxl }}
      contentInsetAdjustmentBehavior="automatic"
      keyboardShouldPersistTaps="handled"
      bottomOffset={space.xxl}
      nestedScrollEnabled
    >
      {children}
    </KeyboardAwareScrollView>
  );
}

const styles = StyleSheet.create({
  list: { flex: 1 },
});
