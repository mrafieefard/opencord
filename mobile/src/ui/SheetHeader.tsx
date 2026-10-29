// The top of a sheet's content: a title, a line under it, and a close
// button. Android's sheets have no grabber of their own, so one is drawn
// there; iOS shows its native one (`sheetGrabberVisible`).
import { Platform, StyleSheet, View } from 'react-native';

import { IconButton } from './IconButton';
import { Text } from './Text';
import { space } from './theme/space';
import { useTheme } from './theme/useTheme';

export interface SheetHeaderProps {
  title: string;
  subtitle?: string;
  onClose: () => void;
}

export const GRABBER = { width: 36, height: 5 } as const;

/** Android's grabber; iOS draws its own. */
export function SheetGrabber() {
  const { colors } = useTheme();
  if (Platform.OS !== 'android') return null;
  return <View style={[styles.grabber, { backgroundColor: colors.textMuted }]} />;
}

export function SheetHeader({ title, subtitle, onClose }: SheetHeaderProps) {
  return (
    <View>
      <SheetGrabber />
      <View style={styles.titleRow}>
        <Text variant="title2" accessibilityRole="header" style={styles.title}>
          {title}
        </Text>
        <IconButton icon="close" label="Close" size={36} onPress={onClose} />
      </View>
      {subtitle !== undefined && (
        <Text variant="callout" color="textSecondary" style={styles.subtitle}>
          {subtitle}
        </Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  grabber: {
    marginTop: space.sm,
    alignSelf: 'center',
    width: GRABBER.width,
    height: GRABBER.height,
    borderRadius: GRABBER.height / 2,
    opacity: 0.5,
    marginBottom: space.sm,
  },
  titleRow: { flexDirection: 'row', alignItems: 'center', gap: space.md, paddingTop: space.sm },
  title: { flex: 1 },
  subtitle: { marginTop: space.xs },
});
