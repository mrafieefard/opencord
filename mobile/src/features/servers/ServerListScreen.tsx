// The Servers tab screen (spec §8.2):
// - the server rows (ServerList: a FlashList on Android, a native list on iOS)
// - Native search bar filtering by name
// - Pull to refresh retrying disconnected servers
// - Empty state offering "Add a server"

import { router, useNavigation } from 'expo-router';
import { useCallback, useLayoutEffect, useState } from 'react';
import { type NativeSyntheticEvent, StyleSheet, View } from 'react-native';

import { ConnectedBar } from '@/features/voice/ConnectedBar';
import { retryDisconnected } from '@/state/actions/servers';
import { useServerRows } from '@/state/selectors';
import { EmptyState, useTheme } from '@/ui';

import { deviceLocale } from './formatTime';
import { ServerList } from './ServerList';
import { useMinuteClock } from './useMinuteClock';

const openServer = (serverKey: string) => router.push(`/server/${serverKey}`);

export interface ServerListScreenProps {
  /** A search field in the header filters the list. */
  searchable: boolean;
  /** The voice bar at the screen's foot while in voice. */
  voiceBar: boolean;
}

export function ServerListScreen({ searchable, voiceBar }: ServerListScreenProps) {
  const [query, setQuery] = useState('');
  const [refreshing, setRefreshing] = useState(false);
  const { colors } = useTheme();
  const navigation = useNavigation();
  const rows = useServerRows(query);
  const nowMs = useMinuteClock();
  const locale = deviceLocale();

  useLayoutEffect(() => {
    if (!searchable) return;
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
  }, [navigation, searchable]);

  const onRefresh = useCallback(async () => {
    setRefreshing(true);
    try {
      await retryDisconnected();
    } finally {
      setRefreshing(false);
    }
  }, []);



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
        {voiceBar && <ConnectedBar />}
      </View>
    );
  }

  const list = (
    <ServerList
      rows={rows}
      nowMs={nowMs}
      locale={locale}
      onOpen={openServer}
      refreshing={refreshing}
      onRefresh={onRefresh}
      emptyTitle={query ? 'No servers found' : undefined}
    />
  );
  // The list is the screen's first view when nothing goes under it: iOS links
  // the large title to the first scroll view it finds that way, so it then
  // follows a pull down and shrinks into the bar as the list scrolls.
  if (!voiceBar) return list;
  return (
    <View style={[styles.container, { backgroundColor: colors.sidebar }]}>
      {list}
      <ConnectedBar />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
  },
});
