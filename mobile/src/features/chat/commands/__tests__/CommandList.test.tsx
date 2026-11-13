import { act, fireEvent, render } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { Keyboard, StyleSheet } from 'react-native';
import * as Reanimated from 'react-native-reanimated';
import type { CommandRecord } from '@opencord/core-native';

import { NIGHT_OWLS } from '@/core/fake/world';
import { freshStore } from '@/test/store';

import { ChatDockContext } from '../../dock';
import { CommandList, LIST_SHARE } from '../CommandList';
import type { CommandGroup, CommandRow } from '../commandRows';

const spec = (name: string, feature: string): CommandRecord => ({
  name,
  description: `About ${name}`,
  options: [],
  feature,
  source: feature.startsWith('plugin:') ? feature : '',
  permissionSummary: '',
  aliases: [],
  needs: [],
  enabled: true,
  channelIds: [],
});

function group(id: string, name: string, count: number): CommandGroup {
  const source = { id, name };
  return {
    source,
    rows: Array.from({ length: count }, (_, index): CommandRow => ({
      kind: 'command',
      key: `${id}/c${index}`,
      spec: spec(`c${index}`, id === '' ? 'music' : id),
      source,
    })),
  };
}

const BODY = 700;
const DOCK = 60;

function list(groups: CommandGroup[], onPick = jest.fn(), onDismiss = jest.fn()) {
  return (
    <GestureHandlerRootView>
      <ChatDockContext.Provider value={{ height: DOCK, bodyHeight: BODY }}>
        <CommandList serverKey={NIGHT_OWLS} groups={groups} onPick={onPick} onDismiss={onDismiss} emptyText="No commands match /x" />
      </ChatDockContext.Provider>
    </GestureHandlerRootView>
  );
}

const panelStyle = (element: { props: Record<string, unknown> }) => StyleSheet.flatten(element.props.style as object) as { height: number; position: string; bottom: unknown };

describe('CommandList', () => {
  beforeEach(() => freshStore());
  afterEach(() => jest.restoreAllMocks());

  it('the_source_row_shows_only_with_two_sources', async () => {
    const one = await render(list([group('', 'Opencord', 2)]));
    expect(one.queryByTestId('command-source-0')).toBeNull();

    const two = await render(list([group('', 'Opencord', 2), group('plugin:a', 'A', 2)]));
    expect(two.getByTestId('command-source-1')).toBeTruthy();
  });

  it('tapping_a_source_scrolls_to_it', async () => {
    const { getByTestId } = await render(list([group('', 'Opencord', 2), group('plugin:a', 'A', 2)]));

    await fireEvent.press(getByTestId('command-source-1'));

    expect(getByTestId('command-source-1').props.accessibilityState.selected).toBe(true);
    expect(getByTestId('command-source-0').props.accessibilityState.selected).toBe(false);
  });

  it('scrolling_moves_the_highlight', async () => {
    const groups = [group('', 'Opencord', 2), group('plugin:a', 'A', 2)];
    const { getByTestId } = await render(list(groups));
    const row = groups[1]?.rows[0];

    await fireEvent(getByTestId('command-list-rows'), 'viewableItemsChanged', {
      viewableItems: [{ item: { type: 'row', key: 'x', group: 1, row } }],
    });

    expect(getByTestId('command-source-1').props.accessibilityState.selected).toBe(true);
  });

  it('never_taller_than_75_percent_of_the_room', async () => {
    expect(LIST_SHARE).toBe(0.75);
    const listeners: Record<string, (event: { endCoordinates: { height: number } }) => void> = {};
    jest.spyOn(Keyboard, 'addListener').mockImplementation(((name: string, listener: never) => {
      listeners[name] = listener;
      return { remove: jest.fn() };
    }) as never);
    const { getByTestId } = await render(list([group('', 'Opencord', 500)]));
    expect(panelStyle(getByTestId('command-list')).height).toBe((BODY - DOCK) * 0.75);

    await act(async () => listeners.keyboardWillShow?.({ endCoordinates: { height: 300 } }));

    expect(panelStyle(getByTestId('command-list')).height).toBe((BODY - DOCK - 300) * 0.75);
  });

  it('a_short_list_is_only_as_tall_as_its_rows', async () => {
    const { getByTestId } = await render(list([group('', 'Opencord', 2)]));

    expect(panelStyle(getByTestId('command-list')).height).toBe(29 + 2 * 56);
  });

  it('opening_filtering_and_closing_keep_the_list_and_composer_in_place', async () => {
    const { getByTestId, rerender } = await render(list([group('', 'Opencord', 5)]));
    const style = panelStyle(getByTestId('command-list'));

    expect(style.position).toBe('absolute');
    expect(style.bottom).toBe('100%');
    await rerender(list([group('', 'Opencord', 1)]));
    expect(panelStyle(getByTestId('command-list')).position).toBe('absolute');
  });

  it('commands_changing_while_open_keep_working', async () => {
    const onPick = jest.fn();
    const groups = [group('', 'Opencord', 1), group('plugin:a', 'A', 1), group('plugin:b', 'B', 1)];
    const { getByTestId, rerender, getByLabelText } = await render(list(groups, onPick));
    await fireEvent.press(getByTestId('command-source-2'));

    const fewer = [group('', 'Opencord', 1)];
    await rerender(list(fewer, onPick));
    await fireEvent.press(getByLabelText('/c0'));

    expect(onPick).toHaveBeenCalledWith(fewer[0]?.rows[0]);
  });

  it('nothing_to_list_shows_the_empty_row', async () => {
    const { getByText } = await render(list([]));

    expect(getByText('No commands match /x')).toBeTruthy();
  });

  it('reduced_motion_has_no_animation', async () => {
    jest.spyOn(Reanimated, 'useReducedMotion').mockReturnValue(true);

    const { getByTestId } = await render(list([group('', 'Opencord', 1)]));

    expect(getByTestId('command-list').props.entering).toBeUndefined();
    expect(getByTestId('command-list').props.exiting).toBeUndefined();
  });

  it('the_keyboard_going_away_closes_it', async () => {
    const listeners: Record<string, () => void> = {};
    jest.spyOn(Keyboard, 'addListener').mockImplementation(((name: string, listener: never) => {
      listeners[name] = listener;
      return { remove: jest.fn() };
    }) as never);
    const onDismiss = jest.fn();
    await render(list([group('', 'Opencord', 1)], jest.fn(), onDismiss));

    await act(async () => listeners.keyboardDidHide?.());

    expect(onDismiss).toHaveBeenCalled();
  });
});
