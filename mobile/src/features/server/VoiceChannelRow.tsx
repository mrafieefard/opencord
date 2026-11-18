import { memo } from 'react';
import { StyleSheet, View } from 'react-native';
import { ChannelRecord } from '@opencord/core-native';

import { Badge, Icon, Row, Text, space, useTheme } from '@/ui';
import { fonts } from '@/ui/theme/typography';

export interface VoiceChannelRowProps {
  channel: ChannelRecord;
  count: number;
  limit: number;
  unread?: number;
  mentions?: number;
  selected?: boolean;
  onSelect(channel: ChannelRecord): void;
}

export const VoiceChannelRow = memo(function VoiceChannelRow({
  channel,
  count,
  limit,
  unread = 0,
  mentions = 0,
  selected = false,
  onSelect,
}: VoiceChannelRowProps) {
  const { colors } = useTheme();
  const countText = limit > 0 ? `${count}/${limit}` : count > 0 ? `${count}` : undefined;
  const isUnread = unread > 0;

  return (
    <Row
      onPress={() => onSelect(channel)}
      selected={selected}
      accessibilityRole="button"
      accessibilityLabel={channel.name}
      style={[
        styles.row,
        selected && { backgroundColor: colors.selected, borderRadius: 8 },
      ]}
    >
      {isUnread && (
        <View
          testID="channel-unread-pill"
          style={[styles.pill, { backgroundColor: colors.text }]}
        />
      )}
      <View style={styles.content}>
        <View style={styles.icon}>
          <Icon name="voiceChannel" size={18} color={isUnread ? 'text' : 'textSecondary'} />
        </View>
        <Text
          variant="body"
          color={isUnread || selected ? 'text' : 'textSecondary'}
          numberOfLines={1}
          style={[styles.name, isUnread && styles.unreadName]}
        >
          {channel.name}
        </Text>
        {countText !== undefined && (
          <Text variant="callout" color="textMuted" testID="voice-channel-count">
            {countText}
          </Text>
        )}
        {mentions > 0 && <Badge count={mentions} testID="channel-mentions-badge" />}
      </View>
    </Row>
  );
});

const styles = StyleSheet.create({
  row: {
    minHeight: 44,
    justifyContent: 'center',
    marginHorizontal: space.sm,
    paddingHorizontal: space.sm,
  },
  pill: {
    position: 'absolute',
    left: 0,
    width: 4,
    height: 8,
    borderRadius: 2,
  },
  content: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
  },
  icon: {
    marginRight: space.sm,
  },
  name: {
    flex: 1,
    fontFamily: fonts.medium,
  },
  unreadName: {
    fontFamily: fonts.semiBold,
  },
});
