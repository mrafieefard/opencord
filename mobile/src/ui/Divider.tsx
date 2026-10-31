import { StyleSheet, View } from 'react-native';

import { useTheme } from './theme/useTheme';

/** A hairline, starting after a row's leading icon (`inset`). */
export function Divider({ inset = 0 }: { inset?: number }) {
  const { colors } = useTheme();
  return <View style={{ height: StyleSheet.hairlineWidth, marginLeft: inset, backgroundColor: colors.border }} />;
}
