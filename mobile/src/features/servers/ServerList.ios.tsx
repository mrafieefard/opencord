// The server rows on iOS, as Apple's own lists: a SwiftUI List, so a swipe
// shows the system's swipe actions (Read leading, Leave trailing, a full
// swipe acting), a long press the system's context menu, and a pull the
// system's refresh. Each row's look is the app's (ServerRowContent), hosted.
import { Button, ContextMenu, Host, List, RNHostView, SwipeActions } from '@expo/ui/swift-ui';
import { listRowBackground, listRowInsets, listRowSeparator, listStyle, onTapGesture, refreshable, scrollContentBackground, tint } from '@expo/ui/swift-ui/modifiers';
import { memo, useState } from 'react';
import { StyleSheet, View } from 'react-native';

import type { ServerRow as ServerRowType } from '@/state/types';
import { Divider, EmptyState, space, useTheme } from '@/ui';
import { icons } from '@/ui/icons';

import type { ServerListProps } from './ServerList.types';
import { ROW_HEIGHT, ROW_INSET, ServerRowContent, rowText, useServerRowActions } from './serverRowParts';

export type { ServerListProps } from './ServerList.types';

interface NativeServerRowProps {
  row: ServerRowType;
  nowMs: number;
  locale: string;
  onOpen: (serverKey: string) => void;
  last: boolean;
  /** The list's width: hosted content takes none from its SwiftUI row. */
  width: number;
}

const NativeServerRow = memo(function NativeServerRow({ row, nowMs, locale, onOpen, last, width }: NativeServerRowProps) {
  const { colors } = useTheme();
  const { menuActions, handleAction, confirmLeave, markRead } = useServerRowActions(row);
  return (
    <SwipeActions
      testID={`server-menu-${row.key}`}
      modifiers={[
        listRowInsets({ top: 0, leading: 0, bottom: 0, trailing: 0 }),
        listRowBackground(colors.sidebar),
        // The hairline is the row's own, from after the icon (design §9.3).
        listRowSeparator('hidden'),
        onTapGesture(() => onOpen(row.key)),
      ]}
    >
      {/* SwiftUI takes every gesture: the tap, the hold for the menu, the swipes.
          The hosted row takes none, or it would keep the hold from the menu. */}
      <ContextMenu>
        <ContextMenu.Items>
          {menuActions.map((action) => (
            <Button
              key={action.id}
              label={action.title}
              systemImage={action.icon ? icons[action.icon].ios : undefined}
              role={action.destructive ? 'destructive' : 'default'}
              onPress={() => handleAction(action.id)}
            />
          ))}
        </ContextMenu.Items>
        <ContextMenu.Trigger>
          <RNHostView matchContents={{ vertical: true }}>
            <View style={{ width, backgroundColor: colors.sidebar }}>
              <View
                pointerEvents="none"
                accessible
                accessibilityRole="button"
                accessibilityLabel={rowText(row, nowMs, locale).label}
                onAccessibilityTap={() => onOpen(row.key)}
                style={styles.row}
              >
                <ServerRowContent row={row} nowMs={nowMs} locale={locale} />
              </View>
              {!last && <Divider inset={ROW_INSET} />}
            </View>
          </RNHostView>
        </ContextMenu.Trigger>
      </ContextMenu>
      <SwipeActions.Actions edge="leading">
        <Button label="Read" systemImage={icons.check.ios} onPress={markRead} />
      </SwipeActions.Actions>
      <SwipeActions.Actions edge="trailing">
        {/* Red, not the destructive role: SwiftUI removes a row whose
            destructive swipe action is tapped, before the question is answered. */}
        <Button label="Leave" systemImage={icons.leaveServer.ios} modifiers={[tint(colors.recording)]} onPress={confirmLeave} />
      </SwipeActions.Actions>
    </SwipeActions>
  );
});

export function ServerList({ rows, nowMs, locale, onOpen, onRefresh, emptyTitle }: ServerListProps) {
  const { colors, scheme } = useTheme();
  const [width, setWidth] = useState<number>();
  // The List fills the host, so its size is the content's.
  const measure = (event: { nativeEvent: { width: number } }) => {
    const measured = Math.round(event.nativeEvent.width);
    if (measured > 0 && measured !== width) setWidth(measured);
  };
  if (rows.length === 0 && emptyTitle) {
    return (
      <View style={[styles.empty, { backgroundColor: colors.sidebar }]}>
        <EmptyState title={emptyTitle} />
      </View>
    );
  }
  return (
    <Host testID="server-list" colorScheme={scheme} onLayoutContent={measure} style={[styles.fill, { backgroundColor: colors.sidebar }]}>
      <List modifiers={[listStyle('plain'), scrollContentBackground('hidden'), refreshable(onRefresh)]}>
        {width !== undefined && rows.map((row, index) => (
          <NativeServerRow
            key={row.key}
            row={row}
            nowMs={nowMs}
            locale={locale}
            onOpen={onOpen}
            last={index === rows.length - 1}
            width={width}
          />
        ))}
      </List>
    </Host>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  empty: { flex: 1, paddingTop: space.xxxl },
  row: { height: ROW_HEIGHT, paddingHorizontal: space.lg, justifyContent: 'center' },
});
