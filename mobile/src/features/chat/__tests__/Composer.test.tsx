import { GestureHandlerRootView } from 'react-native-gesture-handler';
import { ConnectionRecord, type ChannelAccess, type MessageRecord } from '@opencord/core-native';
import { act, fireEvent, render } from '@testing-library/react-native';
import { Alert, Text, TextInput } from 'react-native';
import * as ImagePicker from 'expo-image-picker';

import { getCore } from '@/core';
import { CHANNELS, NIGHT_OWLS, snapshot } from '@/core/fake/world';
import { startEditingMessage } from '@/state/actions/chat';
import { useAppStore } from '@/state/store';
import { freshStore, settle } from '@/test/store';

import { attachMenuActions, Composer } from '../Composer';
import { MessageMenu } from '../MessageMenu';

async function initNightOwls(accessOverrides?: Partial<ChannelAccess>, noCommands = false) {
  freshStore();
  await settle();
  const snap = snapshot(NIGHT_OWLS);
  if (noCommands) snap.commands = { commands: [], resultsPublic: true };
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

  describe('commands', () => {
    const composer = () =>
      render(
        <GestureHandlerRootView>
          <Composer serverKey={NIGHT_OWLS} channelId={CHANNELS.general} />
        </GestureHandlerRootView>,
      );

    const actionIds = (canAttach: boolean, hasCommands: boolean) =>
      attachMenuActions(canAttach, hasCommands).map((action) => action.id);

    it('attach_shows_photo_file_and_commands', async () => {
      expect(actionIds(true, true)).toEqual(['photo', 'file', 'commands']);
      expect((await composer()).getByTestId('composer-attach')).toBeTruthy();
    });

    it('no_attach_permission_leaves_only_commands', async () => {
      await initNightOwls({ attach: false });

      expect(actionIds(false, true)).toEqual(['commands']);
      expect((await composer()).getByTestId('composer-attach')).toBeTruthy();
    });

    it('no_commands_leaves_out_the_entry', () => {
      expect(actionIds(true, false)).toEqual(['photo', 'file']);
    });

    it('nothing_usable_hides_the_button', async () => {
      await initNightOwls({ attach: false }, true);

      expect((await composer()).queryByTestId('composer-attach')).toBeNull();
    });

    it('editing_hides_the_button', async () => {
      const view = await composer();
      await act(async () => {
        startEditingMessage(NIGHT_OWLS, CHANNELS.general, 'm1', 'text');
      });

      expect(view.queryByTestId('composer-attach')).toBeNull();
    });

    it('commands_puts_a_slash_and_opens_the_list', async () => {
      const view = await composer();

      await fireEvent(view.getByTestId('composer-attach'), 'onAction', 'commands');

      expect(view.getByPlaceholderText('Message #general').props.value).toBe('/');
      expect(view.getByTestId('command-list')).toBeTruthy();
    });

    it('typing_a_slash_opens_and_deleting_it_closes', async () => {
      const view = await composer();
      const input = view.getByPlaceholderText('Message #general');

      await fireEvent.changeText(input, '/');
      expect(view.getByTestId('command-list')).toBeTruthy();

      await fireEvent.changeText(input, '');
      expect(view.queryByTestId('command-list')).toBeNull();
    });

    it('typing_filters_the_list_and_says_when_nothing_matches', async () => {
      const view = await composer();
      const input = view.getByPlaceholderText('Message #general');

      await fireEvent.changeText(input, '/xyz');

      expect(view.getByText('No commands match /xyz')).toBeTruthy();
    });

    const pickCommand = async (view: Awaited<ReturnType<typeof composer>>, name: string) => {
      await fireEvent.changeText(view.getByPlaceholderText('Message #general'), `/${name}`);
      await fireEvent.press(view.getByLabelText(`/${name}`));
    };

    it('tapping_the_chip_brings_the_keyboard_back', async () => {
      const focus = jest.spyOn(TextInput.prototype, 'focus').mockImplementation(() => {});
      const view = await composer();
      await pickCommand(view, 'roll');
      focus.mockClear();

      await fireEvent.press(view.getByTestId('command-chip'));

      expect(focus).toHaveBeenCalled();
      focus.mockRestore();
    });

    it('after_removing_a_command_the_field_is_plain_and_the_keyboard_stays_up', async () => {
      const blur = jest.spyOn(TextInput.prototype, 'blur').mockImplementation(() => {});
      const view = await composer();
      await pickCommand(view, 'roll');
      expect(view.getByTestId('command-chip')).toBeTruthy();

      await fireEvent.changeText(view.getByTestId('composer-input'), '');

      expect(view.queryByTestId('command-chip')).toBeNull();
      expect(view.getByTestId('composer-input').props.value).toBe('');
      expect(view.getByPlaceholderText('Message #general')).toBeTruthy();
      expect(blur).not.toHaveBeenCalled();
      blur.mockRestore();
    });

    it('sending_a_command_runs_it_and_clears_the_field', async () => {
      const execute = jest.spyOn(getCore(), 'commandExecute');
      const view = await composer();
      await pickCommand(view, 'roll');

      await fireEvent.press(view.getByTestId('composer-send'));

      expect(execute).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.general, 'roll', 'plugin:ticket-bot', [], expect.any(String));
      expect(view.queryByTestId('command-chip')).toBeNull();
    });

    it('a_command_that_cant_be_used_stays_out_of_the_field', async () => {
      const view = await composer();

      await pickCommand(view, 'skip');

      expect(view.queryByTestId('command-chip')).toBeNull();
    });

    it('a_name_typed_and_sent_without_picking_runs_it', async () => {
      const execute = jest.spyOn(getCore(), 'commandExecute');
      const send = jest.spyOn(getCore(), 'messageSend');
      const view = await composer();

      await fireEvent.changeText(view.getByPlaceholderText('Message #general'), '/roll');
      await fireEvent.press(view.getByTestId('composer-send'));

      expect(execute).toHaveBeenCalledWith(NIGHT_OWLS, CHANNELS.general, 'roll', '', [], expect.any(String));
      expect(send).not.toHaveBeenCalled();
      expect(view.getByPlaceholderText('Message #general').props.value).toBe('');
    });

    it('a_slash_that_is_no_command_is_sent_as_a_message', async () => {
      const execute = jest.spyOn(getCore(), 'commandExecute');
      const send = jest.spyOn(getCore(), 'messageSend');
      const view = await composer();

      await fireEvent.changeText(view.getByPlaceholderText('Message #general'), '/home');
      await fireEvent.press(view.getByTestId('composer-send'));

      expect(execute).not.toHaveBeenCalled();
      expect(send).toHaveBeenCalled();
    });

    it('the_caret_stays_after_the_mark_so_backspace_reaches_it', async () => {
      const view = await composer();
      await pickCommand(view, 'ticket');
      const field = () => view.getByTestId('composer-input');

      expect(field().props.selection).toEqual({ start: 1, end: 1 });

      await fireEvent.changeText(field(), 'billing');
      expect(field().props.selection).toBeUndefined();
    });

    it('a_space_after_the_name_closes_the_list', async () => {
      const view = await composer();

      await fireEvent.changeText(view.getByPlaceholderText('Message #general'), '/play now');

      expect(view.queryByTestId('command-list')).toBeNull();
    });
  });
});
