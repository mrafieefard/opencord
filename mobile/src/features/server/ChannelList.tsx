// A server's channels (spec §8.4), full screen in `channels` and as the
// sidebar beside a channel. Only this server's channels: never servers.
import { FlashList } from '@shopify/flash-list';
import type { ChannelRecord, MusicNowPlayingRecord } from '@opencord/core-native';
import { useCallback, useMemo } from 'react';
import { StyleSheet, View } from 'react-native';

import { useChannelSections } from '@/state/selectors';
import type { ChannelItem, VoiceMember } from '@/state/types';
import { useTheme } from '@/ui';

import { CategoryHeader } from './CategoryHeader';
import { ChannelRow } from './ChannelRow';
import { NowPlayingRow } from './NowPlayingRow';
import { VoiceChannelRow } from './VoiceChannelRow';
import { VoiceMemberRow } from './VoiceMemberRow';

export interface ChannelListProps {
  serverKey: string;
  variant: 'full' | 'sidebar';
  selectedId?: string;
  onSelect(channel: ChannelRecord): void;
}

type ListItem =
  | { type: 'category'; id: string; name: string }
  | { type: 'text'; id: string; item: Extract<ChannelItem, { kind: 'text' }> }
  | { type: 'voice'; id: string; item: Extract<ChannelItem, { kind: 'voice' }> }
  | { type: 'member'; id: string; member: VoiceMember }
  | { type: 'nowPlaying'; id: string; nowPlaying: MusicNowPlayingRecord };

export function ChannelList({ serverKey, variant, selectedId, onSelect }: ChannelListProps) {
  const { colors } = useTheme();
  const sections = useChannelSections(serverKey);
  const highlighted = variant === 'sidebar' ? selectedId : undefined;

  const items = useMemo(() => {
    const result: ListItem[] = [];
    for (const section of sections) {
      if (section.category) {
        result.push({
          type: 'category',
          id: `cat-${section.category.id}`,
          name: section.category.name,
        });
      }
      for (const item of section.items) {
        if (item.kind === 'text') {
          result.push({
            type: 'text',
            id: `text-${item.channel.id}`,
            item,
          });
        } else if (item.kind === 'voice') {
          result.push({
            type: 'voice',
            id: `voice-${item.channel.id}`,
            item,
          });
          for (const member of item.members) {
            result.push({
              type: 'member',
              id: `member-${item.channel.id}-${member.userId}`,
              member,
            });
          }
          if (item.nowPlaying) {
            result.push({
              type: 'nowPlaying',
              id: `nowPlaying-${item.channel.id}`,
              nowPlaying: item.nowPlaying,
            });
          }
        }
      }
    }
    return result;
  }, [sections]);

  const renderItem = useCallback(
    ({ item }: { item: ListItem }) => {
      switch (item.type) {
        case 'category':
          return <CategoryHeader name={item.name} />;
        case 'text':
          return (
            <ChannelRow
              channel={item.item.channel}
              unread={item.item.unread}
              mentions={item.item.mentions}
              selected={item.item.channel.id === highlighted}
              onSelect={onSelect}
            />
          );
        case 'voice':
          return (
            <VoiceChannelRow
              channel={item.item.channel}
              count={item.item.count}
              limit={item.item.limit}
              selected={item.item.channel.id === highlighted}
              onSelect={onSelect}
            />
          );
        case 'member':
          return <VoiceMemberRow serverKey={serverKey} member={item.member} />;
        case 'nowPlaying':
          return <NowPlayingRow nowPlaying={item.nowPlaying} />;
      }
    },
    [highlighted, onSelect, serverKey],
  );

  return (
    <View style={[styles.container, { backgroundColor: colors.sidebar }]}>
      <FlashList
        data={items}
        renderItem={renderItem}
        getItemType={(item) => item.type}
        keyExtractor={(item) => item.id}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
});
