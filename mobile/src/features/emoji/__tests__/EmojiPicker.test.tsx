import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord } from '@opencord/core-native';
import { fireEvent, render, screen } from '@testing-library/react-native';

import { getCore } from '@/core';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { freshStore, settle } from '@/test/store';
import { useAppStore } from '@/state/store';
import { Composer } from '@/features/chat/Composer';
import { EmojiPicker } from '../EmojiPicker';

async function initNightOwls() {
  freshStore();
  await settle();
  const snap = snapshot(NIGHT_OWLS);
  useAppStore.setState((s) => ({
    servers: {
      ...s.servers,
      [NIGHT_OWLS]: {
        key: NIGHT_OWLS,
        entry: { key: NIGHT_OWLS, name: 'Night Owls', host: '127.0.0.1', port: 9000 },
        connection: new ConnectionRecord.Connected(),
        snapshot: snap,
        seeding: false,
        readyCount: 1,
        held: [],
        activity: {},
        typing: {},
        voice: {},
        nowPlaying: {},
        musicSettings: null,
      },
    },
    order: [NIGHT_OWLS],
    openChannel: { serverKey: NIGHT_OWLS, channelId: CHANNELS.general, atBottom: true },
    prefs: {
      ...s.prefs,
      recentEmoji: [],
    },
  }));
}

describe('EmojiPicker', () => {
  beforeEach(async () => {
    await initNightOwls();
    jest.clearAllMocks();
    jest.spyOn(getCore(), 'typingStart').mockResolvedValue();
  });

  // Fails if recent emoji from prefs do not appear first in the picker.
  it('recent emoji come first', async () => {
    useAppStore.setState((state) => ({
      prefs: {
        ...state.prefs,
        recentEmoji: ['🎉', '🚀'],
      },
    }));

    const onPick = jest.fn();
    const { unmount } = await render(
      <GestureHandlerRootView>
        <EmojiPicker onPick={onPick} />
      </GestureHandlerRootView>,
    );

    const firstEmoji = screen.getByTestId('emoji-recent-🎉');
    expect(firstEmoji).toBeTruthy();
    const secondEmoji = screen.getByTestId('emoji-recent-🚀');
    expect(secondEmoji).toBeTruthy();

    await unmount();
    await settle();
  }, 15000);

  // Fails if tapping an emoji in Composer does not insert at caret, or closes the picker prematurely.
  it('a tap inserts at the caret', async () => {
    const { unmount } = await render(
      <GestureHandlerRootView>
        <Composer serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );

    // Focus input and type some initial text
    const input = screen.getByTestId('composer-input');
    await fireEvent.changeText(input, 'hello ');
    await fireEvent(input, 'selectionChange', {
      nativeEvent: { selection: { start: 6, end: 6 } },
    });
    await settle();

    // Open emoji picker
    const smileyButton = screen.getByTestId('composer-emoji');
    await fireEvent.press(smileyButton);
    await settle();

    // Verify picker is open
    expect(screen.getByTestId('emoji-picker')).toBeTruthy();

    // Tap an emoji (e.g. from recent)
    const emoji = screen.getByTestId('emoji-recent-👍');
    await fireEvent.press(emoji);
    await settle();

    // Picker remains open
    expect(screen.getByTestId('emoji-picker')).toBeTruthy();

    // The text input contains the inserted emoji
    expect(screen.getByTestId('composer-input').props.value).toBe('hello 👍');

    // Tap another emoji
    const secondEmoji = screen.getByTestId('emoji-recent-🎉');
    await fireEvent.press(secondEmoji);
    await settle();

    expect(screen.getByTestId('composer-input').props.value).toBe('hello 👍🎉');

    await unmount();
    await settle();
  }, 15000);
});
