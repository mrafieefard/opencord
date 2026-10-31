import { memo } from 'react';
import { StyleSheet, View } from 'react-native';
import { ChannelRecord } from '@opencord/core-native';

import { Icon, Row, Text, space, useTheme } from '@/ui';
import { fonts } from '@/ui/theme/typography';

export interface VoiceChannelRowProps {
  channel: ChannelRecord;
  count: number;
  limit: number;
  selected?: boolean;
  onSelect(channel: ChannelRecord): void;
}

export const VoiceChannelRow = memo(function VoiceChannelRow({
  channel,
  count,
  limit,
  selected = false,
  onSelect,
}: VoiceChannelRowProps) {
  const { colors } = useTheme();
  const countText = limit > 0 ? `${count}/${limit}` : count > 0 ? `${count}` : undefined;

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
      <View style={styles.content}>
        <View style={styles.icon}>
          <Icon name="voiceChannel" size={18} color="textSecondary" />
        </View>
        <Text
          variant="body"
          color={selected ? 'text' : 'textSecondary'}
          numberOfLines={1}
          style={styles.name}
        >
          {channel.name}
        </Text>
        {countText !== undefined && (
          <Text variant="callout" color="textMuted" testID="voice-channel-count">
            {countText}
          </Text>
        )}
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
});
