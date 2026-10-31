// The top of a sheet on iOS, as Apple's sheets have it (Maps, Find My):
// the title leading, bold; a line under it; a round glass close button
// trailing. iOS draws the grabber itself (`sheetGrabberVisible`).
import { StyleSheet, View } from 'react-native';

import { GlassIconButton } from './GlassIconButton.ios';
import type { SheetHeaderProps } from './SheetHeader.types';
import { Text } from './Text';
import { space } from './theme/space';

export { GRABBER, type SheetHeaderProps } from './SheetHeader.types';

/** iOS draws its own grabber. */
export function SheetGrabber() {
  return null;
}

export function SheetHeader({ title, subtitle, leading, onClose }: SheetHeaderProps) {
  return (
    <View>
      <View style={[styles.titleRow, leading !== undefined && styles.centered]}>
        {leading}
        <View style={styles.titles}>
          <Text variant="title2" accessibilityRole="header">
            {title}
          </Text>
          {subtitle !== undefined && (
            <Text variant="callout" color="textSecondary">
              {subtitle}
            </Text>
          )}
        </View>
        <GlassIconButton icon="close" label="Close" onPress={onClose} />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  titleRow: { flexDirection: 'row', alignItems: 'flex-start', gap: space.md, paddingTop: space.lg },
  /** Beside a picture (a member's avatar), the title centres on it. */
  centered: { alignItems: 'center' },
  titles: { flex: 1, gap: space.xs, paddingTop: space.xs },
});
