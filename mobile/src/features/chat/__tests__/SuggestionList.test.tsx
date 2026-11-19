import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { fireEvent, render } from '@testing-library/react-native';
import { ChannelKind } from '@opencord/core-native';
import { SuggestionList } from '../SuggestionList';
import type { Suggestion } from '../suggestions';

describe('SuggestionList', () => {
  const suggestions: Suggestion[] = [
    {
      kind: 'member',
      member: {
        user: { id: '10', displayName: 'Alex', fingerprint: 'fp-10' },
        roleIds: ['1'],
        joinedAtMs: 1000,
      },
      role: 'Admin',
      insert: '@Alex ',
    },
    {
      kind: 'channel',
      channel: {
        id: '20',
        kind: ChannelKind.Text,
        name: 'random',
        topic: 'chat about anything',
        position: 0,
        userLimit: 0,
        textInVoice: false,
      },
      insert: '#random ',
    },
    {
      kind: 'emoji',
      emoji: {
        char: '👍',
        name: 'thumbs up',
        shortcode: 'thumbsup',
        shortcodes: ['thumbsup', '+1'],
        category: 'people',
      },
      insert: '👍 ',
    },
  ];

  it('renders all suggestion types and details', async () => {
    // Fails if items or their labels/details are not rendered.
    const onSelect = jest.fn();
    const { getByText, getByTestId } = await render(
      <GestureHandlerRootView>
        <SuggestionList items={suggestions} onSelect={onSelect} />
      </GestureHandlerRootView>,
    );

    expect(getByTestId('suggestion-list')).toBeTruthy();
    expect(getByText('@Alex')).toBeTruthy();
    expect(getByText('Admin')).toBeTruthy();
    expect(getByText('#random')).toBeTruthy();
    expect(getByText('chat about anything')).toBeTruthy();
    expect(getByText(':thumbsup:')).toBeTruthy();
    expect(getByText('thumbs up')).toBeTruthy();
  });

  it('tapping a suggestion calls onSelect', async () => {
    // Fails if tapping an item does not trigger onSelect with that item.
    const onSelect = jest.fn();
    const { getByTestId } = await render(
      <GestureHandlerRootView>
        <SuggestionList items={suggestions} onSelect={onSelect} />
      </GestureHandlerRootView>,
    );

    fireEvent.press(getByTestId('suggestion-member-10'));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(suggestions[0]);
  });

  it('renders nothing when items is empty', async () => {
    // Fails if empty items renders a container.
    const onSelect = jest.fn();
    const { queryByTestId } = await render(
      <GestureHandlerRootView>
        <SuggestionList items={[]} onSelect={onSelect} />
      </GestureHandlerRootView>,
    );
    expect(queryByTestId('suggestion-list')).toBeNull();
  });
});
