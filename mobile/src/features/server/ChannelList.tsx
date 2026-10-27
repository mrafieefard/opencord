// A server's channels (spec §8.4), full screen in `channels` and as the
// sidebar beside a channel. Only this server's channels: never servers.
// Rows here have their icon, name and selection; Task 12 adds the rest.
import { FlashList } from '@shopify/flash-list';
import { ChannelKind, type ChannelRecord } from '@opencord/core-native';
import { memo, useCallback, useMemo } from 'react';
import { StyleSheet, View } from 'react-native';

import { useChannelSections } from '@/state/selectors';
import type { ChannelItem } from '@/state/types';
import { Icon, Row, Text, space, useTheme } from '@/ui';

export interface ChannelListProps {
  serverKey: string;
  variant: 'full' | 'sidebar';
  selectedId?: string;
  onSelect(channel: ChannelRecord): void;
}

interface ChannelRowProps {
  channel: ChannelRecord;
  selected: boolean;
  onSelect(channel: ChannelRecord): void;
}

const ChannelRow = memo(function ChannelRow({ channel, selected, onSelect }: ChannelRowProps) {
  const voice = channel.kind === ChannelKind.Voice;
  return (
    <Row onPress={() => onSelect(channel)} selected={selected} accessibilityLabel={channel.name} style={styles.row}>
      <View style={styles.icon}>
        <Icon name={voice ? 'voiceChannel' : 'textChannel'} size={18} color="textSecondary" />
      </View>
      <Text variant="body" color={selected ? 'text' : 'textSecondary'} numberOfLines={1} style={styles.name}>
        {channel.name}
      </Text>
    </Row>
  );
});

export function ChannelList({ serverKey, variant, selectedId, onSelect }: ChannelListProps) {
  const { colors } = useTheme();
  const sections = useChannelSections(serverKey);
  const items = useMemo(() => sections.flatMap((section) => section.items), [sections]);
  const highlighted = variant === 'sidebar' ? selectedId : undefined;

  const renderItem = useCallback(
    ({ item }: { item: ChannelItem }) => (
      <ChannelRow channel={item.channel} selected={item.channel.id === highlighted} onSelect={onSelect} />
    ),
    [highlighted, onSelect],
  );

  return (
    <View style={[styles.list, { backgroundColor: colors.sidebar }]}>
      <FlashList
        data={items}
        keyExtractor={(item) => item.channel.id}
        renderItem={renderItem}
        extraData={highlighted}
        contentContainerStyle={styles.content}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  list: { flex: 1 },
  content: { paddingVertical: space.sm },
  row: { marginHorizontal: space.sm, paddingHorizontal: space.sm },
  icon: { width: 24, alignItems: 'center', marginRight: space.sm },
  name: { flex: 1 },
});
