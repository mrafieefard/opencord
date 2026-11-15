import { fireEvent, render } from '@testing-library/react-native';
import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ScrollView } from 'react-native';
import { CommandOptionKindRecord, type CommandOptionRecord } from '@opencord/core-native';

import { freshStore } from '@/test/store';

import { CommandChips } from '../CommandChips';

const option = (name: string, required = false): CommandOptionRecord => ({
  name,
  description: 'd',
  kind: CommandOptionKindRecord.Text,
  required,
  autocomplete: false,
  choices: [],
  min: 0,
  max: 0,
});

const chips = (props: Partial<Parameters<typeof CommandChips>[0]> = {}) => (
  <GestureHandlerRootView>
    <CommandChips options={[option('topic', true), option('note')]} values={{}} active="note" onPress={jest.fn()} {...props} />
  </GestureHandlerRootView>
);

describe('CommandChips', () => {
  beforeEach(() => freshStore());

  it('required_options_are_marked', async () => {
    const { getByTestId, queryByTestId, rerender } = await render(chips());
    expect(getByTestId('command-required-topic')).toBeTruthy();
    expect(queryByTestId('command-required-note')).toBeNull();

    await rerender(chips({ values: { topic: { label: 'x', value: 'x' } } }));
    expect(queryByTestId('command-required-topic')).toBeNull();
  });

  it('tapping_a_chip_edits_that_option', async () => {
    const onPress = jest.fn();
    const { getByTestId } = await render(chips({ onPress }));

    await fireEvent.press(getByTestId('command-option-topic'));

    expect(onPress).toHaveBeenCalledWith('topic');
  });

  it('a_filled_option_shows_its_value', async () => {
    const { getByText } = await render(chips({ values: { topic: { label: 'billing', value: 'b' } } }));

    expect(getByText(': billing')).toBeTruthy();
  });

  it('many_options_scroll_sideways', async () => {
    const many = Array.from({ length: 30 }, (_, index) => option(`o${index}`));
    const { getByTestId } = await render(chips({ options: many }));

    expect(getByTestId('command-chips').props.horizontal).toBe(true);
    expect(ScrollView).toBeTruthy();
  });
});
