// A group of settings rows. On iOS an inset rounded card in `surface`, its
// rows parted by hairlines, an uppercase footnote above it and a footnote
// under it, as in iOS Settings. On Android full-width rows under a section
// header, as in Android Settings.
import { Children, type ReactNode } from 'react';
import { Platform, StyleSheet, View } from 'react-native';

import { Divider } from '../Divider';
import { Text } from '../Text';
import { radius, space } from '../theme/space';
import { useTheme } from '../theme/useTheme';

export interface SettingsSectionProps {
  title?: string;
  footer?: string;
  children: ReactNode;
}

export function SettingsSection({ title, footer, children }: SettingsSectionProps) {
  const { colors } = useTheme();
  if (Platform.OS !== 'ios') {
    return (
      <View style={styles.androidSection}>
        {title !== undefined && (
          <Text variant="subhead" color="textSecondary" accessibilityRole="header" style={styles.androidTitle}>
            {title}
          </Text>
        )}
        {children}
        {footer !== undefined && (
          <Text variant="footnote" color="textMuted" style={styles.androidFooter}>
            {footer}
          </Text>
        )}
      </View>
    );
  }
  // Rows the caller left out (`cond && <Row/>`) take no separator.
  const rows = Children.toArray(children);
  return (
    <View style={styles.iosSection}>
      {title !== undefined && (
        <Text variant="footnote" color="textSecondary" accessibilityRole="header" style={styles.iosTitle}>
          {title}
        </Text>
      )}
      <View style={[styles.card, { backgroundColor: colors.surface }]}>
        {rows.map((row, index) => (
          <View key={index}>
            {index > 0 && <Divider inset={space.lg} />}
            {row}
          </View>
        ))}
      </View>
      {footer !== undefined && (
        <Text variant="footnote" color="textSecondary" style={styles.iosFooter}>
          {footer}
        </Text>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  iosSection: { marginTop: space.xxl, marginHorizontal: space.lg },
  iosTitle: { textTransform: 'uppercase', paddingHorizontal: space.lg, paddingBottom: space.sm },
  iosFooter: { paddingHorizontal: space.lg, paddingTop: space.sm },
  card: { borderRadius: radius.icon, overflow: 'hidden' },
  androidSection: { paddingTop: space.lg },
  androidTitle: { paddingHorizontal: space.lg, paddingTop: space.sm, paddingBottom: space.xs },
  androidFooter: { paddingHorizontal: space.lg, paddingBottom: space.sm },
});
