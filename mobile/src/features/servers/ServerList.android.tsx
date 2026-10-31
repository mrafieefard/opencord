// The server rows on Android: a FlashList of swipeable rows, pulled to
// refresh (ServerList.ios draws iOS's natively).
import { FlashList } from '@shopify/flash-list';
import { useCallback } from 'react';
import { RefreshControl } from 'react-native';

import type { ServerRow as ServerRowType } from '@/state/types';
import { Divider, EmptyState, useTheme } from '@/ui';

import type { ServerListProps } from './ServerList.types';
import { ROW_INSET, ServerRow } from './ServerRow';

export type { ServerListProps } from './ServerList.types';

const Separator = () => <Divider inset={ROW_INSET} />;

export function ServerList({ rows, nowMs, locale, onOpen, refreshing, onRefresh, emptyTitle }: ServerListProps) {
  const { colors } = useTheme();
  const renderItem = useCallback(
    ({ item }: { item: ServerRowType }) => <ServerRow row={item} nowMs={nowMs} locale={locale} onOpen={onOpen} />,
    [nowMs, locale, onOpen],
  );
  return (
    <FlashList
      testID="server-list"
      data={rows}
      keyExtractor={(item) => item.key}
      renderItem={renderItem}
      ItemSeparatorComponent={Separator}
      contentInsetAdjustmentBehavior="automatic"
      maintainVisibleContentPosition={{ disabled: true }}
      keyboardDismissMode="on-drag"
      refreshControl={<RefreshControl refreshing={refreshing} onRefresh={() => void onRefresh()} tintColor={colors.accent} />}
      ListEmptyComponent={emptyTitle ? <EmptyState title={emptyTitle} /> : null}
    />
  );
}
