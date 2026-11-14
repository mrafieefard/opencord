// A plugin channel in the list (plugins M4 plan, Task 12): its type's icon,
// then its name; never an unread dot (§14.5). An unavailable one is dimmed
// and says why instead of opening (§14.4).
import type { ChannelRecord } from '@opencord/core-native';
import { memo } from 'react';
import { StyleSheet, View } from 'react-native';

import { PluginIcon } from '@/features/plugins/PluginIcon';
import { toast } from '@/state/actions/ui';
import type { ChannelItem } from '@/state/types';
import { Row, Text, space, useTheme } from '@/ui';
import { fonts } from '@/ui/theme/typography';

type PluginItem = Extract<ChannelItem, { kind: 'plugin' }>;

/** The app's disabled look. */
const DISABLED_OPACITY = 0.4;
const ICON_SIZE = 20;

const WHY: Record<Exclude<PluginItem['state'], 'available'>, string> = {
  plugin_stopped: 'This channel is unavailable',
  type_gone: 'This plugin no longer provides this channel',
};

export interface PluginChannelRowProps {
  serverKey: string;
  item: PluginItem;
  selected?: boolean;
  onSelect(channel: ChannelRecord): void;
}

export const PluginChannelRow = memo(function PluginChannelRow({ serverKey, item, selected = false, onSelect }: PluginChannelRowProps) {
  const { colors } = useTheme();
  const { channel, state, icon } = item;
  const available = state === 'available';

  const press = () => {
    if (state === 'available') onSelect(channel);
    else toast(WHY[state]);
  };

  return (
    <Row
      onPress={press}
      selected={selected}
      accessibilityRole="button"
      accessibilityLabel={channel.name}
      style={[styles.row, selected && { backgroundColor: colors.selected, borderRadius: 8 }, !available && { opacity: DISABLED_OPACITY }]}
    >
      <View style={styles.content}>
        <View style={styles.icon}>
          <PluginIcon serverKey={serverKey} pluginId={channel.plugin?.pluginId ?? ''} icon={icon} size={ICON_SIZE} testID={`plugin-icon-${channel.id}`} />
        </View>
        <Text variant="body" color={selected ? 'text' : 'textSecondary'} numberOfLines={1} style={styles.name}>
          {channel.name}
        </Text>
      </View>
    </Row>
  );
});

const styles = StyleSheet.create({
  row: { minHeight: 44, justifyContent: 'center', marginHorizontal: space.sm, paddingHorizontal: space.sm },
  content: { flex: 1, flexDirection: 'row', alignItems: 'center' },
  icon: { marginRight: space.sm, width: ICON_SIZE, alignItems: 'center' },
  name: { flex: 1, fontFamily: fonts.medium },
});
