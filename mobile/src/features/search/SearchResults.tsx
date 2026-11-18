import { ChannelKind } from '@opencord/core-native';
import { router } from 'expo-router';
import { useMemo } from 'react';
import { StyleSheet, View } from 'react-native';
import { ScrollView } from 'react-native-gesture-handler';

import { useImageFile } from '@/features/servers/useImageFile';
import { useAppStore } from '@/state/store';
import {
  Avatar,
  Icon,
  Row,
  ServerIcon,
  Text,
  radius,
  space,
  useTheme,
} from '@/ui';

import {
  collectRecentChannels,
  collectSwitcherItems,
  rankSwitcher,
  type SwitcherChannelItem,
  type SwitcherItem,
  type SwitcherMemberItem,
  type SwitcherServerItem,
} from './searchResults';

export interface SearchResultsProps {
  query: string;
  onSelect?: (item: SwitcherItem) => void;
}

export function SearchResults({ query, onSelect }: SearchResultsProps) {
  const { colors } = useTheme();
  const servers = useAppStore((state) => state.servers);
  const order = useAppStore((state) => state.order);
  const serverInFront = useAppStore((state) => state.serverInFront);
  const messages = useAppStore((state) => state.messages);

  const items = useMemo(
    () => collectSwitcherItems({ servers, order, serverInFront }),
    [servers, order, serverInFront],
  );

  const recent = useMemo(
    () => collectRecentChannels({ messages }),
    [messages],
  );

  const results = useMemo(
    () => rankSwitcher(items, query, recent),
    [items, query, recent],
  );

  const handleSelect = (item: SwitcherItem) => {
    if (onSelect) {
      onSelect(item);
      return;
    }
    switch (item.kind) {
      case 'channel':
        router.push(`/servers/${item.serverKey}?channel=${item.channelId}`);
        break;
      case 'server':
        router.push(`/servers/${item.serverKey}`);
        break;
      case 'member':
        router.push(`/server/${item.serverKey}/member/${item.userId}`);
        break;
    }
  };

  if (results.length === 0) {
    return (
      <View style={styles.emptyContainer}>
        <Text variant="subhead" color="textMuted">
          No results found
        </Text>
      </View>
    );
  }

  return (
    <ScrollView
      testID="search-results-list"
      style={[styles.container, { backgroundColor: colors.sidebar }]}
      contentContainerStyle={styles.content}
      keyboardShouldPersistTaps="handled"
    >
      {results.map((item) => {
        const key =
          item.kind === 'channel'
            ? `ch:${item.serverKey}:${item.channelId}`
            : item.kind === 'server'
              ? `srv:${item.serverKey}`
              : `mem:${item.serverKey}:${item.userId}`;

        return (
          <SearchResultRow
            key={key}
            item={item}
            onPress={() => handleSelect(item)}
          />
        );
      })}
    </ScrollView>
  );
}

function SearchResultRow({
  item,
  onPress,
}: {
  item: SwitcherItem;
  onPress: () => void;
}) {
  switch (item.kind) {
    case 'channel':
      return <ChannelRow item={item} onPress={onPress} />;
    case 'server':
      return <ServerRowItem item={item} onPress={onPress} />;
    case 'member':
      return <MemberRowItem item={item} onPress={onPress} />;
  }
}

function ChannelRow({
  item,
  onPress,
}: {
  item: SwitcherChannelItem;
  onPress: () => void;
}) {
  const isVoice = item.channelKind === ChannelKind.Voice;
  const iconName = isVoice ? 'voiceChannel' : 'textChannel';

  return (
    <Row
      accessibilityRole="button"
      accessibilityLabel={`#${item.name}, ${item.serverName}`}
      onPress={onPress}
      height={52}
      style={styles.row}
    >
      <View style={styles.iconSlot}>
        <Icon name={iconName} size={20} color="textSecondary" />
      </View>
      <View style={styles.titles}>
        <Text variant="body" color="text" numberOfLines={1}>
          #{item.name}
        </Text>
        <Text variant="footnote" color="textMuted" numberOfLines={1}>
          {item.serverName}
        </Text>
      </View>
    </Row>
  );
}

function ServerRowItem({
  item,
  onPress,
}: {
  item: SwitcherServerItem;
  onPress: () => void;
}) {
  const iconFile = useImageFile(item.serverKey, item.iconHash);

  return (
    <Row
      accessibilityRole="button"
      accessibilityLabel={`${item.name}, Server`}
      onPress={onPress}
      height={52}
      style={styles.row}
    >
      <View style={styles.iconSlot}>
        <ServerIcon name={item.name} file={iconFile} size={32} />
      </View>
      <View style={styles.titles}>
        <Text variant="body" color="text" numberOfLines={1}>
          {item.name}
        </Text>
        <Text variant="footnote" color="textMuted" numberOfLines={1}>
          Server
        </Text>
      </View>
    </Row>
  );
}

function MemberRowItem({
  item,
  onPress,
}: {
  item: SwitcherMemberItem;
  onPress: () => void;
}) {
  const avatarFile = useImageFile(item.serverKey, item.avatarHash);
  const subtitle = item.nickname ? `@${item.nickname}` : 'Member';

  return (
    <Row
      accessibilityRole="button"
      accessibilityLabel={`${item.name}, ${subtitle}`}
      onPress={onPress}
      height={52}
      style={styles.row}
    >
      <View style={styles.iconSlot}>
        <Avatar name={item.name} file={avatarFile} size={32} />
      </View>
      <View style={styles.titles}>
        <Text variant="body" color="text" numberOfLines={1}>
          {item.name}
        </Text>
        <Text variant="footnote" color="textMuted" numberOfLines={1}>
          {subtitle}
        </Text>
      </View>
    </Row>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
  content: {
    paddingVertical: space.xs,
  },
  emptyContainer: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    padding: space.xl,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: space.md,
    gap: space.md,
    borderRadius: radius.row,
  },
  iconSlot: {
    width: 32,
    height: 32,
    alignItems: 'center',
    justifyContent: 'center',
  },
  titles: {
    flex: 1,
    minWidth: 0,
    justifyContent: 'center',
  },
});
