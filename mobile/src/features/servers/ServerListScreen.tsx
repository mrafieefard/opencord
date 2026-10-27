// The Servers tab screen (spec §8.2):
// - FlashList of server rows
// - Native search bar filtering by name
// - Pull to refresh retrying disconnected servers
// - Empty state offering "Add a server"

import { FlashList } from '@shopify/flash-list';
import { router, useNavigation } from 'expo-router';
import { useCallback, useLayoutEffect, useState } from 'react';
import {
  type NativeSyntheticEvent,
  RefreshControl,
  StyleSheet,
  View,
} from 'react-native';

import { retryDisconnected } from '@/state/actions/servers';
import { useServerRows } from '@/state/selectors';
import type { ServerRow as ServerRowType } from '@/state/types';
import { EmptyState, useTheme } from '@/ui';
import { ServerRow } from './ServerRow';

export interface ServerListScreenProps {
  query?: string;
}

export function ServerListScreen({ query: queryProp }: ServerListScreenProps) {
  const [search, setSearch] = useState('');
  const [refreshing, setRefreshing] = useState(false);
  const { colors } = useTheme();
  const navigation = useNavigation();

  const query = queryProp ?? search;
  const rows = useServerRows(query);

  useLayoutEffect(() => {
    navigation.setOptions({
      headerSearchBarOptions: {
        placeholder: 'Search',
        hideWhenScrolling: false,
        onChangeText: (event: NativeSyntheticEvent<{ text: string }>) => {
          setSearch(event.nativeEvent.text);
        },
        onCancelButtonPress: () => {
          setSearch('');
        },
      },
    });
  }, [navigation]);

  const onRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await retryDisconnected();
    } finally {
      setRefreshing(false);
    }
  }, []);

  const renderItem = useCallback(
    ({ item }: { item: ServerRowType }) => (
      <ServerRow
        row={item}
        onPress={() => {
          router.push(`/server/${item.key}`);
        }}
      />
    ),
    [],
  );

  if (rows.length === 0 && !query) {
    return (
      <View style={[styles.container, { backgroundColor: colors.sidebar }]}>
        <EmptyState
          title="No servers yet"
          body="Add one with an invite link or its address."
          action={{
            title: 'Add a server',
            onPress: () => router.push('/add-server'),
          }}
        />
      </View>
    );
  }

  return (
    <View style={[styles.container, { backgroundColor: colors.sidebar }]}>
      <FlashList
        testID="server-list"
        data={rows}
        keyExtractor={(item) => item.key}
        renderItem={renderItem}
        refreshControl={
          <RefreshControl
            refreshing={refreshing}
            onRefresh={onRefresh}
            tintColor={colors.accent}
          />
        }
        ListEmptyComponent={
          query ? (
            <EmptyState
              title="No servers found"
              body={`No server matching "${query}"`}
            />
          ) : null
        }
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
});
