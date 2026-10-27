// The header a pane draws itself, so it moves with the pane (design §7.2):
// 52 pt under the safe area, the title centered on iOS and at the left on
// Android, a hairline under it.
import type { ReactNode } from 'react';
import { Platform, StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { Icon } from './Icon';
import { IconButton } from './IconButton';
import type { IconName } from './icons';
import { Text } from './Text';
import { space } from './theme/space';
import { useTheme } from './theme/useTheme';

export interface HeaderAction {
  icon: IconName;
  label: string;
  onPress: () => void;
}

export interface PaneHeaderProps {
  title: string;
  subtitle?: string;
  titleIcon?: IconName;
  left?: HeaderAction;
  right?: HeaderAction[];
  rightElement?: ReactNode;
  /** The tablet's split view draws no top inset for the content pane. */
  insetTop?: boolean;
}

const centered = Platform.OS === 'ios';

export function PaneHeader({ title, subtitle, titleIcon, left, right = [], rightElement, insetTop = true }: PaneHeaderProps) {
  const { colors } = useTheme();
  const insets = useSafeAreaInsets();
  return (
    <View
      style={[
        styles.header,
        { paddingTop: insetTop ? insets.top : 0, backgroundColor: colors.sidebar, borderBottomColor: colors.border },
      ]}
    >
      <View style={styles.bar}>
        <View style={[styles.side, centered && styles.sideFixed]}>
          {left && <IconButton icon={left.icon} label={left.label} onPress={left.onPress} />}
        </View>
        <View style={[styles.titles, centered ? styles.titlesCentered : styles.titlesStart]}>
          <View style={styles.titleLine}>
            {titleIcon && <Icon name={titleIcon} size={17} color="textSecondary" />}
            <Text variant="headline" numberOfLines={1} accessibilityRole="header">
              {title}
            </Text>
          </View>
          {subtitle !== undefined && subtitle !== '' && (
            <Text variant="footnote" color="textMuted" numberOfLines={1}>
              {subtitle}
            </Text>
          )}
        </View>
        <View style={[styles.side, styles.sideEnd, centered && styles.sideFixed]}>
          {rightElement ?? right.map((action) => (
            <IconButton key={action.label} icon={action.icon} label={action.label} onPress={action.onPress} />
          ))}
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  header: { borderBottomWidth: StyleSheet.hairlineWidth },
  bar: { height: 52, flexDirection: 'row', alignItems: 'center', paddingHorizontal: space.xs },
  side: { flexDirection: 'row', alignItems: 'center' },
  sideFixed: { width: 96 },
  sideEnd: { justifyContent: 'flex-end' },
  titles: { flex: 1, minWidth: 0 },
  titlesCentered: { alignItems: 'center' },
  titlesStart: { alignItems: 'flex-start', paddingHorizontal: space.sm },
  titleLine: { flexDirection: 'row', alignItems: 'center', gap: space.xs },
});
