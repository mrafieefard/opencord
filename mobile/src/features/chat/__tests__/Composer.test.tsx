import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, type ChannelAccess, type MessageRecord } from '@opencord/core-native';
import { fireEvent, render } from '@testing-library/react-native';
import { Alert, Text } from 'react-native';
import * as ImagePicker from 'expo-image-picker';

import { getCore } from '@/core';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { startEditingMessage } from '@/state/actions/chat';
import { useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { Composer } from '../Composer';
import { MessageMenu } from '../MessageMenu';

async function initNightOwls(accessOverrides?: Partial<ChannelAccess>) {
  freshStore();
  await settle();
  const snap = snapshot(NIGHT_OWLS);
  if (accessOverrides) {
    snap.access = snap.access.map((a) =>
      a.channelId === CHANNELS.general ? { ...a, ...accessOverrides } : a,
    );
  }
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
  }));
}

describe('Composer and message actions', () => {
  beforeEach(async () => {
    await initNightOwls();
    jest.clearAllMocks();
    jest.spyOn(getCore(), 'typingStart').mockResolvedValue();
  });

  it('the composer is replaced when the user can\'t send', async () => {
    await initNightOwls({ send: false });

    const { getByText, queryByPlaceholderText } = await render(
      <GestureHandlerRootView>
        <Composer serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );

    expect(getByText("You can't send messages here.")).toBeTruthy();
    expect(queryByPlaceholderText('Message #general')).toBeNull();
  });

  it('the send button shows only once there is something to send', async () => {
    const { getByPlaceholderText, queryByTestId } = await render(
      <GestureHandlerRootView>
        <Composer serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );
    expect(queryByTestId('composer-send')).toBeNull();

    const input = getByPlaceholderText('Message #general');
    await fireEvent.changeText(input, 'hi');
    expect(queryByTestId('composer-send')).toBeTruthy();

    // Spaces alone aren't something to send.
    await fireEvent.changeText(input, '   ');
    expect(queryByTestId('composer-send')).toBeNull();
  });

  it('the placeholder keeps to one line, and goes once there is text', async () => {
    const { getByTestId, queryByTestId, getByPlaceholderText } = await render(
      <GestureHandlerRootView>
        <Composer serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );
    expect(getByTestId('composer-placeholder').props.numberOfLines).toBe(1);
    await fireEvent.changeText(getByPlaceholderText('Message #general'), 'hi');
    expect(queryByTestId('composer-placeholder')).toBeNull();
  });

  it('the counter appears from 3,900 characters and sending stops above 4,000', async () => {
    const { getByPlaceholderText, queryByTestId, getByTestId } = await render(
      <GestureHandlerRootView>
        <Composer serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );

    const input = getByPlaceholderText('Message #general');

    // Below 3,900 chars: no counter
    await fireEvent.changeText(input, 'a'.repeat(3899));
    expect(queryByTestId('composer-counter')).toBeNull();

    // At 3,900 chars: counter shows 100
    await fireEvent.changeText(input, 'a'.repeat(3900));
    expect(getByTestId('composer-counter').props.children).toBe('100');

    // Send button is enabled
    expect(getByTestId('composer-send').props.accessibilityState?.disabled).toBeFalsy();

    // Above 4,000 chars: counter shows negative, send button is disabled
    await fireEvent.changeText(input, 'a'.repeat(4001));
    expect(getByTestId('composer-counter').props.children).toBe('-1');
    expect(getByTestId('composer-send').props.accessibilityState?.disabled).toBe(true);
  });

  it('editing puts the text in the composer and saves it', async () => {
    const editSpy = jest.spyOn(getCore(), 'messageEdit').mockResolvedValueOnce({
      id: 'msg-edit-1',
      channelId: CHANNELS.general,
      authorId: '1000',
      content: 'Updated content',
      createdAtMs: Date.now(),
      attachments: [],
    });

    startEditingMessage(NIGHT_OWLS, CHANNELS.general, 'msg-edit-1', 'Original content');

    const { getByText, getByDisplayValue, getByTestId } = await render(
      <GestureHandlerRootView>
        <Composer serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );

    expect(getByText('Editing message')).toBeTruthy();
    const input = getByDisplayValue('Original content');

    await fireEvent.changeText(input, 'Updated content');

    const saveBtn = getByTestId('composer-send');
    await fireEvent.press(saveBtn);

    expect(editSpy).toHaveBeenCalledWith(NIGHT_OWLS, 'msg-edit-1', 'Updated content');
    await settle();

    // Editing mode should be cleared after save
    expect(useAppStore.getState().editingMessage).toBeNull();
  });

  it('deleting asks first', async () => {
    const deleteSpy = jest.spyOn(getCore(), 'messageDelete').mockResolvedValueOnce();
    const alertSpy = jest.spyOn(Alert, 'alert');

    const ownMessage: MessageRecord = {
      id: 'msg-del-1',
      channelId: CHANNELS.general,
      authorId: '1000', // Self
      content: 'Delete me',
      createdAtMs: Date.now(),
      attachments: [],
    };

    const { getByTestId } = await render(
      <MessageMenu
        serverKey={NIGHT_OWLS}
        channelId={CHANNELS.general}
        message={ownMessage}
        trigger="press"
      >
        <Text>Message text</Text>
      </MessageMenu>,
    );

    const menu = getByTestId('message-menu');
    await fireEvent(menu, 'onAction', 'delete');

    expect(alertSpy).toHaveBeenCalledWith(
      'Delete message?',
      'This cannot be undone.',
      expect.any(Array),
    );

    // If cancel is pressed, delete is not called
    const buttons = alertSpy.mock.calls[0]?.[2] as { text: string; onPress?: () => void }[];
    const cancelBtn = buttons.find((b) => b.text === 'Cancel');
    cancelBtn?.onPress?.();
    expect(deleteSpy).not.toHaveBeenCalled();

    // If delete button in alert is pressed, core.messageDelete is called
    const deleteBtn = buttons.find((b) => b.text === 'Delete');
    deleteBtn?.onPress?.();
    expect(deleteSpy).toHaveBeenCalledWith(NIGHT_OWLS, 'msg-del-1');
  });

  it('others\' messages offer Delete only with Manage messages', async () => {
    await initNightOwls({ manageMessages: false });
    const otherMessage: MessageRecord = {
      id: 'msg-other-1',
      channelId: CHANNELS.general,
      authorId: '1001', // Kai (other user)
      content: 'Hello from Kai',
      createdAtMs: Date.now(),
      attachments: [],
    };

    const { getByTestId, unmount } = await render(
      <MessageMenu
        serverKey={NIGHT_OWLS}
        channelId={CHANNELS.general}
        message={otherMessage}
        trigger="press"
      >
        <Text>Message text</Text>
      </MessageMenu>,
    );

    let menu = getByTestId('message-menu');
    let actions = menu.props.actions as { id: string }[];
    expect(actions.some((a) => a.id === 'delete')).toBe(false);

    unmount();

    // 2. With manageMessages: menu DOES have 'delete'
    await initNightOwls({ manageMessages: true });

    const screen2 = await render(
      <MessageMenu
        serverKey={NIGHT_OWLS}
        channelId={CHANNELS.general}
        message={otherMessage}
        trigger="press"
      >
        <Text>Message text</Text>
      </MessageMenu>,
    );

    menu = screen2.getByTestId('message-menu');
    actions = menu.props.actions as { id: string }[];
    expect(actions.some((a) => a.id === 'delete')).toBe(true);
  });

  it('photos and files show as chips and send with progress', async () => {
    const sendFilesSpy = jest.spyOn(getCore(), 'messageSendFiles').mockResolvedValueOnce({
      id: 'msg-file-1',
      channelId: CHANNELS.general,
      authorId: '1000',
      content: 'Here are the docs',
      createdAtMs: Date.now(),
      attachments: [],
    });

    jest.spyOn(ImagePicker, 'launchImageLibraryAsync').mockResolvedValueOnce({
      canceled: false,
      assets: [
        {
          uri: 'file:///cache/photo.jpg',
          fileName: 'photo.jpg',
          fileSize: 45000,
          mimeType: 'image/jpeg',
          width: 800,
          height: 600,
        },
      ],
    });

    // The chips are Gesture Handler views, as in the app.
    const { getByTestId, findByText, getByPlaceholderText } = await render(
      <GestureHandlerRootView>
        <Composer serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
      </GestureHandlerRootView>,
    );

    // Press attach button and pick photo
    const attachBtn = getByTestId('composer-attach');
    await fireEvent(attachBtn, 'onAction', 'photo');

    // Chip appears
    const chip = await findByText('photo.jpg');
    expect(chip).toBeTruthy();

    // Type text
    const input = getByPlaceholderText('Message #general');
    await fireEvent.changeText(input, 'Here are the docs');

    // Send
    const sendBtn = getByTestId('composer-send');
    await fireEvent.press(sendBtn);

    expect(sendFilesSpy).toHaveBeenCalledWith(
      NIGHT_OWLS,
      CHANNELS.general,
      'Here are the docs',
      expect.any(String),
      expect.arrayContaining([expect.objectContaining({ path: '/cache/photo.jpg', filename: 'photo.jpg' })]),
    );
  });
});
