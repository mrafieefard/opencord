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
import { Divider, EmptyState, useTheme } from '@/ui';
import { ConnectedBar } from '@/features/voice/ConnectedBar';

import { deviceLocale } from './formatTime';
import { ROW_INSET, ServerRow } from './ServerRow';
import { useMinuteClock } from './useMinuteClock';

const openServer = (serverKey: string) => router.push(`/server/${serverKey}`);
const Separator = () => <Divider inset={ROW_INSET} />;

export function ServerListScreen() {
  const [query, setQuery] = useState('');
  const [refreshing, setRefreshing] = useState(false);
  const { colors } = useTheme();
  const navigation = useNavigation();
  const rows = useServerRows(query);
  const nowMs = useMinuteClock();
  const locale = deviceLocale();

  useLayoutEffect(() => {
    navigation.setOptions({
      headerSearchBarOptions: {
        placeholder: 'Search',
        hideWhenScrolling: false,
        onChangeText: (event: NativeSyntheticEvent<{ text: string }>) => {
          setQuery(event.nativeEvent.text);
        },
        onCancelButtonPress: () => {
          setQuery('');
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
    ({ item }: { item: ServerRowType }) => <ServerRow row={item} nowMs={nowMs} locale={locale} onOpen={openServer} />,
    [nowMs, locale],
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
        <ConnectedBar />
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
        ItemSeparatorComponent={Separator}
        contentInsetAdjustmentBehavior="automatic"
        keyboardDismissMode="on-drag"
        refreshControl={
          <RefreshControl
            refreshing={refreshing}
            onRefresh={onRefresh}
            tintColor={colors.accent}
          />
        }
        ListEmptyComponent={
          query ? <EmptyState title="No servers found" /> : null
        }
      />
      <ConnectedBar />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
});
