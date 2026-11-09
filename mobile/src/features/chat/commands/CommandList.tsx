// The slash command list (design §26.7): floating over the messages, just
// above the composer, it adds nothing to the dock's height, so opening,
// filtering and closing never move the messages or the composer. Above the
// source row, one list of commands with sticky group headers; a long one
// scrolls inside it.
import { FlashList, type FlashListRef, type ListRenderItem, type ViewToken } from '@shopify/flash-list';
import * as Haptics from 'expo-haptics';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { BackHandler, Keyboard, Platform, StyleSheet, View, useWindowDimensions } from 'react-native';
import { Touchable } from 'react-native-gesture-handler';
import Animated, { Easing, Keyframe, useReducedMotion } from 'react-native-reanimated';

import { Text, space, useTheme } from '@/ui';

import { useChatDock } from '../dock';
import { flatRows, headerIndex, type CommandGroup, type CommandListItem, type CommandRow } from './commandRows';
import { SourceIcon } from './SourceIcon';
import { SOURCE_ROW_HEIGHT, SourceRow } from './SourceRow';

/** How much of the room between the header and the composer it may take. */
export const LIST_SHARE = 0.75;
export const LIST_ROW_HEIGHT = 56;
export const LIST_HEADER_HEIGHT = 29;
const EMPTY_HEIGHT = LIST_ROW_HEIGHT;

const OPEN_MS = 180;
const CLOSE_MS = 120;
const RISE = 8;
const FALL = 6;

const entering = new Keyframe({
  0: { opacity: 0, transform: [{ translateY: RISE }] },
  100: { opacity: 1, transform: [{ translateY: 0 }], easing: Easing.bezier(0.2, 0.8, 0.2, 1) },
}).duration(OPEN_MS);

const exiting = new Keyframe({
  0: { opacity: 1, transform: [{ translateY: 0 }] },
  100: { opacity: 0, transform: [{ translateY: FALL }], easing: Easing.in(Easing.ease) },
}).duration(CLOSE_MS);

/** The list's height: its rows', up to its share of the room. */
export function listHeight(contentHeight: number, room: number): number {
  return Math.max(0, Math.min(contentHeight, room * LIST_SHARE));
}

function useKeyboardHeight(): number {
  // Often already up when the list opens, so no event says so.
  const [height, setHeight] = useState(() => Keyboard.metrics?.()?.height ?? 0);
  useEffect(() => {
    const showing = Platform.OS === 'ios' ? 'keyboardWillShow' : 'keyboardDidShow';
    const shown = Keyboard.addListener(showing, (event) => setHeight(event.endCoordinates.height));
    const hidden = Keyboard.addListener('keyboardDidHide', () => setHeight(0));
    return () => {
      shown.remove();
      hidden.remove();
    };
  }, []);
  return height;
}

export interface CommandListProps {
  serverKey: string;
  groups: readonly CommandGroup[];
  /** Shown when there's nothing to list. */
  emptyText?: string;
  onPick: (row: CommandRow) => void;
  onDismiss: () => void;
}

export function CommandList({ serverKey, groups, emptyText, onPick, onDismiss }: CommandListProps) {
  const { colors } = useTheme();
  const reduced = useReducedMotion();
  const window = useWindowDimensions();
  const dock = useChatDock();
  const keyboard = useKeyboardHeight();
  const list = useRef<FlashListRef<CommandListItem>>(null);
  const [selected, setSelected] = useState(0);
  // Keeps the highlight on a tapped source the list can't bring to the top.
  const tapped = useRef<number | null>(null);

  const items = useMemo(() => flatRows(groups), [groups]);
  const stickyHeaders = useMemo(
    () => items.flatMap((item, index) => (item.type === 'header' ? [index] : [])),
    [items],
  );
  const showSources = groups.length >= 2;
  const lastGroup = groups.length - 1;
  if (selected > Math.max(lastGroup, 0)) setSelected(0);

  // Android's back and the keyboard going away close it, before anything else.
  useEffect(() => {
    const back = BackHandler.addEventListener('hardwareBackPress', () => {
      onDismiss();
      return true;
    });
    const hidden = Keyboard.addListener('keyboardDidHide', onDismiss);
    return () => {
      back.remove();
      hidden.remove();
    };
  }, [onDismiss]);

  const selectSource = useCallback(
    (group: number) => {
      tapped.current = group;
      setSelected(group);
      list.current?.scrollToIndex({ index: headerIndex(items, group), animated: false });
    },
    [items],
  );

  const onViewable = useCallback(({ viewableItems }: { viewableItems: ViewToken<CommandListItem>[] }) => {
    const first = viewableItems.find((token) => token.item.type === 'row')?.item;
    if (!first) return;
    if (tapped.current !== null) {
      tapped.current = null;
      return;
    }
    setSelected(first.group);
  }, []);

  const pick = useCallback(
    (row: CommandRow) => {
      void Haptics.selectionAsync();
      onPick(row);
    },
    [onPick],
  );

  const renderItem: ListRenderItem<CommandListItem> = useCallback(
    ({ item }) =>
      item.type === 'header' ? (
        <View style={[styles.header, { backgroundColor: colors.elevated }]}>
          <Text variant="footnote" color="textMuted" numberOfLines={1} style={styles.headerText}>
            {item.title}
          </Text>
        </View>
      ) : (
        <CommandRowView serverKey={serverKey} row={item.row} onPress={pick} />
      ),
    [colors.elevated, serverKey, pick],
  );

  const contentHeight =
    items.length === 0
      ? EMPTY_HEIGHT
      : groups.reduce((sum, group) => sum + LIST_HEADER_HEIGHT + group.rows.length * LIST_ROW_HEIGHT, 0) +
        (showSources ? SOURCE_ROW_HEIGHT : 0);
  const room = (dock.bodyHeight ?? window.height) - dock.height - keyboard;
  const height = listHeight(contentHeight, room);

  return (
    <Animated.View
      entering={reduced ? undefined : entering}
      exiting={reduced ? undefined : exiting}
      testID="command-list"
      style={[
        styles.panel,
        { height, backgroundColor: colors.elevated, borderColor: colors.border },
      ]}
    >
      {showSources && <SourceRow serverKey={serverKey} groups={groups} selected={selected} onSelect={selectSource} />}
      {items.length === 0 ? (
        <Text variant="footnote" color="textMuted" numberOfLines={1} style={styles.empty}>
          {emptyText ?? ''}
        </Text>
      ) : (
        <FlashList
          ref={list}
          testID="command-list-rows"
          data={items}
          renderItem={renderItem}
          getItemType={(item) => item.type}
          keyExtractor={(item) => item.key}
          stickyHeaderIndices={stickyHeaders}
          keyboardShouldPersistTaps="always"
          keyboardDismissMode="none"
          onViewableItemsChanged={onViewable}
          showsVerticalScrollIndicator={false}
        />
      )}
    </Animated.View>
  );
}

function CommandRowView({
  serverKey,
  row,
  onPress,
}: {
  serverKey: string;
  row: CommandRow;
  onPress: (row: CommandRow) => void;
}) {
  const title = row.kind === 'command' ? `/${row.spec.name}` : row.choice.name;
  const detail = row.kind === 'command' ? row.unavailable ?? row.spec.description : undefined;
  const dim = row.kind === 'command' && row.unavailable !== undefined;
  return (
    <Touchable onPress={() => onPress(row)} style={styles.row}>
      <View style={styles.rowInner} accessible accessibilityRole="button" accessibilityLabel={title}>
        {row.kind === 'command' && <SourceIcon serverKey={serverKey} source={row.source} size={32} radius={9} />}
        <View style={styles.rowText}>
          <Text variant="callout" numberOfLines={1} style={[styles.name, dim && styles.dim]}>
            {title}
          </Text>
          {detail !== undefined && (
            <Text variant="footnote" color="textMuted" numberOfLines={1}>
              {detail}
            </Text>
          )}
        </View>
      </View>
    </Touchable>
  );
}

const styles = StyleSheet.create({
  panel: {
    position: 'absolute',
    left: space.sm,
    right: space.sm,
    bottom: '100%',
    marginBottom: 2,
    borderRadius: 20,
    borderWidth: StyleSheet.hairlineWidth,
    overflow: 'hidden',
    shadowColor: '#000',
    shadowOpacity: 0.25,
    shadowRadius: 16,
    shadowOffset: { width: 0, height: 8 },
    elevation: 10,
  },
  header: { height: LIST_HEADER_HEIGHT, justifyContent: 'flex-end', paddingHorizontal: 14, paddingBottom: 5 },
  headerText: { fontWeight: '600' },
  row: { height: LIST_ROW_HEIGHT },
  rowInner: { flex: 1, flexDirection: 'row', alignItems: 'center', gap: 11, paddingHorizontal: 14 },
  rowText: { flex: 1, minWidth: 0 },
  name: { fontWeight: '600' },
  dim: { opacity: 0.5 },
  empty: { paddingHorizontal: 14, paddingVertical: 19 },
});
