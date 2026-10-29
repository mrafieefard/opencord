// Source selector menu for music search (spec §8.7): native menu from available_sources.
import { memo, useRef } from 'react';
import { StyleSheet, View } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';

import {
  Icon,
  Menu,
  type MenuAction,
  type MenuHandle,
  Text,
  radius,
  space,
  useTheme,
} from '@/ui';

export interface SourceMenuProps {
  sources: string[];
  selectedSource: string;
  onSelectSource: (source: string) => void;
}

function sourceLabel(source: string): string {
  if (source === 'youtube') return 'YouTube';
  if (source === 'soundcloud') return 'SoundCloud';
  if (source === 'local') return 'Local';
  return source.charAt(0).toUpperCase() + source.slice(1);
}

export const SourceMenu = memo(function SourceMenu({
  sources,
  selectedSource,
  onSelectSource,
}: SourceMenuProps) {
  const { colors } = useTheme();
  const menuRef = useRef<MenuHandle>(null);

  const actions: MenuAction[] = sources.map((s) => ({
    id: s,
    title: sourceLabel(s),
    icon: s === selectedSource ? 'check' : undefined,
  }));

  return (
    <View style={styles.container}>
      <Menu
        ref={menuRef}
        testID="music-source-menu"
        trigger="press"
        actions={actions}
        onAction={onSelectSource}
      >
        <Touchable
          onPress={() => menuRef.current?.open()}
          accessible
          accessibilityRole="button"
          accessibilityLabel={`Source: ${sourceLabel(selectedSource)}`}
          style={[
            styles.pill,
            { backgroundColor: colors.surface, borderColor: colors.border },
          ]}
        >
          <Text variant="callout" style={styles.pillText}>
            {sourceLabel(selectedSource)}
          </Text>
          <Icon name="more" size={16} color="textSecondary" />
        </Touchable>
      </Menu>
    </View>
  );
});

const styles = StyleSheet.create({
  container: {
    justifyContent: 'center',
  },
  pill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: space.xs,
    paddingHorizontal: space.sm,
    paddingVertical: space.xs,
    borderRadius: radius.pill,
    borderWidth: StyleSheet.hairlineWidth,
  },
  pillText: {
    fontWeight: '500',
  },
});
